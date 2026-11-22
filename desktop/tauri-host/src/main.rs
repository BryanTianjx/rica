// ICA Personal desktop host — Tauri 2 (ADR-0001 accepted, 2026-11-12).
//
// Host contract (packaging implementation plan §P7), productized from the
// G2 Windows spike, extended for the P1 dual-backend routing (F1-1, signed
// ADR-0007 + the F0-AUDIT disposition):
//
//   resolve the active version unit -> spawn the Rust ica-server (PORT=0)
//   AND the bundled python sidecar (PORT=0) -> parse both listening events
//   -> poll /health on both -> serve the unit's ui/ and forward /api over
//   the `ica` scheme (same origin, so the built frontend needs no API-base
//   change) -> window. Closing the app drops the sidecar's stdin (which is
//   run_sidecar.py's graceful exit) and stops the Rust server.
//
// P1 routing (the F0-AUDIT's signed disposition): `/api/browser/v1/*` and
// the AI-EXT model-CRUD routes still belong to the Python sidecar (the
// browser subsystem is B-series scope and the model CRUD is AI-EXT per
// ADR-0003); every other /api route is served by the Rust ica-server —
// which implements the full 68-route migration contract. When the sidecar
// is absent (a Rust-only unit), the Python-routed paths answer 502 with a
// readable reason instead of failing the launch.
//
// Version-unit resolution order:
//   1. ICA_UNIT_DIR env (tests / explicit override)
//   2. <exe dir>/current.json  — update.sh-managed installs (pointer model)
//   3. <exe dir>/bundle-resources — the unit shipped inside the installer
//   4. <exe dir>/ui — flat dev layout (Rust-only dev slice)
//
// Data root: %LOCALAPPDATA%\ICA\data (the paths.py contract). run_sidecar
// creates the subdirectories and the JWT secret itself; the host only logs.
// The Rust server receives a per-launch JWT_SECRET the host generates, so
// the token flow survives the local default "" (which PyJWT refuses).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ica_contracts::proxy_headers;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;
use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

mod ica_backend;
pub use ica_backend::IcaState;

/// The managed IPC state: a `DesktopState` over the same database URL the
/// spawned server serves (F2 persistence parity), plus the invoke handlers
/// for the F1-2 positions slice.
const DATA_ROOT_SUFFIX: &str = r"ICA\data";
// Interpreter path inside a version unit. The PBS layout differs per archive:
// windows = `python/python.exe`, linux = `python/bin/python3`, and macOS
// (MAC-02) will pin whichever its archive ships.
#[cfg(windows)]
const SIDECAR_PY: &str = r"sidecar\runtime\python\python.exe";
#[cfg(not(windows))]
const SIDECAR_PY: &str = "sidecar/runtime/python/bin/python3";
#[cfg(windows)]
const RUST_SERVER_EXE: &str = "ica-server.exe";
#[cfg(not(windows))]
const RUST_SERVER_EXE: &str = "ica-server";
static PY_PORT: AtomicU16 = AtomicU16::new(0);
static RUST_PORT: AtomicU16 = AtomicU16::new(0);
static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

struct Sidecar {
    child: std::process::Child,
}

struct RustServer {
    child: std::process::Child,
}

static SIDECAR: Mutex<Option<Sidecar>> = Mutex::new(None);
static RUST_SERVER: Mutex<Option<RustServer>> = Mutex::new(None);

/// Which backend a `/api` request belongs to (P1 disposition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    /// The Rust ica-server: the full 68-route migration contract.
    Rust,
    /// The Python sidecar: `/api/browser/v1/*` (B-series) and the AI-EXT
    /// model-CRUD routes, both deliberately outside the Rust contract.
    Python,
}

/// The P1 routing rule, as a pure function over `(method, path)` — path is
/// the `/api`-prefixed request path WITHOUT the query string.
///
/// * `api/browser/v1/…` — the browser subsystem: B-series scope, Python.
/// * AI-EXT model CRUD (ADR-0003 D-2 补充裁决): `POST /api/ai/models`,
///   `PUT|DELETE /api/ai/models/{id}`, `PUT /api/ai/models/{id}/default` —
///   Python. `GET /api/ai/models` and every other `/api/ai/*` route ARE
///   implemented by the port (W5) and stay Rust.
/// * everything else — Rust (the 68-route contract).
fn route_backend(method: &str, path: &str) -> Backend {
    if path.starts_with("api/browser/v1") {
        return Backend::Python;
    }
    let method = method.to_ascii_uppercase();
    if path == "api/ai/models" && method == "POST" {
        return Backend::Python;
    }
    if path.starts_with("api/ai/models/") && matches!(method.as_str(), "PUT" | "DELETE") {
        return Backend::Python;
    }
    Backend::Rust
}

fn log_line(file: &Path, msg: &str) {
    let t = T0
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
    {
        let _ = writeln!(f, "[{:>6}ms] {}", t, msg);
    }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Resolve the active version unit (see the resolution order in the module
/// doc). Returns (unit path, how it was resolved) for the host log.
fn resolve_unit() -> Result<(PathBuf, String), String> {
    if let Ok(env_unit) = std::env::var("ICA_UNIT_DIR") {
        let p = PathBuf::from(&env_unit);
        if p.join("ui").is_dir() || p.join("sidecar").is_dir() {
            return Ok((p, "ICA_UNIT_DIR env".into()));
        }
    }
    let exe = exe_dir();
    let pointer = exe.join("current.json");
    if pointer.is_file() {
        let text = std::fs::read_to_string(&pointer).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if let Some(dir) = v["dir"].as_str() {
            let unit = exe.join(dir);
            if unit.join("ui").is_dir() || unit.join("sidecar").is_dir() {
                return Ok((unit, format!("current.json -> {dir}")));
            }
        }
    }
    let bundled = exe.join("bundle-resources");
    if bundled.join("ui").is_dir() || bundled.join("sidecar").is_dir() {
        return Ok((bundled, "installer bundle-resources".into()));
    }
    if exe.join("ui").is_dir() || exe.join("sidecar").is_dir() {
        return Ok((exe.clone(), "flat dev layout".into()));
    }
    Err(format!(
        "no version unit found next to {} (tried current.json, bundle-resources/, sidecar/)",
        exe.display()
    ))
}

/// Per-user data root, matching backend/app/paths.py: %LOCALAPPDATA%\ICA\data
/// (win32), ~/Library/Application Support/ICA/data (darwin), or
/// $XDG_DATA_HOME|~/.local/share over ICA/data elsewhere. ICA_DATA_DIR wins.
fn resolve_data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("ICA_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("USERPROFILE").unwrap_or_default();
                PathBuf::from(home).join("AppData").join("Local")
            });
        base.join(DATA_ROOT_SUFFIX)
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("ICA")
            .join("data")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home).join(".local").join("share")
            });
        base.join("ICA").join("data")
    }
}

/// A per-launch JWT secret (32 hex chars) for the Rust server's token
/// signing. stdlib-only: time + pid are enough for a per-run secret that
/// never persists.
fn generate_jwt_secret() -> String {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        ^ ((std::process::id() as u128) << 64);
    format!("{seed:032x}")
}

/// The Rust server's listening event, parsed exactly like the sidecar's
/// (same `{"listening": true, "port": N}` handshake contract).
fn parse_listening(line: &str) -> Result<u16, String> {
    let v: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|e| format!("listening json: {e}"))?;
    v["port"]
        .as_u64()
        .ok_or_else(|| "no port in listening event".to_string())
        .map(|p| p as u16)
}

fn spawn_rust_server(
    unit: &Path,
    data_root: &Path,
) -> Result<u16, String> {
    // Resolution order: explicit env override -> the version unit -> the exe
    // directory (flat dev layout).
    let exe = match std::env::var("ICA_RUST_SERVER_EXE") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            let candidates = [
                unit.join(RUST_SERVER_EXE),
                unit.join("server").join(RUST_SERVER_EXE),
                exe_dir().join(RUST_SERVER_EXE),
            ];
            candidates
                .into_iter()
                .find(|c| c.is_file())
                .ok_or_else(|| {
                    format!(
                        "the Rust server binary was not found (tried ICA_RUST_SERVER_EXE, \
                         {}/{RUST_SERVER_EXE}, {}/server/{RUST_SERVER_EXE}, {}/{RUST_SERVER_EXE})",
                        unit.display(),
                        unit.display(),
                        exe_dir().display()
                    )
                })?
        }
    };

    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(data_root)
        .env("PORT", "0")
        .env("HOST", "127.0.0.1")
        .env(
            "DATABASE_URL",
            format!("sqlite:///{}/ica.db", data_root.display()),
        )
        .env("ICA_DATA_DIR", data_root)
        .env("JWT_SECRET", generate_jwt_secret())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().map_err(|e| format!("spawn ica-server: {e}"))?;

    // The listening handshake arrives on stdout; everything else is appended
    // to <data>/logs/ica-server.log.
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let server_log = data_root.join("logs").join("ica-server.log");
    std::fs::create_dir_all(server_log.parent().unwrap()).ok();
    let server_log_err = server_log.clone();
    std::thread::spawn(move || {
        let mut err_reader = BufReader::new(stderr);
        let mut sink = Vec::new();
        let _ = err_reader.read_to_end(&mut sink);
        if !sink.is_empty() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&server_log_err)
            {
                let _ = writeln!(f, "=== ica-server stderr ===");
                let _ = f.write_all(&sink);
            }
        }
    });
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&server_log)
            .ok();
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if l.contains("\"listening\"") {
                        let _ = tx.send(Ok(l.clone()));
                    }
                    if let Some(f) = log.as_mut() {
                        let _ = writeln!(f, "{}", l);
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    break;
                }
            }
        }
    });

    let timeout = std::time::Instant::now() + Duration::from_secs(60);
    let listening = loop {
        if std::time::Instant::now() > timeout {
            let _ = child.kill();
            return Err("no listening event within 60s".into());
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(line)) => break line,
            Ok(Err(e)) => return Err(format!("ica-server stdout read: {e}")),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("ica-server exited before listening".into())
            }
        }
    };
    let port = parse_listening(&listening)?;
    *RUST_SERVER.lock().unwrap() = Some(RustServer { child });
    Ok(port)
}

fn spawn_sidecar(
    unit: &Path,
    data_root: &Path,
    log_file: &Path,
) -> Result<Option<u16>, String> {
    let python = unit.join(SIDECAR_PY);
    let entry = unit.join("sidecar/app/run_sidecar.py");
    if !python.is_file() {
        // P1 degradation: a Rust-only unit is legitimate — the browser
        // subsystem and AI-EXT model CRUD answer 502 through the routing
        // rule, and everything else keeps working from ica-server.
        log_line(
            log_file,
            "host: no bundled python sidecar — browser/v1 and AI-EXT routes \
             will answer 502 (P1 dual-backend degradation)",
        );
        return Ok(None);
    }
    let mut cmd = std::process::Command::new(&python);
    cmd.args(["-S", "-s"])
        .arg(&entry)
        .current_dir(unit)
        .env("ICA_DATA_DIR", data_root)
        .env("PORT", "0")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        // BROWSER-05: the bundled Chromium (playwright download, staged outside
        // the version unit, installed as bundle-resources/browsers) is the ONLY
        // browser this build resolves - pointing at the payload explicitly keeps
        // a developer cache (%LOCALAPPDATA%\ms-playwright) out of the picture.
        .env("PLAYWRIGHT_BROWSERS_PATH", unit.join("browsers"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().map_err(|e| format!("spawn sidecar: {e}"))?;

    // Sidecar stdout holds the listening event; everything (stdout+stderr) is
    // appended to <data>/logs/sidecar.log so a failed launch is diagnosable.
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let sidecar_log = data_root.join("logs").join("sidecar.log");
    std::fs::create_dir_all(sidecar_log.parent().unwrap()).ok();
    let sidecar_log_err = sidecar_log.clone();
    std::thread::spawn(move || {
        let mut err_reader = BufReader::new(stderr);
        let mut sink = Vec::new();
        let _ = err_reader.read_to_end(&mut sink);
        if !sink.is_empty() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&sidecar_log_err)
            {
                let _ = writeln!(f, "=== sidecar stderr ===");
                let _ = f.write_all(&sink);
            }
        }
    });
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&sidecar_log)
            .ok();
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if l.contains("\"listening\"") {
                        let _ = tx.send(Ok(l.clone()));
                    }
                    if let Some(f) = log.as_mut() {
                        let _ = writeln!(f, "{}", l);
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    break;
                }
            }
        }
    });

    let timeout = std::time::Instant::now() + Duration::from_secs(60);
    let listening = loop {
        if std::time::Instant::now() > timeout {
            let _ = child.kill();
            return Err("no listening event within 60s".into());
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(line)) => break line,
            Ok(Err(e)) => return Err(format!("sidecar stdout read: {e}")),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("sidecar exited before listening".into())
            }
        }
    };
    let port = parse_listening(&listening)?;
    *SIDECAR.lock().unwrap() = Some(Sidecar { child });
    Ok(Some(port))
}

/// Minimal blocking HTTP/1.1 forward (Connection: close).
///
/// SEC-02: the webview's full header set is passed in and reduced by the
/// explicit `ica_contracts::proxy_headers` allowlist — nothing is hand-built
/// from browser input. `Host`, `Connection` and `Content-Length` are generated
/// by that shared policy; hop-by-hop / unsafe headers never leave the host.
/// Both directions use the policy's separate allowlists, so the proxy and a
/// direct HTTP call to the same backend observe the same business behaviour.
fn forward_to(
    port: u16,
    method: &str,
    path: &str,
    headers: &tauri::http::HeaderMap,
    body: &[u8],
) -> Result<proxy_headers::ForwardResponse, String> {
    // The `http` crate already parses/validates names and values; a value
    // that is not valid UTF-8 cannot be a token or key and is simply skipped.
    // Values are never logged (SEC-02 §4.5.7).
    let pairs: Vec<(String, String)> = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect();
    let selected = proxy_headers::select_request_headers(pairs).map_err(|e| e.to_string())?;
    let req = proxy_headers::render_request(method, path, port, &selected, body.len())
        .map_err(|e| e.to_string())?;

    let mut stream =
        std::net::TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    stream.write_all(&req).map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| e.to_string())?;
    proxy_headers::parse_response(&response).map_err(|e| e.to_string())
}

fn poll_health(timeout_ms: u64, port: u16) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    while std::time::Instant::now() < deadline {
        if let Ok(forwarded) =
            forward_to(port, "GET", "health", &tauri::http::HeaderMap::new(), b"")
        {
            if forwarded.status == 200 {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

const MIME: &[(&str, &str)] = &[
    (".html", "text/html"),
    (".js", "text/javascript"),
    (".css", "text/css"),
    (".svg", "image/svg+xml"),
    (".png", "image/png"),
    (".ico", "image/x-icon"),
    (".json", "application/json"),
    (".woff2", "font/woff2"),
    (".map", "application/json"),
];

fn mime_of(file: &std::path::Path) -> &'static str {
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    MIME.iter()
        .find(|(e, _)| *e == format!(".{ext}"))
        .map(|(_, m)| *m)
        .unwrap_or("application/octet-stream")
}

fn handle_ica(
    request: tauri::http::Request<Vec<u8>>,
    ui_dir: &Path,
    host_log: &Path,
) -> tauri::http::Response<Vec<u8>> {
    // Diagnostic: log the first three raw request URIs. The custom-scheme URI
    // form varies with the webview layer, and assuming one literal form here
    // once made every request fall back to index.html (blank window).
    static URI_DEBUG_COUNT: AtomicUsize = AtomicUsize::new(0);
    if URI_DEBUG_COUNT.load(Ordering::SeqCst) < 3 {
        URI_DEBUG_COUNT.fetch_add(1, Ordering::SeqCst);
        log_line(
            host_log,
            &format!("host: ica request uri={}", request.uri()),
        );
    }

    // Parse via the Uri components, not string splitting: .path() is
    // authoritative whatever host/port form the webview hands over, and
    // .query() preserves API query strings (audit WP-01 — the old code
    // dropped ?search=…&limit=… on desktop).
    let uri_path = request.uri().path().trim_start_matches('/').to_string();
    let uri_path = if uri_path.is_empty() {
        "index.html".into()
    } else {
        uri_path
    };
    let query = request.uri().query().unwrap_or("");

    if uri_path.starts_with("api/") {
        // The routing decision reads the PATH only (the query can carry a
        // `method`-shaped parameter that must not influence routing).
        let api_path = uri_path.split('?').next().unwrap_or(&uri_path).to_string();
        let api_target = if query.is_empty() {
            uri_path.clone()
        } else {
            format!("{}?{}", uri_path, query)
        };
        let method = request.method().to_string();
        // SEC-02: capture the full webview header set before consuming the
        // body. Values are reduced to the explicit allowlist inside forward_to.
        let headers = request.headers().clone();
        let body: Vec<u8> = request.into_body();
        // P1 dual-backend routing: the browser subsystem and AI-EXT model
        // CRUD live on the Python sidecar, the 68-route contract lives on
        // the Rust server.
        let backend = route_backend(&method, &api_path);
        let (port, backend_name) = match backend {
            Backend::Rust => (RUST_PORT.load(Ordering::SeqCst), "ica-server"),
            Backend::Python => (PY_PORT.load(Ordering::SeqCst), "sidecar"),
        };
        return match forward_to(port, &method, &api_target, &headers, &body) {
            Ok(forwarded) => {
                // SEC-02 §4.6: response headers use their OWN allowlist.
                // Backend errors (401/403/400/500) keep their status, body and
                // safe headers, so desktop answers match a direct HTTP call.
                let mut builder = tauri::http::Response::builder().status(forwarded.status);
                let mut has_content_type = false;
                for (name, value) in &forwarded.headers {
                    if name == "content-type" {
                        has_content_type = true;
                    }
                    builder = builder.header(name.as_str(), value.as_str());
                }
                if !has_content_type {
                    // Preserve the host's legacy default for header-less replies.
                    builder = builder.header("content-type", "application/json");
                }
                builder.body(forwarded.body).unwrap()
            }
            Err(e) => tauri::http::Response::builder()
                .status(502)
                .body(format!("{backend_name} forward failed: {e}").into_bytes())
                .unwrap(),
        };
    }

    let rel = uri_path.trim_start_matches("app/");
    let rel = if rel.is_empty() { "index.html" } else { rel };
    // Adversarial rejection (R3-008): traversal segments, Windows path
    // separators and percent-encoding are never legitimate UI asset names.
    // `PathBuf::starts_with` alone is NOT sufficient — a literal `..`
    // component still passes the prefix check while the OS resolves it
    // outside the UI directory.
    if rel.contains("..") || rel.contains('\\') || rel.contains('%') || rel.contains(':') {
        return tauri::http::Response::builder()
            .status(403)
            .body(b"forbidden".to_vec())
            .unwrap();
    }
    let file = ui_dir.join(rel);
    // Defense in depth: canonicalize and re-verify containment, so even a
    // form this check missed cannot escape the UI directory. A missing file
    // (canonicalize error) falls through to the SPA fallback below.
    let canon_ui = ui_dir.canonicalize().ok();
    let existing = file.canonicalize().ok().filter(|f| match &canon_ui {
        Some(u) => f.starts_with(u),
        None => false,
    });
    if let Some(served) = existing {
        return match std::fs::read(&served) {
            Ok(data) => tauri::http::Response::builder()
                .header("content-type", mime_of(&file))
                .body(data)
                .unwrap(),
            Err(_) => tauri::http::Response::builder()
                .status(404)
                .body(b"not found".to_vec())
                .unwrap(),
        };
    }
    // SPA fallback: the frontend uses BrowserRouter, so a restart or
    // reload on a deep route (/candidates/…, /settings) arrives here
    // as a path with no file behind it. Serve the shell instead of a
    // 404, which used to leave a blank window. Asset-like paths (with
    // an extension) still 404 so missing files stay diagnosable.
    if !rel.contains('.') {
        return match std::fs::read(ui_dir.join("index.html")) {
            Ok(data) => tauri::http::Response::builder()
                .header("content-type", "text/html")
                .body(data)
                .unwrap(),
            Err(_) => tauri::http::Response::builder()
                .status(500)
                .body(b"ui shell missing".to_vec())
                .unwrap(),
        };
    }
    tauri::http::Response::builder()
        .status(404)
        .body(b"not found".to_vec())
        .unwrap()
}

fn stop_backends() {
    // Python: EOF on stdin = run_sidecar's graceful exit. Rust: an explicit
    // stop (the server drains in-flight requests on SIGTERM, but a child
    // kill is the reliable cross-platform close for the host).
    let mut sidecar = SIDECAR.lock().unwrap();
    if let Some(sc) = sidecar.as_mut() {
        drop(sc.child.stdin.take()); // EOF = sidecar graceful exit
    }
    let mut server = RUST_SERVER.lock().unwrap();
    if let Some(rs) = server.as_mut() {
        let _ = rs.child.kill();
    }
}

fn main() {
    let data_root = resolve_data_root();
    let _ = std::fs::create_dir_all(data_root.join("logs"));
    let host_log = data_root.join("logs").join("host.log");
    T0.get_or_init(std::time::Instant::now);

    let (unit, how) = match resolve_unit() {
        Ok(r) => r,
        Err(e) => {
            log_line(&host_log, &format!("host: FATAL {e}"));
            eprintln!("FATAL: {e}");
            std::process::exit(1);
        }
    };
    log_line(
        &host_log,
        &format!("host: starting; unit={} ({how})", unit.display()),
    );

    // The Rust server is the primary backend: failure is fatal. Dev affordance
    // (mirror of ICA_SIDECAR_PORT): adopt an externally started server — e.g.
    // both backends running inside WSL so the shared SQLite file gets native
    // same-OS locking (a WAL database does not survive the /mnt 9p boundary
    // between a Windows backend and a WSL sidecar — measured split-brain).
    let spawn_or_fatal = |log: &PathBuf| match spawn_rust_server(&unit, &data_root) {
        Ok(port) => port,
        Err(e) => {
            log_line(log, &format!("host: FATAL rust server: {e}"));
            eprintln!("FATAL: rust server: {e}");
            std::process::exit(1);
        }
    };
    let rust_port = match std::env::var("ICA_RUST_SERVER_PORT") {
        Ok(port) if !port.is_empty() => match port.parse::<u16>() {
            Ok(p) if p != 0 => {
                log_line(
                    &host_log,
                    &format!("host: adopted external ica-server on 127.0.0.1:{p} (ICA_RUST_SERVER_PORT)"),
                );
                p
            }
            _ => {
                log_line(&host_log, "host: ICA_RUST_SERVER_PORT invalid; spawning");
                spawn_or_fatal(&host_log)
            }
        },
        _ => spawn_or_fatal(&host_log),
    };
    RUST_PORT.store(rust_port, Ordering::SeqCst);
    log_line(
        &host_log,
        &format!("host: ica-server listening on 127.0.0.1:{rust_port}"),
    );
    if !poll_health(20_000, rust_port) {
        log_line(&host_log, "host: FATAL ica-server /health never turned 200");
        eprintln!("FATAL: ica-server /health never turned 200");
        std::process::exit(1);
    }

    // The Python sidecar is the secondary backend (browser/v1 + AI-EXT):
    // absence degrades to 502 on those routes instead of failing the launch.
    let py_port = match std::env::var("ICA_SIDECAR_PORT") {
        // Dev/WSL affordance (F1-1 drill): adopt an externally started sidecar
        // — e.g. the baseline running under WSL, reachable via WSL2's
        // localhost forwarding — instead of spawning the bundled one.
        Ok(port) if !port.is_empty() => match port.parse::<u16>() {
            Ok(p) if p != 0 => {
                log_line(
                    &host_log,
                    &format!("host: adopted external sidecar on 127.0.0.1:{p} (ICA_SIDECAR_PORT)"),
                );
                p
            }
            _ => {
                log_line(&host_log, "host: ICA_SIDECAR_PORT invalid; no sidecar");
                0
            }
        },
        Ok(_) => 0,
        _ => match spawn_sidecar(&unit, &data_root, &host_log) {
            Ok(Some(p)) => p,
            Ok(None) => 0,
            Err(e) => {
                log_line(
                    &host_log,
                    &format!("host: python sidecar failed to spawn ({e}); continuing Rust-only"),
                );
                0
            }
        },
    };
    PY_PORT.store(py_port, Ordering::SeqCst);
    if py_port != 0 {
        log_line(
            &host_log,
            &format!("host: sidecar listening on 127.0.0.1:{py_port}"),
        );
        if !poll_health(20_000, py_port) {
            log_line(
                &host_log,
                "host: sidecar /health never turned 200; continuing (browser routes will 502)",
            );
            PY_PORT.store(0, Ordering::SeqCst);
        }
    }

    // F1-2: connect the host's own DesktopState over the SAME database URL
    // the server got, so an IPC write and an HTTP read observe each other.
    let ica_database_url = format!("sqlite:///{}/ica.db", data_root.display());
    let ica_state = tauri::async_runtime::block_on(IcaState::connect(
        &ica_database_url,
        ica_desktop::DesktopConfig {
            app_name: "ICA Personal".to_string(),
            ai_enabled: false,
            ocr_upload_dir: std::path::Path::new(&data_root).join("ocr"),
            ai_provider: "openai_compatible".to_string(),
            ai_model: "glm-4-flash".to_string(),
            ai_base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            ai_include_candidate_name: false,
            database_url: ica_database_url.clone(),
            backup_dir: std::path::Path::new(&data_root)
                .join("backups")
                .display()
                .to_string(),
            backup_keep_count: 10,
            data_retention_days: 180,
            ai_max_tokens: 1500,
            ai_timeout: 60,
            ai_temperature: 0.4,
            ai_daily_budget_cents: 0,
            ai_budget_warning_threshold: 90,
            jwt_secret: String::new(),
            jwt_access_token_expire_minutes: 15,
            jwt_refresh_token_expire_days: 7,
            bcrypt_cost: 4,
            database_path: format!("{}/ica.db", data_root.display()),
        },
    ))
    .map_err(|e| {
        log_line(&host_log, &format!("host: FATAL ipc state: {e}"));
        eprintln!("FATAL: ipc state: {e}");
    })
    .unwrap_or_else(|_| std::process::exit(1));

    let ui_dir = unit.join("ui");
    let host_log_setup = host_log.clone();
    let host_log_event = host_log.clone();
    let host_log_scheme = host_log.clone();
    let host_log_single = host_log.clone();
    let host_log_window = host_log.clone();
    tauri::Builder::default()
        // R3-006: a second launch must never open a duplicate window sharing
        // the same SQLite database — focus the running instance instead. This
        // plugin must be registered before any other.
        .plugin(tauri_plugin_single_instance::init(
            move |app, _args, _cwd| {
                log_line(
                    &host_log_single,
                    "host: second launch detected; focusing the running window",
                );
                use tauri::Manager;
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_focus();
                }
            },
        ))
        .register_uri_scheme_protocol("ica", move |ctx, request| {
            let _ = ctx;
            handle_ica(request, &ui_dir, &host_log_scheme)
        })
        .manage(ica_state)
        .invoke_handler(tauri::generate_handler![
            ica_backend::positions_list,
            ica_backend::positions_get,
            ica_backend::positions_create,
            ica_backend::positions_patch,
            ica_backend::positions_delete,
        ])
        .setup(move |app| {
            // Open at the app root so the SPA router lands on the Dashboard
            // route (工作台) directly; navigating to /index.html instead would
            // match no BrowserRouter route and leave the content area empty
            // until the operator clicks a nav link.
            let url: tauri::Url = "http://ica.localhost/".parse().expect("valid url");
            // R3-006 / G409-D5: a window-creation failure (e.g. the WebView2
            // profile directory locked by a leftover browser process) used to
            // panic the host with a bare exit 101. Fail with a readable log
            // line, shut the backends down, and exit 2 instead.
            let win_result = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("ICA Personal")
                .inner_size(1280.0, 860.0)
                .build();
            let win = match win_result {
                Ok(win) => win,
                Err(err) => {
                    log_line(
                        &host_log_window,
                        &format!(
                            "host: main window creation failed: {err}. Remedy: close \
                             leftover ICA processes (they can hold the WebView2 profile \
                             lock), or delete the WebView2 data folder named EBWebView \
                             next to this app's data, then relaunch."
                        ),
                    );
                    stop_backends();
                    std::process::exit(2);
                }
            };
            let _ = win.set_focus();
            log_line(&host_log_setup, "host: ui loaded (http://ica.localhost/)");
            Ok(())
        })
        .on_window_event(move |_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                log_line(&host_log_event, "host: window destroyed; stopping backends");
                stop_backends();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── R3-008 adversarial tests: ica:// protocol boundary ─────────────────────
//
// These pin the security contract of `handle_ica` against the traversal /
// encoding / malformed-URI class of attacks the R3.0 audit (risk R3-008)
// requires evidence for. Every rejected case must return 403 BEFORE any
// filesystem access; every legitimate case must keep serving.

#[cfg(test)]
mod protocol_boundary_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ui_fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ica-host-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), b"<html>shell</html>").unwrap();
        std::fs::write(dir.join("assets").join("app.js"), b"console.log(1);").unwrap();
        dir
    }

    fn req(uri: &str) -> tauri::http::Request<Vec<u8>> {
        tauri::http::Request::builder()
            .uri(uri)
            .body(Vec::new())
            .unwrap()
    }

    fn status_of(uri: &str, ui_dir: &PathBuf, log: &PathBuf) -> u16 {
        let response = handle_ica(req(uri), ui_dir, log);
        response.status().as_u16()
    }

    fn next_log(dir: &PathBuf) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let log = dir.join(format!(
            "test-host-{}.log",
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_file(&log);
        log
    }

    #[test]
    fn traversal_parent_segments_are_rejected() {
        let ui = ui_fixture("traversal");
        let log = next_log(&ui);
        for uri in [
            "ica://localhost/../version-manifest.json",
            "ica://localhost/../../../Windows/win.ini",
            "ica://localhost/assets/../../secrets.env",
            "ica://localhost/a/../../b/../../../x.js",
        ] {
            assert_eq!(
                status_of(uri, &ui, &log),
                403,
                "traversal must be rejected: {uri}"
            );
        }
    }

    #[test]
    fn backslash_traversal_is_rejected() {
        let ui = ui_fixture("backslash");
        let log = next_log(&ui);
        assert_eq!(status_of("ica://localhost/..\\..\\win.ini", &ui, &log), 403);
        assert_eq!(
            status_of("ica://localhost/assets\\..\\..\\secret.env", &ui, &log),
            403
        );
    }

    #[test]
    fn percent_encoded_paths_are_rejected() {
        let ui = ui_fixture("encoded");
        let log = next_log(&ui);
        // Encoded traversal and any %-sequence: UI asset names never contain
        // `%`, so rejecting the character class is safe and simpler than
        // decoding (which would risk double-encoding bypasses).
        assert_eq!(
            status_of("ica://localhost/%2e%2e/%2e%2e/win.ini", &ui, &log),
            403
        );
        assert_eq!(status_of("ica://localhost/assets/app%2Ejs", &ui, &log), 403);
    }

    #[test]
    fn drive_qualified_and_absolute_forms_are_rejected() {
        let ui = ui_fixture("absolute");
        let log = next_log(&ui);
        // A path with its own drive prefix would REPLACE the joined base.
        assert_eq!(
            status_of("ica://localhost/O:/Windows/win.ini", &ui, &log),
            403
        );
        assert_eq!(
            status_of("ica://localhost//O:/Windows/win.ini", &ui, &log),
            403
        );
    }

    #[test]
    fn legitimate_assets_and_shell_still_served() {
        let ui = ui_fixture("legit");
        let log = next_log(&ui);
        assert_eq!(status_of("ica://localhost/", &ui, &log), 200);
        assert_eq!(status_of("ica://localhost/index.html", &ui, &log), 200);
        let resp = handle_ica(req("ica://localhost/assets/app.js"), &ui, &log);
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/javascript"
        );
        assert_eq!(resp.body(), b"console.log(1);");
        // SPA fallback: extension-less deep routes serve the shell.
        assert_eq!(status_of("ica://localhost/settings", &ui, &log), 200);
        // app/ prefix still resolves into the UI dir.
        assert_eq!(status_of("ica://localhost/app/index.html", &ui, &log), 200);
    }

    #[test]
    fn missing_assets_404_and_unknown_api_forwards_to_a_backend() {
        let ui = ui_fixture("missing");
        let log = next_log(&ui);
        // Asset-like missing file: 404 (not the shell).
        assert_eq!(status_of("ica://localhost/assets/nope.js", &ui, &log), 404);
        // The api branch is reached (both backends absent in tests -> the
        // routed forward 502s); this proves the adversarial filter does not
        // swallow /api traffic — for the RUST-routed default…
        assert_eq!(status_of("ica://localhost/api/health", &ui, &log), 502);
        // …and for the PYTHON-routed browser prefix.
        assert_eq!(
            status_of("ica://localhost/api/browser/v1/runtime/status", &ui, &log),
            502
        );
    }

    // ── P1 dual-backend routing table ──────────────────────────────────────

    #[test]
    fn browser_v1_routes_to_python() {
        for (method, path) in [
            ("GET", "api/browser/v1/runtime/status"),
            ("POST", "api/browser/v1/runtime/start"),
            ("POST", "api/browser/v1/sessions"),
            ("DELETE", "api/browser/v1/sessions/abc"),
            ("POST", "api/browser/v1/sessions/abc/actions"),
            ("GET", "api/browser/v1/candidate-extractions"),
            ("POST", "api/browser/v1/permissions"),
        ] {
            assert_eq!(
                route_backend(method, path),
                Backend::Python,
                "{method} {path} must ride the sidecar"
            );
        }
        // The bare prefix routes too (a trailing-slash or list-form request).
        assert_eq!(route_backend("GET", "api/browser/v1"), Backend::Python);
    }

    #[test]
    fn ai_ext_model_crud_routes_to_python() {
        // AI-EXT (ADR-0003 D-2 补充裁决): the four mutating model routes.
        assert_eq!(route_backend("POST", "api/ai/models"), Backend::Python);
        assert_eq!(
            route_backend("PUT", "api/ai/models/glm-4-flash"),
            Backend::Python
        );
        assert_eq!(
            route_backend("DELETE", "api/ai/models/glm-4-flash"),
            Backend::Python
        );
        // The default slot carries a slash in the model id.
        assert_eq!(
            route_backend("PUT", "api/ai/models/glm-4-flash/default"),
            Backend::Python
        );
    }

    #[test]
    fn implemented_w5_routes_stay_on_rust() {
        // GET /ai/models IS the port's (W5) — only the mutating model routes
        // are AI-EXT.
        assert_eq!(route_backend("GET", "api/ai/models"), Backend::Rust);
        assert_eq!(route_backend("GET", "api/ai/routing"), Backend::Rust);
        assert_eq!(route_backend("PATCH", "api/ai/routing"), Backend::Rust);
        assert_eq!(route_backend("GET", "api/ai/usage"), Backend::Rust);
        assert_eq!(route_backend("PATCH", "api/ai/budget"), Backend::Rust);
        // The extraction confirm-candidate lives under /api/imports — Rust.
        assert_eq!(
            route_backend("POST", "api/imports/abc/confirm-candidate"),
            Backend::Rust
        );
    }

    #[test]
    fn the_68_route_contract_stays_on_rust() {
        for (method, path) in [
            ("GET", "api/positions"),
            ("POST", "api/positions"),
            ("DELETE", "api/positions/p-1"),
            ("GET", "api/candidates"),
            ("GET", "api/candidates/c-1/timeline"),
            ("GET", "api/candidates/c-1/context"),
            ("POST", "api/conversations"),
            ("POST", "api/messages"),
            ("POST", "api/follow-ups/evaluate"),
            ("POST", "api/follow-ups/t-1/record-result"),
            ("GET", "api/dashboard"),
            ("GET", "api/drafts"),
            ("POST", "api/drafts/generate"),
            ("GET", "api/draft-groups/g-1"),
            ("POST", "api/imports/preview"),
            ("POST", "api/imports/ocr"),
            ("POST", "api/export"),
            ("POST", "api/import"),
            ("GET", "api/settings"),
            ("PUT", "api/settings/ai-key"),
            ("DELETE", "api/settings/ai-key"),
            ("GET", "api/settings/ai-egress"),
            ("POST", "api/settings/ai-egress/consent"),
            ("GET", "api/audit-events"),
            ("GET", "api/backups"),
            ("POST", "api/backups/n.db/restore"),
            ("POST", "api/auth/register"),
            ("POST", "api/auth/login"),
            ("POST", "api/auth/refresh"),
            ("POST", "api/auth/logout"),
            ("GET", "api/auth/me"),
            ("POST", "api/shares"),
            ("GET", "api/shares"),
            ("DELETE", "api/shares/s-1"),
        ] {
            assert_eq!(
                route_backend(method, path),
                Backend::Rust,
                "{method} {path} must ride ica-server"
            );
        }
        // The browser/v1 prefix must not swallow sibling namespaces: a route
        // like /api/browserish would be a Rust contract route.
        assert_eq!(route_backend("GET", "api/browser"), Backend::Rust);
        // /api/ai/models-default (no slash) is not the AI-EXT default slot.
        assert_eq!(route_backend("PUT", "api/ai/models-default"), Backend::Rust);
    }

    #[test]
    fn the_listening_handshake_contract_matches_the_sidecar() {
        // The Rust server's stdout handshake is the SAME line the sidecar
        // emits (run_sidecar.py) — the host parses both with one parser.
        assert_eq!(
            parse_listening(r#"{"listening": true, "port": 51234}"#).unwrap(),
            51234
        );
        assert!(parse_listening("{}").is_err());
        assert!(parse_listening("not json").is_err());
    }
}
