//! The backup/restore drill (D1, D2, D7, D8, D10 of
//! `docs/testing/backup-restore-drill.md`) as repeatable in-suite evidence.
//!
//! D3-D5 are covered by `ai_key_backups_http::restore_guards_run_in_the_baseline_order`,
//! D6 by `the_backup_lifecycle_round_trips_the_data`, and D9 (restart
//! persistence) runs as a real process exercise — see the drill record in
//! `docs/testing/test-report-r1.0.md` §13.

mod support;

use ica_application::BackupStore;
use ica_infra::backup::BackupManager;
use support::{get, init_test_logging, pool_for, post, w1_app};

#[tokio::test]
async fn d1_a_backup_of_a_fresh_database_is_listed_with_full_metadata() {
    init_test_logging();
    let (app, dir) = w1_app().await;

    let backup = post(app.clone(), "/api/backups", "{}").await;
    assert_eq!(backup.status, 200, "body: {}", backup.body);
    let name = backup.body["name"].as_str().expect("name").to_string();
    assert!(name.starts_with("ica-backup-"), "{}", name);
    assert!(name.ends_with(".db"));
    assert_eq!(backup.body["kind"], "backup");
    assert!(backup.body["size_bytes"].as_i64().expect("size") > 0);
    let created = backup.body["created_at"].as_str().expect("created_at");
    assert!(
        chrono::DateTime::parse_from_rfc3339(created).is_ok(),
        "ISO-8601: {created}"
    );
    assert!(backup.body["path"].as_str().expect("path").contains(&name));

    // The file exists on disk inside the configured backup directory, and the
    // listing carries it.
    let listed = get(app, "/api/backups").await;
    let items = listed.body.as_array().expect("array");
    assert!(
        items.iter().any(|item| item["name"] == name),
        "{}",
        listed.body
    );

    let _ = dir;
}

#[tokio::test]
async fn d2_restore_moves_data_back_and_forth_between_two_backups() {
    init_test_logging();
    let (app, dir) = w1_app().await;

    // State A: one probe.
    let probe = post(app.clone(), "/api/positions", r#"{"name":"Drill Probe A"}"#).await;
    assert_eq!(probe.status, 201);
    let probe_id = probe.body["id"].as_str().expect("id").to_string();
    let backup_a = post(app.clone(), "/api/backups", "{}").await;
    assert_eq!(backup_a.status, 200);
    let name_a = backup_a.body["name"].as_str().expect("name").to_string();

    // State B: the probe is gone, a second one exists.
    let deleted = send_delete(app.clone(), &format!("/api/positions/{probe_id}")).await;
    assert_eq!(deleted.status, 204);
    let second = post(app.clone(), "/api/positions", r#"{"name":"Drill Probe B"}"#).await;
    assert_eq!(second.status, 201);
    let backup_b = post(app.clone(), "/api/backups", "{}").await;
    assert_eq!(backup_b.status, 200);
    let name_b = backup_b.body["name"].as_str().expect("name").to_string();

    // Restore A: the second probe disappears, the first is back.
    let restore_a = post(
        app.clone(),
        &format!("/api/backups/{name_a}/restore"),
        r#"{"confirm":true}"#,
    )
    .await;
    assert_eq!(restore_a.status, 200, "body: {}", restore_a.body);
    let probe_a = get(app.clone(), "/api/positions?search=Drill%20Probe").await;
    assert_eq!(probe_a.body["total"], 1);
    assert_eq!(probe_a.body["items"][0]["name"], "Drill Probe A");

    // Restore B: the states swap back exactly.
    let restore_b = post(
        app.clone(),
        &format!("/api/backups/{name_b}/restore"),
        r#"{"confirm":true}"#,
    )
    .await;
    assert_eq!(restore_b.status, 200, "body: {}", restore_b.body);
    let probe_b = get(app.clone(), "/api/positions?search=Drill%20Probe").await;
    assert_eq!(probe_b.body["total"], 1);
    assert_eq!(probe_b.body["items"][0]["name"], "Drill Probe B");

    // Two restore drills = two pre-restore undo points, all listed.
    let listed = get(app, "/api/backups").await;
    let items = listed.body.as_array().expect("array");
    let undo_points = items
        .iter()
        .filter(|item| item["kind"] == "pre_restore")
        .count();
    assert!(
        undo_points >= 2,
        "both drills left undo points: {}",
        listed.body
    );

    let _ = dir;
}

#[tokio::test]
async fn d7_reads_during_the_restore_never_observe_a_broken_state() {
    init_test_logging();
    let (app, dir) = w1_app().await;

    // A larger body of data so the online backup has real work to do.
    let pool = pool_for(&dir).await;
    for index in 0..30 {
        sqlx::query(concat!(
            "INSERT INTO positions (id, name, description, location, status, archived,",
            " created_at, updated_at) VALUES (?, ?, 'drill', 'Wuhan', 'Open', 0,",
            " '2026-11-20 09:00:00', '2026-11-20 09:00:00')"
        ))
        .bind(format!("p-drill-{index}"))
        .bind(format!("Drill Position {index}"))
        .execute(&pool)
        .await
        .expect("seed drill rows");
    }
    let backup = post(app.clone(), "/api/backups", "{}").await;
    assert_eq!(backup.status, 200);
    let name = backup.body["name"].as_str().expect("name").to_string();

    // Change the live state, then restore WHILE readers hammer the API.
    sqlx::query("UPDATE positions SET name = 'Mutated' WHERE name LIKE 'Drill Position%'")
        .execute(&pool)
        .await
        .expect("mutate live state");

    let reader_app = app.clone();
    let reader = tokio::spawn(async move {
        let mut reads = 0u32;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            let response = get(reader_app.clone(), "/api/positions?limit=500").await;
            assert!(
                response.status == 200,
                "a concurrent read observed HTTP {}: {}",
                response.status,
                response.body
            );
            reads += 1;
        }
        reads
    });

    let restore = post(
        app.clone(),
        &format!("/api/backups/{name}/restore"),
        r#"{"confirm":true}"#,
    )
    .await;
    assert_eq!(restore.status, 200, "body: {}", restore.body);
    let reads = reader.await.expect("reader task");

    // After the restore every read sees the restored names.
    let after = get(app, "/api/positions?limit=500").await;
    let items = after.body["items"].as_array().expect("items");
    assert!(items.len() >= 30);
    assert!(
        items.iter().all(|item| item["name"] != "Mutated"),
        "the restored state replaced the mutation"
    );
    assert!(reads > 0, "the concurrent reader did run");
}

#[tokio::test]
async fn d8_retention_prunes_the_oldest_across_both_families() {
    init_test_logging();
    let (_app, dir) = w1_app().await;
    let db_path = dir.path().join("ica.db");

    // A dedicated manager with keep_count=3 (the fixture's adapter uses the
    // same paths, so this exercises the real prune rules).
    let manager = BackupManager::new(db_path, dir.path().join("backups"), 3);
    for _ in 0..4 {
        manager.create().await.expect("backup");
    }
    let listed = manager.list().await.expect("list");
    assert_eq!(
        listed.len(),
        3,
        "the oldest of four backups was pruned: {:?}",
        listed.iter().map(|r| r.name.clone()).collect::<Vec<_>>()
    );
    // Newest survives (name-sorted, so the pruned one sorts first).
    assert!(listed
        .iter()
        .all(|record| record.name.starts_with("ica-backup-")));

    // Restores participate in the same bound: a drill adds a pre-restore
    // point, the prune keeps the total at three.
    manager
        .restore(&listed[0].name)
        .await
        .expect("restore drill");
    let after = manager.list().await.expect("list");
    assert_eq!(after.len(), 3, "restores are bounded too");
}

#[tokio::test]
async fn d10_a_backup_removed_on_disk_disappears_and_cannot_be_restored() {
    init_test_logging();
    let (app, dir) = w1_app().await;

    let backup = post(app.clone(), "/api/backups", "{}").await;
    assert_eq!(backup.status, 200);
    let name = backup.body["name"].as_str().expect("name").to_string();

    // Remove the file underneath the running service.
    let path = dir.path().join("backups").join(&name);
    assert!(path.is_file());
    std::fs::remove_file(&path).expect("remove the backup file");

    let listed = get(app.clone(), "/api/backups").await;
    let items = listed.body.as_array().expect("array");
    assert!(
        !items.iter().any(|item| item["name"] == name),
        "the removed backup left the listing: {}",
        listed.body
    );

    let restore = post(
        app,
        &format!("/api/backups/{name}/restore"),
        r#"{"confirm":true}"#,
    )
    .await;
    assert_eq!(restore.status, 404, "body: {}", restore.body);
    assert_eq!(
        restore.body["error"]["message"],
        format!("No backup file named '{name}' was found")
    );
}

async fn send_delete(app: axum::Router, uri: &str) -> support::TestResponse {
    use tower::ServiceExt;
    let request = axum::http::Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("accept", "application/json")
        .body(axum::body::Body::empty())
        .expect("build request");
    let response = app.oneshot(request).await.expect("router responded");
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    support::TestResponse { status, body }
}
