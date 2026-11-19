//! W4 IPC command tests: imports, export, import, OCR.
//!
//! The desktop layer is a thin adapter over the same use cases the HTTP layer
//! calls, so these tests check the two things that are specific to it:
//!
//! * the command surface parses the SAME JSON bodies the HTTP side parses, and
//!   returns the same DTO shapes — a desktop command that drifted from the
//!   HTTP contract would silently diverge the two clients;
//! * `IpcError` still serialises to the `{"error": ...}` envelope, so the
//!   shell renders failures the same way.
//!
//! Every string literal below is on ONE line on purpose: a Rust line
//! continuation swallows the next line's leading whitespace, which silently
//! corrupts both SQL and the CJK sample text.

use ica_desktop::{commands, DesktopConfig, DesktopState};
use serde_json::json;

/// A throwaway database with the baseline schema, wired into a `DesktopState`.
async fn desktop_app() -> (DesktopState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    let pool = ica_infra::connect(&format!("sqlite:///{}?mode=rwc", path.display()))
        .await
        .expect("open temp database");
    sqlx::raw_sql(include_str!("../../ica-infra/schema/ica-baseline.sql"))
        .execute(&pool)
        .await
        .expect("apply baseline schema");
    let insert = "INSERT INTO candidates (id, name, source, position_id, status, archived, created_at, updated_at) VALUES ('c-1','Alice','boss',NULL,'active',0,'2026-11-10 09:00:00.000000','2026-11-10 09:00:00.000000')";
    sqlx::query(insert)
        .execute(&pool)
        .await
        .expect("insert candidate");
    pool.close().await;

    let state = DesktopState::connect(
        &format!("sqlite:///{}", path.display()),
        DesktopConfig {
            app_name: "ICA Personal".to_string(),
            ai_enabled: false,
            ocr_upload_dir: dir.path().join("ocr"),
        },
    )
    .await
    .expect("assemble desktop state");
    (state, dir)
}

/// Two records, no timestamps: the parser's simplest non-empty shape.
const PASTED: &str = "候选人：你好，我对这个岗位感兴趣\n我：您好，请问方便聊聊吗";

fn preview_payload() -> serde_json::Value {
    json!({ "candidate_id": "c-1", "raw_text": PASTED })
}

#[tokio::test]
async fn preview_then_confirm_then_reject_round_trip() {
    let (state, _dir) = desktop_app().await;

    let session = commands::imports_preview(&state, &preview_payload())
        .await
        .expect("preview");
    assert_eq!(session.status, "previewed");
    assert!(!session.records.is_empty(), "the parser produced records");
    let session_id = session.id.clone();

    let confirmed =
        commands::imports_confirm(&state, &session_id, &json!({ "candidate_id": "c-1" }))
            .await
            .expect("confirm");
    assert!(!confirmed.saved_message_ids.is_empty());
    assert_eq!(confirmed.session_id, session_id);

    let fetched = commands::imports_get(&state, &session_id)
        .await
        .expect("get");
    assert_eq!(fetched.status, "confirmed");
}

#[tokio::test]
async fn confirm_twice_is_a_conflict_and_the_envelope_is_the_http_one() {
    let (state, _dir) = desktop_app().await;

    let session = commands::imports_preview(&state, &preview_payload())
        .await
        .expect("preview");
    let session_id = session.id.clone();

    commands::imports_confirm(&state, &session_id, &json!({ "candidate_id": "c-1" }))
        .await
        .expect("first confirm");

    let error = commands::imports_confirm(&state, &session_id, &json!({ "candidate_id": "c-1" }))
        .await
        .expect_err("second confirm must be refused");

    // IPC carries no status code — the envelope is code/message/details, the
    // same shape the HTTP side emits.
    assert_eq!(error.error.code, "business_rule_violation");
    let rendered = serde_json::to_value(&error).expect("serialise");
    assert_eq!(rendered["error"]["code"], "business_rule_violation");
}

#[tokio::test]
async fn reject_marks_the_session() {
    let (state, _dir) = desktop_app().await;
    let session = commands::imports_preview(&state, &preview_payload())
        .await
        .expect("preview");
    let session_id = session.id.clone();

    let rejected = commands::imports_reject(&state, &session_id)
        .await
        .expect("reject");
    assert_eq!(rejected.status, "rejected");
}

#[tokio::test]
async fn an_unknown_session_is_a_404_with_the_baseline_message() {
    let (state, _dir) = desktop_app().await;
    let error = commands::imports_get(&state, "nope")
        .await
        .expect_err("unknown session");
    assert_eq!(error.error.code, "not_found");
    assert_eq!(error.error.message, "Import session 'nope' was not found");
}

#[tokio::test]
async fn export_is_the_baseline_document_shape() {
    let (state, _dir) = desktop_app().await;
    let exported = commands::export_data(&state, None).await.expect("export");

    assert_eq!(exported["version"], "0.2.0");
    let exported_at = exported["exported_at"].as_str().unwrap();
    assert!(
        exported_at.ends_with("+00:00"),
        "isoformat() carries +00:00: {exported_at}"
    );
    assert!(exported["data"].is_object());
    assert!(exported["data"].get("candidates").is_some());
}

#[tokio::test]
async fn export_with_a_key_returns_the_sealed_envelope() {
    let (state, _dir) = desktop_app().await;
    let key = base64_of_32_bytes();
    let exported = commands::export_data(&state, Some(&key))
        .await
        .expect("export");

    assert_eq!(exported["alg"], "A256GCM");
    assert!(exported.get("data").is_none(), "no plaintext alongside");
    assert_eq!(base64_len(exported["iv"].as_str().unwrap()), 12);
    assert_eq!(base64_len(exported["tag"].as_str().unwrap()), 16);
}

#[tokio::test]
async fn export_with_an_unusable_key_is_a_422() {
    let (state, _dir) = desktop_app().await;
    let error = commands::export_data(&state, Some("not-a-valid-key"))
        .await
        .expect_err("unusable key");
    assert_eq!(error.error.code, "invalid_encryption_key");
}

#[tokio::test]
async fn import_is_an_idempotent_upsert() {
    let (state, _dir) = desktop_app().await;
    let exported = commands::export_data(&state, None).await.expect("export");
    let payload = json!({ "data": exported["data"] });

    let first = commands::import_data(&state, &payload)
        .await
        .expect("import");
    // Re-importing a snapshot of THIS database matches every row by id with an
    // equal `updated_at` and identical data, so nothing is written: the
    // baseline answers `imported=0, updated=0, skipped=N`. (An earlier version
    // of this assertion demanded `imported>0 || updated>0`, which only held
    // because the port at the time treated "equal timestamp" as "newer" —
    // exactly the bug the LWW rewrite fixed.)
    assert_eq!(
        (first.imported, first.updated, first.conflicts),
        (0, 0, 0),
        "an unchanged snapshot writes nothing and reports no conflict"
    );
    assert!(first.skipped > 0, "every row was examined and left alone");

    let second = commands::import_data(&state, &payload)
        .await
        .expect("re-import");
    assert_eq!(second.imported, 0, "a repeated import inserts nothing new");
}

#[tokio::test]
async fn ocr_refuses_a_path_outside_the_staging_directory() {
    let (state, _dir) = desktop_app().await;
    let error = commands::imports_ocr(
        &state,
        &json!({ "candidate_id": "c-1", "image_path": "/etc/passwd" }),
    )
    .await
    .expect_err("out-of-directory path");
    assert_eq!(error.error.code, "ocr_path_not_allowed");
}

#[tokio::test]
async fn ocr_inside_the_staging_directory_reports_an_undecodable_image() {
    let (state, dir) = desktop_app().await;
    let staging = dir.path().join("ocr");
    std::fs::create_dir_all(&staging).expect("create staging");
    let image = staging.join("w4-probe.png");
    std::fs::write(&image, b"not really a png").expect("write probe");

    let payload = json!({ "candidate_id": "c-1", "image_path": image.display().to_string() });
    let error = commands::imports_ocr(&state, &payload)
        .await
        .expect_err("not an image");
    std::fs::remove_file(&image).ok();

    // The engine needs its models; they live next to the baseline's Python
    // install (the port runs the SAME RapidOCR model set). Without them the
    // engine must still answer with a READABLE error rather than a 500 or a
    // panic.
    if ocr_models_available() {
        assert_eq!(error.error.code, "ocr_image_undecodable");
    } else {
        assert_eq!(error.error.code, "ocr_unavailable");
    }
}

/// Whether the OCR models are present on this machine.
fn ocr_models_available() -> bool {
    let dir = std::path::Path::new(
        "/mnt/o/ica/backend/.venv/lib/python3.12/site-packages/rapidocr/models",
    );
    dir.join("PP-OCRv6_det_small.onnx").is_file() && dir.join("PP-OCRv6_rec_small.onnx").is_file()
}

/// 32 bytes of key material, standard base64.
fn base64_of_32_bytes() -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode([9u8; 32])
}

/// Decoded length of a base64 string.
fn base64_len(encoded: &str) -> usize {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("base64")
        .len()
}
