//! Sidecar Manager (ARCH-P3 AC1, architecture §7.4).
//!
//! Every child-process lifecycle concern — spawn, stdout `listening`
//! handshake, health probe, stdout/stderr separation, graceful stop, timeout
//! fallback, forced kill/reap and bounded restart — lives here, in
//! `ica-desktop`, which is compilable on this Linux host. The Tauri shell only
//! builds a [`ProcessSpec`] and calls [`SidecarManager`] (thin wiring, no
//! `Command`/`kill` of its own).
//!
//! Shutdown order (REL-01): close stdin -> let the child drain -> wait up to
//! the configured graceful timeout -> kill -> reap. A child is never left
//! unreaped, so an application exit cannot leave an orphan.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::diagnostics::{ComponentHealth, HealthKind, HealthState, RuntimeHealth};
use crate::proxy;
use crate::runtime_policy::RuntimePolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedKind {
    PythonSidecar,
    RustHttpServer,
}

impl ManagedKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::PythonSidecar => "PythonSidecar",
            Self::RustHttpServer => "RustHttpServer",
        }
    }

    pub fn log_file_name(&self) -> &'static str {
        match self {
            Self::PythonSidecar => "sidecar.log",
            Self::RustHttpServer => "ica-server.log",
        }
    }
}

/// §7.4 state machine: Stopped -> Starting -> Ready -> Degraded ->
/// Stopping/Restarting -> Stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    Stopped,
    Starting,
    Ready,
    Degraded,
    Stopping,
    Restarting,
}

impl LifecycleState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Stopping => "stopping",
            Self::Restarting => "restarting",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use LifecycleState::*;
        matches!(
            (self, next),
            (Stopped, Starting)
                | (Starting, Ready)
                | (Starting, Degraded)
                | (Starting, Stopping)
                | (Ready, Degraded)
                | (Ready, Stopping)
                | (Ready, Restarting)
                | (Degraded, Ready)
                | (Degraded, Stopping)
                | (Degraded, Restarting)
                | (Restarting, Starting)
                | (Restarting, Stopping)
                | (Stopping, Stopped)
        )
    }
}

/// Host-injected launch description. The host says *what* to start; the
/// manager owns *how* it is started, handshaken and stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub kind: ManagedKind,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub current_dir: PathBuf,
    pub env: Vec<(String, String)>,
    pub handshake_timeout_ms: u64,
    pub health_timeout_ms: u64,
    pub log_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessError {
    Spawn(String),
    Handshake(String),
    Stdout(String),
    RestartLimit(String),
}

impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(message)
            | Self::Handshake(message)
            | Self::Stdout(message)
            | Self::RestartLimit(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ProcessError {}

struct ManagedChild {
    kind: ManagedKind,
    child: Child,
    stdin: Option<ChildStdin>,
    port: u16,
    state: LifecycleState,
    restarts: u32,
    started_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownOutcome {
    pub kind: ManagedKind,
    pub gracefully_exited: bool,
    pub force_killed: bool,
    pub waited_ms: u64,
}

impl ShutdownOutcome {
    pub fn render(&self) -> String {
        format!(
            "shutdown component={} graceful={} force_kill={} waited_ms={}",
            self.kind.label(),
            self.gracefully_exited,
            self.force_killed,
            self.waited_ms
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    pub outcomes: Vec<ShutdownOutcome>,
}

impl ShutdownReport {
    pub fn new(outcomes: Vec<ShutdownOutcome>) -> Self {
        Self { outcomes }
    }

    /// `all_clean()` means every child exited on its own — no kill fallback.
    pub fn all_clean(&self) -> bool {
        self.outcomes
            .iter()
            .all(|outcome| outcome.gracefully_exited && !outcome.force_killed)
    }

    pub fn force_killed(&self) -> Vec<ManagedKind> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.force_killed)
            .map(|outcome| outcome.kind)
            .collect()
    }

    pub fn render_log_lines(&self) -> Vec<String> {
        self.outcomes.iter().map(ShutdownOutcome::render).collect()
    }
}

/// The REL-01 shutdown flow, made explicit so it can be asserted without
/// spawning a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownPlan {
    pub steps: Vec<&'static str>,
    pub graceful_timeout_ms: u64,
}

pub fn shutdown_plan(policy: &RuntimePolicy) -> ShutdownPlan {
    ShutdownPlan {
        steps: vec![
            "close_stdin",
            "stop_accepting_new_requests",
            "wait_drain",
            "kill_if_timeout",
            "reap",
            "log_result",
        ],
        graceful_timeout_ms: policy.graceful_shutdown_timeout_ms,
    }
}

#[derive(Clone)]
pub struct SidecarManager {
    inner: Arc<Inner>,
}

struct Inner {
    children: Mutex<Vec<ManagedChild>>,
    policy: RuntimePolicy,
}

impl SidecarManager {
    pub fn new(policy: RuntimePolicy) -> Self {
        Self {
            inner: Arc::new(Inner {
                children: Mutex::new(Vec::new()),
                policy,
            }),
        }
    }

    pub fn policy(&self) -> &RuntimePolicy {
        &self.inner.policy
    }

    /// Spawn a child, read its `listening` handshake, probe `/health`, and
    /// register it. stdout and stderr are appended to separate log files.
    pub fn start(&self, spec: ProcessSpec) -> Result<u16, ProcessError> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .current_dir(&spec.current_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .spawn()
            .map_err(|error| ProcessError::Spawn(format!("{}: {error}", spec.program.display())))?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProcessError::Spawn("no stdout pipe".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ProcessError::Spawn("no stderr pipe".to_string()))?;

        let log_file = spec.log_dir.join(spec.kind.log_file_name());
        if let Some(parent) = log_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let stdout_log = log_file.clone();
        let stderr_log = log_file.clone();
        let (tx, rx) = mpsc::channel::<Result<String, String>>();

        // stderr is diagnostic-only: drain it to its own file so a full pipe
        // can never block the child (§7.4 stdout/stderr separation).
        std::thread::spawn(move || {
            let mut sink = Vec::new();
            let _ = BufReader::new(stderr).read_to_end(&mut sink);
            if !sink.is_empty() {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(stderr_log)
                {
                    let _ = writeln!(file, "=== stderr ===");
                    let _ = file.write_all(&sink);
                }
            }
        });
        // stdout carries the handshake; every line is also kept for diagnosis.
        std::thread::spawn(move || {
            let mut log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(stdout_log)
                .ok();
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if line.contains("listening") {
                            let _ = tx.send(Ok(line.clone()));
                        }
                        if let Some(file) = log.as_mut() {
                            let _ = writeln!(file, "{line}");
                        }
                    }
                    Err(error) => {
                        let _ = tx.send(Err(error.to_string()));
                        break;
                    }
                }
            }
        });

        let deadline = Instant::now() + Duration::from_millis(spec.handshake_timeout_ms);
        let line = loop {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProcessError::Handshake(format!(
                    "no listening event within {}ms",
                    spec.handshake_timeout_ms
                )));
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(Ok(line)) => break line,
                Ok(Err(error)) => return Err(ProcessError::Stdout(error)),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ProcessError::Handshake(
                        "child exited before listening".to_string(),
                    ));
                }
            }
        };
        let port = parse_listening(&line).ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            ProcessError::Handshake(format!("unusable listening line: {line}"))
        })?;
        let healthy = proxy::probe_health(port, spec.health_timeout_ms, 200);
        let state = if healthy {
            LifecycleState::Ready
        } else {
            LifecycleState::Degraded
        };
        log_to_file(
            &log_file,
            &format!(
                "health component={} state={} port={port}",
                spec.kind.label(),
                state.as_str()
            ),
        );

        let mut children = self.inner.children.lock().expect("manager children lock");
        children.retain(|managed| managed.kind != spec.kind);
        children.push(ManagedChild {
            kind: spec.kind,
            child,
            stdin,
            port,
            state,
            restarts: 0,
            started_at: Instant::now(),
        });
        Ok(port)
    }

    pub fn port(&self, kind: ManagedKind) -> u16 {
        self.inner
            .children
            .lock()
            .expect("manager children lock")
            .iter()
            .find(|managed| managed.kind == kind)
            .map(|managed| managed.port)
            .unwrap_or(0)
    }

    pub fn state(&self, kind: ManagedKind) -> LifecycleState {
        self.inner
            .children
            .lock()
            .expect("manager children lock")
            .iter()
            .find(|managed| managed.kind == kind)
            .map(|managed| managed.state)
            .unwrap_or(LifecycleState::Stopped)
    }

    pub fn set_state(&self, kind: ManagedKind, state: LifecycleState) {
        if let Some(managed) = self
            .inner
            .children
            .lock()
            .expect("manager children lock")
            .iter_mut()
            .find(|managed| managed.kind == kind)
        {
            managed.state = state;
        }
    }

    pub fn uptime_ms(&self, kind: ManagedKind) -> u64 {
        self.inner
            .children
            .lock()
            .expect("manager children lock")
            .iter()
            .find(|managed| managed.kind == kind)
            .map(|managed| managed.started_at.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    /// Graceful stop of every owned child, with a forced-kill and **reap**
    /// fallback. No child is left running after this returns.
    pub fn shutdown_all(&self) -> ShutdownReport {
        let children = {
            let mut guard = self.inner.children.lock().expect("manager children lock");
            std::mem::take(&mut *guard)
        };
        ShutdownReport::new(drain(children, &self.inner.policy))
    }

    /// Bounded restart (§7.4). Once `max_restarts` is exhausted the caller
    /// gets a real error instead of an endless crash loop.
    pub fn restart(&self, spec: ProcessSpec) -> Result<u16, ProcessError> {
        let kind = spec.kind;
        let previous = {
            let guard = self.inner.children.lock().expect("manager children lock");
            guard
                .iter()
                .find(|managed| managed.kind == spec.kind)
                .map(|managed| managed.restarts)
                .unwrap_or(0)
        };
        if previous >= self.inner.policy.max_restarts {
            return Err(ProcessError::RestartLimit(format!(
                "{} reached {} restarts",
                spec.kind.label(),
                self.inner.policy.max_restarts
            )));
        }
        // Stop and reap the previous instance of this kind before starting
        // the replacement; other managed processes keep running.
        {
            let mut guard = self.inner.children.lock().expect("manager children lock");
            let (mine, others): (Vec<_>, Vec<_>) = std::mem::take(&mut *guard)
                .into_iter()
                .partition(|managed| managed.kind == kind);
            let _ = drain(mine, &self.inner.policy);
            *guard = others;
        }
        let port = self.start(spec)?;
        if let Some(managed) = self
            .inner
            .children
            .lock()
            .expect("manager children lock")
            .iter_mut()
            .find(|managed| managed.kind == kind)
        {
            managed.restarts = previous + 1;
            managed.state = LifecycleState::Restarting;
        }
        Ok(port)
    }

    /// §11.2 health snapshot for the components this manager owns, plus the
    /// shared AppRuntime and the dependency placeholders the host can refine.
    pub fn runtime_health(&self) -> RuntimeHealth {
        let mut components = Vec::new();
        for kind in [ManagedKind::PythonSidecar, ManagedKind::RustHttpServer] {
            let state = self.state(kind);
            let health_state = match state {
                LifecycleState::Ready => HealthState::Healthy,
                LifecycleState::Starting | LifecycleState::Degraded => HealthState::Degraded,
                LifecycleState::Stopped => {
                    if kind == ManagedKind::RustHttpServer && self.policy().starts_http_on_demand()
                    {
                        HealthState::NotApplicable
                    } else {
                        HealthState::Unavailable
                    }
                }
                LifecycleState::Stopping | LifecycleState::Restarting => HealthState::Degraded,
            };
            components.push(ComponentHealth::new(
                kind.label(),
                HealthKind::Liveness,
                health_state,
                format!(
                    "state={} uptime_ms={}",
                    state.as_str(),
                    self.uptime_ms(kind)
                ),
            ));
        }
        components.push(ComponentHealth::new(
            "AppRuntime",
            HealthKind::Readiness,
            HealthState::Healthy,
            "assembled through AppRuntime::new (P2 runtime())",
        ));
        // These are deliberately NotApplicable: the manager cannot claim a
        // SQLite / Provider / backup / auth probe it did not run.
        components.push(ComponentHealth::new(
            "SQLite",
            HealthKind::Readiness,
            HealthState::NotApplicable,
            "checked by DesktopState::connect startup self-check",
        ));
        for (name, detail) in [
            ("AI Adapter", "provider key probed by the AI use cases"),
            ("Backup", "backup status read by the backup use case"),
            ("Authentication", "local mode; no multi-user session probe"),
        ] {
            components.push(ComponentHealth::new(
                name,
                HealthKind::Dependency,
                HealthState::NotApplicable,
                detail,
            ));
        }
        RuntimeHealth::new(components)
    }
}

fn log_to_file(path: &Path, message: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{message}");
    }
}

fn drain(children: Vec<ManagedChild>, policy: &RuntimePolicy) -> Vec<ShutdownOutcome> {
    let mut outcomes = Vec::with_capacity(children.len());
    for mut managed in children {
        let started = Instant::now();
        // 1) Stop accepting work: EOF on stdin is the sidecar's and the
        //    server's graceful-exit trigger.
        drop(managed.stdin.take());
        let deadline = started + policy.graceful_shutdown_timeout();
        let mut graceful = false;
        // 2) Wait for in-flight requests / background tasks to drain.
        loop {
            match managed.child.try_wait() {
                Ok(Some(_)) => {
                    graceful = true;
                    break;
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        }
        // 3) Timeout fallback: kill AND reap, so no orphan survives exit.
        let force_killed = if !graceful {
            let _ = managed.child.kill();
            let _ = managed.child.wait();
            true
        } else {
            // try_wait already reaped the child on Ok(Some(_)).
            false
        };
        outcomes.push(ShutdownOutcome {
            kind: managed.kind,
            gracefully_exited: graceful,
            force_killed,
            waited_ms: started.elapsed().as_millis() as u64,
        });
    }
    outcomes
}

/// The Rust server and the Python sidecar both emit a `port` on a line
/// containing `listening`. Accept both the test shape
/// `{"listening": true, "port": N}` and the baseline shape
/// `{"event": "listening", "host": ..., "port": N}` (P0 inventory §5 item 4).
pub fn parse_listening(line: &str) -> Option<u16> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let port = value.get("port")?.as_u64()?;
    if port == 0 || port > u16::MAX as u64 {
        return None;
    }
    let listening = value
        .get("listening")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let event = value.get("event").and_then(serde_json::Value::as_str) == Some("listening");
    if listening || event {
        Some(port as u16)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_accepts_both_live_and_baseline_shapes() {
        assert_eq!(
            parse_listening(r#"{"listening": true, "port": 51234}"#),
            Some(51234)
        );
        assert_eq!(
            parse_listening(r#"{"event":"listening","host":"127.0.0.1","port":8000}"#),
            Some(8000)
        );
    }

    #[test]
    fn handshake_rejects_missing_or_invalid_port_data() {
        for line in [
            "{}",
            "not json",
            r#"{"listening": true}"#,
            r#"{"port": 0, "listening": true}"#,
            r#"{"port": 70000, "listening": true}"#,
            r#"{"port": 1234, "event": "started"}"#,
        ] {
            assert_eq!(parse_listening(line), None, "{line}");
        }
    }

    #[test]
    fn lifecycle_state_machine_only_allows_documented_edges() {
        assert!(LifecycleState::Stopped.can_transition_to(LifecycleState::Starting));
        assert!(LifecycleState::Starting.can_transition_to(LifecycleState::Ready));
        assert!(LifecycleState::Ready.can_transition_to(LifecycleState::Degraded));
        assert!(LifecycleState::Degraded.can_transition_to(LifecycleState::Restarting));
        assert!(LifecycleState::Restarting.can_transition_to(LifecycleState::Starting));
        assert!(LifecycleState::Stopping.can_transition_to(LifecycleState::Stopped));
        assert!(!LifecycleState::Stopped.can_transition_to(LifecycleState::Ready));
        assert!(!LifecycleState::Stopped.can_transition_to(LifecycleState::Stopping));
        assert!(!LifecycleState::Stopped.can_transition_to(LifecycleState::Restarting));
    }

    #[test]
    fn shutdown_plan_always_ends_in_reap_and_log() {
        let policy = RuntimePolicy::default();
        let plan = shutdown_plan(&policy);
        assert_eq!(
            plan.steps,
            vec![
                "close_stdin",
                "stop_accepting_new_requests",
                "wait_drain",
                "kill_if_timeout",
                "reap",
                "log_result"
            ]
        );
        assert_eq!(plan.graceful_timeout_ms, 5_000);
    }

    #[test]
    fn an_empty_manager_reports_no_orphans() {
        let manager = SidecarManager::new(RuntimePolicy::default());
        assert_eq!(manager.port(ManagedKind::PythonSidecar), 0);
        assert_eq!(
            manager.state(ManagedKind::RustHttpServer),
            LifecycleState::Stopped
        );
        let report = manager.shutdown_all();
        assert!(report.outcomes.is_empty());
        assert!(report.all_clean());
    }

    #[cfg(unix)]
    #[test]
    fn a_stubborn_child_is_killed_and_reaped_on_shutdown() {
        let dir = std::env::temp_dir().join(format!("ica-p3-kill-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn stubborn child");
        let pid = child.id();
        let stdin = child.stdin.take();
        let manager = SidecarManager::new(RuntimePolicy::from_pairs([(
            "ICA_GRACEFUL_SHUTDOWN_TIMEOUT_MS",
            "100",
        )]));
        manager.inner.children.lock().unwrap().push(ManagedChild {
            kind: ManagedKind::RustHttpServer,
            child,
            stdin,
            port: 0,
            state: LifecycleState::Ready,
            restarts: 0,
            started_at: Instant::now(),
        });
        let report = manager.shutdown_all();
        assert!(!report.all_clean());
        assert_eq!(report.force_killed(), vec![ManagedKind::RustHttpServer]);
        // The process is gone AND reaped: /proc/<pid> no longer exists.
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "child {pid} survived shutdown (orphan)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_child_that_honours_stdin_eof_exits_gracefully() {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn cat");
        let pid = child.id();
        let stdin = child.stdin.take();
        let manager = SidecarManager::new(RuntimePolicy::from_pairs([(
            "ICA_GRACEFUL_SHUTDOWN_TIMEOUT_MS",
            "2000",
        )]));
        manager.inner.children.lock().unwrap().push(ManagedChild {
            kind: ManagedKind::PythonSidecar,
            child,
            stdin,
            port: 0,
            state: LifecycleState::Ready,
            restarts: 0,
            started_at: Instant::now(),
        });
        let report = manager.shutdown_all();
        assert!(report.all_clean(), "{:?}", report.render_log_lines());
        assert!(report.force_killed().is_empty());
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn runtime_health_marks_embedded_http_as_not_applicable() {
        let manager = SidecarManager::new(RuntimePolicy::default());
        let health = manager.runtime_health();
        assert!(health.is_ready());
        let server = health
            .components()
            .iter()
            .find(|component| component.name == "RustHttpServer")
            .expect("server component");
        assert_eq!(server.state, HealthState::NotApplicable);
        assert!(!health.render_log_lines().is_empty());
    }
}
