//! The AI API key must land in a REAL credential store.
//!
//! `keyring` silently falls back to its in-memory **mock** store when the
//! platform backend feature is missing. The mock is per-`Entry`: a write
//! succeeds and the very next read (a new `Entry`) sees nothing. The user
//! sees "API key saved" and then "API key unavailable" when a draft is
//! generated — with no error anywhere.
//!
//! These tests pin the backend so the fallback can never come back unnoticed.

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn the_platform_credential_store_is_not_the_in_memory_mock() {
    let entry = keyring::Entry::new("ICA-backend-selftest", "probe").expect("entry");
    let is_mock = entry
        .get_credential()
        .downcast_ref::<keyring::mock::MockCredential>()
        .is_some();
    assert!(
        !is_mock,
        "the keyring crate fell back to its mock store: credentials would be written \
         and then lost, so the AI API key would never survive a request"
    );
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn the_declared_features_keep_the_platform_backend_enabled() {
    // A source-level guard: the Cargo features are the actual fix, and a
    // future "simplification" of the dependency block would silently bring the
    // mock back. Only the enabled platform exposes its module.
    let manifest = include_str!("../Cargo.toml");
    assert!(
        manifest.contains("windows-native") && manifest.contains("apple-native"),
        "crates/ica-infra/Cargo.toml must enable the platform credential backend"
    );
}

/// Linux keeps the baseline's 0600-file fallback instead of a keyring
/// backend, so the round trip must work there too.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_linux_file_fallback_round_trips_a_slot() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store =
        ica_infra::secret_store::KeyringSecretStore::with_fallback_dir(dir.path().to_path_buf());
    store.store("selftest_slot", "value").await.expect("store");
    assert_eq!(
        store.load("selftest_slot").await.expect("load"),
        Some("value".to_string())
    );
}
