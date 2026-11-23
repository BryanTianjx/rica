//! SEC-01 — JWT signing-key lifecycle (audit §5.1 / RICA-SEC-001 §3).
//!
//! This module owns the *one* place where the JWT signing key is created,
//! persisted, loaded and rotated. The old desktop-host implementation derived
//! the secret from `SystemTime` + PID; that is replaced here by the operating
//! system CSPRNG (`rand::rngs::OsRng`, 256 bit).
//!
//! Storage tiers (RICA-SEC-001 §3.5):
//!   * Windows Credential Manager / macOS Keychain via `keyring` (the same
//!     service/account naming as [`crate::secret_store`]);
//!   * Linux development host: a `0600` file under `<data root>/secrets/`,
//!     mirroring the project's existing baseline fallback. This is a
//!     *development* fallback, not hardware-backed protection.
//!
//! No key value is ever written to a log, a diagnostic message, source code or
//! a plaintext config file. [`JwtKey`]'s `Debug`/`Display` are redacted, and the
//! storage adapters never include the value in error messages. A missing or
//! corrupt slot fails closed into a fresh CSPRNG key — never a fixed default.

use rand::rngs::OsRng;
use rand::RngCore;
use std::fmt;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Mutex;

/// Credential-slot name (never the value). Reuses the `ICA` / `ICA_` naming
/// convention from the existing OS credential-store adapter.
pub const JWT_KEY_SLOT: &str = "JWT_SIGNING_KEY";
/// 256-bit signing key, in bytes (RICA-SEC-001 §3.4).
pub const KEY_LEN: usize = 32;

/// Key-management failures. Messages describe the *slot* and the system error
/// only — never the key value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// The backing store could not be read or written.
    Unavailable(String),
    /// A stored value existed but was not a valid 32-byte hex key.
    InvalidStoredKey,
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::Unavailable(msg) => write!(f, "JWT key store unavailable: {msg}"),
            KeyError::InvalidStoredKey => {
                write!(f, "JWT key slot {JWT_KEY_SLOT} holds an invalid key")
            }
        }
    }
}

impl std::error::Error for KeyError {}

/// A 256-bit JWT signing key.
///
/// Deliberately neither `Debug` nor `Display` exposes the bytes: a key that is
/// interpolated into a log or an error is a leaked key. Both render
/// `[REDACTED]`; [`JwtKey::to_hex`] is the only accessor that materialises the
/// value, and it exists solely to hand the key to the signer via `JWT_SECRET`.
pub struct JwtKey {
    bytes: [u8; KEY_LEN],
}

impl JwtKey {
    /// Wrap raw CSPRNG bytes as a signing key.
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self { bytes }
    }

    /// Borrow the raw bytes (for `jsonwebtoken::EncodingKey::from_secret`).
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }

    /// Hex form (64 chars) for the `JWT_SECRET` process-environment handoff.
    /// The returned `String` contains secret material: never log it.
    pub fn to_hex(&self) -> String {
        hex_encode(&self.bytes)
    }

    /// Strictly parse a stored hex value back into a 256-bit key.
    pub fn from_hex(text: &str) -> Result<Self, KeyError> {
        Ok(Self {
            bytes: decode_hex_32(text)?,
        })
    }
}

impl fmt::Debug for JwtKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JwtKey([REDACTED])")
    }
}

impl fmt::Display for JwtKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Drop for JwtKey {
    fn drop(&mut self) {
        // Volatile writes stop the optimiser from eliding the wipe. Rotation
        // drops the previous key here, so old key material is cleared.
        for byte in self.bytes.iter_mut() {
            // SAFETY: `byte` is a unique, live mutable reference into
            // `self.bytes`, so the volatile write is aligned, in-bounds and
            // has no aliasing readers for the duration of the write. Volatile
            // is required so the optimiser cannot elide the key wipe.
            unsafe { std::ptr::write_volatile(byte, 0u8) };
        }
    }
}

/// Generate a fresh 256-bit secret from the OS CSPRNG.
///
/// SEC-01-T01/T02/T03: 32 bytes, no fixed value, no `SystemTime`, no PID.
pub fn generate_jwt_secret() -> [u8; KEY_LEN] {
    let mut secret = [0u8; KEY_LEN];
    OsRng.fill_bytes(&mut secret);
    secret
}

/// The persistence seam for one key slot. Implementations must never include
/// the stored value in an error or a log message.
pub trait JwtKeyStorage: Send + Sync {
    /// `Ok(None)` means the slot is absent (first launch / after rotation
    /// cleanup); a present value is validated by the provider.
    fn load(&self) -> Result<Option<String>, KeyError>;
    /// Persist/overwrite the slot. The argument is secret material.
    fn store(&self, hex_value: &str) -> Result<(), KeyError>;
}

/// 0600-file fallback (Linux dev host; also usable where a vault is absent).
/// The file lives under the *data root*, never in the project directory.
pub struct FileKeyStorage {
    dir: PathBuf,
}

impl FileKeyStorage {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(format!("{JWT_KEY_SLOT}.secret"))
    }
}

impl JwtKeyStorage for FileKeyStorage {
    fn load(&self) -> Result<Option<String>, KeyError> {
        match std::fs::read_to_string(self.path()) {
            Ok(value) => Ok(Some(value.trim().to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(KeyError::Unavailable(format!(
                "cannot read {JWT_KEY_SLOT} slot: {error}"
            ))),
        }
    }

    fn store(&self, hex_value: &str) -> Result<(), KeyError> {
        std::fs::create_dir_all(&self.dir).map_err(|error| {
            KeyError::Unavailable(format!("cannot create secrets directory: {error}"))
        })?;
        let path = self.path();
        {
            use std::io::Write;
            #[cfg(unix)]
            use std::os::unix::fs::OpenOptionsExt;

            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            // Create with restrictive permissions from the start: a brief
            // world-readable window is not acceptable for a signing key.
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options
                .open(&path)
                .map_err(|error| KeyError::Unavailable(format!("cannot open key slot: {error}")))?;
            file.write_all(hex_value.as_bytes()).map_err(|error| {
                KeyError::Unavailable(format!("cannot write key slot: {error}"))
            })?;
        }
        // Tighten a pre-existing file that may have been created with wider
        // permissions by an older build.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

/// OS credential store (Windows Credential Manager / macOS Keychain).
#[cfg(not(target_os = "linux"))]
pub struct KeyringKeyStorage;

#[cfg(not(target_os = "linux"))]
impl KeyringKeyStorage {
    fn entry(&self) -> Result<keyring::Entry, KeyError> {
        keyring::Entry::new(
            crate::secret_store::SERVICE_NAME,
            &format!("{}{}", crate::secret_store::ACCOUNT_PREFIX, JWT_KEY_SLOT),
        )
        .map_err(|error| KeyError::Unavailable(format!("credential store unavailable: {error}")))
    }
}

#[cfg(not(target_os = "linux"))]
impl JwtKeyStorage for KeyringKeyStorage {
    fn load(&self) -> Result<Option<String>, KeyError> {
        match self.entry()?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(KeyError::Unavailable(format!(
                "credential store read failed: {error}"
            ))),
        }
    }

    fn store(&self, hex_value: &str) -> Result<(), KeyError> {
        self.entry()?.set_password(hex_value).map_err(|error| {
            KeyError::Unavailable(format!("credential store write failed: {error}"))
        })
    }
}

/// Pick the strongest tier available on this platform.
pub fn platform_storage(data_root: &Path) -> Box<dyn JwtKeyStorage> {
    #[cfg(target_os = "linux")]
    {
        Box::new(FileKeyStorage::new(data_root.join("secrets")))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = data_root;
        Box::new(KeyringKeyStorage)
    }
}

/// Creation / load / rotation of the single active signing key.
pub struct JwtKeyProvider {
    storage: Box<dyn JwtKeyStorage>,
}

impl JwtKeyProvider {
    /// Production constructor: OS vault where integrated, 0600 file fallback.
    pub fn new(data_root: impl AsRef<Path>) -> Self {
        Self {
            storage: platform_storage(data_root.as_ref()),
        }
    }

    /// Constructor for tests / alternate storage backends.
    pub fn with_storage(storage: Box<dyn JwtKeyStorage>) -> Self {
        Self { storage }
    }

    /// Return the active key, creating a fresh CSPRNG key on first launch or
    /// when the stored value is absent/invalid. A restart therefore reuses
    /// the persisted key (session continuity); a corrupt slot never silently
    /// degrades to a fixed secret (RICA-SEC-001 §3.5).
    pub fn load_or_create(&self) -> Result<JwtKey, KeyError> {
        match self.storage.load()? {
            None => self.rotate(),
            Some(hex) => match JwtKey::from_hex(&hex) {
                Ok(key) => Ok(key),
                Err(_) => self.rotate(),
            },
        }
    }

    /// Mint and persist a new 256-bit key. The previous key is dropped (and
    /// wiped); with a single active slot there is no historical verification
    /// key, so tokens signed by the old key fail immediately after rotation.
    /// See `docs/audits/sec-01-jwt-key-report.md` §Compatibility.
    pub fn rotate(&self) -> Result<JwtKey, KeyError> {
        let key = JwtKey::from_bytes(generate_jwt_secret());
        self.storage.store(&key.to_hex())?;
        Ok(key)
    }
}

// ── Encoding helpers (no external `hex` dependency needed) ────────────────

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    text
}

fn decode_hex_32(text: &str) -> Result<[u8; KEY_LEN], KeyError> {
    if text.len() != KEY_LEN * 2 {
        return Err(KeyError::InvalidStoredKey);
    }
    let bytes = text.as_bytes();
    let mut out = [0u8; KEY_LEN];
    for (index, slot) in out.iter_mut().enumerate() {
        let high = hex_value(bytes[2 * index]).ok_or(KeyError::InvalidStoredKey)?;
        let low = hex_value(bytes[2 * index + 1]).ok_or(KeyError::InvalidStoredKey)?;
        *slot = (high << 4) | low;
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// In-memory slot for tests (never the operator's real vault). Cloning
    /// shares the same slot, so a provider can simulate a process restart.
    #[derive(Clone, Default)]
    struct MemoryStorage {
        value: Arc<Mutex<Option<String>>>,
    }

    impl MemoryStorage {
        fn new() -> Self {
            Self::default()
        }
    }

    impl JwtKeyStorage for MemoryStorage {
        fn load(&self) -> Result<Option<String>, KeyError> {
            Ok(self.value.lock().expect("memory slot lock").clone())
        }
        fn store(&self, hex_value: &str) -> Result<(), KeyError> {
            *self.value.lock().expect("memory slot lock") = Some(hex_value.to_string());
            Ok(())
        }
    }

    /// Storage that always fails, for fail-closed testing.
    struct FailingStorage;

    impl JwtKeyStorage for FailingStorage {
        fn load(&self) -> Result<Option<String>, KeyError> {
            Err(KeyError::Unavailable("simulated outage".into()))
        }
        fn store(&self, _hex_value: &str) -> Result<(), KeyError> {
            Err(KeyError::Unavailable("simulated outage".into()))
        }
    }

    const HOST_MAIN: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../desktop/tauri-host/src/main.rs"
    );

    #[test]
    fn sec01_t01_secret_is_256_bit_hex() {
        let secret = generate_jwt_secret();
        assert_eq!(secret.len(), KEY_LEN);
        let key = JwtKey::from_bytes(secret);
        let hex = key.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(JwtKey::from_hex(&hex).unwrap().as_bytes(), key.as_bytes());
    }

    #[test]
    fn sec01_t02_independent_generations_do_not_repeat() {
        let first = JwtKey::from_bytes(generate_jwt_secret()).to_hex();
        let second = JwtKey::from_bytes(generate_jwt_secret()).to_hex();
        assert_ne!(first, second, "CSPRNG must not reuse a fixed secret");
    }

    /// Extract a function's source body (signature through its closing brace).
    /// Comments and the test module mention forbidden identifiers on purpose,
    /// so only the executed body is inspected.
    fn body_of<'a>(source: &'a str, signature: &str) -> &'a str {
        let start = source.find(signature).expect("function signature present");
        let rest = &source[start..];
        let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
        &rest[..end]
    }

    #[test]
    fn sec01_t03_no_time_or_pid_in_generators() {
        let own_source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/jwt_key.rs"))
                .unwrap();
        let generator = body_of(&own_source, "pub fn generate_jwt_secret()");
        assert!(!generator.contains("SystemTime"), "no time-based seed");
        assert!(!generator.contains("UNIX_EPOCH"), "no epoch-based seed");
        assert!(!generator.contains("process::id"), "no PID-based seed");
        assert!(generator.contains("OsRng"), "must use the OS CSPRNG");
        assert!(
            generator.contains("fill_bytes"),
            "must fill 32 CSPRNG bytes"
        );

        let host_source = std::fs::read_to_string(HOST_MAIN)
            .expect("tauri-host main.rs must exist in the repository");
        let body = body_of(&host_source, "fn generate_jwt_secret()");
        assert!(
            body.contains("JwtKeyProvider"),
            "host must delegate to the provider"
        );
        assert!(
            body.contains("load_or_create"),
            "host must load-or-create the key"
        );
        assert!(!body.contains("SystemTime"), "host must not seed from time");
        assert!(
            !body.contains("UNIX_EPOCH"),
            "host must not seed from epoch time"
        );
        assert!(!body.contains("process::id"), "host must not seed from PID");
    }

    #[test]
    fn sec01_t04_restart_reuses_persisted_key() {
        let storage = MemoryStorage::new();
        let first = JwtKeyProvider::with_storage(Box::new(storage.clone()))
            .load_or_create()
            .unwrap();
        // Simulate a process restart with a fresh provider over the same slot.
        let second = JwtKeyProvider::with_storage(Box::new(storage.clone()))
            .load_or_create()
            .unwrap();
        assert_eq!(first.to_hex(), second.to_hex());
        assert!(storage.value.lock().unwrap().is_some());
    }

    #[test]
    fn sec01_t05_unavailable_store_fails_closed() {
        let provider = JwtKeyProvider::with_storage(Box::new(FailingStorage));
        let error = provider.load_or_create().unwrap_err();
        assert!(matches!(error, KeyError::Unavailable(_)));
        // No fixed secret is returned in place of a working store.
        assert!(!error.to_string().contains("default"));
    }

    #[test]
    fn sec01_t06_key_never_renders_in_logs() {
        let key = JwtKey::from_bytes(generate_jwt_secret());
        let hex = key.to_hex();
        let debug = format!("{key:?}");
        let display = format!("{key}");
        assert!(debug.contains("[REDACTED]"));
        assert!(display.contains("[REDACTED]"));
        assert!(!debug.contains(&hex));
        assert!(!display.contains(&hex));
    }

    #[test]
    fn sec01_t07_rotation_replaces_the_active_key() {
        let storage = MemoryStorage::new();
        let provider = JwtKeyProvider::with_storage(Box::new(storage.clone()));
        let initial = provider.load_or_create().unwrap();
        let rotated = provider.rotate().unwrap();
        assert_ne!(initial.to_hex(), rotated.to_hex());
        assert_eq!(
            storage.value.lock().unwrap().as_deref(),
            Some(rotated.to_hex().as_str())
        );
        // After rotation the persisted slot is the new key, so a restart
        // validates against the new key only (old tokens are invalidated).
        let after_restart = JwtKeyProvider::with_storage(Box::new(storage))
            .load_or_create()
            .unwrap();
        assert_eq!(after_restart.to_hex(), rotated.to_hex());
    }

    #[test]
    fn sec01_t08_invalid_stored_value_is_regenerated() {
        let storage = MemoryStorage::new();
        *storage.value.lock().unwrap() = Some("not-a-32-byte-hex-key".into());
        let key = JwtKeyProvider::with_storage(Box::new(storage.clone()))
            .load_or_create()
            .unwrap();
        assert_eq!(key.to_hex().len(), 64);
        assert_ne!(
            storage.value.lock().unwrap().as_deref(),
            Some("not-a-32-byte-hex-key")
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_fallback_creates_0600_key_outside_project_dir() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let provider = JwtKeyProvider::new(&root);
        let first = provider.load_or_create().unwrap();
        let path = root.join("secrets").join(format!("{JWT_KEY_SLOT}.secret"));
        assert!(path.is_file());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "fallback key file must be owner-only");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first.to_hex());
        let second = JwtKeyProvider::new(&root).load_or_create().unwrap();
        assert_eq!(first.to_hex(), second.to_hex());
    }
}
