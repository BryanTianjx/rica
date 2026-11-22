//! ARCH-P4 architecture acceptance assertions (host without glib/WebKit).
//!
//! This host cannot type-check the Tauri shell, so the properties that must
//! hold across the process boundary are proven three ways: (1) real tests of
//! the compilable protocol/supervisor layers, (2) `include_str!` source
//! assertions on the actual shipped host/worker, and (3) recursive scans of
//! Rust Core sources for Python/business coupling. These tests are NOT a
//! substitute for a Windows build or an independent security review.

use std::fs;
use std::path::{Path, PathBuf};

use ica_contracts::sidecar_protocol::method;
use ica_desktop::processes::LifecycleState;
use ica_desktop::runtime_policy::RuntimePolicy;
use ica_desktop::sidecar::{crash_decision, shutdown_chain, CrashDecision};

const HOST: &str = include_str!("../../../desktop/tauri-host/src/main.rs");
const WORKER: &str = include_str!("../src/bin/ica-browser-worker.rs");
const MANAGER: &str = include_str!("../src/processes.rs");
const SUPERVISOR: &str = include_str!("../src/sidecar.rs");
const CONTRACT: &str = include_str!("../../ica-contracts/src/sidecar_protocol.rs");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    for entry in fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn ac1_protocol_is_transport_neutral_and_not_duplicated_in_the_host() {
    // The DTOs live in ica-contracts: no axum/sqlx/tokio/tauri import there.
    for forbidden in [
        "use axum",
        "use sqlx",
        "use tokio",
        "tauri::",
        "std::process::Command",
    ] {
        assert!(
            !CONTRACT.contains(forbidden),
            "contract must stay transport-neutral: {forbidden}"
        );
    }
    // The shell carries no protocol codec; it only routes bytes and wires
    // the manager (P3). Encoding/negotiation stay in the shared layers.
    assert!(!HOST.contains("protocol_version"));
    assert!(!HOST.contains("decode_request("));
    assert!(!HOST.contains("encode_response("));
    assert!(SUPERVISOR.contains("to_legacy_command("));
    assert!(SUPERVISOR.contains("from_legacy_reply("));
}

#[test]
fn ac2_timeout_cancel_and_crash_reuse_p3_instead_of_rebuilding_it() {
    // The supervisor owns only request discipline; process lifecycle stays
    // in the P3 manager and health transport stays in P3's proxy.
    for forbidden in [
        "Command::new(",
        ".spawn()",
        ".kill()",
        "TcpStream",
        "probe_health(",
        "fn shutdown_plan(",
    ] {
        assert!(
            !SUPERVISOR.contains(forbidden),
            "supervisor must not duplicate P3 process machinery: {forbidden}"
        );
    }
    for reused in ["shutdown_plan(", "LifecycleState", "crash_decision("] {
        assert!(SUPERVISOR.contains(reused), "missing P3 reuse: {reused}");
    }
    for p3 in [
        "pub fn restart(",
        "proxy::probe_health",
        "pub fn shutdown_all(",
    ] {
        assert!(MANAGER.contains(p3), "P3 manager lost {p3}");
    }
    assert!(HOST.contains("manager().shutdown_all()"));
    let decision = crash_decision(Some(9), LifecycleState::Ready, 0, 2);
    assert_eq!(decision, CrashDecision::Restart);
    assert_eq!(shutdown_chain(&RuntimePolicy::default())[4], "reap");
}

#[test]
fn ac3_executor_gets_no_database_and_no_undeclared_capability() {
    let env = ica_desktop::browser::worker_env();
    for key in env.keys() {
        let upper = key.to_ascii_uppercase();
        assert!(
            !upper.contains("DATABASE")
                && !upper.contains("DSN")
                && upper != "ICA_DATA_DIR"
                && !upper.contains("SECRET")
                && !upper.contains("TOKEN"),
            "executor environment leaks a database/credential handle: {key}"
        );
    }
    for forbidden in ["sqlx::", "SqliteStore", "ica_application::", "ica.db"] {
        assert!(
            !WORKER.contains(forbidden),
            "executor must not carry storage or business code: {forbidden}"
        );
    }
    assert!(WORKER.contains("env_probe"));
    // Path grants that point at the core DB are rejected by the shared
    // validator (behaviour proven in the contract test); here we only assert
    // the guard exists in the shared contract instead of in the host.
    assert!(CONTRACT.contains("pub fn is_safe(&self) -> bool"));
    assert!(CONTRACT.contains("pub fn redact(value: &str) -> String"));
    assert!(!HOST.contains("protocol_version"));
}

#[test]
fn ac4_sidecar_is_an_external_capability_executor_only() {
    // ADR-005 / review decision 3: no recruiting business, transactions,
    // state machine or authorization in the executor or the protocol surface.
    for forbidden in [
        "ica_application::",
        "ica_infra::",
        "approve_draft(",
        "reject_draft(",
        "generate_draft(",
        "advance_task(",
        "confirm_candidate(",
        "authorize(",
        "SqliteStore",
    ] {
        assert!(
            !WORKER.contains(forbidden),
            "executor implements business logic: {forbidden}"
        );
        assert!(
            !SUPERVISOR.contains(forbidden),
            "protocol supervisor implements business logic: {forbidden}"
        );
        assert!(
            !HOST.contains(forbidden),
            "shell implements business logic: {forbidden}"
        );
    }
    // The method allowlist must never contain a business prefix, and the
    // explicit deny-list must be present as the backstop.
    for method_name in method::ALL {
        for prefix in method::BUSINESS_PREFIXES {
            assert!(
                !method_name.starts_with(prefix),
                "business method leaked into capability set: {method_name}"
            );
        }
    }
    assert!(CONTRACT.contains("pub const BUSINESS_PREFIXES"));
    assert!(CONTRACT.contains("SidecarProtocolError::BusinessMethod"));
}

#[test]
fn ac5_rust_core_never_imports_python_implementation_details() {
    let root = repo_root();
    let mut files = Vec::new();
    for crate_name in [
        "ica-domain",
        "ica-application",
        "ica-contracts",
        "ica-api",
        "ica-server",
    ] {
        collect_rs(
            &root.join("crates").join(crate_name).join("src"),
            &mut files,
        );
    }
    assert!(
        files.len() > 50,
        "scan unexpectedly shallow: {}",
        files.len()
    );
    let forbidden = [
        "run_sidecar.py",
        "playwright",
        "pyo3",
        "from app.",
        "import app.",
        "Python::",
    ];
    let mut offenders = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path).expect("read core source");
        for token in forbidden {
            if text.contains(token) {
                offenders.push(format!("{}: {token}", path.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "Rust Core coupled to Python: {offenders:?}"
    );
}

#[test]
fn independence_is_documented_not_self_signed() {
    assert!(MANAGER.contains("#[cfg(windows)]"));
    assert!(HOST.contains("#![cfg_attr(not(debug_assertions), windows_subsystem"));
    // No test here claims a Windows build, a real Chromium sidecar, or a
    // human security sign-off; the delivery record states those as gaps.
    let doc = fs::read_to_string(repo_root().join("docs/architecture/sidecar-protocol-arch-p4.md"))
        .expect("delivery record exists");
    assert!(doc.contains("PENDING_HUMAN"));
    assert!(doc.contains("不自签 Verified"));
}
