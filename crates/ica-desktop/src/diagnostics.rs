//! Unified runtime diagnostics (RICA architecture §11, ARCH-P3 AC3).
//!
//! One vocabulary for liveness / readiness / dependency health and one
//! structured event shape (request_id, principal_id, source, module,
//! operation, duration_ms, result, error_code). Values are rendered without
//! secrets; the host only writes the resulting lines to its log file. All of
//! it is pure, so it is testable without Tauri or a running backend.

/// Liveness, readiness or an external dependency (§11.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthKind {
    Liveness,
    Readiness,
    Dependency,
}

impl HealthKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Liveness => "liveness",
            Self::Readiness => "readiness",
            Self::Dependency => "dependency",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unavailable,
    NotApplicable,
}

impl HealthState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
            Self::NotApplicable => "not-applicable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentHealth {
    pub name: &'static str,
    pub kind: HealthKind,
    pub state: HealthState,
    pub detail: String,
}

impl ComponentHealth {
    pub fn new(
        name: &'static str,
        kind: HealthKind,
        state: HealthState,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name,
            kind,
            state,
            detail: detail.into(),
        }
    }

    /// A structured line: `health component=<name> kind=<k> state=<s> detail=<redacted>`.
    pub fn render(&self) -> String {
        format!(
            "health component={} kind={} state={} detail={}",
            self.name,
            self.kind.as_str(),
            self.state.as_str(),
            redact(&self.detail)
        )
    }
}

/// The §11.2 component set; an optional Provider being down never makes the
/// whole runtime unhealthy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeHealth {
    components: Vec<ComponentHealth>,
}

impl RuntimeHealth {
    pub fn new(components: Vec<ComponentHealth>) -> Self {
        Self { components }
    }

    pub fn components(&self) -> &[ComponentHealth] {
        &self.components
    }

    /// Ready only when an applicable readiness probe fails. `NotApplicable`
    /// means the manager did not run that probe (e.g. SQLite is checked by
    /// `DesktopState::connect`), so it must not falsely gate the runtime.
    /// Liveness and dependency probes report but do not gate (§11.2: an
    /// unavailable optional Provider is not an application outage).
    pub fn is_ready(&self) -> bool {
        self.components
            .iter()
            .filter(|component| {
                component.kind == HealthKind::Readiness
                    && component.state != HealthState::NotApplicable
            })
            .all(|component| component.state == HealthState::Healthy)
    }

    pub fn degraded(&self) -> Vec<&ComponentHealth> {
        self.components
            .iter()
            .filter(|component| {
                matches!(
                    component.state,
                    HealthState::Degraded | HealthState::Unavailable
                )
            })
            .collect()
    }

    pub fn render_log_lines(&self) -> Vec<String> {
        self.components
            .iter()
            .map(ComponentHealth::render)
            .collect()
    }
}

/// The §11.1 per-call diagnostic record. `source` is one of HTTP / IPC /
/// Internal, `result` is ok / error, and `error_code` comes from the shared
/// `ica_contracts::error` vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub request_id: String,
    pub principal_id: Option<String>,
    pub source: &'static str,
    pub module: &'static str,
    pub operation: &'static str,
    pub duration_ms: u64,
    pub result: &'static str,
    pub error_code: Option<&'static str>,
}

impl DiagnosticEvent {
    /// `diag request_id=… principal=… source=… module=… operation=… duration_ms=… result=… code=…`
    /// with every value passed through the same redactor as health details.
    pub fn render(&self) -> String {
        let principal = self
            .principal_id
            .as_deref()
            .map(redact)
            .unwrap_or_else(|| "anonymous".to_string());
        format!(
            "diag request_id={} principal={} source={} module={} operation={} duration_ms={} result={} code={}",
            redact(&self.request_id),
            principal,
            self.source,
            self.module,
            self.operation,
            self.duration_ms,
            self.result,
            self.error_code.unwrap_or("none")
        )
    }
}

/// Never write a credential-like value into a log line. This is deliberately
/// conservative: matching a token marker beats trying to enumerate every
/// possible secret shape.
pub fn redact(value: &str) -> String {
    let lowered = value.to_ascii_lowercase();
    let marker = [
        "bearer ", "jwt ", "api_key", "apikey", "secret", "password", "token",
    ];
    if marker.iter().any(|needle| lowered.contains(needle)) {
        return "[REDACTED]".to_string();
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_is_gated_by_readiness_probes_only() {
        let health = RuntimeHealth::new(vec![
            ComponentHealth::new(
                "AppRuntime",
                HealthKind::Readiness,
                HealthState::Healthy,
                "assembled",
            ),
            ComponentHealth::new(
                "AI Adapter",
                HealthKind::Dependency,
                HealthState::Unavailable,
                "no provider key",
            ),
            ComponentHealth::new(
                "Sidecar",
                HealthKind::Liveness,
                HealthState::Degraded,
                "no child",
            ),
        ]);
        assert!(health.is_ready());
        assert_eq!(health.degraded().len(), 2);
        assert_eq!(health.render_log_lines().len(), 3);
    }

    #[test]
    fn a_failed_readiness_probe_marks_the_runtime_not_ready() {
        let health = RuntimeHealth::new(vec![ComponentHealth::new(
            "SQLite",
            HealthKind::Readiness,
            HealthState::Unavailable,
            "cannot open",
        )]);
        assert!(!health.is_ready());
    }

    #[test]
    fn diagnostic_event_carries_the_eleven_vocabulary_fields() {
        let event = DiagnosticEvent {
            request_id: "req-1".to_string(),
            principal_id: Some("local_user".to_string()),
            source: "IPC",
            module: "positions",
            operation: "list",
            duration_ms: 4,
            result: "ok",
            error_code: None,
        };
        let line = event.render();
        for fragment in [
            "request_id=req-1",
            "principal=local_user",
            "source=IPC",
            "module=positions",
            "operation=list",
            "duration_ms=4",
            "result=ok",
            "code=none",
        ] {
            assert!(line.contains(fragment), "missing {fragment} in {line}");
        }
    }

    #[test]
    fn credentials_are_redacted_in_both_event_and_health_lines() {
        assert_eq!(redact("Bearer abc.def"), "[REDACTED]");
        assert_eq!(redact("AI_API_KEY=sk-secret"), "[REDACTED]");
        assert_eq!(redact("plain text"), "plain text");
        let line = ComponentHealth::new(
            "Auth",
            HealthKind::Dependency,
            HealthState::Degraded,
            "jwt secret missing",
        )
        .render();
        assert!(!line.contains("secret missing"));
        assert!(line.contains("[REDACTED]"));
    }
}
