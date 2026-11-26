// ICA Personal desktop host — Tauri 2 (ADR-0001 accepted, 2026-11-12).
//
// Runtime contract (ARCH-P3, review decision 1): the Tauri shell is thin
// wiring only. It assembles P2's `AppRuntime` in-process via `DesktopState`
// (Embedded Mode) and delegates every child-process concern — spawn,
// `listening` handshake, health probe, graceful stop with timeout/kill
// fallback, bounded restart — to `ica_desktop::processes::SidecarManager`.
// There is no `Command::new` / `kill` and no business rule in this file.
// The local Rust HTTP listener is **not** started at boot by default; the
// `ica://` bridge starts it on the first Rust-routed `/api` request. Set
// `ICA_HTTP_SERVER_MODE=always` for the §6.4 scenarios (third-party tools,
// local web client, automation, debugging, future remote service) or `never`
// for pure-IPC mode. All lifecycle knobs come from `RuntimePolicy`.
//
// P1 routing (the F0-AUDIT's signed disposition): `/api/browser/v1/*` and
// the AI-EXT model-CRUD routes belong to the Python sidecar (the browser
// subsystem is B-series scope and the model CRUD is AI-EXT per ADR-0003);
// every other /api route is served by the Rust ica-server — which implements
// the full 68-route migration contract. When the sidecar is absent (a Rust-
// only unit), the Python-routed paths answer 502 with a readable reason.
//
// Version-unit resolution order:
//   1. ICA_UNIT_DIR env (tests / explicit override)
//   2. <exe dir>/current.json  — update.sh-managed installs (pointer model)
//   3. <exe dir>/bundle-resources — the unit shipped inside the installer
//   4. <exe dir>/ui — flat dev layout (Rust-only dev slice)
//
// Data root: %LOCALAPPDATA%\ICA\data (the paths.py contract). run_sidecar
// creates its own secret; the host loads-or-creates a separate 256-bit CSPRNG
// signing key for the Rust server and passes it via JWT_SECRET, so the token
// flow survives the local default "" (which PyJWT refuses). SEC-01: that key
// lives in the OS credential store (0600 file fallback) and is never logged.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ica_desktop::diagnostics::DiagnosticEvent;
use ica_desktop::processes::{ManagedKind, ProcessSpec, SidecarManager};
use ica_desktop::routing::{route_backend, Backend};
use ica_desktop::runtime_policy::RuntimePolicy;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::OnceLock;
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

// Routing state only (no child handles). Ownership of child processes moved
// into `SidecarManager`; these atomics just publish the current ports to the
// `ica://` bridge.
static PY_PORT: AtomicU16 = AtomicU16::new(0);
static RUST_PORT: AtomicU16 = AtomicU16::new(0);
static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
static MANAGER: OnceLock<SidecarManager> = OnceLock::new();
static POLICY: OnceLock<RuntimePolicy> = OnceLock::new();
static HOST_LOG: OnceLock<PathBuf> = OnceLock::new();
static APP_STATE: OnceLock<IcaState> = OnceLock::new();
static HTTP_STARTED: AtomicBool = AtomicBool::new(false);

/// Everything the lazy on-demand HTTP launch needs, captured once at boot.
struct BootContext {
    unit: PathBuf,
    data_root: PathBuf,
    jwt_secret: String,
    /// SEC-P1 AUTH-08: process-local shared secret for the signed identity
    /// assertion attached to Python-bound requests in MultiUser mode.
    sidecar_assertion_secret: String,
}
static BOOT: OnceLock<BootContext> = OnceLock::new();

/// Same switch the Rust API middleware reads (`ica-api::auth::AuthMode`).
/// The shell does not own auth policy, but it must know whether to delegate
/// identity to the Python sidecar and enforce the internal assertion.
fn multiuser_mode() -> bool {
    matches!(
        std::env::var("ICA_AUTH_MODE").as_deref(),
        Ok("multiuser") | Ok("multi-user") | Ok("true")
    )
}

fn log_path() -> PathBuf {
    HOST_LOG
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("host.log"))
}

fn manager() -> &'static SidecarManager {
    MANAGER
        .get()
        .expect("SidecarManager initialized before use")
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

/// Load-or-create the 256-bit JWT signing key for the Rust server.
///
/// SEC-01: the key comes from the OS CSPRNG via the shared infrastructure
/// component, is persisted in the OS credential store (0600 file fallback on
/// Linux), and never appears in source or logs. No `SystemTime`, no PID, no
/// fixed default. If the store is unavailable we fail fast rather than sign
/// tokens with a predictable key.
fn generate_jwt_secret() -> String {
    let provider = ica_desktop::jwt_key::JwtKeyProvider::new(resolve_data_root());
    let key = provider
        .load_or_create()
        .expect("JWT signing key store unavailable: refusing to start with a predictable secret");
    key.to_hex()
}

/// Build the launch description for the Rust HTTP server. The host decides
/// *what* to start; `SidecarManager::start` owns spawn, handshake, health,
/// stdout/stderr logging and shutdown (ARCH-P3 AC1).
fn rust_server_spec(boot: &BootContext, policy: &RuntimePolicy) -> Result<ProcessSpec, String> {
    let exe = match std::env::var("ICA_RUST_SERVER_EXE") {
        Ok(path) if !path.is_empty() => PathBuf::from(path),
        _ => {
            let candidates = [
                boot.unit.join(RUST_SERVER_EXE),
                boot.unit.join("server").join(RUST_SERVER_EXE),
                exe_dir().join(RUST_SERVER_EXE),
            ];
            candidates
                .into_iter()
                .find(|candidate| candidate.is_file())
                .ok_or_else(|| {
                    format!(
                        "the Rust server binary was not found (tried ICA_RUST_SERVER_EXE, \
                         {}/{RUST_SERVER_EXE}, {}/server/{RUST_SERVER_EXE}, {}/{RUST_SERVER_EXE})",
                        boot.unit.display(),
                        boot.unit.display(),
                        exe_dir().display()
                    )
                })?
        }
    };
    let database_url = format!("sqlite:///{}/ica.db", boot.data_root.display());
    Ok(ProcessSpec {
        kind: ManagedKind::RustHttpServer,
        program: exe,
        args: Vec::new(),
        current_dir: boot.data_root.clone(),
        env: vec![
            ("PORT".to_string(), "0".to_string()),
            ("HOST".to_string(), "127.0.0.1".to_string()),
            ("DATABASE_URL".to_string(), database_url),
            (
                "ICA_DATA_DIR".to_string(),
                boot.data_root.display().to_string(),
            ),
            ("JWT_SECRET".to_string(), boot.jwt_secret.clone()),
        ],
        handshake_timeout_ms: policy.handshake_timeout_ms,
        health_timeout_ms: policy.health_timeout_ms,
        log_dir: boot.data_root.join("logs"),
    })
}

/// Start (or restart) the optional local HTTP server through the manager.
fn start_rust_http_server(boot: &BootContext) -> Result<u16, String> {
    let policy = POLICY.get().expect("policy initialized before use");
    let spec = rust_server_spec(boot, policy)?;
    let port = manager().start(spec).map_err(|error| error.to_string())?;
    HTTP_STARTED.store(true, Ordering::SeqCst);
    RUST_PORT.store(port, Ordering::SeqCst);
    log_line(
        &log_path(),
        &format!("host: ica-server listening on 127.0.0.1:{port} (managed by SidecarManager)"),
    );
    Ok(port)
}

/// The first Rust-routed `/api` request starts the listener in the accepted
/// default Embedded mode; `never` keeps the desktop pure-IPC.
fn ensure_rust_backend() -> u16 {
    let port = RUST_PORT.load(Ordering::SeqCst);
    if port != 0 {
        return port;
    }
    let policy = match POLICY.get() {
        Some(policy) if policy.starts_http_on_demand() => policy,
        _ => return 0,
    };
    // One attempt only: a failed spawn must not spin on every request.
    if HTTP_STARTED.swap(true, Ordering::SeqCst) {
        return 0;
    }
    let boot = match BOOT.get() {
        Some(boot) => boot,
        None => return 0,
    };
    log_line(
        &log_path(),
        &format!(
            "host: Embedded Mode — starting ica-server on demand (policy={})",
            policy.http_mode.as_str()
        ),
    );
    match start_rust_http_server(boot) {
        Ok(port) => port,
        Err(error) => {
            log_line(
                &log_path(),
                &format!("host: on-demand ica-server failed: {error}"),
            );
            0
        }
    }
}

/// Build the Python sidecar launch description. `None` means no bundled
/// interpreter, which is a legitimate Rust-only unit (P1 degradation).
fn sidecar_spec(
    unit: &Path,
    data_root: &Path,
    policy: &RuntimePolicy,
    assertion_secret: &str,
    multiuser: bool,
) -> Option<ProcessSpec> {
    let python = unit.join(SIDECAR_PY);
    if !python.is_file() {
        return None;
    }
    let entry = unit.join("sidecar/app/run_sidecar.py");
    // SEC-P1 AUTH-08: the sidecar must know the same process-local secret in
    // order to verify assertions. In MultiUser mode it also has to verify the
    // caller's bearer token, so it receives the host's signing key and mode.
    let mut env = vec![
        ("ICA_DATA_DIR".to_string(), data_root.display().to_string()),
        ("PORT".to_string(), "0".to_string()),
        ("PYTHONDONTWRITEBYTECODE".to_string(), "1".to_string()),
        (
            "PLAYWRIGHT_BROWSERS_PATH".to_string(),
            unit.join("browsers").display().to_string(),
        ),
        (
            "ICA_SIDECAR_ASSERTION_SECRET".to_string(),
            assertion_secret.to_string(),
        ),
    ];
    if multiuser {
        env.push(("AUTH_MODE".to_string(), "multiuser".to_string()));
        env.push(("JWT_SECRET".to_string(), BOOT.get().map(|b| b.jwt_secret.clone()).unwrap_or_default()));
    }
    Some(ProcessSpec {
        kind: ManagedKind::PythonSidecar,
        program: python,
        args: vec![
            "-S".to_string(),
            "-s".to_string(),
            entry.display().to_string(),
        ],
        current_dir: unit.to_path_buf(),
        env,
        handshake_timeout_ms: policy.handshake_timeout_ms,
        health_timeout_ms: policy.health_timeout_ms,
        log_dir: data_root.join("logs"),
    })
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
        // Thin wiring: decide the backend, ask for the port, delegate the
        // HTTP forward (including both SEC-02 header allowlists) to the
        // compilable lower layer. `ensure_rust_backend` is the §6.4 on-demand
        // trigger; it is a no-op unless the policy is Embedded.
        let backend = route_backend(&method, &api_path);
        let (port, backend_name) = match backend {
            Backend::Rust => (ensure_rust_backend(), "ica-server"),
            Backend::Python => (PY_PORT.load(Ordering::SeqCst), "sidecar"),
        };
        let mut pairs: Vec<(String, String)> = headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect();

        // SEC-P1 AUTH-08: for Python-bound routes the host verifies the access
        // bearer and mints a short-lived, bearer-bound assertion. The webview
        // cannot forge it because the secret never leaves the host process.
        if backend == Backend::Python && multiuser_mode() {
            let boot = BOOT.get().expect("boot context initialized");
            let bearer = pairs
                .iter()
                .find(|(name, _)| name == "authorization")
                .and_then(|(_, value)| value.strip_prefix("Bearer "))
                .unwrap_or_default()
                .to_string();
            let request_id = pairs
                .iter()
                .find(|(name, _)| name == "x-request-id")
                .map(|(_, value)| value.as_str())
                .unwrap_or_default()
                .to_string();
            let verified = ica_desktop::internal_assertion::verify_access_bearer(
                &bearer,
                &boot.jwt_secret,
            );
            match verified.and_then(|identity| {
                ica_desktop::internal_assertion::mint_assertion_now(
                    &bearer,
                    &identity,
                    &request_id,
                    &boot.sidecar_assertion_secret,
                )
            }) {
                Ok(assertion) => pairs.push((
                    ica_desktop::internal_assertion::ASSERTION_HEADER.to_string(),
                    assertion,
                )),
                Err(_) => {
                    return tauri::http::Response::builder()
                        .status(401)
                        .header("content-type", "application/json")
                        .body(
                            br#"{"error":{"code":"unauthorized","message":"Authentication is required"}}"#
                                .to_vec(),
                        )
                        .expect("401 response");
                }
            }
        }

        return match ica_desktop::proxy::forward(
            port,
            &method,
            &api_target,
            &pairs,
            &body,
            ica_desktop::proxy::FORWARD_TIMEOUT_MS,
        ) {
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

/// Graceful application exit (REL-01): drain the in-process `AppRuntime`,
/// then let the Sidecar Manager stop children with timeout + kill fallback.
/// The host owns no child handle, so it cannot forget to reap one.
fn stop_backends() {
    let started = std::time::Instant::now();
    if let Some(state) = APP_STATE.get() {
        state.state().runtime().lifecycle().begin_drain();
    }
    let report = manager().shutdown_all();
    for line in report.render_log_lines() {
        log_line(&log_path(), &line);
    }
    if !report.all_clean() {
        log_line(
            &log_path(),
            &format!(
                "host: graceful shutdown timed out; force-killed {:?} (no orphan left)",
                report.force_killed()
            ),
        );
    }
    if let Some(policy) = POLICY.get() {
        if policy.diagnostics_enabled {
            let event = DiagnosticEvent {
                request_id: "shutdown".to_string(),
                principal_id: None,
                source: "Internal",
                module: "host",
                operation: "stop_backends",
                duration_ms: started.elapsed().as_millis() as u64,
                result: "ok",
                error_code: None,
            };
            log_line(&log_path(), &event.render());
        }
    }
    if let Some(state) = APP_STATE.get() {
        state.state().runtime().lifecycle().mark_stopped();
    }
}

fn main() {
    let data_root = resolve_data_root();
    let _ = std::fs::create_dir_all(data_root.join("logs"));
    let host_log = data_root.join("logs").join("host.log");
    let _ = HOST_LOG.set(host_log.clone());
    T0.get_or_init(std::time::Instant::now);

    // One policy, one manager, shared by every wiring site below.
    let policy = RuntimePolicy::from_env();
    let _ = POLICY.set(policy.clone());
    let _ = MANAGER.set(SidecarManager::new(policy.clone()));

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

    log_line(
        &host_log,
        &format!(
            "host: runtime policy http_mode={} handshake_ms={} health_ms={} graceful_ms={}",
            policy.http_mode.as_str(),
            policy.handshake_timeout_ms,
            policy.health_timeout_ms,
            policy.graceful_shutdown_timeout_ms
        ),
    );
    // The JWT key is generated once and shared by the (optional) server and
    // the in-process AppRuntime, so an IPC token verifies against the same
    // secret an HTTP token would use.
    let jwt_secret = generate_jwt_secret();
    // Separate from the long-lived JWT key: assertions are single-request
    // credentials and a restart simply invalidates any outstanding ones.
    let sidecar_assertion_secret = ica_desktop::internal_assertion::generate_secret();
    let _ = BOOT.set(BootContext {
        unit: unit.clone(),
        data_root: data_root.clone(),
        jwt_secret,
        sidecar_assertion_secret,
    });

    // Rust HTTP server: an explicitly adopted external port always wins;
    // otherwise only the legacy `always` policy spawns one at boot. The
    // accepted Embedded default starts nothing here (§6.4).
    let boot_or_zero = || -> u16 {
        if !policy.starts_http_at_boot() {
            return 0;
        }
        let boot = BOOT.get().expect("boot context initialized");
        match start_rust_http_server(boot) {
            Ok(port) => port,
            Err(error) => {
                log_line(&host_log, &format!("host: FATAL rust server: {error}"));
                eprintln!("FATAL: rust server: {error}");
                std::process::exit(1);
            }
        }
    };
    let rust_port = match std::env::var("ICA_RUST_SERVER_PORT") {
        Ok(port) if !port.is_empty() => {
            match port.parse::<u16>() {
                Ok(p) if p != 0 => {
                    log_line(
                    &host_log,
                    &format!("host: adopted external ica-server on 127.0.0.1:{p} (ICA_RUST_SERVER_PORT)"),
                );
                    p
                }
                _ => {
                    log_line(
                        &host_log,
                        "host: ICA_RUST_SERVER_PORT invalid; applying policy",
                    );
                    boot_or_zero()
                }
            }
        }
        _ => boot_or_zero(),
    };
    RUST_PORT.store(rust_port, Ordering::SeqCst);
    if rust_port != 0 {
        log_line(
            &host_log,
            &format!("host: ica-server listening on 127.0.0.1:{rust_port}"),
        );
        if !ica_desktop::proxy::probe_health(rust_port, policy.health_timeout_ms, 200) {
            log_line(&host_log, "host: FATAL ica-server /health never turned 200");
            eprintln!("FATAL: ica-server /health never turned 200");
            std::process::exit(1);
        }
    } else {
        log_line(
            &host_log,
            &format!(
                "host: Embedded Mode — local HTTP server NOT started at boot \
                 (on demand on first /api; ICA_HTTP_SERVER_MODE={})",
                policy.http_mode.as_str()
            ),
        );
    }

    // The Python sidecar is the secondary backend (browser/v1 + AI-EXT):
    // absence degrades to 502 on those routes instead of failing the launch.
    let py_port = match std::env::var("ICA_SIDECAR_PORT") {
        // Dev/WSL affordance (F1-1 drill): adopt an externally started
        // sidecar instead of spawning the bundled one. The host owns no
        // child in that case and will not kill a process it did not start.
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
        _ => {
            let boot = BOOT.get().expect("boot context initialized");
            match sidecar_spec(
                &boot.unit,
                &boot.data_root,
                &policy,
                &boot.sidecar_assertion_secret,
                multiuser_mode(),
            ) {
                None => {
                    log_line(
                        &host_log,
                        "host: no bundled python sidecar — browser/v1 and AI-EXT routes will 502",
                    );
                    0
                }
                Some(spec) => match manager().start(spec) {
                    Ok(port) => port,
                    Err(error) => {
                        log_line(
                            &host_log,
                            &format!("host: python sidecar failed to spawn ({error}); continuing Rust-only"),
                        );
                        0
                    }
                },
            }
        }
    };
    PY_PORT.store(py_port, Ordering::SeqCst);
    if py_port != 0 {
        log_line(
            &host_log,
            &format!("host: sidecar listening on 127.0.0.1:{py_port}"),
        );
        if !ica_desktop::proxy::probe_health(py_port, policy.health_timeout_ms, 200) {
            log_line(
                &host_log,
                "host: sidecar /health never turned 200; continuing (browser routes will 502)",
            );
            PY_PORT.store(0, Ordering::SeqCst);
            manager().set_state(
                ManagedKind::PythonSidecar,
                ica_desktop::processes::LifecycleState::Degraded,
            );
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
            jwt_secret: BOOT
                .get()
                .expect("boot context initialized")
                .jwt_secret
                .clone(),
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

    // The single in-process AppRuntime is now ready (P2 runtime() assembly,
    // AC3 init/destroy flow). Keep a clone for the shutdown path before the
    // builder takes ownership of the managed state.
    let _ = APP_STATE.set(ica_state.clone());
    log_line(
        &host_log,
        &format!(
            "host: AppRuntime ready (phase={})",
            ica_state.state().runtime().lifecycle().phase().as_str()
        ),
    );
    // §11 health snapshot for the components the manager can actually probe.
    for line in manager().runtime_health().render_log_lines() {
        log_line(&host_log, &line);
    }

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
        // Both the Rust server and run_sidecar.py emit a `port` on a line
        // containing `listening`; the parser now lives in the Sidecar Manager
        // and is shared by both (P0 inventory §5 item 4).
        use ica_desktop::processes::parse_listening;
        assert_eq!(
            parse_listening(r#"{"listening": true, "port": 51234}"#),
            Some(51234)
        );
        assert_eq!(
            parse_listening(r#"{"event":"listening","host":"127.0.0.1","port":8000}"#),
            Some(8000)
        );
        assert_eq!(parse_listening("{}"), None);
        assert_eq!(parse_listening("not json"), None);
    }
}
