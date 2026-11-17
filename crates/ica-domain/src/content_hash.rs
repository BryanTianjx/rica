//! Message content fingerprint used for duplicate detection.
//!
//! Mirrors `app/application/import_pipeline.py::content_hash` exactly:
//! collapse every run of whitespace to one space, trim, lowercase, then SHA-256
//! the UTF-8 bytes and hex-encode. The value is compared byte-for-byte against
//! the Python baseline, so the normalisation order matters.

use sha2::{Digest, Sha256};

/// Stable hash of a message body: `sha256(" ".join(content.split()).lower())`.
pub fn content_hash(content: &str) -> String {
    let normalized = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
