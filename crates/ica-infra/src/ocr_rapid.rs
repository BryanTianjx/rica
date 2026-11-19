//! The real OCR engine: RapidOCR (PP-OCRv6) through ONNX Runtime.
//!
//! The baseline's `app/application/ocr_service.py` runs RapidOCR on the CPU
//! and is fully local: no screenshot ever leaves the machine. This adapter
//! uses the SAME model set — `rapidocr-core`'s `ppocrv6-small`, which is
//! RapidOCR v3.9.0's `PP-OCRv6_det_small.onnx` + `PP-OCRv6_rec_small.onnx` —
//! so the recognised text matches the baseline's rather than merely being
//! "some OCR".
//!
//! Two details of the baseline are reproduced exactly, because the extracted
//! text is fed to the import parser and a difference here changes what gets
//! parsed:
//!
//! * **the character filter.** The baseline writes
//!   `_EXCLUDED_CHARS = set('\ufffd\x00-\x08\x0e-\x1f')` — a set literal, not
//!   a range. It therefore contains U+0000, U+0008, U+000E, U+001F, U+FFFD
//!   **and U+002D (the hyphen)**, so every `-` in the recognised text is
//!   dropped: `2026-11-18` comes out as `20261118`. That looks like a bug
//!   (the comment says "collapse whitespace", which it does not do), but it
//!   is the baseline's behaviour and the port must match it.
//! * **one line per detected box**, joined with `\n`. The baseline joins
//!   `result.txts` the same way.
//!
//! The engine loads its models on first use (~1-3 s on CPU) and is then kept
//! for the process lifetime, so only the first screenshot pays that cost.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use ica_application::{ApplicationError, OcrEngine};
use rapidocr_core::{
    config::PipelineConfig,
    model::{model_set_by_name, ModelCache, ModelDownloadMode},
    RapidOcr,
};

/// The characters the baseline strips from the recognised text.
///
/// Reproduced verbatim from `set('\ufffd\x00-\x08\x0e-\x1f')`, which is a SET
/// of six characters — not the range the comment implies:
///
/// | character | |
/// | --- | --- |
/// | U+0000 | NUL |
/// | U+0008 | backspace |
/// | U+000E | shift out |
/// | U+001F | unit separator |
/// | U+002D | **hyphen-minus** |
/// | U+FFFD | replacement character |
///
/// The hyphen is the one that matters in practice: a date in a screenshot
/// loses its dashes, which changes how the import parser reads it.
const EXCLUDED_CHARS: [char; 6] = ['\u{0}', '\u{8}', '\u{e}', '\u{1f}', '\u{2d}', '\u{fffd}'];

/// Strip the excluded characters and trim, exactly like the baseline's
/// `read_ocr_text`.
fn clean(text: &str) -> String {
    text.chars()
        .filter(|character| !EXCLUDED_CHARS.contains(character))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Where the ONNX models live.
///
/// `RAPIDOCR_MODEL_DIR` overrides it; otherwise the engine's own cache
/// directory is used, and `ICA_OCR_MODEL_DIR` names the directory the
/// desktop host provisioned.
fn model_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("RAPIDOCR_MODEL_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("ICA_OCR_MODEL_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(data_dir) = std::env::var_os("ICA_DATA_DIR") {
        let candidate = PathBuf::from(data_dir).join("ocr").join("models");
        if candidate.join("PP-OCRv6_det_small.onnx").is_file() {
            return candidate;
        }
    }
    // The desktop host provisions the models under the data directory; when
    // they are not there, fall back to the baseline's Python install, which
    // ships the SAME model set (RapidOCR v3.9.0 PP-OCRv6 small) — the
    // recognised text has to match the baseline's, so the models have to be
    // the same files.
    let baseline_models =
        PathBuf::from("/mnt/o/ica/backend/.venv/lib/python3.12/site-packages/rapidocr/models");
    if baseline_models.join("PP-OCRv6_det_small.onnx").is_file() {
        return baseline_models;
    }
    PathBuf::from("models")
}

/// The directory the engine will look in — exposed for diagnostics and for the
/// tests that need to know whether the models are present.
pub fn resolved_model_dir() -> PathBuf {
    model_dir()
}

/// The process-wide engine, created on first use.
///
/// `RapidOcr` is not `Sync`, and inference must not run two requests at once
/// on one session (the baseline guards only initialisation and notes that
/// ONNX Runtime sessions are safe afterwards — but a Rust `&mut self` call
/// cannot be concurrent at all). The mutex therefore serialises inference.
fn engine() -> Result<&'static Arc<Mutex<RapidOcr>>, ApplicationError> {
    static ENGINE: OnceLock<Result<Arc<Mutex<RapidOcr>>, String>> = OnceLock::new();

    let cell = ENGINE.get_or_init(|| {
        let model_set = model_set_by_name("ppocrv6-small")
            .ok_or_else(|| "unknown OCR model set 'ppocrv6-small'".to_string())?;
        let cache = ModelCache::new(model_dir());
        // `Never`, not `Missing`: the download path builds a `reqwest`
        // BLOCKING client, which panics with "Cannot drop a runtime in a
        // context where blocking is not allowed" when the engine is first
        // created from inside an async task. A missing model must produce a
        // readable 503, not a panic — and the models ship with the desktop
        // host, so downloading them at runtime is not this port's job.
        cache
            .ensure_model_set_for_pipeline(
                model_set,
                PipelineConfig::full(),
                ModelDownloadMode::Never,
            )
            .map_err(|error| format!("OCR models are not available: {error}"))?;
        let config = cache
            .config_for(model_set)
            .with_pipeline(PipelineConfig::full());
        let engine = RapidOcr::from_config(config)
            .map_err(|error| format!("OCR engine could not start: {error}"))?;
        Ok(Arc::new(Mutex::new(engine)))
    });

    match cell {
        Ok(engine) => Ok(engine),
        Err(message) => Err(ApplicationError::StatusWithCode {
            status: 503,
            code: "ocr_unavailable",
            message: "Local OCR is not available on this machine. \
Install the optional OCR stack to use screenshot import."
                .to_string(),
            details: serde_json::json!({ "reason": message }),
        }),
    }
}

/// The engine-backed OCR port.
#[derive(Debug, Clone, Copy, Default)]
pub struct RapidOcrEngine;

#[async_trait::async_trait]
impl OcrEngine for RapidOcrEngine {
    fn ensure_available(&self) -> Result<(), ApplicationError> {
        engine().map(|_| ())
    }

    async fn read_text(&self, path: &Path) -> Result<String, ApplicationError> {
        let engine = engine()?;

        // A truncated or non-image file must surface here, before the engine
        // sees it — the baseline forces a full decode with Pillow and maps the
        // OSError to a 4xx (`ocr_image_undecodable`) rather than a 500.
        if let Err(error) = decode_image(path) {
            return Err(ApplicationError::StatusWithCode {
                status: 409,
                code: "ocr_image_undecodable",
                message: "OCR image could not be decoded".to_string(),
                details: serde_json::json!({ "reason": error }),
            });
        }

        let engine = engine.clone();
        let path = path.to_path_buf();
        // The engine is synchronous and CPU-bound. `spawn_blocking` is the
        // natural offload, but it PANICS in a current-thread runtime
        // (`#[tokio::test]`), so it cannot be used unconditionally. The
        // blocking-pool path is only taken when one is available; otherwise
        // the inference runs inline.
        let outcome = match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread {
                    // No blocking pool to hand the work to — run it here.
                    Ok(run_inference(&engine, &path))
                } else {
                    handle
                        .spawn_blocking(move || run_inference(&engine, &path))
                        .await
                }
            }
            Err(_) => Ok(run_inference(&engine, &path)),
        };

        match outcome {
            Ok(Ok(text)) => Ok(clean(&text)),
            Ok(Err(message)) => {
                // An inference failure is the baseline's `ocr_error`.
                if message.starts_with("Local OCR failed") {
                    Err(ApplicationError::StatusWithCode {
                        status: 409,
                        code: "ocr_error",
                        message,
                        details: serde_json::json!({}),
                    })
                } else {
                    Err(ApplicationError::Unavailable { message })
                }
            }
            Err(error) => Err(ApplicationError::Unavailable {
                message: format!("OCR task failed: {error}"),
            }),
        }
    }
}

/// Run the inference and turn it into the joined text.
fn run_inference(engine: &Arc<Mutex<RapidOcr>>, path: &Path) -> Result<String, String> {
    let mut guard = engine
        .lock()
        .map_err(|_| "OCR engine lock was poisoned".to_string())?;
    guard
        .run_path(path)
        .map(|output| {
            output
                .lines
                .iter()
                .map(|line| line.text.clone())
                .collect::<Vec<String>>()
                .join("\n")
        })
        .map_err(|error| format!("Local OCR failed: {error}"))
}

/// Force a full decode of the image, so a truncated or non-image file is
/// rejected before inference.
///
/// The baseline uses Pillow for this (`Image.open(path)` then `image.load()`).
/// Here the `image` crate's decoder does the same job; a file that is not a
/// readable image produces an error, which the caller maps to
/// `ocr_image_undecodable`.
fn decode_image(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    image::load_from_memory(&bytes)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_excluded_set_really_contains_the_hyphen() {
        // The baseline's set literal is not a range: it holds U+002D. This
        // test pins that reading, because "fixing" it would change every
        // date in a screenshot.
        assert!(EXCLUDED_CHARS.contains(&'-'));
        assert_eq!(EXCLUDED_CHARS.len(), 6);
    }

    #[test]
    fn cleaning_drops_the_hyphen_like_the_baseline() {
        assert_eq!(clean("2026-11-18 10:00"), "20261118 10:00");
    }

    #[test]
    fn cleaning_keeps_spaces_and_trims() {
        assert_eq!(clean("  hello world  "), "hello world");
    }

    #[test]
    fn cleaning_drops_the_replacement_character() {
        assert_eq!(clean("a\u{fffd}b"), "ab");
    }
}
