//! Regressions for the findings of the SECOND independent W4 audit
//! (`docs/audits/w4-independent/FINDINGS-REPORT.md` + `LWW-REPORT.md`,
//! 2026-11-19). The first audit round proved that a green suite can hide a
//! real divergence ("假绿"), so every test here names the audit item it pins
//! and asserts the baseline's exact value — not just "not a 500".
//!
//! Covered audit items:
//!
//! * **P0-4 / LWW P1-1** — the import validator enforces the baseline's
//!   `_position_problems` rules, and `details.errors` is an ARRAY of
//!   row problems, not a JSON string;
//! * **N-1** — `"id": null` mints a UUID (was a 500);
//! * **N-2 / LWW P2-4** — `"id": ""` is "no id" and mints a UUID (was an
//!   empty-string primary key);
//! * **N-3** — a corrected content is hashed with its FINAL text (the
//!   preview hash of the original text used to be kept);
//! * **LWW P2-1** — a bare-date `updated_at` is midnight UTC and WINS the
//!   LWW race (the row used to be skipped and reported as a conflict);
//! * **LWW P2-2** — a numeric `0`/`1` for a boolean column compares equal to
//!   `false`/`true` (Python's `0 == False`), so it is not a phantom conflict;
//! * **LWW P2-3** — a snapshot may rewrite `import_sessions.raw_text`;
//! * **observation §2.4** — imported timestamps land in the baseline's
//!   SQLite text (`2030-01-01 00:30:00.000000`), not the wire's ISO shape.

mod support;

use support::{pool_for, post, seeded_app, w1_app};

/// **P0-4**: every `_position_problems` rule the first fix missed, with the
/// baseline's exact message text, and `details.errors` as a real array.
#[tokio::test]
async fn positions_import_validates_like_the_baseline_and_lists_its_problems() {
    let (app, _dir) = seeded_app().await;

    let body = serde_json::json!({
        "data": {"positions": [
            // row 0: unknown status
            {"name": "Rule Probe", "status": "bogus"},
            // row 1: non-string location, and a non-boolean archived
            {"name": "Rule Probe", "location": 123, "archived": "yes"},
            // row 2: blank name, oversized name is checked separately below
            {"name": "   "},
            // row 3: a negative salary has its OWN message
            {"name": "Rule Probe", "salary_min": -5},
            // row 4: an explicit null status is "not in the enum"
            {"name": "Rule Probe", "status": null},
        ]}
    })
    .to_string();

    let response = post(app.clone(), "/api/import", &body).await;
    assert_eq!(response.status, 422, "body: {}", response.body);
    assert_eq!(response.body["error"]["code"], "import_validation_error");

    let errors = &response.body["error"]["details"]["errors"];
    assert!(
        errors.is_array(),
        "details.errors must be a JSON array — the baseline puts a list there \
         and a client parses it; a stringified list was audit P1-1: {}",
        response.body
    );
    let errors = errors.as_array().unwrap();

    let message_of = |field: &str, index: usize| -> String {
        errors
            .iter()
            .find(|entry| {
                entry["field"] == field
                    && entry["index"] == serde_json::json!(index)
                    && entry["table"] == "positions"
            })
            .map(|entry| entry["message"].as_str().unwrap().to_string())
            .unwrap_or_else(|| panic!("no problem for {field} at row {index}: {errors:?}"))
    };

    // The messages are contract text — byte-equal with the baseline.
    assert_eq!(
        message_of("status", 0),
        "status must be one of Closed, Draft, OnHold, Open"
    );
    assert_eq!(message_of("location", 1), "location must be a string");
    assert_eq!(message_of("archived", 1), "archived must be a boolean");
    assert_eq!(
        message_of("name", 2),
        "name is required and must be a string"
    );
    assert_eq!(
        message_of("salary_min", 3),
        "salary_min must not be negative"
    );
    assert_eq!(
        message_of("status", 4),
        "status must be one of Closed, Draft, OnHold, Open"
    );

    // A 201-character name has its own message (the first fix collapsed both
    // name failures into the "required" text).
    let too_long = "x".repeat(201);
    let body = serde_json::json!({"data": {"positions": [{"name": too_long}]}}).to_string();
    let response = post(app.clone(), "/api/import", &body).await;
    assert_eq!(response.status, 422, "body: {}", response.body);
    let errors = response.body["error"]["details"]["errors"]
        .as_array()
        .unwrap();
    assert_eq!(errors[0]["message"], "name must be at most 200 characters");
    assert_eq!(errors[0]["index"], 0);

    // And a raw SQLite dump's `archived: 0` is READABLE — the baseline
    // deliberately accepts 0/1 for a boolean column.
    let body = serde_json::json!({
        "data": {"positions": [{"name": "Bool Dump", "archived": 0}]}
    })
    .to_string();
    let response = post(app, "/api/import", &body).await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(response.body["imported"], 1, "body: {}", response.body);
}

/// **N-1**: `"id": null` is "no id" — a UUID is minted and the row is
/// imported, exactly like the baseline's `if not row_id`. The old code bound
/// the JSON null into the INSERT twice and answered 500.
#[tokio::test]
async fn an_id_of_null_mints_a_uuid_instead_of_a_500() {
    let (app, dir) = seeded_app().await;
    let pool = pool_for(&dir).await;

    let body = serde_json::json!({
        "data": {"candidates": [{"id": null, "name": "Null Id Row"}]}
    })
    .to_string();
    let response = post(app, "/api/import", &body).await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(response.body["imported"], 1, "body: {}", response.body);

    let (id, name): (String, String) =
        sqlx::query_as("SELECT id, name FROM candidates WHERE name = 'Null Id Row'")
            .fetch_one(&pool)
            .await
            .expect("row written");
    assert_eq!(name, "Null Id Row");
    // 8-4-4-4-12 hex — a minted UUID, not a 500 and not NULL.
    let dashes = id.matches('-').count();
    let hex_only = id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    assert_eq!(id.len(), 36, "the id must be a minted UUID: {id}");
    assert_eq!(dashes, 4, "UUID shape: {id}");
    assert!(hex_only, "UUID shape: {id}");
}

/// **N-2 / LWW P2-4**: `"id": ""` is ALSO "no id" — the baseline's
/// `if not row_id` treats an empty string as absent, mints a UUID, and a
/// second identical row becomes a SECOND row (no id ⇒ nothing to match). The
/// old code inserted `id = ''` and then skipped the same payload as a match.
#[tokio::test]
async fn an_empty_string_id_is_treated_as_no_id() {
    let (app, dir) = seeded_app().await;
    let pool = pool_for(&dir).await;

    let payload = serde_json::json!({
        "data": {"candidates": [{"id": "", "name": "Empty Id"}]}
    })
    .to_string();

    let first = post(app.clone(), "/api/import", &payload).await;
    assert_eq!(first.status, 200, "body: {}", first.body);
    assert_eq!(first.body["imported"], 1, "body: {}", first.body);

    let second = post(app, "/api/import", &payload).await;
    assert_eq!(second.status, 200, "body: {}", second.body);
    assert_eq!(
        second.body["imported"], 1,
        "an empty id cannot match anything, so a re-send imports again: {}",
        second.body
    );

    let (empty_ids, total): (i64, i64) =
        sqlx::query_as("SELECT COUNT(*) FILTER (WHERE id = ''), COUNT(*) FROM candidates")
            .fetch_one(&pool)
            .await
            .expect("count rows");
    assert_eq!(empty_ids, 0, "no empty-string primary key may exist");
    assert_eq!(total, 5, "3 seeded + 2 minted rows");
}

/// **N-3**: a correction that rewrites the content must hash the FINAL text.
/// `messages.content_hash` is what the next preview's duplicate check matches
/// on, so keeping the preview-time hash of the ORIGINAL text silently broke
/// deduplication for the rewritten message.
#[tokio::test]
async fn a_corrected_content_gets_a_hash_that_matches_its_text() {
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;

    let preview = post(
        app.clone(),
        "/api/imports/preview",
        r#"{"candidate_id":"c-1","raw_text":"候选人：原文内容\n我：第二条"}"#,
    )
    .await;
    assert_eq!(preview.status, 200, "body: {}", preview.body);
    let session_id = preview.body["id"].as_str().unwrap().to_string();
    let records = preview.body["records"].as_array().unwrap();
    let first_record = records[0]["id"].as_str().unwrap().to_string();

    let body = serde_json::json!({
        "candidate_id": "c-1",
        "records": [{"id": first_record, "content": "改写后的内容"}]
    })
    .to_string();
    let confirmed = post(app, &format!("/api/imports/{session_id}/confirm"), &body).await;
    assert_eq!(confirmed.status, 200, "body: {}", confirmed.body);

    let (content, hash): (String, String) = sqlx::query_as(
        "SELECT content, content_hash FROM messages \
         WHERE import_record_id = ?",
    )
    .bind(&first_record)
    .fetch_one(&pool)
    .await
    .expect("saved message");
    assert_eq!(content, "改写后的内容");
    assert_eq!(
        hash,
        ica_domain::content_hash("改写后的内容"),
        "the stored hash must describe the SAVED text, not the previewed one"
    );
}

/// **LWW P2-1**: `updated_at: "2033-01-01"` is a legal naive datetime in the
/// baseline (`datetime.fromisoformat`), so the row WINS the race. The old
/// parser rejected bare dates and fell into the "no provenance" branch —
/// discarding the update and reporting a phantom conflict.
#[tokio::test]
async fn a_bare_date_updated_at_still_wins_the_lww_race() {
    let (app, dir) = seeded_app().await;
    let pool = pool_for(&dir).await;

    let body = serde_json::json!({
        "data": {"candidates": [
            {"id": "c-1", "name": "Bare Date Win", "updated_at": "2033-01-01"}
        ]}
    })
    .to_string();
    let response = post(app, "/api/import", &body).await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(
        response.body["updated"], 1,
        "a date-only timestamp carries provenance and wins: {}",
        response.body
    );
    assert_eq!(response.body["conflicts"], 0, "body: {}", response.body);

    let name: String = sqlx::query_scalar("SELECT name FROM candidates WHERE id = 'c-1'")
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(name, "Bare Date Win", "the update must be applied");
}

/// **LWW P2-2**: a numeric `0` for a boolean column equals `false` in Python
/// (`0 == False`), so it is NOT a conflict. The old `Value` comparison made
/// `Number(0) != Bool(false)` and reported one.
#[tokio::test]
async fn a_numeric_boolean_does_not_report_a_phantom_conflict() {
    let (app, _dir) = seeded_app().await;

    // `c-1` is stored with `archived = 0`. The payload repeats it as a number
    // with a newer timestamp: identical business data ⇒ plain skip.
    let body = serde_json::json!({
        "data": {"candidates": [
            {"id": "c-1", "archived": 0, "updated_at": "2033-01-01"}
        ]}
    })
    .to_string();
    let response = post(app, "/api/import", &body).await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(response.body["updated"], 1, "body: {}", response.body);
    assert_eq!(
        response.body["conflicts"], 0,
        "0 == false in the baseline's comparison: {}",
        response.body
    );
}

/// **LWW P2-3**: the baseline writes any model column the payload mentions —
/// including `import_sessions.raw_text`, which is merely EXCLUDED from
/// exports. The old import ignored it while still answering `updated: 1`.
#[tokio::test]
async fn an_import_can_rewrite_a_sessions_raw_text() {
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;

    let preview = post(
        app.clone(),
        "/api/imports/preview",
        r#"{"candidate_id":"c-1","raw_text":"候选人：种子原文\n我：回复"}"#,
    )
    .await;
    let session_id = preview.body["id"].as_str().unwrap().to_string();

    let body = serde_json::json!({
        "data": {"import_sessions": [
            {"id": session_id, "raw_text": "NEW RAW TEXT FROM SNAPSHOT",
             "updated_at": "2037-01-01"}
        ]}
    })
    .to_string();
    let response = post(app, "/api/import", &body).await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(response.body["updated"], 1, "body: {}", response.body);

    let raw_text: String = sqlx::query_scalar("SELECT raw_text FROM import_sessions WHERE id = ?")
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(
        raw_text, "NEW RAW TEXT FROM SNAPSHOT",
        "the baseline writes a mentioned raw_text even though exports exclude it"
    );
}

/// **Observation (LWW §2.4)**: a timestamp the snapshot carries in wire shape
/// (`+08:00` offset, `T` separator) is stored the way the baseline's
/// SQLAlchemy binding stores it — naive UTC text with fixed microseconds.
/// Mixed formats inside one column make SQLite's text `ORDER BY`/`MAX`
/// disagree with chronological order.
#[tokio::test]
async fn imported_timestamps_land_in_the_baselines_sqlite_text() {
    let (app, dir) = seeded_app().await;
    let pool = pool_for(&dir).await;

    let body = serde_json::json!({
        "data": {"candidates": [
            {"id": "c-2", "name": "TZ2", "updated_at": "2030-01-01T00:30:00+08:00"}
        ]}
    })
    .to_string();
    let response = post(app, "/api/import", &body).await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(response.body["updated"], 1, "body: {}", response.body);

    let stored: String = sqlx::query_scalar("SELECT updated_at FROM candidates WHERE id = 'c-2'")
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(
        stored, "2029-12-31 16:30:00.000000",
        "the baseline normalises to naive UTC SQLite text: {stored}"
    );
}
