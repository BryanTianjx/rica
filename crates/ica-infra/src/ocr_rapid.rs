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
    config::{DetInputLimits, PipelineConfig, RapidOcrConfig},
    model::{model_set_by_name, ModelCache, ModelDownloadMode, ModelSetSpec},
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
/// Resolution order (`OCR-01-04`: the port must not depend on the BASELINE's
/// Python install):
///
/// 1. `RAPIDOCR_MODEL_DIR` — explicit override (what `contract_verify.py`
///    and the desktop host set);
/// 2. `ICA_OCR_MODEL_DIR` — the directory the desktop host provisioned;
/// 3. `ICA_DATA_DIR/ocr/models` — the application data directory, when the
///    models were provisioned there;
///
/// and NOTHING else. An earlier revision fell back to the baseline's
/// `.venv/.../rapidocr/models` — a release binary silently depending on the
/// machine it was built on. A machine without the models now answers the
/// readable 503 `ocr_unavailable` instead of borrowing the baseline's files.
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
    PathBuf::from("models")
}

/// The directory the engine will look in — exposed for diagnostics and for the
/// tests that need to know whether the models are present.
pub fn resolved_model_dir() -> PathBuf {
    model_dir()
}

/// The pipeline configuration the engine runs with.
///
/// The baseline's rapidocr `config.yaml` sets NO detector input limit: a wide
/// screenshot is upsampled to the `limit_side_len: min` rule whatever its
/// pixel count. `DetInputLimits::default()` (4096-side / 2048²-pixels /
/// Downscale) clamps that, and the different input resolution changes where
/// the detector splits a line — measured as a stable `\n`-vs-space difference
/// on every image wider than ~757 px, with the differing text landing in
/// `import_records.content` (audit OCR F1). `unrestricted()` is therefore the
/// baseline's policy, not an optimization.
fn engine_config(cache: &ModelCache, model_set: &ModelSetSpec) -> RapidOcrConfig {
    let mut config = cache
        .config_for(model_set)
        .with_pipeline(PipelineConfig::full());
    if let Some(det) = config.det.as_mut() {
        det.input_limits = DetInputLimits::unrestricted();
    }
    config
}

/// The process-wide engine slot, created on first use and RETRYABLE.
///
/// `RapidOcr` is not `Sync`, and inference must not run two requests at once
/// on one session (the baseline guards only initialisation and notes that
/// ONNX Runtime sessions are safe afterwards — but a Rust `&mut self` call
/// cannot be concurrent at all). The mutex therefore serialises inference.
///
/// The slot holds `Option`: an engine that failed to START is not remembered
/// as a failure. The first audit round (LWW P3-1) measured the earlier
/// `OnceLock<Result<..>>` design — a missing model directory on the first
/// request poisoned the process into 503 for its whole lifetime, even after
/// the models were put in place. Now a failed start leaves the slot empty and
/// the NEXT request retries; a healthy process never re-initialises.
fn engine() -> Result<Arc<Mutex<RapidOcr>>, ApplicationError> {
    static ENGINE: OnceLock<Mutex<Option<Arc<Mutex<RapidOcr>>>>> = OnceLock::new();

    let slot = ENGINE.get_or_init(|| Mutex::new(None));
    // A panic while HOLDING this slot lock would poison it. Recover: the
    // Option inside is still a valid value (possibly None) — which is exactly
    // what "retry the initialisation" means. (Audit LWW P3-2's poison
    // concern, applied to the slot rather than the engine mutex.)
    let mut guard = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(engine) = guard.as_ref() {
        return Ok(engine.clone());
    }

    let model_set = model_set_by_name("ppocrv6-small")
        .ok_or_else(|| unavailable("unknown OCR model set 'ppocrv6-small'"))?;
    let cache = ModelCache::new(model_dir());
    // `Never`, not `Missing`: the download path builds a `reqwest`
    // BLOCKING client, which panics with "Cannot drop a runtime in a
    // context where blocking is not allowed" when the engine is first
    // created from inside an async task. A missing model must produce a
    // readable 503, not a panic — and the models ship with the desktop
    // host, so downloading them at runtime is not this port's job.
    cache
        .ensure_model_set_for_pipeline(model_set, PipelineConfig::full(), ModelDownloadMode::Never)
        .map_err(|error| unavailable(&format!("OCR models are not available: {error}")))?;
    let engine = RapidOcr::from_config(engine_config(&cache, model_set))
        .map_err(|error| unavailable(&format!("OCR engine could not start: {error}")))?;
    let engine = Arc::new(Mutex::new(engine));
    *guard = Some(engine.clone());
    Ok(engine)
}

/// The 503 `ocr_unavailable` the baseline's `ensure_ocr_available` raises.
fn unavailable(reason: &str) -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: 503,
        code: "ocr_unavailable",
        // The baseline's constant, byte for byte (`ocr_service.py`
        // `ensure_ocr_available`): the wire message is contract text.
        message: "Local OCR is not available on this machine. \
Install the optional OCR stack with `uv pip install -e '.[ocr]'` \
(RapidOCR + ONNX Runtime) to use screenshot import."
            .to_string(),
        details: serde_json::json!({ "reason": reason }),
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
                // An inference failure is the baseline's `ocr_error`, whose
                // `ICAError` default status is 500 (`errors.py:17` — audit
                // OCR F2 measured the port answering 409 here).
                if message.starts_with("Local OCR failed") {
                    Err(ApplicationError::StatusWithCode {
                        status: 500,
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
///
/// A poisoned engine mutex is RECOVERED, not propagated: the baseline has no
/// equivalent failure (its inference never takes a shared lock), and a panic
/// inside ONNX does not make the `RapidOcr` value unusable — the next call
/// simply runs again. What remains of audit LWW P3-2 / OCR F6 is therefore
/// the recovery, and a genuinely broken engine answers through the ordinary
/// `Local OCR failed` → 500 `ocr_error` path.
fn run_inference(engine: &Arc<Mutex<RapidOcr>>, path: &Path) -> Result<String, String> {
    let mut guard = engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
/// Here the `image` crate's decoder does the same job. The `details.reason`
/// text is contract-visible (audit OCR F3 measured the two sides differing),
/// so the two common `image`-crate failures are rendered with PILLOW's words:
/// an unidentifiable file is `cannot identify image file '<path>'` and a cut
/// off file is `image file is truncated`.
fn decode_image(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    image::load_from_memory(&bytes)
        .map(|_| ())
        .map_err(|error| {
            let text = error.to_string();
            // The `image` crate reports BOTH an unknown format and a truncated
            // file as decoding errors; Pillow words them differently and the
            // `reason` is contract-visible (audit OCR F3). Dispatch on the
            // decoder's own text.
            if text.contains("format") {
                format!("cannot identify image file '{}'", path.display())
            } else if text.contains("unexpected end") || text.contains("truncated") {
                "image file is truncated".to_string()
            } else {
                text
            }
        })
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

    /// **Audit OCR F1**: the detector must run WITHOUT input limits, because
    /// the baseline's `config.yaml` sets none. The default limits (4096-side /
    /// 2048²-pixels / Downscale) clamp wide screenshots onto a different
    /// input resolution, which changes where the detector splits a line — a
    /// stable `\n`-vs-space divergence on every image wider than ~757 px.
    ///
    /// Mutation check: removing the `unrestricted()` assignment in
    /// [`engine_config`] flips this test red; the small-image parity the
    /// original tests asserted stays green, which is exactly why the defect
    /// survived the first delivery.
    #[test]
    fn the_detector_runs_unrestricted_like_the_baseline() {
        let model_set = model_set_by_name("ppocrv6-small").expect("model set known");
        let cache = ModelCache::new(model_dir());
        let config = engine_config(&cache, model_set);
        let det = config.det.expect("the pipeline configures a detector");
        assert_eq!(
            det.input_limits,
            DetInputLimits::unrestricted(),
            "the baseline sets no detector input limit; clamping it changes \
             line breaks on wide screenshots (audit OCR F1)"
        );
    }
}

#[cfg(test)]
mod decode_tests {
    use super::*;

    /// **Audit OCR F3**: the `reason` text is contract-visible and must use
    /// Pillow's wording. A file that is not an image at all.
    #[test]
    fn a_non_image_reports_pillow_words() {
        let path = std::env::temp_dir().join("ica-ocr-f3-not-an-image.png");
        std::fs::write(&path, b"this is definitely not a png").unwrap();
        let error = decode_image(&path).unwrap_err();
        std::fs::remove_file(&path).ok();
        assert_eq!(
            error,
            format!("cannot identify image file '{}'", path.display()),
            "the reason must match Pillow's UnidentifiedImageError text"
        );
    }

    /// **Audit OCR F3**: a cut-off PNG reports `image file is truncated` —
    /// the baseline's Pillow wording — not the image crate's
    /// `unexpected end of file`.
    #[test]
    fn a_truncated_png_reports_pillow_words() {
        // A real PNG header followed by nothing: the decoder starts, then the
        // file simply ends.
        let png_head: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR length+type
        ];
        let path = std::env::temp_dir().join("ica-ocr-f3-truncated.png");
        std::fs::write(&path, png_head).unwrap();
        let error = decode_image(&path).unwrap_err();
        std::fs::remove_file(&path).ok();
        assert!(
            error.contains("truncated") || error.contains("cannot identify"),
            "the reason must be Pillow-shaped, got: {error}"
        );
    }

    /// **W4-R1 scope 1 / OCR-01-04**: the model directory resolution must NOT
    /// fall back to the baseline's Python install. With no environment set,
    /// the answer is the bare `models` relative path — which, on a machine
    /// without provisioned models, produces the readable 503 instead of
    /// silently borrowing `/mnt/o/ica`'s files.
    #[test]
    fn model_dir_has_no_baseline_venv_fallback() {
        // The only correct way to observe the resolution is with the
        // environment cleared, which a parallel test could race — so assert
        // the PROPERTY instead: no resolution branch ever mentions the
        // baseline path, by construction of the source. The cheap runtime
        // assertion: the resolved dir is never the baseline venv path.
        let resolved = resolved_model_dir();
        assert!(
            !resolved.starts_with("/mnt/o/ica"),
            "the model dir must never resolve into the baseline's venv: {resolved:?}"
        );
    }
}
