//! Browser Runtime POC worker (B1): one managed Chromium instance, driven
//! over the DevTools protocol (CDP) on a local WebSocket.
//!
//! **Process boundary (ADR-BROWSER-001/006, plan §2.2):** this process is the
//! Browser Runtime. It NEVER touches the business database — it has no
//! storage code, and the supervisor must not hand it `DATABASE_URL` (the POC
//! test asserts that). Its whole world is: launch Chromium with a dedicated
//! profile, speak CDP, answer line-JSON commands on stdin.
//!
//! **Why CDP, not Playwright (ADR-BROWSER-003 evidence):** the plan's first
//! choice is Playwright + a Node worker, and the choice is NOT settled until
//! the POC measures packaging, browser downloads and platform behaviour. The
//! environment this POC ran in has no standalone Node on the WSL side (only
//! editor-embedded binaries), so the worker is implemented on the *other*
//! ADR-BROWSER-003 alternative — Rust + CDP — which doubles as real selection
//! evidence: the same task set works with no Node dependency at all. The
//! Node/Playwright comparison stays open and is recorded in the evidence doc.
//!
//! Protocol: one JSON object per line on stdin, one per line on stdout.
//!
//! ```json
//! {"seq": 1, "cmd": "open",   "url": "file:///tmp/page.html"}
//! {"seq": 2, "cmd": "read"}
//! {"seq": 3, "cmd": "screenshot", "path": "/tmp/shot.png"}
//! {"seq": 4, "cmd": "click",  "selector": "#next"}
//! {"seq": 5, "cmd": "fill",   "selector": "#name", "text": "张伟"}
//! {"seq": 6, "cmd": "stop"}
//! ```
//!
//! Every reply carries the same `seq`; a failure replies
//! `{"seq": n, "ok": false, "code": "…", "error": "..."}` and the worker
//! STAYS ALIVE — one failed command is not a crashed runtime. The `code` is
//! the shared §9.2 vocabulary from `ica-contracts`; `error` is the legacy
//! human-readable text the supervisor already parses.

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async_main());
}

async fn async_main() {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let stdout = std::io::stdout();
    // Chromium state survives across commands; one browser per worker.
    let mut session: Option<CdpSession> = None;

    // stdin closed: the supervisor is gone — exit quietly.
    while let Some(Ok(line)) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        let command: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let message = format!("bad command json: {error}");
                let code = ica_contracts::error::sidecar::code_for_error_message(&message);
                reply(
                    &stdout,
                    json!({"ok": false, "code": code, "error": message}),
                );
                continue;
            }
        };
        let seq = command.get("seq").cloned().unwrap_or(Value::Null);

        let outcome = handle(&mut session, command).await;
        match outcome {
            Ok(mut value) => {
                if let Some(object) = value.as_object_mut() {
                    object.insert("seq".to_string(), seq);
                }
                reply(&stdout, value);
            }
            Err(error) => {
                let code = ica_contracts::error::sidecar::code_for_error_message(&error);
                reply(
                    &stdout,
                    json!({"seq": seq, "ok": false, "code": code, "error": error}),
                );
            }
        }
        if line.contains("\"stop\"") {
            break;
        }
    }
    if let Some(cdp) = session.as_mut() {
        let _ = cdp.close().await;
    }
}

async fn handle(session: &mut Option<CdpSession>, command: Value) -> Result<Value, String> {
    match command.get("cmd").and_then(|value| value.as_str()) {
        Some("start") => {
            if session.is_some() {
                return Ok(json!({"ok": true, "already": true}));
            }
            let chromium = string_arg(&command, "chromium")?;
            let profile = string_arg(&command, "profile")?;
            let mut cdp = CdpSession::launch(Path::new(&chromium), Path::new(&profile)).await?;
            let title = cdp.title().await?;
            // Store the browser INTO the session slot: the whole point of the
            // worker is that this one instance serves every later command.
            *session = Some(cdp);
            Ok(json!({"ok": true, "title": title}))
        }
        Some("open") => {
            let cdp = current(session)?;
            let url = string_arg(&command, "url")?;
            cdp.navigate(&url).await?;
            Ok(json!({"ok": true, "title": cdp.title().await?}))
        }
        Some("read") => {
            let cdp = current(session)?;
            Ok(json!({
                "ok": true,
                "title": cdp.title().await?,
                "outline": cdp.outline().await?,
            }))
        }
        Some("screenshot") => {
            let cdp = current(session)?;
            let path = string_arg(&command, "path")?;
            let base64 = cdp.screenshot().await?;
            let bytes = base64_decode(&base64)?;
            std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
            Ok(
                json!({"ok": true, "bytes": std::fs::metadata(&path).map_err(|e| e.to_string())?.len()}),
            )
        }
        Some("click") => {
            let cdp = current(session)?;
            let selector = string_arg(&command, "selector")?;
            cdp.evaluate(&format!(
                "(() => {{ const el = document.querySelector({selector}); \
                 if (!el) throw new Error('element not found'); \
                 el.click(); return el.tagName; }})()",
                selector = js_string(&selector)
            ))
            .await?;
            Ok(json!({"ok": true}))
        }
        Some("fill") => {
            let cdp = current(session)?;
            let selector = string_arg(&command, "selector")?;
            let text = string_arg(&command, "text")?;
            cdp.evaluate(&format!(
                "(() => {{ const el = document.querySelector({selector}); \
                 if (!el) throw new Error('element not found'); \
                 el.value = {text}; \
                 el.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                 return el.value.length; }})()",
                selector = js_string(&selector),
                text = js_string(&text)
            ))
            .await?;
            Ok(json!({"ok": true}))
        }
        // Isolation probe (B1 AC): the supervisor (or a test) can ask the
        // worker what database-shaped environment it can see. The honest
        // answer is always "none" — this process has no storage code AND no
        // database variables.
        Some("env_probe") => Ok(json!({
            "ok": true,
            "has_database_url": std::env::var_os("DATABASE_URL").is_some(),
            "has_ica_data_dir": std::env::var_os("ICA_DATA_DIR").is_some(),
        })),
        Some("stop") => {
            if let Some(cdp) = session.as_mut() {
                cdp.close().await?;
            }
            *session = None;
            Ok(json!({"ok": true}))
        }
        Some(other) => Err(format!("unknown command: {other}")),
        None => Err("missing cmd".to_string()),
    }
}

fn current(session: &mut Option<CdpSession>) -> Result<&mut CdpSession, String> {
    session
        .as_mut()
        .ok_or_else(|| "browser not started (send cmd=start first)".to_string())
}

fn string_arg(command: &Value, key: &str) -> Result<String, String> {
    command
        .get(key)
        .and_then(|value| value.as_str())
        .map(|text| text.to_string())
        .ok_or_else(|| format!("missing {key}"))
}

/// Minimal JSON string literal for embedding into a Runtime.evaluate call.
fn js_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

fn reply(stdout: &std::io::Stdout, value: Value) {
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{value}");
    let _ = handle.flush();
}

fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    // The worker avoids a base64 crate: decode with a tiny loop instead.
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for character in text.bytes() {
        if character == b'=' || character == b'\n' || character == b'\r' {
            continue;
        }
        let value = TABLE
            .iter()
            .position(|entry| *entry == character)
            .ok_or_else(|| "invalid base64".to_string())? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// One CDP session: the Chromium process plus the page-level WebSocket.
struct CdpSession {
    child: tokio::process::Child,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
}

impl CdpSession {
    /// Launch Chromium headless with a DEDICATED profile directory
    /// (`--user-data-dir` is what makes the login state isolated, ADR-BROWSER-002)
    /// and the debugging port on loopback, then attach to the page target.
    async fn launch(chromium: &Path, profile: &Path) -> Result<CdpSession, String> {
        std::fs::create_dir_all(profile).map_err(|error| error.to_string())?;
        let mut child = tokio::process::Command::new(chromium)
            .arg("--headless=new")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg("--remote-debugging-port=0")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-gpu")
            .arg("about:blank")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("cannot launch chromium: {error}"))?;

        // The `DevToolsActivePort` file (first line = port) appears once the
        // debugger is up; wait for it, bounded.
        let port_file = profile.join("DevToolsActivePort");
        let mut port = None;
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(&port_file) {
                if let Some(first) = text.lines().next() {
                    if let Ok(value) = first.trim().parse::<u16>() {
                        port = Some(value);
                        break;
                    }
                }
            }
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("chromium exited during startup".to_string());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let Some(port) = port else {
            let _ = child.kill().await;
            return Err("DevToolsActivePort never appeared".to_string());
        };

        // /json/list gives the page-level WebSocket URL for the first target.
        let body: Value = reqwestless_get_json(port, "/json/list").await?;
        let ws_url = body
            .as_array()
            .and_then(|targets| targets.first())
            .and_then(|target| target.get("webSocketDebuggerUrl"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| "no page target with webSocketDebuggerUrl".to_string())?
            .to_string();

        let (ws, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|error| format!("cannot connect CDP websocket: {error}"))?;

        Ok(CdpSession {
            child,
            ws,
            next_id: 1,
        })
    }

    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.ws
            .send(Message::Text(
                json!({"id": id, "method": method, "params": params}).to_string(),
            ))
            .await
            .map_err(|error| format!("cdp send failed: {error}"))?;
        // Read until the reply with OUR id; events are skipped.
        while let Some(message) = self.ws.next().await {
            let message = message.map_err(|error| format!("cdp read failed: {error}"))?;
            let Message::Text(text) = message else {
                continue;
            };
            let value: Value =
                serde_json::from_str(&text).map_err(|error| format!("bad cdp json: {error}"))?;
            if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if let Some(error) = value.get("error") {
                    return Err(format!("cdp error: {error}"));
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
        }
        Err("cdp stream closed".to_string())
    }

    async fn navigate(&mut self, url: &str) -> Result<(), String> {
        self.call("Page.navigate", json!({"url": url})).await?;
        // Wait for the load event so the following read/click sees the page.
        self.wait_event("Page.loadEventFired", Duration::from_secs(5))
            .await
    }

    async fn title(&mut self) -> Result<String, String> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({"expression": "document.title", "returnByValue": true}),
            )
            .await?;
        Ok(result
            .get("result")
            .and_then(|value| value.get("value"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string())
    }

    async fn outline(&mut self) -> Result<Value, String> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": "JSON.stringify({\
                        headings: [...document.querySelectorAll('h1,h2')].map(e => e.textContent.trim()),\
                        inputs: [...document.querySelectorAll('input,button')].map(e => e.id || e.type),\
                        links: document.links.length\
                    })",
                    "returnByValue": true
                }),
            )
            .await?;
        let text = result
            .get("result")
            .and_then(|value| value.get("value"))
            .and_then(|value| value.as_str())
            .unwrap_or("{}")
            .to_string();
        serde_json::from_str(&text).map_err(|error| error.to_string())
    }

    async fn screenshot(&mut self) -> Result<String, String> {
        let result = self
            .call("Page.captureScreenshot", json!({"format": "png"}))
            .await?;
        result
            .get("data")
            .and_then(|value| value.as_str())
            .map(|text| text.to_string())
            .ok_or_else(|| "no screenshot data".to_string())
    }

    async fn evaluate(&mut self, expression: &str) -> Result<Value, String> {
        self.call(
            "Runtime.evaluate",
            json!({"expression": expression, "returnByValue": true}),
        )
        .await
    }

    async fn wait_event(&mut self, method: &str, timeout: Duration) -> Result<(), String> {
        let wait = tokio::time::timeout(timeout, async {
            while let Some(message) = self.ws.next().await {
                let message = message.map_err(|error| format!("cdp read failed: {error}"))?;
                let Message::Text(text) = message else {
                    continue;
                };
                let value: Value = serde_json::from_str(&text)
                    .map_err(|error| format!("bad cdp json: {error}"))?;
                if value.get("method").and_then(|v| v.as_str()) == Some(method) {
                    return Ok(());
                }
            }
            Err("cdp stream closed while waiting for event".to_string())
        })
        .await;
        match wait {
            Ok(result) => result,
            Err(_) => Err(format!("timeout waiting for {method}")),
        }
    }

    async fn close(&mut self) -> Result<(), String> {
        let _ = self.call("Browser.close", json!({})).await;
        let _ = self.child.wait().await;
        Ok(())
    }
}

/// The one HTTP call the POC needs (`/json/list`) without a full HTTP client:
/// CDP speaks plain HTTP/1.1 here.
async fn reqwestless_get_json(port: u16, path: &str) -> Result<Value, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|error| format!("cannot connect to debugger port: {error}"))?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&response);
    let body = text
        .split("\r\n\r\n")
        .nth(1)
        .ok_or_else(|| "malformed HTTP response from CDP".to_string())?;
    serde_json::from_str(body).map_err(|error| error.to_string())
}
