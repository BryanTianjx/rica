//! AES-256-GCM sealing for `POST /api/export` (`app/infrastructure/export_crypto.py`).
//!
//! The envelope carries the parts separately so a client can see and verify
//! both halves:
//!
//! ```text
//! {"alg": "A256GCM", "iv": "<b64 12 bytes>", "tag": "<b64 16 bytes>",
//!  "ciphertext": "<b64>"}
//! ```
//!
//! Three details are contract, not implementation taste:
//!
//! * the supplied bytes are the AES-256 key **directly** — nothing is derived,
//!   and the key is never logged, persisted or echoed;
//! * a fresh 96-bit nonce comes from the OS CSPRNG on every call, because a
//!   repeated GCM nonce under one key is a real break;
//! * the associated data is the literal `ica:export:0.2.0`, binding a sealed
//!   export to this endpoint and payload format.
//!
//! The plaintext is serialised with sorted keys and no incidental whitespace,
//! which is what Python's `json.dumps(sort_keys=True, separators=(",", ":"))`
//! produces. `serde_json`'s default `Map` is a `BTreeMap`, so key order matches
//! without extra sorting.
//!
//! An unusable key is refused (`invalid_encryption_key`, 422), never ignored:
//! an encrypted export must not silently degrade to plaintext.

use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{AeadCore, Aes256Gcm, Key};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ica_application::{ApplicationError, ExportSealer};

/// Envelope algorithm marker (the JWA-style name for AES-256-GCM).
const ALGORITHM: &str = "A256GCM";
const KEY_BYTES: usize = 32;
const TAG_BYTES: usize = 16;
/// Associated data authenticated with every sealed export; not secret, it just
/// pins the ciphertext to this endpoint and payload format.
const AAD: &[u8] = b"ica:export:0.2.0";

/// The 422 the baseline raises for a key it cannot use.
///
/// Two messages, matching the baseline's two failure points: a value that does
/// not decode as standard base64, and one that decodes to the wrong length.
fn invalid_key(message: String) -> ApplicationError {
    ApplicationError::ValidationWithCode {
        // A 422 in the baseline (`ValidationError`), not a 409.
        code: "invalid_encryption_key",
        message,
        details: serde_json::json!({}),
    }
}

/// Decode `X-Encrypt-Aes-Key` into 32 raw bytes (`decode_key`).
///
/// Standard base64 (with padding) of exactly 32 bytes; surrounding whitespace
/// is tolerated because header folding can introduce it.
fn decode_key(encoded: &str) -> Result<[u8; KEY_BYTES], ApplicationError> {
    let decoded = STANDARD.decode(encoded.trim()).map_err(|_| {
        invalid_key(
            "X-Encrypt-Aes-Key must be base64-encoded 32-byte AES-256 key material".to_string(),
        )
    })?;

    if decoded.len() != KEY_BYTES {
        return Err(invalid_key(format!(
            "X-Encrypt-Aes-Key must decode to exactly {KEY_BYTES} bytes (AES-256); got {} bytes",
            decoded.len()
        )));
    }

    let mut key = [0u8; KEY_BYTES];
    key.copy_from_slice(&decoded);
    Ok(key)
}

/// The AES-256-GCM sealer.
#[derive(Debug, Clone, Copy, Default)]
pub struct AesGcmSealer;

impl ExportSealer for AesGcmSealer {
    /// Seal `document` and return `(iv, tag, ciphertext)`, base64-encoded.
    fn seal(
        &self,
        document: &serde_json::Value,
        key_b64: &str,
    ) -> Result<(String, String, String), ApplicationError> {
        let key_bytes = decode_key(key_b64)?;

        // Compact, sorted-key JSON — the same bytes Python seals. `to_vec`
        // would keep INSERTION order (the crate enables `preserve_order` for
        // the audit column), so the sorted form is asked for explicitly.
        let plaintext = crate::json_fmt::python_json_sorted_compact(document).into_bytes();

        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        // `encrypt` returns ciphertext || tag; the envelope splits them.
        let sealed = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: AAD,
                },
            )
            .map_err(|_| ApplicationError::Unavailable {
                message: "export sealing failed".to_string(),
            })?;

        if sealed.len() < TAG_BYTES {
            return Err(ApplicationError::Unavailable {
                message: "export sealing produced no tag".to_string(),
            });
        }
        let (ciphertext, tag) = sealed.split_at(sealed.len() - TAG_BYTES);

        Ok((
            STANDARD.encode(nonce.as_slice()),
            STANDARD.encode(tag),
            STANDARD.encode(ciphertext),
        ))
    }
}

/// The algorithm marker the DTO reports (`alg`).
pub fn algorithm() -> &'static str {
    ALGORITHM
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::Nonce;

    fn key_b64() -> String {
        STANDARD.encode([7u8; 32])
    }

    #[test]
    fn a_round_trip_recovers_the_document() {
        let document = serde_json::json!({
            "version": "0.2.0",
            "exported_at": "2026-11-18T12:00:00.000000+00:00",
            "data": {"candidates": [{"id": "c-1"}]},
        });
        let sealer = AesGcmSealer;
        let (iv, tag, ciphertext) = sealer.seal(&document, &key_b64()).expect("seal");

        let key = decode_key(&key_b64()).expect("key");
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let combined = [
            STANDARD.decode(&ciphertext).unwrap(),
            STANDARD.decode(&tag).unwrap(),
        ]
        .concat();
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&STANDARD.decode(&iv).unwrap()),
                Payload {
                    msg: &combined,
                    aad: AAD,
                },
            )
            .expect("decrypt");

        let recovered: serde_json::Value = serde_json::from_slice(&plaintext).expect("json");
        assert_eq!(recovered, document);
    }

    #[test]
    fn the_iv_is_twelve_bytes_and_the_tag_sixteen() {
        let sealer = AesGcmSealer;
        let (iv, tag, _) = sealer
            .seal(&serde_json::json!({"a": 1}), &key_b64())
            .expect("seal");
        assert_eq!(STANDARD.decode(iv).unwrap().len(), 12);
        assert_eq!(STANDARD.decode(tag).unwrap().len(), 16);
    }

    #[test]
    fn two_seals_of_the_same_document_differ() {
        // A fresh nonce every call; identical ciphertext would mean reuse.
        let sealer = AesGcmSealer;
        let document = serde_json::json!({"a": 1});
        let (iv1, _, c1) = sealer.seal(&document, &key_b64()).expect("seal");
        let (iv2, _, c2) = sealer.seal(&document, &key_b64()).expect("seal");
        assert_ne!(iv1, iv2);
        assert_ne!(c1, c2);
    }

    #[test]
    fn a_key_that_is_not_base64_is_refused_with_the_baseline_message() {
        let error = AesGcmSealer
            .seal(&serde_json::json!({}), "not-a-valid-key")
            .unwrap_err();
        assert_eq!(
            error.message(),
            "X-Encrypt-Aes-Key must be base64-encoded 32-byte AES-256 key material"
        );
    }

    #[test]
    fn a_key_of_the_wrong_length_reports_the_actual_length() {
        let short = STANDARD.encode([1u8; 16]);
        let error = AesGcmSealer
            .seal(&serde_json::json!({}), &short)
            .unwrap_err();
        assert_eq!(
            error.message(),
            "X-Encrypt-Aes-Key must decode to exactly 32 bytes (AES-256); got 16 bytes"
        );
    }
}
