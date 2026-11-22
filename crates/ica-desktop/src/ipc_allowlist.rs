//! The IPC command allowlist (F1-4, plan §3).
//!
//! Tauri's `generate_handler!` is itself a registration allowlist: a command
//! not listed there cannot be invoked — the runtime rejects it before any
//! adapter code runs. This module makes that allowlist an explicit, auditable
//! single source of truth so it can be asserted in tests that run on a host
//! without the Tauri/GTK system libraries (the shell host cannot compile here).
//!
//! The shell host's `invoke_handler` in `desktop/tauri-host/src/main.rs` must
//! register exactly [`ALLOWED_COMMANDS`]; `reject_if_not_allowed` is the
//! in-adapter guard used by any future generic dispatcher, and its error is
//! the same `{"error": {code, message, details}}` envelope the HTTP side
//! emits, so the frontend keeps one error path.

use crate::error::IpcError;

/// Every command name the host registers on its `invoke_handler`.
///
/// Keep in sync with `desktop/tauri-host/src/main.rs::invoke_handler` and
/// `desktop/tauri-host/capabilities/default.json`. F1-2 shipped the first
/// vertical slice: the five positions commands.
pub const ALLOWED_COMMANDS: &[&str] = &[
    "positions_list",
    "positions_get",
    "positions_create",
    "positions_patch",
    "positions_delete",
];

/// True when `name` is registered on the host — i.e. may be invoked.
pub fn is_allowed(name: &str) -> bool {
    ALLOWED_COMMANDS.contains(&name)
}

/// Reject a command that is not on the allowlist.
///
/// Returns the baseline's unknown-target envelope (`unknown_command`), the
/// IPC analogue of an unknown HTTP path — never a silent no-op.
pub fn reject_if_not_allowed(name: &str) -> Result<(), IpcError> {
    if is_allowed(name) {
        Ok(())
    } else {
        Err(IpcError::unknown_command())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_command_is_allowed() {
        for name in ALLOWED_COMMANDS {
            assert!(
                is_allowed(name),
                "registered command {name} must be allowed"
            );
            assert!(reject_if_not_allowed(name).is_ok());
        }
    }

    #[test]
    fn a_command_outside_the_allowlist_is_rejected_with_the_envelope() {
        for name in [
            "positions_export",
            "drafts_generate",
            "audit_events_list",
            "",
            "PosList",
        ] {
            assert!(!is_allowed(name), "{name} must not be on the allowlist");
            let err = reject_if_not_allowed(name).expect_err("out-of-list invoke is rejected");
            // The rejection is a real contract error, not a silent failure.
            assert_eq!(
                err.error.code, "http_error",
                "unknown command -> unknown_path() renders http_error"
            );
            assert!(!err.error.message.is_empty());
        }
    }

    #[test]
    fn allowlist_is_unique_and_matches_the_f1_2_slice_size() {
        let mut sorted = ALLOWED_COMMANDS.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), before, "duplicate allowlist entries");
        assert_eq!(
            ALLOWED_COMMANDS.len(),
            5,
            "F1-2 positions slice = 5 commands"
        );
    }

    #[test]
    fn host_invoke_handler_registers_exactly_the_allowlist() {
        // The shell host cannot compile on this Linux box (missing glib-2.0 /
        // WebKit system libs), so assert sync at the source level: parse the
        // `generate_handler![...]` block out of main.rs and compare it with
        // ALLOWED_COMMANDS. This fails if a command is added to one side only.
        let host_main = include_str!("../../../desktop/tauri-host/src/main.rs");
        let start = host_main
            .find("generate_handler![")
            .expect("host has a generate_handler! block");
        let rest = &host_main[start..];
        let end = rest.find(']').expect("handler block closes");
        let block = &rest[..end];
        let mut registered: Vec<&str> = block
            .split(|c: char| c == ',' || c.is_whitespace() || c == '[' || c == '!')
            .map(str::trim)
            .filter(|s| s.starts_with("ica_backend::"))
            .map(|s| s.trim_start_matches("ica_backend::"))
            .collect();
        let mut expected = ALLOWED_COMMANDS.to_vec();
        registered.sort_unstable();
        expected.sort_unstable();
        assert_eq!(
            registered, expected,
            "host invoke_handler and ALLOWED_COMMANDS must be identical"
        );
    }
}
