//! The OS credential store adapter (W5 ai-key, project ruling: OS
//! credential store).
//!
//! Slots live in the platform credential vault through the `keyring` crate —
//! Windows Credential Manager, macOS Keychain — under service `ICA`, account
//! `ICA_<slot>` (the baseline's `_KEYCHAIN_SERVICE`/`_KEYCHAIN_ACCOUNT_PREFIX`
//! naming, so the vault entries are recognisable by either implementation).
//! On Linux — a development host, not a release target — the baseline stores
//! secrets as `0600` files under `<data root>/secrets/`; the adapter mirrors
//! that dispatch exactly (`app/infrastructure/secret_store.py`), so the two
//! implementations can read each other's slots on every platform.
//! Tests never touch the real vault: they wire the in-memory
//! [`MemorySecretStore`] through the same port.

use ica_application::ports::{RepositoryError, SecretStore};
use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::sync::Mutex;

/// The service name the baseline uses for its credential entries.
pub const SERVICE_NAME: &str = "ICA";
/// The account-name prefix (one credential per slot).
pub const ACCOUNT_PREFIX: &str = "ICA_";

/// The platform credential store. Windows/macOS go through `keyring`; Linux
/// (dev host) mirrors the baseline's `0600`-file fallback under the data
/// root's `secrets/` directory.
pub struct KeyringSecretStore {
    /// The `<data root>/secrets` directory for the Linux file fallback; the
    /// keyring platforms never read it.
    #[cfg(target_os = "linux")]
    fallback_dir: Mutex<Option<PathBuf>>,
}

impl KeyringSecretStore {
    /// The keyring-backed store (Windows Credential Manager / macOS Keychain).
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            fallback_dir: Mutex::new(None),
        }
    }

    /// The store with the baseline's Linux file fallback rooted at
    /// `<data root>/secrets` — pass the directory, not the file.
    pub fn with_fallback_dir(dir: std::path::PathBuf) -> Self {
        #[cfg(target_os = "linux")]
        {
            Self {
                fallback_dir: Mutex::new(Some(dir)),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            // The keyring platforms ignore the fallback: the vault is the
            // store, and the parameter exists for the Linux build only.
            let _ = dir;
            Self {}
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn entry(&self, name: &str) -> Result<keyring::Entry, RepositoryError> {
        keyring::Entry::new(SERVICE_NAME, &format!("{ACCOUNT_PREFIX}{name}")).map_err(|error| {
            RepositoryError::unavailable(format!("credential store unavailable: {error}"))
        })
    }

    /// The file path for one slot on the Linux fallback (baseline
    /// `_file_*`): `<secrets dir>/<slot>.secret`, where the slot is already
    /// percent-encoded so it is a legal file name everywhere.
    #[cfg(target_os = "linux")]
    fn fallback_path(&self, name: &str) -> Result<PathBuf, RepositoryError> {
        let dir = self
            .fallback_dir
            .lock()
            .expect("secret fallback lock")
            .clone()
            .ok_or_else(|| {
                RepositoryError::unavailable(
                    "credential store unavailable: no fallback directory configured",
                )
            })?;
        std::fs::create_dir_all(&dir).map_err(|error| {
            RepositoryError::unavailable(format!("cannot create secrets dir: {error}"))
        })?;
        Ok(dir.join(format!("{name}.secret")))
    }
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Linux: the baseline's 0600-file dev fallback ──────────────────────────

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl SecretStore for KeyringSecretStore {
    async fn store(&self, name: &str, value: &str) -> Result<(), RepositoryError> {
        let path = self.fallback_path(name)?;
        // Write through a 0600 file, warning exactly like the baseline: a
        // dev host has no OS credential store integration.
        tracing::warn!(
            "Secret {name} stored as a 0600 file: this platform has no OS credential \
             store integration; it is a development fallback, not a release target."
        );
        std::fs::write(&path, value.as_bytes()).map_err(|error| {
            RepositoryError::unavailable(format!("cannot store '{name}': {error}"))
        })?;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        Ok(())
    }

    async fn load(&self, name: &str) -> Result<Option<String>, RepositoryError> {
        let path = match self.fallback_path(name) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        match std::fs::read_to_string(&path) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => {
                tracing::error!("secret '{name}' could not be loaded: {error}");
                Ok(None)
            }
        }
    }

    async fn delete(&self, name: &str) -> Result<(), RepositoryError> {
        let path = match self.fallback_path(name) {
            Ok(path) => path,
            // No directory configured: nothing was ever stored here.
            Err(_) => return Ok(()),
        };
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RepositoryError::unavailable(format!(
                "cannot delete '{name}': {error}"
            ))),
        }
    }
}

// ── Windows / macOS: the platform vault through `keyring` ─────────────────

#[cfg(not(target_os = "linux"))]
#[async_trait::async_trait]
impl SecretStore for KeyringSecretStore {
    async fn store(&self, name: &str, value: &str) -> Result<(), RepositoryError> {
        let entry = self.entry(name)?;
        entry.set_password(value).map_err(|error| {
            RepositoryError::unavailable(format!("cannot store '{name}': {error}"))
        })
    }

    async fn load(&self, name: &str) -> Result<Option<String>, RepositoryError> {
        let entry = self.entry(name)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            // A broken store must not block resolution: degrade to absent and
            // let the caller fall through (the baseline's `load_secret`).
            Err(error) => {
                tracing::error!("secret '{name}' could not be loaded: {error}");
                Ok(None)
            }
        }
    }

    async fn delete(&self, name: &str) -> Result<(), RepositoryError> {
        let entry = self.entry(name)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(RepositoryError::unavailable(format!(
                "cannot delete '{name}': {error}"
            ))),
        }
    }
}

/// The in-memory store for tests (never the operator's real vault).
#[derive(Default)]
pub struct MemorySecretStore {
    slots: Mutex<BTreeMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.slots.lock().expect("secret lock").contains_key(name)
    }
}

#[async_trait::async_trait]
impl SecretStore for MemorySecretStore {
    async fn store(&self, name: &str, value: &str) -> Result<(), RepositoryError> {
        self.slots
            .lock()
            .expect("secret lock")
            .insert(name.to_string(), value.to_string());
        Ok(())
    }

    async fn load(&self, name: &str) -> Result<Option<String>, RepositoryError> {
        Ok(self.slots.lock().expect("secret lock").get(name).cloned())
    }

    async fn delete(&self, name: &str) -> Result<(), RepositoryError> {
        self.slots.lock().expect("secret lock").remove(name);
        Ok(())
    }
}
