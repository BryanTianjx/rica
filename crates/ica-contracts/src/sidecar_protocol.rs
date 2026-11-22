//! Versioned Sidecar Protocol — transport-neutral (architecture §7.3, ADR-005).
//!
//! This crate is the single source of truth shared by the Rust Core and any
//! external capability executor. The wire is a JSON *line* protocol: one UTF-8
//! JSON object per line on stdout (protocol messages only); stderr carries
//! diagnostics. No Python type, module or runtime detail appears here — the
//! shape is language-neutral, so a future non-Python executor can implement it
//! without changing the Rust Core.
//!
//! Invariants made mechanical here:
//! - every message carries `protocol_version`, negotiated before any request;
//! - every request carries a unique `request_id`, a timeout and an explicit
//!   capability grant — an ungranted method or path is rejected before it
//!   reaches the executor;
//! - business methods (drafts, positions, candidates, tasks, auth, …) are
//!   rejected outright: the Sidecar is an external-capability executor only
//!   (review decision 3 / ADR-005);
//! - error DTOs reuse the shared §9.2 code vocabulary and are redacted on
//!   *encode*, so credentials can never be echoed to a caller or a log.
//!
//! This module depends only on `serde` / `serde_json`: no tokio, no socket,
//! no process spawning. Those concerns stay in `ica-desktop`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// Current protocol version. Bump together with the contract test vectors.
pub const PROTOCOL_VERSION: &str = "1.0";
/// Only these versions are accepted during negotiation.
pub const SUPPORTED_VERSIONS: &[&str] = &[PROTOCOL_VERSION];

/// Capability methods an external executor may be asked to perform.
/// Everything else is out of scope by construction (ADR-005).
pub mod method {
    pub const START: &str = "browser.start";
    pub const OPEN: &str = "browser.open";
    pub const READ: &str = "browser.read";
    pub const SCREENSHOT: &str = "browser.screenshot";
    pub const CLICK: &str = "browser.click";
    pub const FILL: &str = "browser.fill";
    pub const ENV_PROBE: &str = "browser.env_probe";
    pub const STOP: &str = "browser.stop";
    pub const HEALTH_CHECK: &str = "health.check";
    pub const CANCEL: &str = "request.cancel";

    pub const ALL: &[&str] = &[
        START,
        OPEN,
        READ,
        SCREENSHOT,
        CLICK,
        FILL,
        ENV_PROBE,
        STOP,
        HEALTH_CHECK,
        CANCEL,
    ];

    /// Prefixes that would make the executor a second business implementation.
    /// Rejected even if a caller tries to smuggle one through a grant.
    pub const BUSINESS_PREFIXES: &[&str] = &[
        "draft.",
        "drafts.",
        "position.",
        "positions.",
        "candidate.",
        "candidates.",
        "conversation.",
        "conversations.",
        "message.",
        "messages.",
        "follow_up.",
        "import.",
        "share.",
        "shares.",
        "auth.",
        "backup.",
        "task.",
        "tasks.",
        "audit.",
    ];
}

/// Protocol-level error codes. Where a failure is an ordinary business/wire
/// error we reuse the shared §9.2 vocabulary instead of inventing a second one.
pub mod error_code {
    pub const UNSUPPORTED_VERSION: &str = "sidecar_unsupported_version";
    pub const INVALID_REQUEST: &str = "sidecar_invalid_request";
    pub const CAPABILITY_DENIED: &str = "sidecar_capability_denied";
    pub const PATH_DENIED: &str = "sidecar_path_denied";
    pub const BUSINESS_METHOD: &str = "sidecar_business_method_rejected";
    pub const FORBIDDEN_PARAMETER: &str = "sidecar_forbidden_parameter";
    pub const REQUEST_TIMEOUT: &str = "sidecar_request_timeout";
    pub const REQUEST_CANCELLED: &str = "sidecar_request_cancelled";
    pub const CRASHED: &str = "sidecar_crashed";
    pub const RESTART_LIMIT: &str = "sidecar_restart_limit";
}

static REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

/// Generate a process-local unique request id without pulling in a UUID crate.
/// Uniqueness is only required for the lifetime of one supervisor process.
pub fn new_request_id() -> String {
    let seq = REQUEST_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("r-{}-{seq}", std::process::id())
}

/// A file-system scope explicitly granted to the executor. Paths are checked
/// by the Rust Core before a request is sent; the grant travels with the
/// request so the executor never has to guess what it may touch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathGrant {
    /// Absolute root the executor may touch. Relative roots are rejected.
    pub root: String,
    pub access: PathAccess,
    /// Human-readable reason, recorded in diagnostics. Never a secret.
    #[serde(default)]
    pub purpose: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathAccess {
    Read,
    Write,
}

impl PathGrant {
    /// No grant may point at the core business database or at a database
    /// file at all. The Sidecar never receives a DSN, `ICA_DATA_DIR` or a path
    /// under the application's data directory (§7.5 / "明确不做").
    pub fn is_safe(&self) -> bool {
        if !self.root.starts_with('/') && !is_windows_absolute(&self.root) {
            return false;
        }
        let lowered = self.root.to_ascii_lowercase();
        for forbidden in [
            ".db",
            "database",
            "ica_data_dir",
            "ica.db",
            "sqlite",
            "secrets",
            "credential",
            "backend",
        ] {
            if lowered.contains(forbidden) {
                return false;
            }
        }
        !self.root.contains("..")
    }
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// One explicitly granted capability. A request is valid only if a grant
/// covers its method; an empty `allowed_paths` means "no file access at all".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGrant {
    /// A method from [`method::ALL`], or a wildcard such as `browser.*`.
    pub capability: String,
    #[serde(default)]
    pub allowed_paths: Vec<PathGrant>,
    /// Optional network/host allowlist for browser navigation. Empty means
    /// no outbound host is authorised.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

impl CapabilityGrant {
    pub fn covers(&self, method_name: &str) -> bool {
        if let Some(prefix) = self.capability.strip_suffix(".*") {
            method_name.starts_with(prefix)
        } else {
            self.capability == method_name
        }
    }
}

/// A request from the Rust Core to the external capability executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub method: String,
    #[serde(default = "Value::default")]
    pub params: Value,
    /// Explicit, least-privilege grants for THIS request. No implicit authority.
    #[serde(default)]
    pub grants: Vec<CapabilityGrant>,
    /// Per-request deadline (§7.3 "每个请求必须有超时"), milliseconds.
    pub timeout_ms: u64,
}

/// Error DTO. `message` and `details` are redacted on encode, never echoed raw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarError {
    pub code: String,
    pub message: String,
    #[serde(default = "Value::default")]
    pub details: Value,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarResponse {
    pub protocol_version: String,
    pub request_id: String,
    #[serde(default = "Value::default")]
    pub result: Value,
    #[serde(default)]
    pub error: Option<SidecarError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarProtocolError {
    Malformed(String),
    UnsupportedVersion(String),
    MissingField(&'static str),
    BusinessMethod(String),
    CapabilityDenied(String),
    PathDenied(String),
    ForbiddenParameter(String),
}

impl SidecarProtocolError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Malformed(_) | Self::MissingField(_) => error_code::INVALID_REQUEST,
            Self::UnsupportedVersion(_) => error_code::UNSUPPORTED_VERSION,
            Self::BusinessMethod(_) => error_code::BUSINESS_METHOD,
            Self::CapabilityDenied(_) => error_code::CAPABILITY_DENIED,
            Self::PathDenied(_) => error_code::PATH_DENIED,
            Self::ForbiddenParameter(_) => error_code::FORBIDDEN_PARAMETER,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Malformed(text) => format!("malformed sidecar message: {text}"),
            Self::UnsupportedVersion(text) => format!("unsupported protocol_version: {text}"),
            Self::MissingField(field) => format!("missing {field}"),
            Self::BusinessMethod(text) => format!("business method not allowed in sidecar: {text}"),
            Self::CapabilityDenied(text) => format!("no capability grant for: {text}"),
            Self::PathDenied(text) => format!("path not granted to sidecar: {text}"),
            Self::ForbiddenParameter(text) => format!("parameter not allowed: {text}"),
        }
    }

    /// Convert a validation failure into the wire error DTO. The message goes
    /// through the redactor so a malformed payload cannot leak a credential.
    pub fn to_dto(&self) -> SidecarError {
        SidecarError {
            code: self.code().to_string(),
            message: redact(&self.message()),
            details: Value::Null,
            retryable: false,
        }
    }
}

impl std::fmt::Display for SidecarProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for SidecarProtocolError {}

/// Conservative credential scrubber, shared with `ica-desktop::diagnostics`
/// so log lines and protocol DTOs cannot disagree about what is a secret.
pub fn redact(value: &str) -> String {
    let lowered = value.to_ascii_lowercase();
    let markers = [
        "bearer ", "jwt ", "api_key", "apikey", "secret", "password", "token",
    ];
    if markers.iter().any(|needle| lowered.contains(needle)) {
        "[REDACTED]".to_string()
    } else {
        value.to_string()
    }
}

/// Recursively scrub string leaves of a JSON value before it goes on the wire.
pub fn redact_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(redact(text)),
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), redact_value(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

impl SidecarError {
    /// A copy safe to encode or log: message and details scrubbed.
    pub fn sanitized(&self) -> Self {
        Self {
            code: self.code.clone(),
            message: redact(&self.message),
            details: redact_value(&self.details),
            retryable: self.retryable,
        }
    }
}

impl SidecarResponse {
    pub fn ok(request_id: impl Into<String>, result: Value) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            request_id: request_id.into(),
            result,
            error: None,
        }
    }

    pub fn failure(request_id: impl Into<String>, error: SidecarError) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            request_id: request_id.into(),
            result: Value::Null,
            error: Some(error),
        }
    }

    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// Negotiate the protocol version before exchanging any capability request.
pub fn negotiate_version(offered: &str) -> Result<&'static str, SidecarProtocolError> {
    if SUPPORTED_VERSIONS.contains(&offered) {
        Ok(PROTOCOL_VERSION)
    } else {
        Err(SidecarProtocolError::UnsupportedVersion(
            offered.to_string(),
        ))
    }
}

/// Parameter names that must never cross the process boundary: they are the
/// core database / credential handles the Sidecar is explicitly denied (§7.5).
const FORBIDDEN_PARAMETER_KEYS: &[&str] = &[
    "database_url",
    "dsn",
    "db_path",
    "database_path",
    "ica_data_dir",
    "api_key",
    "apikey",
    "secret",
    "password",
    "token",
    "jwt",
    "connection_string",
];

fn check_params(params: &Value) -> Result<(), SidecarProtocolError> {
    fn walk(value: &Value) -> Result<(), SidecarProtocolError> {
        match value {
            Value::Object(map) => {
                for (key, item) in map {
                    let lowered = key.to_ascii_lowercase();
                    if FORBIDDEN_PARAMETER_KEYS
                        .iter()
                        .any(|forbidden| lowered.contains(forbidden))
                    {
                        return Err(SidecarProtocolError::ForbiddenParameter(key.clone()));
                    }
                    walk(item)?;
                }
                Ok(())
            }
            Value::Array(items) => items.iter().try_for_each(walk),
            _ => Ok(()),
        }
    }
    walk(params)
}

/// Validate a request *before* it reaches an executor. This is the single
/// gate for version, unique id, method scope, capability grant and paths.
pub fn validate_request(request: &SidecarRequest) -> Result<(), SidecarProtocolError> {
    negotiate_version(&request.protocol_version)?;
    if request.request_id.trim().is_empty() {
        return Err(SidecarProtocolError::MissingField("request_id"));
    }
    if request.method.trim().is_empty() {
        return Err(SidecarProtocolError::MissingField("method"));
    }
    for prefix in method::BUSINESS_PREFIXES {
        if request.method.starts_with(prefix) {
            return Err(SidecarProtocolError::BusinessMethod(request.method.clone()));
        }
    }
    if !method::ALL.contains(&request.method.as_str()) {
        return Err(SidecarProtocolError::CapabilityDenied(
            request.method.clone(),
        ));
    }
    if request.timeout_ms == 0 {
        return Err(SidecarProtocolError::MissingField("timeout_ms"));
    }
    check_params(&request.params)?;
    if !request
        .grants
        .iter()
        .any(|grant| grant.covers(&request.method))
    {
        return Err(SidecarProtocolError::CapabilityDenied(
            request.method.clone(),
        ));
    }
    for grant in &request.grants {
        for path in &grant.allowed_paths {
            if !path.is_safe() {
                return Err(SidecarProtocolError::PathDenied(path.root.clone()));
            }
        }
    }
    Ok(())
}

/// Validate a response envelope. Version and correlation id are mandatory;
/// a response with neither result nor error is malformed.
pub fn validate_response(response: &SidecarResponse) -> Result<(), SidecarProtocolError> {
    negotiate_version(&response.protocol_version)?;
    if response.request_id.trim().is_empty() {
        return Err(SidecarProtocolError::MissingField("request_id"));
    }
    if response.error.is_none() && response.result.is_null() {
        return Err(SidecarProtocolError::MissingField("result or error"));
    }
    Ok(())
}

fn line_guard(line: &str) -> Result<(), SidecarProtocolError> {
    if line.contains('\n') || line.contains('\r') {
        Err(SidecarProtocolError::Malformed(
            "embedded newline in JSON line".to_string(),
        ))
    } else {
        Ok(())
    }
}

pub fn encode_request(request: &SidecarRequest) -> Result<String, SidecarProtocolError> {
    validate_request(request)?;
    serde_json::to_string(request)
        .map_err(|error| SidecarProtocolError::Malformed(error.to_string()))
}

pub fn decode_request(line: &str) -> Result<SidecarRequest, SidecarProtocolError> {
    line_guard(line)?;
    let request: SidecarRequest = serde_json::from_str(line.trim())
        .map_err(|error| SidecarProtocolError::Malformed(error.to_string()))?;
    validate_request(&request)?;
    Ok(request)
}

/// Encoding a response always sanitises it: credentials cannot be echoed by
/// an executor error even if the executor included them in its message.
pub fn encode_response(response: &SidecarResponse) -> Result<String, SidecarProtocolError> {
    validate_response(response)?;
    let safe = SidecarResponse {
        error: response.error.as_ref().map(SidecarError::sanitized),
        ..response.clone()
    };
    serde_json::to_string(&safe).map_err(|error| SidecarProtocolError::Malformed(error.to_string()))
}

pub fn decode_response(line: &str) -> Result<SidecarResponse, SidecarProtocolError> {
    line_guard(line)?;
    let response: SidecarResponse = serde_json::from_str(line.trim())
        .map_err(|error| SidecarProtocolError::Malformed(error.to_string()))?;
    validate_response(&response)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn grant(method_name: &str) -> Vec<CapabilityGrant> {
        vec![CapabilityGrant {
            capability: method_name.to_string(),
            allowed_paths: vec![],
            allowed_hosts: vec![],
        }]
    }

    fn request(method_name: &str) -> SidecarRequest {
        SidecarRequest {
            protocol_version: PROTOCOL_VERSION.to_string(),
            request_id: new_request_id(),
            method: method_name.to_string(),
            params: json!({"url": "https://example.com"}),
            grants: grant(method_name),
            timeout_ms: 30_000,
        }
    }

    #[test]
    fn a_granted_request_round_trips_as_one_json_line() {
        let original = request(method::OPEN);
        let line = encode_request(&original).expect("encode");
        assert!(!line.contains('\n'));
        let decoded = decode_request(&line).expect("decode");
        assert_eq!(decoded, original);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["protocol_version"], json!("1.0"));
        assert_eq!(value["method"], json!(method::OPEN));
        assert_eq!(value["timeout_ms"], json!(30_000));
    }

    #[test]
    fn request_ids_are_unique_within_a_process() {
        let ids: std::collections::BTreeSet<_> = (0..128).map(|_| new_request_id()).collect();
        assert_eq!(ids.len(), 128);
    }

    #[test]
    fn version_negotiation_accepts_only_supported_versions() {
        assert_eq!(negotiate_version("1.0"), Ok(PROTOCOL_VERSION));
        for bad in ["", "0.9", "2.0", "1.0.0", "v1"] {
            let error = negotiate_version(bad).unwrap_err();
            assert_eq!(error.code(), error_code::UNSUPPORTED_VERSION);
        }
    }

    #[test]
    fn business_methods_are_rejected_even_when_granted() {
        let mut business = request("draft.approve");
        business.grants = vec![CapabilityGrant {
            capability: "draft.approve".to_string(),
            allowed_paths: vec![],
            allowed_hosts: vec![],
        }];
        let error = decode_request(&serde_json::to_string(&business).unwrap()).unwrap_err();
        assert_eq!(error.code(), error_code::BUSINESS_METHOD);
    }

    #[test]
    fn an_ungranted_or_unknown_method_is_rejected() {
        let mut no_grant = request(method::OPEN);
        no_grant.grants = vec![];
        assert_eq!(
            validate_request(&no_grant).unwrap_err().code(),
            error_code::CAPABILITY_DENIED
        );
        let mut unknown = request("browser.launch_missiles");
        unknown.grants = grant("browser.launch_missiles");
        assert_eq!(
            validate_request(&unknown).unwrap_err().code(),
            error_code::CAPABILITY_DENIED
        );
    }

    #[test]
    fn database_paths_and_credentials_are_not_allowed_in_params() {
        for (key, value) in [
            ("database_url", "sqlite:///mnt/o/ica/backend/ica.db"),
            ("dsn", "file:ica.db"),
            ("api_key", "sk-secret"),
            ("nested", "ignored"),
        ] {
            let mut bad = request(method::OPEN);
            bad.params = if key == "nested" {
                json!({"wrapper": {"password": value}})
            } else {
                json!({key: value})
            };
            let code = validate_request(&bad).unwrap_err().code();
            if key == "nested" {
                assert_eq!(code, error_code::FORBIDDEN_PARAMETER);
            } else {
                assert_eq!(code, error_code::FORBIDDEN_PARAMETER, "{key}");
            }
        }
    }

    #[test]
    fn unsafe_path_grants_are_rejected() {
        for root in [
            "/home/user/ica.db".to_string(),
            "/mnt/o/ica/backend".to_string(),
            "relative/path".to_string(),
            "/tmp/../etc/passwd".to_string(),
        ] {
            let mut bad = request(method::SCREENSHOT);
            bad.grants = vec![CapabilityGrant {
                capability: method::SCREENSHOT.to_string(),
                allowed_paths: vec![PathGrant {
                    root,
                    access: PathAccess::Write,
                    purpose: "test".to_string(),
                }],
                allowed_hosts: vec![],
            }];
            assert_eq!(
                validate_request(&bad).unwrap_err().code(),
                error_code::PATH_DENIED
            );
        }
        let safe = PathGrant {
            root: "/tmp/ica-screenshots".to_string(),
            access: PathAccess::Write,
            purpose: "poc".to_string(),
        };
        assert!(safe.is_safe());
    }

    #[test]
    fn error_responses_round_trip_and_are_redacted_on_encode() {
        let reply = SidecarResponse::failure(
            "r-1",
            SidecarError {
                code: crate::error::code::INTERNAL.to_string(),
                message: "cdp send failed with Bearer abc.def".to_string(),
                details: json!({"hint": "AI_API_KEY=sk-secret"}),
                retryable: false,
            },
        );
        let line = encode_response(&reply).expect("encode");
        assert!(line.contains("[REDACTED]"));
        assert!(!line.contains("sk-secret"));
        assert!(!line.contains("abc.def"));
        let decoded = decode_response(&line).expect("decode");
        assert_eq!(decoded.request_id, "r-1");
        assert!(decoded.is_error());
        assert_eq!(decoded.error.unwrap().code, crate::error::code::INTERNAL);
    }

    #[test]
    fn malformed_lines_and_missing_fields_are_rejected() {
        for line in [
            "not json",
            "{\"protocol_version\":\"1.0\"}\n{\"x\":1}",
            r#"{"protocol_version":"1.0","request_id":"","method":"browser.read","grants":[],"timeout_ms":1}"#,
            r#"{"protocol_version":"1.0","request_id":"r","method":"browser.read","grants":[{"capability":"browser.read"}],"timeout_ms":0}"#,
        ] {
            assert!(decode_request(line).is_err(), "{line}");
        }
        for line in [
            r#"{"protocol_version":"2.0","request_id":"r","result":{}}"#,
            r#"{"protocol_version":"1.0","request_id":"","result":{}}"#,
            r#"{"protocol_version":"1.0","request_id":"r"}"#,
        ] {
            assert!(decode_response(line).is_err(), "{line}");
        }
    }

    #[test]
    fn wildcard_grant_covers_the_capability_family() {
        let wildcard = CapabilityGrant {
            capability: "browser.*".to_string(),
            allowed_paths: vec![],
            allowed_hosts: vec!["example.com".to_string()],
        };
        assert!(wildcard.covers(method::OPEN));
        assert!(wildcard.covers(method::SCREENSHOT));
        assert!(!wildcard.covers("draft.approve"));
    }
}
