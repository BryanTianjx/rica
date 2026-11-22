//! Sidecar Protocol supervisor — request discipline on top of ARCH-P3.
//!
//! ARCH-P3 already owns the *process* lifecycle in `crate::processes`
//! (spawn / handshake / graceful shutdown / bounded restart) and the *wire*
//! forwarder in `crate::proxy`. This module deliberately adds only what the
//! protocol needs and does **not** re-implement any of those: no `Command`,
//! no `kill`, no second health probe, no second state machine. Crash handling
//! calls `SidecarManager::restart`; shutdown calls the same `shutdown_plan`
//! that the P3 manager executes.
//!
//! What is new here (architecture §7.3/§7.4/§7.5):
//! - a bounded pending-request table with per-request deadlines and cancel;
//! - crash detection feeding the P3 restart decision;
//! - a health-check request built from the shared versioned DTO;
//! - a bridge from the versioned protocol to the existing line-JSON worker
//!   wire, so the Rust Core never learns a Python module path or a worker
//!   implementation detail (the bridge only maps method names and fields).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ica_contracts::error::sidecar::code_for_error_message;
use ica_contracts::sidecar_protocol::{
    error_code, method, validate_request, CapabilityGrant, SidecarError, SidecarProtocolError,
    SidecarRequest, SidecarResponse, PROTOCOL_VERSION,
};
use serde_json::{json, Value};

use crate::processes::{shutdown_plan, LifecycleState};
use crate::runtime_policy::RuntimePolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorFailure {
    Timeout { request_id: String },
    Cancelled { request_id: String },
    Capacity,
    Crashed { component: &'static str },
    RestartLimit { component: &'static str },
    Protocol(SidecarProtocolError),
}

impl SupervisorFailure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Timeout { .. } => error_code::REQUEST_TIMEOUT,
            Self::Cancelled { .. } => error_code::REQUEST_CANCELLED,
            Self::Capacity => error_code::INVALID_REQUEST,
            Self::Crashed { .. } => error_code::CRASHED,
            Self::RestartLimit { .. } => error_code::RESTART_LIMIT,
            Self::Protocol(error) => error.code(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Timeout { request_id } => format!("request {request_id} timed out"),
            Self::Cancelled { request_id } => format!("request {request_id} cancelled"),
            Self::Capacity => "too many concurrent sidecar requests".to_string(),
            Self::Crashed { component } => format!("{component} exited unexpectedly"),
            Self::RestartLimit { component } => format!("{component} restart limit reached"),
            Self::Protocol(error) => error.message(),
        }
    }

    /// A failure always becomes a protocol error DTO, using the same codes as
    /// HTTP/IPC where they overlap; `sanitized()` is applied by the encoder.
    pub fn to_error_dto(&self) -> SidecarError {
        SidecarError {
            code: self.code().to_string(),
            message: self.message(),
            details: Value::Null,
            retryable: matches!(self, Self::Crashed { .. } | Self::Timeout { .. }),
        }
    }

    pub fn to_response(&self, request_id: &str) -> SidecarResponse {
        SidecarResponse::failure(request_id, self.to_error_dto())
    }
}

impl From<SidecarProtocolError> for SupervisorFailure {
    fn from(error: SidecarProtocolError) -> Self {
        Self::Protocol(error)
    }
}

/// One in-flight request. Only the supervisor owns it; a reply is correlated
/// by `request_id`, never by arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub request_id: String,
    pub started_at: Instant,
    pub deadline: Instant,
    pub timeout: Duration,
}

impl Pending {
    fn new(request: &SidecarRequest, now: Instant) -> Self {
        let timeout = Duration::from_millis(request.timeout_ms);
        Self {
            request_id: request.request_id.clone(),
            started_at: now,
            deadline: now + timeout,
            timeout,
        }
    }

    pub fn expired_at(&self, now: Instant) -> bool {
        now >= self.deadline
    }
}

/// Bounded pending-request table (§7.4: request timeout, max concurrency,
/// cancellation). Pure std: testable without a running executor.
#[derive(Debug)]
pub struct PendingRequests {
    pending: HashMap<String, Pending>,
    max: usize,
}

impl PendingRequests {
    pub fn new(max: usize) -> Self {
        Self {
            pending: HashMap::new(),
            max: max.max(1),
        }
    }

    /// Validate the request at the boundary and register its deadline.
    /// An invalid or ungranted request never reaches the process.
    pub fn insert(
        &mut self,
        request: &SidecarRequest,
        now: Instant,
    ) -> Result<Pending, SupervisorFailure> {
        validate_request(request)?;
        if self.pending.len() >= self.max && !self.pending.contains_key(&request.request_id) {
            return Err(SupervisorFailure::Capacity);
        }
        let pending = Pending::new(request, now);
        self.pending
            .insert(request.request_id.clone(), pending.clone());
        Ok(pending)
    }

    pub fn take(&mut self, request_id: &str) -> Option<Pending> {
        self.pending.remove(request_id)
    }

    pub fn get(&self, request_id: &str) -> Option<&Pending> {
        self.pending.get(request_id)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Remove and return every deadline that has passed. A timed-out request
    /// must be cancelled downstream so the executor cannot leave work behind.
    pub fn expire(&mut self, now: Instant) -> Vec<Pending> {
        let expired: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.expired_at(now))
            .map(|(id, _)| id.clone())
            .collect();
        expired
            .into_iter()
            .filter_map(|id| self.pending.remove(&id))
            .collect()
    }

    /// Cancelling is idempotent: an unknown id is simply already gone.
    pub fn cancel(&mut self, request_id: &str) -> Option<Pending> {
        self.take(request_id)
    }
}

/// Build the DTO that asks the executor to abandon work on another request.
pub fn cancel_request(target_request_id: &str, reason: &str) -> SidecarRequest {
    SidecarRequest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        request_id: ica_contracts::new_request_id(),
        method: method::CANCEL.to_string(),
        params: json!({"target_request_id": target_request_id, "reason": reason}),
        grants: vec![CapabilityGrant {
            capability: method::CANCEL.to_string(),
            allowed_paths: vec![],
            allowed_hosts: vec![],
        }],
        timeout_ms: 5_000,
    }
}

/// §7.3 `health.check` as a first-class protocol request. Liveness transport
/// probing stays in the P3 `proxy::probe_health`; this is the protocol-level
/// check that the executor itself answers.
pub fn health_check_request(timeout: Duration) -> SidecarRequest {
    SidecarRequest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        request_id: ica_contracts::new_request_id(),
        method: method::HEALTH_CHECK.to_string(),
        params: json!({}),
        grants: vec![CapabilityGrant {
            capability: method::HEALTH_CHECK.to_string(),
            allowed_paths: vec![],
            allowed_hosts: vec![],
        }],
        timeout_ms: timeout.as_millis().min(u64::MAX as u128) as u64,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashDecision {
    /// The P3 manager's bounded `restart()` should be used.
    Restart,
    /// Limit reached: surface an error, never loop forever.
    GiveUp,
    /// This exit is expected (idle/stopping) or not a managed child.
    Ignore,
}

/// Only a process that was Ready/Degraded/Restarting and *exits without a
/// graceful stop* is a crash. A Stopping child exiting is the planned EOF
/// shutdown of P3 and must not trigger a restart.
pub fn is_crash(exit_code: Option<i32>, state: LifecycleState) -> bool {
    let live = matches!(
        state,
        LifecycleState::Ready | LifecycleState::Degraded | LifecycleState::Restarting
    );
    live && exit_code.is_some()
}

/// Crash policy: decide, then delegate the actual restart to P3.
pub fn crash_decision(
    exit_code: Option<i32>,
    state: LifecycleState,
    restarts: u32,
    max_restarts: u32,
) -> CrashDecision {
    if !is_crash(exit_code, state) {
        return CrashDecision::Ignore;
    }
    if restarts < max_restarts {
        CrashDecision::Restart
    } else {
        CrashDecision::GiveUp
    }
}

/// The graceful-shutdown chain is the P3 one, with no second implementation.
pub fn shutdown_chain(policy: &RuntimePolicy) -> Vec<&'static str> {
    shutdown_plan(policy).steps
}

/// Translate a validated protocol request to the existing line-JSON worker
/// wire. Grants and protocol fields stay on the Rust side: the worker sees
/// only the capability parameters it needs. No Python path or module is
/// embedded here — `params` is an opaque JSON object to Rust.
pub fn to_legacy_command(request: &SidecarRequest) -> Result<Value, SupervisorFailure> {
    validate_request(request)?;
    let command_name = match request.method.as_str() {
        method::START => "start",
        method::OPEN => "open",
        method::READ => "read",
        method::SCREENSHOT => "screenshot",
        method::CLICK => "click",
        method::FILL => "fill",
        method::ENV_PROBE | method::HEALTH_CHECK => "env_probe",
        method::STOP => "stop",
        method::CANCEL => {
            return Err(SupervisorFailure::Protocol(
                SidecarProtocolError::CapabilityDenied(method::CANCEL.to_string()),
            ))
        }
        other => {
            return Err(SupervisorFailure::Protocol(
                SidecarProtocolError::CapabilityDenied(other.to_string()),
            ))
        }
    };
    let mut command = match &request.params {
        Value::Object(map) => Value::Object(map.clone()),
        _ => json!({}),
    };
    let object = command
        .as_object_mut()
        .expect("command is always an object here");
    object.insert("cmd".to_string(), json!(command_name));
    object.insert("request_id".to_string(), json!(request.request_id));
    Ok(command)
}

/// Translate a legacy line-JSON worker reply back into the versioned DTO.
/// Text from the executor is never trusted: `encode_response` redacts it.
pub fn from_legacy_reply(reply: &Value, fallback_request_id: &str) -> SidecarResponse {
    let request_id = reply
        .get("request_id")
        .and_then(Value::as_str)
        .unwrap_or(fallback_request_id)
        .to_string();
    let ok = reply.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        let message = reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("(no error text)")
            .to_string();
        let code = reply
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_else(|| code_for_error_message(&message));
        return SidecarResponse::failure(
            request_id,
            SidecarError {
                code: code.to_string(),
                message,
                details: Value::Null,
                retryable: false,
            },
        );
    }
    let mut result = reply.clone();
    if let Some(object) = result.as_object_mut() {
        for key in ["ok", "seq", "cmd", "request_id"] {
            object.remove(key);
        }
    }
    SidecarResponse::ok(request_id, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ica_contracts::error::code as shared_code;
    use std::time::Duration as StdDuration;

    fn granted_request(method_name: &str) -> SidecarRequest {
        SidecarRequest {
            protocol_version: PROTOCOL_VERSION.to_string(),
            request_id: ica_contracts::new_request_id(),
            method: method_name.to_string(),
            params: json!({"url": "https://example.com"}),
            grants: vec![CapabilityGrant {
                capability: method_name.to_string(),
                allowed_paths: vec![],
                allowed_hosts: vec![],
            }],
            timeout_ms: 1_000,
        }
    }

    #[test]
    fn a_request_gets_a_deadline_and_replies_correlate_by_id() {
        let mut table = PendingRequests::new(4);
        let request = granted_request(method::OPEN);
        let now = Instant::now();
        let pending = table.insert(&request, now).expect("insert");
        assert_eq!(pending.timeout, StdDuration::from_millis(1_000));
        assert_eq!(table.len(), 1);
        assert!(table.get(&request.request_id).is_some());
        assert!(table.take(&request.request_id).is_some());
        assert!(table.is_empty());
    }

    #[test]
    fn expired_requests_are_removed_and_cancelled_requests_are_idempotent() {
        let mut table = PendingRequests::new(4);
        let request = granted_request(method::READ);
        let now = Instant::now();
        table.insert(&request, now).expect("insert");
        assert!(table.expire(now + StdDuration::from_millis(999)).is_empty());
        let expired = table.expire(now + StdDuration::from_millis(1_001));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].request_id, request.request_id);
        assert!(table.is_empty());
        assert!(table.cancel(&request.request_id).is_none());
    }

    #[test]
    fn capacity_and_ungranted_requests_are_refused_at_the_boundary() {
        let mut table = PendingRequests::new(1);
        let first = granted_request(method::OPEN);
        table.insert(&first, Instant::now()).expect("first");
        let second = granted_request(method::READ);
        assert_eq!(
            table.insert(&second, Instant::now()).unwrap_err(),
            SupervisorFailure::Capacity
        );
        let mut bad = granted_request(method::READ);
        bad.grants.clear();
        assert_eq!(
            table.insert(&bad, Instant::now()).unwrap_err().code(),
            error_code::CAPABILITY_DENIED
        );
    }

    #[test]
    fn cancel_and_health_requests_are_valid_protocol_requests() {
        let cancel = cancel_request("r-123", "user cancelled");
        assert_eq!(cancel.method, method::CANCEL);
        assert!(validate_request(&cancel).is_ok());
        let health = health_check_request(StdDuration::from_secs(30));
        assert_eq!(health.method, method::HEALTH_CHECK);
        assert!(validate_request(&health).is_ok());
    }

    #[test]
    fn crashes_feed_the_p3_bounded_restart_policy() {
        assert_eq!(
            crash_decision(Some(1), LifecycleState::Ready, 0, 2),
            CrashDecision::Restart
        );
        assert_eq!(
            crash_decision(Some(1), LifecycleState::Ready, 2, 2),
            CrashDecision::GiveUp
        );
        assert_eq!(
            crash_decision(Some(0), LifecycleState::Stopping, 0, 2),
            CrashDecision::Ignore
        );
        assert!(!is_crash(None, LifecycleState::Ready));
    }

    #[test]
    fn shutdown_is_the_p3_chain_not_a_second_implementation() {
        let chain = shutdown_chain(&RuntimePolicy::default());
        assert_eq!(chain[0], "close_stdin");
        assert_eq!(chain[4], "reap");
        assert_eq!(chain.last(), Some(&"log_result"));
    }

    #[test]
    fn failure_dtos_use_the_shared_vocabulary_and_redact() {
        let timeout = SupervisorFailure::Timeout {
            request_id: "r-1".to_string(),
        };
        assert_eq!(timeout.code(), error_code::REQUEST_TIMEOUT);
        let response = timeout.to_response("r-1");
        let line = ica_contracts::encode_response(&response).unwrap();
        assert!(line.contains(error_code::REQUEST_TIMEOUT));
        assert_eq!(
            SupervisorFailure::Protocol(SidecarProtocolError::MissingField("x")).code(),
            error_code::INVALID_REQUEST
        );
    }

    #[test]
    fn legacy_bridge_maps_methods_without_forwarding_grants() {
        let request = granted_request(method::SCREENSHOT);
        let command = to_legacy_command(&request).expect("translate");
        assert_eq!(command["cmd"], json!("screenshot"));
        assert_eq!(command["request_id"], json!(request.request_id));
        assert!(command.get("grants").is_none());
        assert!(command.get("protocol_version").is_none());
        let health = health_check_request(StdDuration::from_secs(1));
        assert_eq!(
            to_legacy_command(&health).unwrap()["cmd"],
            json!("env_probe")
        );
        let cancel = cancel_request("r-1", "done");
        assert!(to_legacy_command(&cancel).is_err());
    }

    #[test]
    fn legacy_replies_become_versioned_success_or_error_dtos() {
        let ok = json!({"ok": true, "seq": 3, "title": "page"});
        let response = from_legacy_reply(&ok, "r-9");
        assert_eq!(response.protocol_version, PROTOCOL_VERSION);
        assert_eq!(response.request_id, "r-9");
        assert!(!response.is_error());
        assert_eq!(response.result["title"], json!("page"));
        assert!(response.result.get("ok").is_none());

        let failure =
            json!({"ok": false, "error": "bad command json: x", "code": shared_code::VALIDATION});
        let response = from_legacy_reply(&failure, "r-9");
        assert!(response.is_error());
        let error = response.error.unwrap();
        assert_eq!(error.code, shared_code::VALIDATION);
        assert!(error.message.contains("bad command json"));
        let fallback = json!({"ok": false, "error": "cdp send failed"});
        assert_eq!(
            from_legacy_reply(&fallback, "r-10").error.unwrap().code,
            shared_code::INTERNAL
        );
    }
}
