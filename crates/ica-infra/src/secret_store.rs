//! The OS credential store adapter (W5 ai-key, project ruling: OS store).
//!
//! Slots live in the platform credential vault through the `keyring` crate —
//! Windows Credential Manager, macOS Keychain — under service `ICA`, account
//! `ICA_<slot>` (the baseline's `_KEYCHAIN_SERVICE`/`_KEYCHAIN_ACCOUNT_PREFIX`
//! naming, so the vault entries are recognisable by either implementation).
//! Tests never touch the real vault: they wire the in-memory
//! [`MemorySecretStore`] through the same port.

use ica_application::ports::{RepositoryError, SecretStore};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The service name the baseline uses for its credential entries.
pub const SERVICE_NAME: &str = "ICA";
/// The account-name prefix (one credential per slot).
pub const ACCOUNT_PREFIX: &str = "ICA_";

/// The OS credential store (Windows Credential Manager / macOS Keychain).
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    fn entry(&self, name: &str) -> Result<keyring::Entry, RepositoryError> {
        keyring::Entry::new(SERVICE_NAME, &format!("{ACCOUNT_PREFIX}{name}")).map_err(|error| {
            RepositoryError::unavailable(format!("credential store unavailable: {error}"))
        })
    }
}

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
