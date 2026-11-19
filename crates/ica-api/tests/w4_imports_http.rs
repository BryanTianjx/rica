//! W4 HTTP behaviour: imports, export, import, OCR.
//!
//! These tests exist because the contract is behavioural, not structural. A
//! route that compiles and returns 200 can still be wrong in ways only the
//! database or the wire shows:
//!
//! * preview must persist the session and its records and must NOT write a
//!   single `messages` row (the earlier task draft claimed preview was
//!   "zero-write", which the baseline contradicts);
//! * confirm must write exactly the records that are neither skipped nor
//!   unflagged duplicates, and must not write anything twice;
//! * reject must clear `raw_text` while leaving the records;
//! * export must carry `version: "0.2.0"` and an `isoformat()` timestamp, and
//!   must NOT leak `import_sessions.raw_text`;
//! * a sealed export must be the AES-256-GCM envelope, never plaintext.

mod support;

use support::{count, get, post, seeded_app, seeded_w1_database, w1_app};

/// Two records, no timestamps: the parser's simplest non-empty shape.
const PASTED: &str = "候选人：你好，我对这个岗位感兴趣\n我：您好，请问方便聊聊吗";

/// A preview body built with `serde_json` rather than `format!`: the pasted
/// text contains newlines, and a raw newline inside a JSON string is illegal —
/// the server correctly answers 422 `json_invalid` for one, which is a fine
/// behaviour but not what these tests are about.
fn preview_body() -> String {
    serde_json::json!({ "candidate_id": "c-1", "raw_text": PASTED }).to_string()
}

fn json(body: &serde_json::Value, key: &str) -> serde_json::Value {
    body.get(key).cloned().unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn preview_persists_the_session_and_records_but_never_a_message() {
    let (app, dir) = w1_app().await;
    let pool = support::pool_for(&dir).await;

    let messages_before = count(&pool, "SELECT COUNT(*) FROM messages", &[]).await;
    let sessions_before = count(&pool, "SELECT COUNT(*) FROM import_sessions", &[]).await;

    let response = post(app.clone(), "/api/imports/preview", &preview_body()).await;

    assert_eq!(response.status, 200, "body: {}", response.body);

    // 200, not 201: the baseline declares no status_code for this route.
    let session_id = json(&response.body, "id").as_str().unwrap().to_string();
    let records = json(&response.body, "records");
    assert!(
        records
            .as_array()
            .map(|rows| !rows.is_empty())
            .unwrap_or(false),
        "the parser should have produced at least one record: {}",
        response.body
    );

    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM import_sessions", &[]).await,
        sessions_before + 1,
        "preview persists the session"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM import_records WHERE session_id = ?",
            &[&session_id]
        )
        .await,
        records.as_array().unwrap().len() as i64,
        "preview persists one row per parsed record"
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM messages", &[]).await,
        messages_before,
        "preview must NOT write a single message — that is confirm's job"
    );
}

#[tokio::test]
async fn preview_rejects_an_unknown_candidate() {
    let (app, _dir) = w1_app().await;
    let response = post(
        app,
        "/api/imports/preview",
        r#"{"candidate_id":"nope","raw_text":"候选人：你好"}"#,
    )
    .await;
    assert_eq!(response.status, 404, "body: {}", response.body);
}

#[tokio::test]
async fn get_reports_the_baseline_message_for_an_unknown_session() {
    let (app, _dir) = w1_app().await;
    let response = get(app, "/api/imports/does-not-exist").await;
    assert_eq!(response.status, 404);
    assert_eq!(
        json(&response.body, "error")["message"],
        "Import session 'does-not-exist' was not found"
    );
}

#[tokio::test]
async fn confirm_writes_one_message_per_record_and_stamps_the_session() {
    let (app, dir) = w1_app().await;
    let pool = support::pool_for(&dir).await;

    let preview = post(app.clone(), "/api/imports/preview", &preview_body()).await;
    let session_id = json(&preview.body, "id").as_str().unwrap().to_string();
    let parsed = json(&preview.body, "records").as_array().unwrap().len() as i64;

    let before = count(&pool, "SELECT COUNT(*) FROM messages", &[]).await;
    let response = post(
        app.clone(),
        &format!("/api/imports/{session_id}/confirm"),
        r#"{"candidate_id":"c-1"}"#,
    )
    .await;
    assert_eq!(response.status, 200, "body: {}", response.body);

    let saved = json(&response.body, "saved_message_ids")
        .as_array()
        .unwrap()
        .len() as i64;
    assert_eq!(saved, parsed, "every parsed record becomes a message");
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM messages", &[]).await,
        before + saved
    );
    let status: String = sqlx::query_scalar("SELECT status FROM import_sessions WHERE id = ?")
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "confirmed");
}

#[tokio::test]
async fn confirm_twice_is_a_conflict_and_writes_nothing_more() {
    let (app, dir) = w1_app().await;
    let pool = support::pool_for(&dir).await;

    let preview = post(app.clone(), "/api/imports/preview", &preview_body()).await;
    let session_id = json(&preview.body, "id").as_str().unwrap().to_string();

    let first = post(
        app.clone(),
        &format!("/api/imports/{session_id}/confirm"),
        r#"{"candidate_id":"c-1"}"#,
    )
    .await;
    assert_eq!(first.status, 200);

    let after_first = count(&pool, "SELECT COUNT(*) FROM messages", &[]).await;
    let second = post(
        app,
        &format!("/api/imports/{session_id}/confirm"),
        r#"{"candidate_id":"c-1"}"#,
    )
    .await;
    assert_eq!(second.status, 409, "body: {}", second.body);
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM messages", &[]).await,
        after_first,
        "a refused confirm must not write messages"
    );
}

#[tokio::test]
async fn reject_marks_the_session_and_blanks_the_raw_text() {
    let (app, dir) = w1_app().await;
    let pool = support::pool_for(&dir).await;

    let preview = post(app.clone(), "/api/imports/preview", &preview_body()).await;
    let session_id = json(&preview.body, "id").as_str().unwrap().to_string();

    let response = post(app, &format!("/api/imports/{session_id}/reject"), "").await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(json(&response.body, "status"), "rejected");

    let raw: String = sqlx::query_scalar("SELECT raw_text FROM import_sessions WHERE id = ?")
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw, "", "rejecting clears the pasted page");
    assert!(
        count(
            &pool,
            "SELECT COUNT(*) FROM import_records WHERE session_id = ?",
            &[&session_id]
        )
        .await
            > 0,
        "the records stay, which is why a rejected session can still be confirmed"
    );
}

#[tokio::test]
async fn export_is_the_baseline_document_shape_and_hides_raw_text() {
    let (app, _dir) = seeded_app().await;
    let response = post(app, "/api/export", "").await;
    assert_eq!(response.status, 200, "body: {}", response.body);

    assert_eq!(json(&response.body, "version"), "0.2.0");
    let exported_at = json(&response.body, "exported_at")
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        exported_at.ends_with("+00:00"),
        "isoformat() carries +00:00, not the wire `Z`: {exported_at}"
    );

    let data = json(&response.body, "data");
    assert!(data.is_object(), "data is a table map: {}", response.body);
    assert!(data.get("candidates").is_some());
    assert!(data.get("messages").is_some());

    // The privacy rule: import_sessions is exported WITHOUT raw_text.
    if let Some(sessions) = data.get("import_sessions").and_then(|v| v.as_array()) {
        for session in sessions {
            assert!(
                session.get("raw_text").is_none(),
                "export must not carry import_sessions.raw_text: {session}"
            );
        }
    }
}

#[tokio::test]
async fn export_with_a_key_returns_the_sealed_envelope_not_plaintext() {
    let (app, _dir) = seeded_app().await;
    let key = base64_of_32_bytes();

    let response = support::post_with_headers(
        app,
        "/api/export",
        "",
        &[("x-encrypt-aes-key", key.as_str())],
    )
    .await;
    assert_eq!(response.status, 200, "body: {}", response.body);

    assert_eq!(json(&response.body, "alg"), "A256GCM");
    let iv = json(&response.body, "iv");
    let tag = json(&response.body, "tag");
    assert_eq!(base64_len(iv.as_str().unwrap()), 12, "96-bit GCM nonce");
    assert_eq!(base64_len(tag.as_str().unwrap()), 16, "128-bit tag");
    assert!(
        response.body.get("data").is_none(),
        "a sealed export must never also carry the plaintext: {}",
        response.body
    );
}

#[tokio::test]
async fn export_with_an_unusable_key_is_a_422_with_the_baseline_code() {
    let (app, _dir) = seeded_app().await;
    let response = support::post_with_headers(
        app,
        "/api/export",
        "",
        &[("x-encrypt-aes-key", "not-a-valid-key")],
    )
    .await;
    assert_eq!(response.status, 422, "body: {}", response.body);
    assert_eq!(
        json(&response.body, "error")["code"],
        "invalid_encryption_key"
    );
}

#[tokio::test]
async fn import_is_an_idempotent_upsert() {
    let (app, dir) = seeded_app().await;
    let pool = support::pool_for(&dir).await;

    let exported = post(app.clone(), "/api/export", "").await;
    let payload = serde_json::json!({ "data": json(&exported.body, "data") }).to_string();

    let first = post(app.clone(), "/api/import", &payload).await;
    assert_eq!(first.status, 200, "body: {}", first.body);

    let candidates = count(&pool, "SELECT COUNT(*) FROM candidates", &[]).await;

    // Re-importing the same snapshot must not duplicate a single row.
    let second = post(app, "/api/import", &payload).await;
    assert_eq!(second.status, 200, "body: {}", second.body);
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM candidates", &[]).await,
        candidates,
        "a repeated import is an upsert, not an append"
    );
}

#[tokio::test]
async fn ocr_refuses_a_path_outside_the_staging_directory() {
    let (app, _dir) = w1_app().await;
    let response = post(
        app,
        "/api/imports/ocr",
        r#"{"candidate_id":"c-1","image_path":"/etc/passwd"}"#,
    )
    .await;
    assert_eq!(response.status, 409, "body: {}", response.body);
    assert_eq!(
        json(&response.body, "error")["code"],
        "ocr_path_not_allowed",
        "the guard runs before any I/O: {}",
        response.body
    );
}

#[tokio::test]
async fn ocr_inside_the_staging_directory_reports_an_undecodable_image() {
    let (app, dir) = w1_app().await;
    // The path guard passes (the file is inside the staging directory), so
    // the DECODE is what answers. A file that is not an image is a 4xx
    // `ocr_image_undecodable`, never a 500 — the baseline maps Pillow's
    // OSError the same way.
    //
    // The engine needs its models. They live next to the baseline's Python
    // install (the port runs the SAME RapidOCR model set, so the recognised
    // text matches); when they are not there, the engine reports a readable
    // 503 `ocr_unavailable` instead of panicking, and that is what this test
    // asserts — the decode path itself is covered by the contract comparison,
    // which runs both sides with the models present.
    let staging = std::path::Path::new("/tmp/ica-test-ocr");
    std::fs::create_dir_all(staging).ok();
    let image = staging.join("w4-probe.png");
    std::fs::write(&image, b"not really a png").ok();

    let body = format!(
        r#"{{"candidate_id":"c-1","image_path":"{}"}}"#,
        image.display()
    );
    let response = post(app, "/api/imports/ocr", &body).await;
    std::fs::remove_file(&image).ok();
    let _ = dir;

    let code = json(&response.body, "error")["code"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if ocr_models_available() {
        assert_eq!(
            response.status, 409,
            "with the engine present a non-image is a decode failure: {}",
            response.body
        );
        assert_eq!(code, "ocr_image_undecodable");
    } else {
        // No models on this machine: the engine must still answer with a
        // READABLE error rather than a 500 or a panic.
        assert_eq!(response.status, 503, "body: {}", response.body);
        assert_eq!(code, "ocr_unavailable");
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

/// Decoded length of a base64 string, without pulling in a decoder in the test.
fn base64_len(encoded: &str) -> usize {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("base64")
        .len()
}

/// `seeded_w1_database` is re-exported for tests that need the richer fixture.
#[allow(dead_code)]
async fn rich_database(path: &std::path::Path) {
    seeded_w1_database(path).await;
}
