//! ARCH-P5 — transport migration boundary assertions (§5.3/§5.4/§5.5).
//!
//! These are source-level guards, not a second parity harness: the executable
//! HTTP/IPC equivalence suite is P2's `adapter_contract_parity.rs`, which is
//! reused by reference. This file proves that (a) the UI cannot reach a
//! transport, (b) only the migrated domain can switch, (c) the rollback flag
//! and manifest are wired, and (d) the HTTP server capability stays intact.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in
        fs::read_dir(dir).unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
    {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_sources(&path, out);
            continue;
        }
        let is_source = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "ts" || extension == "tsx");
        let is_test = path.to_string_lossy().contains(".test.");
        if is_source && !is_test {
            out.push(path);
        }
    }
}

#[test]
fn p2_adapter_contract_parity_is_reused_not_rebuilt() {
    // P2 owns the executable parity harness; P5 only references it.
    const P2: &str = include_str!("adapter_contract_parity.rs");
    for case in [
        "health_and_positions_reads_produce_identical_results",
        "error_envelopes_are_identical_across_transports",
        "a_write_and_its_generated_fields_are_equivalent",
        "shared_application_guards_render_identically_over_both_transports",
        "adapter_bodies_only_parse_validate_and_call_use_cases",
    ] {
        assert!(P2.contains(case), "P2 parity must still pin {case}");
    }
    let delivery = read("docs/architecture/frontend-backend-client-arch-p5.md");
    assert!(
        delivery.contains("adapter_contract_parity.rs"),
        "the delivery record must reference the P2 parity suite, not replace it"
    );
}

#[test]
fn host_registers_only_the_migrated_positions_commands() {
    let host = read("desktop/tauri-host/src/main.rs");
    let marker = "generate_handler![";
    let start = host
        .find(marker)
        .unwrap_or_else(|| panic!("host has no generate_handler! block"))
        + marker.len();
    let end = start
        + host[start..]
            .find("])")
            .unwrap_or_else(|| panic!("unterminated generate_handler! block"));
    let block = &host[start..end];
    let commands: Vec<&str> = block
        .split("ica_backend::")
        .skip(1)
        .map(|chunk| {
            chunk
                .trim_start()
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .next()
                .unwrap_or("")
        })
        .filter(|name| !name.is_empty())
        .collect();
    assert_eq!(
        commands,
        vec![
            "positions_list",
            "positions_get",
            "positions_create",
            "positions_patch",
            "positions_delete",
        ],
        "host command registration must match the migrated domain set"
    );

    let manifest = read("frontend/src/services/backend/migration.ts");
    let migrated = manifest
        .lines()
        .find(|line| line.contains("MIGRATED_DOMAINS:"))
        .expect("MIGRATED_DOMAINS declaration");
    assert!(migrated.contains("'positions'"));
    for other in [
        "candidates",
        "conversations",
        "messages",
        "imports",
        "followUps",
        "drafts",
        "draftGroups",
        "ai",
        "dashboard",
        "auth",
        "settings",
        "browser",
    ] {
        assert!(
            !migrated.contains(&format!("'{other}'")),
            "{other} is not migrated and must stay on HTTP"
        );
    }
}

#[test]
fn http_server_capability_is_still_registered() {
    let routes = read("crates/ica-api/src/routes.rs");
    for needle in [
        ".route(\"/health\"",
        ".route(\"/api/positions\"",
        "\"/api/candidates\",",
        ".route(\"/api/auth/login\"",
        ".route(\"/api/settings\"",
        ".route(\"/api/imports/preview\"",
    ] {
        assert!(routes.contains(needle), "HTTP server lost {needle}");
    }
    // Browser/AI-EXT remain behind the sidecar route, i.e. still HTTP.
    let routing = read("crates/ica-desktop/src/routing.rs");
    assert!(routing.contains("api/browser/v1"));

    let http_client = read("frontend/src/services/backend/http.ts");
    for group in [
        "candidates: candidatesApi",
        "positions: positionsApi",
        "conversations: conversationsApi",
        "messages: messagesApi",
        "imports: importsApi",
        "followUps: followUpsApi",
        "drafts: draftsApi",
        "draftGroups: draftGroupsApi",
        "ai: aiApi",
        "dashboard: dashboardApi",
        "auth: authApi",
        "settings: settingsApi",
        "browser: browserApi",
    ] {
        assert!(http_client.contains(group), "HTTP backend lost {group}");
    }
    assert!(http_client.contains("export type HttpBackend = typeof httpBackend;"));
    assert!(read("frontend/src/services/api.ts").contains("fetch("));
}

#[test]
fn ui_components_do_not_depend_on_transport_implementations() {
    let src = repo_root().join("frontend/src");
    let mut files = Vec::new();
    for dir in ["pages", "components", "auth", "hooks"] {
        collect_sources(&src.join(dir), &mut files);
    }
    for file in ["App.tsx", "main.tsx"] {
        files.push(src.join(file));
    }
    assert!(files.len() >= 25, "UI source scan is suspiciously small");

    let forbidden = [
        "fetch(",
        "invoke(",
        "@tauri-apps/api",
        "services/api",
        "services/backend/ipc",
        "services/backend/http",
        "services/backend/config",
        "services/backend/migration",
    ];
    let mut consumers = 0usize;
    for file in &files {
        let source = fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", file.display()));
        for needle in forbidden {
            assert!(
                !source.contains(needle),
                "{} must not depend on transport detail {needle}",
                file.display()
            );
        }
        if source.contains("backend.") {
            consumers += 1;
        }
    }
    assert!(consumers > 0, "UI scan never saw a backend client consumer");
}

#[test]
fn rollback_flag_and_migration_manifest_are_wired() {
    let config = read("frontend/src/services/backend/config.ts");
    for needle in [
        "VITE_ICA_BACKEND_TRANSPORT",
        "ica.backend.transport",
        "__ICA_BACKEND_CONFIG__",
        "transport: isTauriWindow() ? 'ipc' : 'http'",
    ] {
        assert!(config.contains(needle), "startup config lost {needle}");
    }

    let manifest = read("frontend/src/services/backend/migration.ts");
    for domain in [
        "candidates",
        "positions",
        "conversations",
        "messages",
        "imports",
        "followUps",
        "drafts",
        "draftGroups",
        "ai",
        "dashboard",
        "auth",
        "settings",
        "browser",
    ] {
        assert!(manifest.contains(domain), "manifest lost domain {domain}");
    }

    let facade = read("frontend/src/services/backend/index.ts");
    for needle in ["createBackend", "resolveBackendConfig", "transport: 'http'"] {
        assert!(facade.contains(needle), "backend facade lost {needle}");
    }
}
