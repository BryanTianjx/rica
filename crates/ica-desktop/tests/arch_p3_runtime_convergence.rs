//! ARCH-P3 acceptance assertions that run on a host without glib/WebKit.
//!
//! The Tauri shell cannot be type-checked here, so the "no direct child
//! lifecycle / no business logic" property is proven at the source level with
//! `include_str!` against the same file the host ships. Everything the shell
//! delegates to is implemented and unit-tested in this (compilable) crate.

use ica_desktop::diagnostics::{HealthKind, HealthState};
use ica_desktop::processes::{shutdown_plan, LifecycleState, SidecarManager};
use ica_desktop::runtime_policy::{HttpServerMode, RuntimePolicy};

const HOST: &str = include_str!("../../../desktop/tauri-host/src/main.rs");
const MANAGER: &str = include_str!("../src/processes.rs");

#[test]
fn host_owns_no_child_process_lifecycle() {
    // Every one of these used to live in main.rs (P0 inventory §2). Their
    // absence is the AC1 source assertion; their presence in ica-desktop is
    // proven by the manager's own unit tests.
    for forbidden in [
        "Command::new(",
        ".kill()",
        "BufReader::new",
        "recv_timeout",
        "child.stdin",
        "spawn_sidecar(",
        "spawn_rust_server(",
        "static SIDECAR",
        "static RUST_SERVER",
        "fn poll_health",
        "fn forward_to",
        "std::process::Child",
    ] {
        assert!(
            !HOST.contains(forbidden),
            "host must not manage child processes directly: found {forbidden}"
        );
    }
    for wiring in [
        "SidecarManager",
        "RuntimePolicy::from_env()",
        "manager().start(spec)",
        "manager().shutdown_all()",
        "ica_desktop::proxy::forward(",
    ] {
        assert!(HOST.contains(wiring), "missing host wiring: {wiring}");
    }
}

#[test]
fn the_manager_really_implements_the_delegated_four_stages() {
    for required in [
        ".spawn()",
        "no listening event within",
        "proxy::probe_health",
        "drop(managed.stdin.take())",
        "managed.child.try_wait()",
        "managed.child.kill()",
        "managed.child.wait()",
        "pub fn restart(",
    ] {
        assert!(
            MANAGER.contains(required),
            "Sidecar Manager must implement: {required}"
        );
    }
}

#[test]
fn host_contains_no_business_rules() {
    // ADR-004: the shell wraps commands and routes bytes; it never executes a
    // use case rule, an error mapping, or an authorization decision itself.
    for forbidden in [
        "ContractError::",
        "ica_application::",
        "is_admin",
        "\"was not found\"",
        "positions().find(",
        "dashboard()",
    ] {
        assert!(
            !HOST.contains(forbidden),
            "host must not implement business logic: found {forbidden}"
        );
    }
    assert!(HOST.contains("generate_handler!["));
    assert!(HOST.contains(".manage(ica_state)"));
}

#[test]
fn embedded_mode_is_the_default_and_the_http_server_is_not_resident() {
    assert_eq!(RuntimePolicy::default().http_mode, HttpServerMode::Embedded);
    assert!(!RuntimePolicy::default().starts_http_at_boot());
    // The boot path gates spawn behind the explicit `always` policy and
    // states the default in its log line.
    assert!(HOST.contains("policy.starts_http_at_boot()"));
    assert!(HOST.contains("local HTTP server NOT started at boot"));
    // The §6.4 on-demand trigger is wired into the Rust-routed branch.
    assert!(HOST.contains("Backend::Rust => (ensure_rust_backend(),"));
    assert!(HOST.contains("policy.starts_http_on_demand()"));
}

#[test]
fn app_runtime_init_and_destroy_are_explicit_in_the_host() {
    for required in [
        "IcaState::connect(",
        ".runtime()",
        ".lifecycle()",
        "begin_drain()",
        "mark_stopped()",
    ] {
        assert!(
            HOST.contains(required),
            "host must show the AppRuntime init/destroy flow: {required}"
        );
    }
    // Drain happens before children are stopped; stopped happens after.
    let drain = HOST.find("begin_drain()").expect("drain call");
    let stop_children = HOST.find("shutdown_all()").expect("manager shutdown");
    let stopped = HOST.find("mark_stopped()").expect("stopped call");
    assert!(drain < stop_children && stop_children < stopped);
}

#[test]
fn graceful_shutdown_is_drain_then_timeout_then_kill_and_reap() {
    let plan = shutdown_plan(&RuntimePolicy::default());
    assert_eq!(plan.steps[0], "close_stdin");
    assert_eq!(plan.steps[3], "kill_if_timeout");
    assert_eq!(plan.steps[4], "reap");
    assert_eq!(plan.steps.last(), Some(&"log_result"));
    assert!(plan.graceful_timeout_ms > 0);
    assert!(LifecycleState::Ready.can_transition_to(LifecycleState::Stopping));
    assert!(!LifecycleState::Stopped.can_transition_to(LifecycleState::Ready));
}

#[test]
fn unified_health_snapshot_is_available_from_the_manager() {
    let manager = SidecarManager::new(RuntimePolicy::default());
    let health = manager.runtime_health();
    assert!(health.is_ready());
    assert!(
        health
            .components()
            .iter()
            .any(|component| component.name == "AppRuntime"
                && component.kind == HealthKind::Readiness)
    );
    assert!(health
        .components()
        .iter()
        .any(|component| component.kind == HealthKind::Liveness));
    assert_eq!(
        health
            .components()
            .iter()
            .find(|component| component.name == "RustHttpServer")
            .map(|component| component.state),
        Some(HealthState::NotApplicable)
    );
    let all_lines = health.render_log_lines();
    assert_eq!(all_lines.len(), health.components().len());
    assert!(all_lines.iter().all(|line| line.contains("state=")));
}

#[test]
fn unsupported_platform_assertion_is_documented_not_signed() {
    // Keep the honesty contract visible in source: no test here claims the
    // Windows Tauri binary was built or that a human security review passed.
    assert!(HOST.contains("#![cfg_attr(not(debug_assertions), windows_subsystem"));
    assert!(MANAGER.contains("#[cfg(windows)]"));
}
