//! `GET /ai/models` + routing/usage/budget (W5 AI slice).
//!
//! The contract is the baseline's read/patch endpoints: the registry falls
//! back to the V1.0 settings-built model when `ai_model_configs` is empty,
//! the routing table layers overrides over the nine-key default, and
//! `PATCH /ai/budget` writes rows the baseline never reads back (no audit)
//! while mutating the in-process pair. Model CRUD is AI-EXT — absent here.

mod support;

use support::{get, patch, pool_for, w1_app};

#[tokio::test]
async fn an_empty_registry_falls_back_to_the_v1_0_settings_model() {
    let (app, _dir) = w1_app().await;
    let response = get(app, "/api/ai/models").await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    let models = response.body["models"].as_array().expect("models");
    assert_eq!(models.len(), 1, "the fallback model: {}", response.body);
    let model = &models[0];
    assert_eq!(model["model_id"], "glm-4-flash");
    assert_eq!(model["display_name"], "glm-4-flash");
    assert_eq!(model["provider_type"], "openai_compatible");
    assert_eq!(model["base_url"], "https://open.bigmodel.cn/api/paas/v4");
    assert_eq!(model["api_key_env"], "AI_API_KEY");
    assert_eq!(model["quality_tier"], "economy");
    assert_eq!(model["max_output_tokens"], 1500);
    assert_eq!(model["max_input_tokens"], 8000);
    assert_eq!(model["timeout"], 60);
    assert_eq!(model["temperature"], 0.4);
    assert_eq!(model["enabled"], true);
    assert_eq!(model["is_default"], true);
    assert_eq!(model["sort_order"], 0);
    assert_eq!(model["source"], "settings");
    // The fixture configures no AI key, so the presence flag is false.
    assert_eq!(model["api_key_configured"], false);

    // The routing table is the nine-key default.
    let routing = &response.body["routing"];
    assert_eq!(routing["greeting"], "economy");
    assert_eq!(routing["salary_talk"], "premium");
    assert_eq!(routing.as_object().expect("object").len(), 9);
}

#[tokio::test]
async fn routing_patch_persists_overrides_and_unknowns_are_rejected() {
    let (app, dir) = w1_app().await;

    let patched = patch(
        app.clone(),
        "/api/ai/routing",
        r#"{"routing":{"salary_talk":"standard","greeting":"premium"}}"#,
    )
    .await;
    assert_eq!(patched.status, 200, "body: {}", patched.body);
    assert_eq!(patched.body["routing"]["salary_talk"], "standard");
    assert_eq!(patched.body["routing"]["greeting"], "premium");
    // Untouched keys keep their defaults.
    assert_eq!(patched.body["routing"]["reply"], "standard");

    // The override persists in `ai_model_routing`.
    let pool = pool_for(&dir).await;
    let (tier,): (String,) =
        sqlx::query_as("SELECT tier FROM ai_model_routing WHERE message_type = 'salary_talk'")
            .fetch_one(&pool)
            .await
            .expect("override row");
    assert_eq!(tier, "standard");

    // Unknown message type / invalid tier: the baseline's ValidationError.
    for (body, message) in [
        (
            r#"{"routing":{"not_a_type":"standard"}}"#,
            "Unknown message type 'not_a_type'",
        ),
        (
            r#"{"routing":{"reply":"ultra"}}"#,
            "Unknown quality tier 'ultra'",
        ),
    ] {
        let error = patch(app.clone(), "/api/ai/routing", body).await;
        assert_eq!(error.status, 422, "{body}: {}", error.body);
        assert_eq!(error.body["error"]["code"], "validation_error");
        assert_eq!(error.body["error"]["message"], message);
    }

    // Routing changes are NOT audited (the baseline writes no event here).
    let trail = get(app, "/api/audit-events?event_type=ai.routing_changed").await;
    assert_eq!(trail.body.as_array().expect("array").len(), 0);
}

#[tokio::test]
async fn a_null_routing_body_is_a_dict_type_422() {
    let (app, _dir) = w1_app().await;
    let response = patch(app, "/api/ai/routing", r#"{"routing":null}"#).await;
    assert_eq!(response.status, 422, "body: {}", response.body);
    let issue = &response.body["error"]["details"]["errors"][0];
    assert_eq!(issue["type"], "dict_type");
    assert_eq!(issue["loc"], serde_json::json!(["body", "routing"]));
}

#[tokio::test]
async fn usage_aggregates_today_history_and_all_time() {
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;
    let today = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d")
        .to_string();
    for (date, model, cost, requests) in [
        (today.as_str(), "glm-4-flash", 12, 3),
        (today.as_str(), "glm-4-plus", 5, 1),
        ("2026-11-19", "glm-4-flash", 7, 2),
    ] {
        sqlx::query(concat!(
            "INSERT INTO ai_usage_daily (usage_date, model_id, prompt_tokens,",
            " completion_tokens, estimated_cost_cents, request_count, id,",
            " created_at, updated_at) VALUES (?, ?, 10, 20, ?, ?, ?, datetime('now'), datetime('now'))"
        ))
        .bind(date)
        .bind(model)
        .bind(cost)
        .bind(requests)
        .bind(format!("u-{date}-{model}"))
        .execute(&pool)
        .await
        .expect("seed usage row");
    }

    let response = get(app.clone(), "/api/ai/usage?days=3").await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    let today_obj = &response.body["today"];
    assert_eq!(today_obj["total_cost_cents"], 17);
    assert_eq!(today_obj["request_count"], 4);
    assert_eq!(today_obj["total_prompt_tokens"], 20);
    assert_eq!(today_obj["total_completion_tokens"], 40);
    assert_eq!(today_obj["daily_budget_cents"], 0);
    assert_eq!(today_obj["remaining_cents"], 0);
    assert_eq!(today_obj["budget_utilization"], 0.0);
    assert_eq!(today_obj["tier_downgrade_active"], false);

    let history = response.body["history"].as_array().expect("history");
    assert_eq!(history.len(), 3, "one entry per requested day");
    assert_eq!(history[0]["date"].as_str().unwrap(), today);
    assert_eq!(history[0]["total_cost_cents"], 17);
    assert_eq!(history[0]["by_model"].as_array().unwrap().len(), 2);
    assert_eq!(history[1]["total_cost_cents"], 0, "zero-filled days");

    let all_time = response.body["by_model_all_time"].as_array().expect("all");
    assert_eq!(all_time.len(), 2);
    assert_eq!(all_time[0]["model_id"], "glm-4-flash");
    assert_eq!(all_time[0]["total_cost_cents"], 19);
    assert_eq!(all_time[0]["total_requests"], 5);

    // Out-of-range days: the pydantic messages differ from the page limit.
    for (uri, kind, message) in [
        (
            "/api/ai/usage?days=0",
            "greater_than_equal",
            "Input should be greater than or equal to 1",
        ),
        (
            "/api/ai/usage?days=91",
            "less_than_equal",
            "Input should be less than or equal to 90",
        ),
    ] {
        let error = get(app.clone(), uri).await;
        assert_eq!(error.status, 422, "{uri}");
        assert_eq!(
            error.body["error"]["details"]["errors"][0]["type"], kind,
            "{uri}"
        );
        assert_eq!(
            error.body["error"]["details"]["errors"][0]["msg"], message,
            "{uri}"
        );
    }
}

#[tokio::test]
async fn budget_patch_persists_rows_mutates_the_process_and_never_audits() {
    let (app, dir) = w1_app().await;

    let response = patch(
        app.clone(),
        "/api/ai/budget",
        r#"{"daily_budget_cents":1000,"warning_threshold":50}"#,
    )
    .await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(
        response.body,
        serde_json::json!({
            "daily_budget_cents": 1000,
            "warning_threshold": 50,
            "today_spend_cents": 0,
            "remaining_cents": 1000,
            "status": "ok",
        })
    );

    // The rows exist (and the baseline never reads them back — reproduced).
    let pool = pool_for(&dir).await;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT \"key\", value FROM app_settings WHERE \"key\" LIKE 'ai_%' ORDER BY \"key\"",
    )
    .fetch_all(&pool)
    .await
    .expect("budget rows");
    assert_eq!(
        rows,
        vec![
            ("ai_budget_warning_threshold".to_string(), "50".to_string()),
            ("ai_daily_budget_cents".to_string(), "1000".to_string()),
        ]
    );

    // GET reflects the in-process mutation.
    let read = get(app.clone(), "/api/ai/budget").await;
    assert_eq!(read.body["daily_budget_cents"], 1000);
    assert_eq!(read.body["remaining_cents"], 1000);

    // The spend state machine: warning at the threshold, exceeded past it.
    let pool = pool_for(&dir).await;
    let today = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d")
        .to_string();
    sqlx::query(concat!(
        "INSERT INTO ai_usage_daily (usage_date, model_id, prompt_tokens,",
        " completion_tokens, estimated_cost_cents, request_count, id,",
        " created_at, updated_at) VALUES (?, 'glm-4-flash', 0, 0, ?, 1, ?,",
        " datetime('now'), datetime('now'))"
    ))
    .bind(&today)
    .bind(600)
    .bind("u-spend")
    .execute(&pool)
    .await
    .expect("seed spend");

    let warned = get(app.clone(), "/api/ai/budget").await;
    assert_eq!(warned.body["status"], "warning", "{}", warned.body);
    assert_eq!(warned.body["remaining_cents"], 400);

    sqlx::query("UPDATE ai_usage_daily SET estimated_cost_cents = 1200 WHERE id = 'u-spend'")
        .execute(&pool)
        .await
        .expect("raise spend");
    let exceeded = get(app.clone(), "/api/ai/budget").await;
    assert_eq!(exceeded.body["status"], "exceeded");
    assert_eq!(exceeded.body["remaining_cents"], 0);

    // Budget changes are NOT audited (the baseline writes no event here).
    let trail = get(
        app.clone(),
        "/api/audit-events?event_type=ai.budget_changed",
    )
    .await;
    assert_eq!(trail.body.as_array().expect("array").len(), 0);

    // Validation: ge=0 / ge=1 le=100, both fields required.
    for (body, kind, field) in [
        (
            r#"{"warning_threshold":50}"#,
            "missing",
            "daily_budget_cents",
        ),
        (
            r#"{"daily_budget_cents":-1,"warning_threshold":50}"#,
            "greater_than_equal",
            "daily_budget_cents",
        ),
        (
            r#"{"daily_budget_cents":10,"warning_threshold":0}"#,
            "greater_than_equal",
            "warning_threshold",
        ),
        (
            r#"{"daily_budget_cents":10,"warning_threshold":101}"#,
            "less_than_equal",
            "warning_threshold",
        ),
    ] {
        let error = patch(app.clone(), "/api/ai/budget", body).await;
        assert_eq!(error.status, 422, "{body}");
        assert_eq!(
            error.body["error"]["details"]["errors"][0]["type"], kind,
            "{body}"
        );
        assert_eq!(
            error.body["error"]["details"]["errors"][0]["loc"],
            serde_json::json!(["body", field]),
            "{body}"
        );
    }
}
