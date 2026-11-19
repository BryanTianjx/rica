//! Regressions for the W4 differences the cross-language diff found.
//!
//! Each of these was a REAL divergence from the Python baseline that the HTTP
//! tests did not catch — they were found by `scripts/contract_verify.sh` and
//! only by it. Adding them here is what makes the fix stick: mutation testing
//! confirmed that removing each fix left the whole HTTP suite green.
//!
//! * confirming an import must blank `raw_text` — the verbatim page is the
//!   richest PII payload in the database and nothing reads it after
//!   confirmation. The baseline drops it in the same transaction;
//! * `import.confirmed` must carry its metadata, in the baseline's key order.

mod support;

use support::{post, w1_app};

const PASTED: &str = "候选人：你好，我对这个岗位感兴趣\n我：您好，请问方便聊聊吗";

fn preview_body() -> String {
    serde_json::json!({ "candidate_id": "c-1", "raw_text": PASTED }).to_string()
}

/// Confirming blanks `raw_text`.
///
/// This is a privacy rule, not a cosmetic one: the pasted page is the most
/// sensitive row in the database, and the saved messages come from the parsed
/// records — so the page itself has no reader after confirmation.
///
/// Mutation check: deleting `raw_text = ''` from the confirm `UPDATE` leaves
/// every other test in this crate green. That is exactly why this test exists.
#[tokio::test]
async fn confirming_blanks_the_raw_text() {
    let (app, dir) = w1_app().await;
    let pool = support::pool_for(&dir).await;

    let preview = post(app.clone(), "/api/imports/preview", &preview_body()).await;
    assert_eq!(preview.status, 200, "body: {}", preview.body);
    let session_id = preview.body["id"].as_str().unwrap().to_string();

    // Before confirmation the page is preserved, because confirm reads it.
    let before: String = sqlx::query_scalar("SELECT raw_text FROM import_sessions WHERE id = ?")
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .expect("read raw_text");
    assert_eq!(
        before, PASTED,
        "preview must keep the page it is previewing"
    );

    let confirmed = post(
        app,
        &format!("/api/imports/{session_id}/confirm"),
        r#"{"candidate_id":"c-1"}"#,
    )
    .await;
    assert_eq!(confirmed.status, 200, "body: {}", confirmed.body);

    let after: String = sqlx::query_scalar("SELECT raw_text FROM import_sessions WHERE id = ?")
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .expect("read raw_text");
    assert_eq!(
        after, "",
        "confirming must drop the verbatim page (it is the richest PII payload \
         and nothing reads it afterwards)"
    );
}

/// `import.confirmed` records `{saved, skipped, duplicates}` — in that order.
///
/// The metadata is a TEXT column, so its BYTES are contract: the baseline uses
/// `json.dumps` with the default separators and does NOT sort the keys. A
/// `BTreeMap`-ordered object, or a compact rendering, would produce a different
/// string even though it parses to the same value.
///
/// Mutation check: dropping `.with_metadata(...)` leaves every other test in
/// this crate green.
#[tokio::test]
async fn confirming_records_its_audit_metadata_byte_for_byte() {
    let (app, dir) = w1_app().await;
    let pool = support::pool_for(&dir).await;

    let preview = post(app.clone(), "/api/imports/preview", &preview_body()).await;
    let session_id = preview.body["id"].as_str().unwrap().to_string();
    let parsed = preview.body["records"].as_array().unwrap().len();

    let confirmed = post(
        app,
        &format!("/api/imports/{session_id}/confirm"),
        r#"{"candidate_id":"c-1"}"#,
    )
    .await;
    assert_eq!(confirmed.status, 200, "body: {}", confirmed.body);

    let metadata: Option<String> = sqlx::query_scalar(
        "SELECT metadata_json FROM audit_events WHERE event_type = 'import.confirmed'",
    )
    .fetch_one(&pool)
    .await
    .expect("read audit metadata");

    let metadata = metadata.expect("import.confirmed must carry metadata");
    // Exact bytes: default Python separators, insertion order.
    assert_eq!(
        metadata,
        format!(r#"{{"saved": {parsed}, "skipped": 0, "duplicates": 0}}"#),
        "audit metadata is a TEXT column — its bytes are contract"
    );
}

/// `data.exported` records the per-table row counts in DECLARATION order.
///
/// The baseline builds this map with a dict comprehension over its table list,
/// so the keys come out in `_BUSINESS_TABLES` order — not alphabetically. This
/// only bites the ENCRYPTED export, because that is the branch that used to
/// carry a sorted map.
#[tokio::test]
async fn the_encrypted_export_audit_keeps_the_declaration_order() {
    let (app, _dir) = support::seeded_app().await;

    let key = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([9u8; 32])
    };
    let response = support::post_with_headers(
        app,
        "/api/export",
        "",
        &[("x-encrypt-aes-key", key.as_str())],
    )
    .await;
    assert_eq!(response.status, 200, "body: {}", response.body);

    let pool = support::pool_for(&_dir).await;
    let metadata: Option<String> = sqlx::query_scalar(
        "SELECT metadata_json FROM audit_events WHERE event_type = 'data.exported'",
    )
    .fetch_one(&pool)
    .await
    .expect("read audit metadata");
    let metadata = metadata.expect("data.exported must carry metadata");

    // `candidates` comes before `positions` in the baseline's table list;
    // alphabetically `audit_events` would come first.
    let candidates_at = metadata.find("\"candidates\"").expect("candidates key");
    let ai_at = metadata.find("\"ai_model_configs\"");
    assert!(
        ai_at.is_none() || candidates_at < ai_at.unwrap(),
        "the table counts must follow the baseline's declaration order, \
         not alphabetical order: {metadata}"
    );
    assert!(
        metadata.contains("\"encrypted\": true"),
        "an encrypted export must record that it was encrypted: {metadata}"
    );
}
