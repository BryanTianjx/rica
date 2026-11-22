//! Browser Runtime supervisor (B1 POC): process lifecycle, worker IPC and
//! the state records the acceptance requires.
//!
//! Division of labour (ADR-BROWSER-006): the HOST owns the process (spawn,
//! crash detection, restart); the APPLICATION owns the business state — so
//! the supervisor writes `browser_sessions` / `browser_tasks` /
//! `browser_actions` through the same `SqliteStore` every other write uses,
//! while the WORKER process receives no database information at all (the POC
//! test asserts its environment carries no `DATABASE_URL`).
//!
//! Worker protocol: one JSON line per command/reply (see the
//! `ica-browser-worker` binary). The supervisor is the only speaker of the
//! business state machine; the worker only knows browsers.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use chrono::Utc;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use ica_infra::store::{new_uuid, to_sql_timestamp, SqliteStore};

/// The intent the POC task records (`browser_tasks.intent`).
pub const POC_INTENT: &str = "B1 POC: open a local test page, read, screenshot, click, fill";

/// Everything the supervisor needs to reach the worker.
#[derive(Debug, Clone)]
pub struct WorkerLaunch {
    /// Path to the worker binary (`ica-browser-worker`).
    pub worker_bin: PathBuf,
    /// Chromium executable (env `ICA_BROWSER_CHROMIUM`, or the Playwright
    /// cache, or snap — see `discover_chromium`).
    pub chromium: PathBuf,
    /// Dedicated profile directory (`--user-data-dir`): the isolated login
    /// state (ADR-BROWSER-002). Under `ICA_DATA_DIR/browser/profiles/`.
    pub profile_dir: PathBuf,
}

/// Discover a Chromium binary, in the order the POC machines need:
/// explicit env, the Playwright cache (the SAME browser family the plan
/// downloads for Playwright), then the distro snap.
pub fn discover_chromium() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ICA_BROWSER_CHROMIUM") {
        let path = PathBuf::from(dir);
        if path.is_file() {
            return Some(path);
        }
    }

    // Well-known install locations on Windows. Edge ships on every Windows
    // install, Chrome does not — try both, and trust the env vars rather than
    // hardcoding "C:\Program Files" (the 64-bit Program Files is spelled
    // differently on 32-bit hosts, and LOCALAPPDATA covers per-user installs).
    #[cfg(windows)]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        for var in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                let base = PathBuf::from(base);
                candidates.push(
                    base.join("Google")
                        .join("Chrome")
                        .join("Application")
                        .join("chrome.exe"),
                );
                candidates.push(
                    base.join("Microsoft")
                        .join("Edge")
                        .join("Application")
                        .join("msedge.exe"),
                );
            }
        }
        for candidate in candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // Playwright's cache, newest revision first. The directory was renamed
    // `chrome-linux` -> `chrome-linux64` in a later Playwright, so accept both:
    // pinning only the old name silently finds nothing on current caches.
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home {
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut roots = vec![PathBuf::from(&home).join(".cache").join("ms-playwright")];
        #[cfg(windows)]
        roots.push(
            PathBuf::from(&home)
                .join("AppData")
                .join("Local")
                .join("ms-playwright"),
        );
        for root in roots {
            if let Ok(mut entries) = glob_version_dirs(&root.join("chromium-*")) {
                entries.sort();
                for dir in entries.iter().rev() {
                    for sub in ["chrome-linux64", "chrome-linux", "chrome-win"] {
                        let mut candidate = dir.join(sub);
                        if sub == "chrome-win" {
                            candidate.push("chrome.exe");
                        } else {
                            candidate.push("chrome");
                        }
                        if candidate.is_file() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }

    let snap = PathBuf::from("/snap/bin/chromium");
    if snap.is_file() {
        return Some(snap);
    }
    None
}

/// Expand the `chromium-*` glob by hand (no glob crate for one call).
fn glob_version_dirs(pattern: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    let Some(parent) = pattern.parent() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("chromium-") {
            out.push(entry.path());
        }
    }
    out.sort();
    Ok(out)
}

/// One supervised browser session: the rows it wrote and the handles needed
/// to stop it.
pub struct SupervisedSession {
    pub session_id: String,
    pub task_id: String,
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    seq: u64,
    /// Reply pump: the worker's stdout lines, parsed, in arrival order.
    replies: Option<tokio::sync::mpsc::Receiver<Value>>,
}

impl SupervisedSession {
    /// Send one command and wait for the reply line (bounded).
    async fn ask(&mut self, command: Value, timeout: Duration) -> Result<Value, String> {
        let seq = self.seq + 1;
        self.seq = seq;
        let mut command = command;
        command["seq"] = json!(seq);
        let line = command.to_string();
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(|error| format!("worker stdin closed: {error}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| error.to_string())?;
        // The reply arrives on the child's stdout, which the supervisor reads
        // through the piped task started in `spawn` — see `next_reply`.
        let reply = tokio::time::timeout(timeout, self.next_reply(seq))
            .await
            .map_err(|_| format!("worker reply timeout ({timeout:?})"))??;
        Ok(reply)
    }

    /// The replies are pumped by a background task into a shared queue so a
    /// timeout cannot wedge the pipe reader.
    async fn next_reply(&mut self, seq: u64) -> Result<Value, String> {
        loop {
            let reply = self
                .replies
                .as_mut()
                .expect("reply pump started")
                .recv()
                .await
                .ok_or_else(|| "worker stdout closed".to_string())?;
            if reply.get("seq").and_then(|value| value.as_u64()) == Some(seq) {
                return Ok(reply);
            }
            // Not our reply (an out-of-band event): keep looking.
        }
    }
}

/// The environment the worker process runs with: the minimum needed to load
/// and run a binary, and NOTHING else — in particular no `DATABASE_URL`, no
/// `ICA_DATA_DIR`, no DSN of any kind.
/// B1 AC "Browser Worker 无法直接访问 ICA SQLite", made mechanical.
///
/// What "minimum" means differs by platform. On Unix a PATH is enough. On
/// Windows the process will not even start without `SystemRoot`, and the CRT
/// has to be findable through `System32`, so those are added — they carry no
/// database meaning, and a test asserts the absence of the DB variables
/// specifically rather than "the env is empty".
pub fn worker_env() -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    #[cfg(windows)]
    {
        let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        env.insert(
            "PATH".to_string(),
            format!("{sysroot}\\System32;{sysroot}\\System32\\Wbem;{sysroot}"),
        );
        env.insert("SystemRoot".to_string(), sysroot);
        env.insert(
            "TEMP".to_string(),
            std::env::temp_dir().to_string_lossy().to_string(),
        );
        env.insert(
            "USERPROFILE".to_string(),
            std::env::var("USERPROFILE").unwrap_or_default(),
        );
    }
    #[cfg(not(windows))]
    {
        env.insert(
            "PATH".to_string(),
            "/usr/bin:/bin:/usr/local/bin".to_string(),
        );
    }
    env
}

/// Spawn the worker, create the session + task rows, and mark the task
/// `running` — the POC's tasks 1–4 (start Chromium, dedicated profile,
/// worker, worker IPC).
///
/// The worker's environment is built HERE and deliberately carries NO
/// database variable: the isolation proof (B1 AC) is "the worker cannot open
/// the business database", and the test asserts the env as well as the
/// process's own inability (it has no storage code at all).
pub async fn start_session(
    store: &SqliteStore,
    launch: &WorkerLaunch,
    worker_db_env_free: bool,
) -> Result<SupervisedSession, String> {
    let now = to_sql_timestamp(&Utc::now());
    let session_id = new_uuid();
    let task_id = new_uuid();

    let mut connection = store.pool().acquire().await.map_err(|e| e.to_string())?;

    // The dedicated profile row first (the session FK is RESTRICT on it):
    // one profile per POC run, its data dir under the ICA data directory.
    let profile_id = new_uuid();
    let profile_name = format!("poc-{}", &session_id[..8]);
    let data_dir_name = format!("profile-{}", &session_id[..8]);
    sqlx::query(concat!(
        "INSERT INTO browser_profiles (id, name, description, data_dir_name, engine, ",
        "state, created_at, updated_at) ",
        "VALUES (?, ?, ?, ?, 'chromium', 'idle', ?, ?)",
    ))
    .bind(&profile_id)
    .bind(&profile_name)
    .bind("B1 POC managed profile")
    .bind(&data_dir_name)
    .bind(&now)
    .bind(&now)
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    // The session row: one managed browser session (`browser_sessions`),
    // starting; the supervisor flips it to `ready` once the worker is up.
    sqlx::query(concat!(
        "INSERT INTO browser_sessions (id, profile_id, state, started_at, ",
        "created_at, updated_at) VALUES (?, ?, 'starting', ?, ?, ?)",
    ))
    .bind(&session_id)
    .bind(&profile_id)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    // The task row: what this POC intends to do, now `running`.
    let intent_sha = sha256_hex(POC_INTENT.as_bytes());
    let plan = json!({
        "steps": ["open", "read", "screenshot", "click", "fill"],
        "origin": "B1 POC",
    });
    let plan_text = plan.to_string();
    let plan_sha = sha256_hex(plan_text.as_bytes());
    sqlx::query(concat!(
        "INSERT INTO browser_tasks (id, session_id, intent, intent_sha256, plan_json, ",
        "plan_sha256, state, risk_level, current_step, steps_count, executed_steps, ",
        "started_at, created_at, updated_at)",
        " VALUES (?, ?, ?, ?, ?, ?, 'running', 'L1', 0, 5, 0, ?, ?, ?)",
    ))
    .bind(&task_id)
    .bind(&session_id)
    .bind(POC_INTENT)
    .bind(&intent_sha)
    .bind(&plan_text)
    .bind(&plan_sha)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;
    drop(connection);

    // Spawn the worker with a CLEAN environment: the process boundary made
    // mechanical. (`worker_db_env_free` lets a test request the strict form
    // where even PATH-inherited DB-ish variables are stripped.)
    let mut command = tokio::process::Command::new(&launch.worker_bin);
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    if worker_db_env_free {
        command.env_clear();
        for (key, value) in worker_env() {
            command.env(key, value);
        }
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot spawn worker: {error}"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "worker stdin missing".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "worker stdout missing".to_string())?;

    // Pump replies into a channel: a slow reply cannot block the reader.
    let (tx, rx) = tokio::sync::mpsc::channel::<Value>(16);
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if tx.send(value).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut session = SupervisedSession {
        session_id: session_id.clone(),
        task_id: task_id.clone(),
        child,
        stdin,
        seq: 0,
        replies: Some(rx),
    };

    // tasks 1–2: launch Chromium with the dedicated profile.
    let reply = session
        .ask(
            json!({
                "cmd": "start",
                "chromium": launch.chromium.display().to_string(),
                "profile": launch.profile_dir.display().to_string(),
            }),
            Duration::from_secs(30),
        )
        .await;
    let reply = match reply {
        Ok(value) => value,
        Err(error) => {
            mark_task_failed(store, &task_id, "worker_start_failed", &error).await;
            return Err(error);
        }
    };
    // The worker answers ok:false when Chromium did not start — swallowing
    // that here would leave the session "running" over a dead runtime.
    if reply.get("ok") != Some(&json!(true)) {
        let error = format!(
            "worker start failed: {}",
            reply
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("(no error text)")
        );
        mark_task_failed(store, &task_id, "worker_start_failed", &error).await;
        return Err(error);
    }

    // The worker answered: the session is READY (a ready session is NOT a
    // logged-in platform session - plan §2.5's distinction, recorded here).
    let now_ready = to_sql_timestamp(&Utc::now());
    {
        let mut connection = store.pool().acquire().await.map_err(|e| e.to_string())?;
        sqlx::query("UPDATE browser_sessions SET state = 'ready', updated_at = ? WHERE id = ?")
            .bind(&now_ready)
            .bind(&session_id)
            .execute(&mut *connection)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(session)
}

impl SupervisedSession {
    /// Kill the worker process outright (the crash-recovery test's trigger).
    pub async fn kill_worker(&mut self) -> Result<(), String> {
        self.child
            .kill()
            .await
            .map_err(|error| format!("kill failed: {error}"))
    }

    /// Run one browser action and record it (`browser_actions`) — the
    /// supervisor owns the state machine, the worker only executes.
    pub async fn action(
        &mut self,
        store: &SqliteStore,
        kind: &str,
        command: Value,
        target_url: Option<&str>,
    ) -> Result<Value, String> {
        let action_id = new_uuid();
        let started = to_sql_timestamp(&Utc::now());
        {
            let mut connection = store.pool().acquire().await.map_err(|e| e.to_string())?;
            sqlx::query(concat!(
                "INSERT INTO browser_actions (id, session_id, task_id, kind, risk_level,",
                " target_url, state, started_at, created_at, updated_at)",
                " VALUES (?, ?, ?, ?, 'L1', ?, 'executing', ?, ?, ?)",
            ))
            .bind(&action_id)
            .bind(&self.session_id)
            .bind(&self.task_id)
            .bind(kind)
            .bind(target_url)
            .bind(&started)
            .bind(&started)
            .bind(&started)
            .execute(&mut *connection)
            .await
            .map_err(|e| e.to_string())?;
        }

        let outcome = self.ask(command, Duration::from_secs(20)).await;
        let finished = to_sql_timestamp(&Utc::now());
        let mut connection = store.pool().acquire().await.map_err(|e| e.to_string())?;
        match outcome {
            Ok(reply) => {
                sqlx::query(concat!(
                    "UPDATE browser_actions SET state = 'succeeded', result_json = ?,",
                    " finished_at = ?, updated_at = ? WHERE id = ?",
                ))
                .bind(reply.to_string())
                .bind(&finished)
                .bind(&finished)
                .bind(&action_id)
                .execute(&mut *connection)
                .await
                .map_err(|e| e.to_string())?;
                Ok(reply)
            }
            Err(error) => {
                sqlx::query(concat!(
                    "UPDATE browser_actions SET state = 'failed', error_code = ?,",
                    " finished_at = ?, updated_at = ? WHERE id = ?",
                ))
                .bind("action_failed")
                .bind(&finished)
                .bind(&finished)
                .bind(&action_id)
                .execute(&mut *connection)
                .await
                .map_err(|e| e.to_string())?;
                Err(error)
            }
        }
    }

    /// Graceful stop: the worker closes Chromium; the task completes.
    pub async fn stop(mut self, store: &SqliteStore) -> Result<(), String> {
        let _ = self
            .ask(json!({"cmd": "stop"}), Duration::from_secs(10))
            .await;
        let _ = self.child.wait().await;
        let now = to_sql_timestamp(&Utc::now());
        let mut connection = store.pool().acquire().await.map_err(|e| e.to_string())?;
        sqlx::query(concat!(
            "UPDATE browser_tasks SET state = 'completed', executed_steps = 5,",
            " finished_at = ?, updated_at = ? WHERE id = ?",
        ))
        .bind(&now)
        .bind(&now)
        .bind(&self.task_id)
        .execute(&mut *connection)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query("UPDATE browser_sessions SET state = 'closed', updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&self.session_id)
            .execute(&mut *connection)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Crash path (B1 AC: browser crash must NOT take the server down): the
/// supervisor notices the dead child, marks the task `failed` with the
/// reason, and a fresh start works — independent restart.
pub async fn supervise_until_exit(
    store: &SqliteStore,
    session: &mut SupervisedSession,
) -> Result<(), String> {
    let status = session
        .child
        .wait()
        .await
        .map_err(|error| format!("wait failed: {error}"))?;
    mark_task_failed(
        store,
        &session.task_id,
        "worker_exited",
        &format!("worker exited: {status}"),
    )
    .await;
    Ok(())
}

async fn mark_task_failed(store: &SqliteStore, task_id: &str, code: &str, message: &str) -> () {
    let now = to_sql_timestamp(&Utc::now());
    if let Ok(mut connection) = store.pool().acquire().await {
        let _ = sqlx::query(concat!(
            "UPDATE browser_tasks SET state = 'failed', error_code = ?, error_message = ?,",
            " finished_at = ?, updated_at = ? WHERE id = ?",
        ))
        .bind(code)
        .bind(message)
        .bind(&now)
        .bind(&now)
        .bind(task_id)
        .execute(&mut *connection)
        .await;
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}
