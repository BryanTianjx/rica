//! The OCR adapter for `POST /imports/ocr`.
//!
//! The port exists so `ica-application` never touches an image or an engine.
//! This build has no OCR stack, so the adapter mirrors the baseline's
//! `require_ocr()`: it reports a **readable 503 `ocr_unavailable`** naming the
//! optional extra to install, instead of an `ImportError`-shaped 500 or a panic.
//! That is what the baseline does when rapidocr/Pillow are absent — the guard is
//! called at the start of the route precisely so the failure is legible.
//!
//! When a real engine is wired in, only `read_text` changes; the guard and its
//! message stay, because "not installed" remains a legitimate state of this
//! machine.

use std::path::Path;

use async_trait::async_trait;
use ica_application::{ApplicationError, OcrEngine};

/// Why the engine cannot run, as the baseline words it.
const UNAVAILABLE_MESSAGE: &str = "Local OCR is not available on this machine. \
Install the optional OCR stack with `uv pip install -e '.[ocr]'` \
(RapidOCR + ONNX Runtime) to use screenshot import.";

/// An OCR port with no engine behind it.
///
/// `ensure_available` always refuses; `read_text` therefore never runs. Keeping
/// the two separate means the route still validates the request (path guard,
/// existence) before it reports that OCR is unavailable — the baseline's order.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotConfiguredOcrEngine;

#[async_trait]
impl OcrEngine for NotConfiguredOcrEngine {
    fn ensure_available(&self) -> Result<(), ApplicationError> {
        Err(ApplicationError::StatusWithCode {
            status: 503,
            code: "ocr_unavailable",
            message: UNAVAILABLE_MESSAGE.to_string(),
            details: serde_json::json!({
                "reason": "the OCR stack (rapidocr, Pillow) is not installed",
            }),
        })
    }

    async fn read_text(&self, _path: &Path) -> Result<String, ApplicationError> {
        // Unreachable through the route (the guard fails first), but a port must
        // answer: a bare `unimplemented!()` here would be a panic waiting for a
        // future caller.
        Err(ApplicationError::StatusWithCode {
            status: 409,
            code: "ocr_error",
            message: "Local OCR failed: no OCR engine is wired into this build".to_string(),
            details: serde_json::json!({}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guard_reports_a_503_with_the_baseline_code_and_message() {
        let error = NotConfiguredOcrEngine.ensure_available().unwrap_err();
        assert_eq!(error.message(), UNAVAILABLE_MESSAGE);
        match error {
            ApplicationError::StatusWithCode { status, code, .. } => {
                assert_eq!(status, 503);
                assert_eq!(code, "ocr_unavailable");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_text_never_panics() {
        let error = NotConfiguredOcrEngine
            .read_text(Path::new("/tmp/nothing.png"))
            .await
            .unwrap_err();
        assert_eq!(
            error.message(),
            "Local OCR failed: no OCR engine is wired into this build"
        );
    }
}
