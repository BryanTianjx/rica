//! The AI model registry, scenario routing table and budget state (W5).
//!
//! Read-only counterparts of the baseline's `app/ai/registry.py`,
//! `app/ai/router.py` and `app/api/ai.py` read/patch endpoints. The mutating
//! model-CRUD routes belong to AI-EXT (ADR-0003 D-2 补充裁决), and the
//! credential store is the ai-key decision — so key resolution here covers the
//! environment paths only, with model-slot lookups deferred.
//!
//! Behavioural notes carried over verbatim:
//! * an **empty `ai_model_configs` table falls back to the V1.0 default**
//!   built from settings, so the endpoint always reflects what a draft would
//!   actually use (`source: "settings"`);
//! * the routing table is the nine-key default **layered with** the
//!   `ai_model_routing` overrides; an invalid stored tier is ignored, never a
//!   failure;
//! * `PATCH /ai/budget` writes its two `app_settings` rows **without an
//!   audit event** and mutates the in-process settings — the rows are never
//!   read back by the baseline, so a restart resets to the env/default
//!   (baseline quirk, reproduced rather than "fixed").

use crate::ai_key::resolve_model_api_key;
use crate::error::ApplicationError;
use crate::ports::{AiModelRepository, AiUsageRepository, SecretStore};
use std::collections::BTreeMap;

fn unavailable(error: crate::ports::RepositoryError) -> ApplicationError {
    error.application_error()
}

/// The default scenario routing table: message type → quality tier
/// (`app/ai/router.py::DEFAULT_ROUTING`, insertion order preserved here).
pub const DEFAULT_ROUTING: [(&str, &str); 9] = [
    ("greeting", "economy"),
    ("polite_follow_up", "economy"),
    ("reply", "standard"),
    ("follow_up_question", "standard"),
    ("position_intro", "standard"),
    ("salary_talk", "premium"),
    ("interview_invite", "premium"),
    ("post_interview", "premium"),
    ("decline_ack", "premium"),
];

/// The quality tiers a routing override may carry.
pub const QUALITY_TIERS: [&str; 3] = ["economy", "standard", "premium"];

/// The AI-side configuration a use case needs, built by the adapter from its
/// injected config (never from the environment directly).
#[derive(Debug, Clone, PartialEq)]
pub struct AiSettingsView {
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_base_url: String,
    pub ai_max_tokens: i64,
    pub ai_timeout: i64,
    pub ai_temperature: f64,
}

/// The process-local budget pair; `PATCH /ai/budget` mutates it, `GET` reads
/// it, and a restart resets it to the configured default (baseline quirk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiBudgetSettings {
    pub daily_budget_cents: i64,
    pub warning_threshold: i64,
}

/// One configured model — the wire shape of `ModelRead` minus the two
/// transport-only fields (`api_key_configured`/`source` are filled by the
/// use case).
#[derive(Debug, Clone, PartialEq)]
pub struct AiModelData {
    pub model_id: String,
    pub display_name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key_env: String,
    pub quality_tier: String,
    pub input_cost_per_1k_tokens: f64,
    pub output_cost_per_1k_tokens: f64,
    pub max_output_tokens: i64,
    pub max_input_tokens: i64,
    pub timeout: i64,
    pub temperature: f64,
    pub enabled: bool,
    pub is_default: bool,
    pub sort_order: i64,
}

/// One raw `ai_usage_daily` row.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageRow {
    pub usage_date: String,
    pub model_id: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub estimated_cost_cents: i64,
    pub request_count: i64,
}

/// One day of usage history (`UsageDay`).
#[derive(Debug, Clone, PartialEq)]
pub struct UsageDay {
    pub date: String,
    pub total_cost_cents: i64,
    pub request_count: i64,
    pub by_model: Vec<AiUsageByModel>,
}

/// One per-model entry inside a day.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageByModel {
    pub model_id: String,
    pub cost_cents: i64,
    pub request_count: i64,
}

/// One per-model all-time total.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageAllTime {
    pub model_id: String,
    pub total_cost_cents: i64,
    pub total_requests: i64,
}

/// `ModelsRead` payload.
#[derive(Debug, Clone, PartialEq)]
pub struct AiModelsOverview {
    pub models: Vec<AiModelRead>,
    pub routing: BTreeMap<String, String>,
}

/// `ModelRead` — one configured model, key presence only (never the key).
#[derive(Debug, Clone, PartialEq)]
pub struct AiModelRead {
    pub model_id: String,
    pub display_name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key_env: String,
    pub quality_tier: String,
    pub input_cost_per_1k_tokens: f64,
    pub output_cost_per_1k_tokens: f64,
    pub max_output_tokens: i64,
    pub max_input_tokens: i64,
    pub timeout: i64,
    pub temperature: f64,
    pub enabled: bool,
    pub is_default: bool,
    pub sort_order: i64,
    pub api_key_configured: bool,
    pub source: String,
}

/// `GET /ai/models` — the registry with the V1.0 fallback, plus the routing
/// table.
///
/// The fallback fires only when the table has **no rows at all**; the single
/// fallback model carries `source: "settings"` and `api_key_env: "AI_API_KEY"`.
/// Key presence follows the baseline's three-level resolution (the model's
/// credential-store slot → its `api_key_env` → the installation-wide slot).
pub async fn list_ai_models(
    models: &dyn AiModelRepository,
    secrets: &dyn SecretStore,
    view: &AiSettingsView,
) -> Result<AiModelsOverview, ApplicationError> {
    let rows = models.list_configs().await.map_err(unavailable)?;
    let fallback_needed = rows.is_empty() && !view.ai_model.is_empty();
    let mut items: Vec<AiModelRead> = Vec::with_capacity(rows.len());
    for row in rows {
        // An invalid stored tier degrades to economy on load, never a
        // failure (registry._load).
        let quality_tier = if QUALITY_TIERS.contains(&row.quality_tier.as_str()) {
            row.quality_tier.clone()
        } else {
            "economy".to_string()
        };
        let api_key_configured = resolve_model_api_key(secrets, &row.api_key_env, &row.model_id)
            .await
            .map_err(unavailable)?
            .is_some();
        items.push(AiModelRead {
            api_key_configured,
            source: "database".to_string(),
            quality_tier,
            ..AiModelRead::from(row)
        });
    }
    if fallback_needed {
        let api_key_configured = resolve_model_api_key(secrets, "AI_API_KEY", &view.ai_model)
            .await
            .map_err(unavailable)?
            .is_some();
        items.push(AiModelRead {
            model_id: view.ai_model.clone(),
            display_name: view.ai_model.clone(),
            provider_type: view.ai_provider.clone(),
            base_url: view.ai_base_url.clone(),
            api_key_env: "AI_API_KEY".to_string(),
            quality_tier: "economy".to_string(),
            input_cost_per_1k_tokens: 0.0,
            output_cost_per_1k_tokens: 0.0,
            max_output_tokens: view.ai_max_tokens,
            max_input_tokens: 8000,
            timeout: view.ai_timeout,
            temperature: view.ai_temperature,
            enabled: true,
            is_default: true,
            sort_order: 0,
            api_key_configured,
            source: "settings".to_string(),
        });
    }
    let routing = routing_table(models).await?;
    Ok(AiModelsOverview {
        models: items,
        routing,
    })
}

/// `registry.get_model`: resolve one id, refusing unknown ids and disabled
/// rows with the baseline's exact 503 texts. With an empty table the V1.0
/// settings model resolves to the fallback config (`ModelRegistry._load`).
pub async fn get_model(
    models: &dyn AiModelRepository,
    view: &AiSettingsView,
    model_id: &str,
) -> Result<AiModelData, ApplicationError> {
    let rows = models.list_configs().await.map_err(unavailable)?;
    if rows.is_empty() {
        if view.ai_model == model_id && !view.ai_model.is_empty() {
            return Ok(fallback_model(view));
        }
        return Err(ApplicationError::ProviderUnavailable {
            message: format!("Unknown model '{model_id}'. No matching configuration found."),
            details: serde_json::json!({ "model_id": model_id }),
        });
    }
    let Some(mut row) = rows.into_iter().find(|row| row.model_id == model_id) else {
        return Err(ApplicationError::ProviderUnavailable {
            message: format!("Unknown model '{model_id}'. No matching configuration found."),
            details: serde_json::json!({ "model_id": model_id }),
        });
    };
    if !QUALITY_TIERS.contains(&row.quality_tier.as_str()) {
        row.quality_tier = "economy".to_string();
    }
    if !row.enabled {
        return Err(ApplicationError::ProviderUnavailable {
            message: format!("Model '{model_id}' is disabled and cannot be used."),
            details: serde_json::json!({ "model_id": model_id, "enabled": false }),
        });
    }
    Ok(row)
}

/// The V1.0 fallback built from the settings view (`_default_config_from_settings`).
fn fallback_model(view: &AiSettingsView) -> AiModelData {
    AiModelData {
        model_id: view.ai_model.clone(),
        display_name: view.ai_model.clone(),
        provider_type: view.ai_provider.clone(),
        base_url: view.ai_base_url.clone(),
        api_key_env: "AI_API_KEY".to_string(),
        quality_tier: "economy".to_string(),
        input_cost_per_1k_tokens: 0.0,
        output_cost_per_1k_tokens: 0.0,
        max_output_tokens: view.ai_max_tokens,
        max_input_tokens: 8000,
        timeout: view.ai_timeout,
        temperature: view.ai_temperature,
        enabled: true,
        is_default: true,
        sort_order: 0,
    }
}

/// `registry.get_default_model`: the enabled `is_default` row with the lowest
/// `sort_order`, else the first enabled row, else `None` (the settings
/// fallback fires when the table is empty, mirroring `ModelRegistry._load`).
pub async fn get_default_model(
    models: &dyn AiModelRepository,
    view: &AiSettingsView,
) -> Result<Option<AiModelData>, ApplicationError> {
    let rows = models.list_configs().await.map_err(unavailable)?;
    if rows.is_empty() {
        if view.ai_model.is_empty() {
            return Ok(None);
        }
        return Ok(Some(fallback_model(view)));
    }
    let enabled: Vec<&AiModelData> = rows.iter().filter(|row| row.enabled).collect();
    let default = enabled
        .iter()
        .filter(|row| row.is_default)
        .min_by_key(|row| row.sort_order)
        .or_else(|| enabled.first());
    Ok(default.map(|row| (*row).clone()))
}

/// `registry.get_model_by_tier`: the lowest-`sort_order` enabled model in the
/// tier, or the baseline's 503 when the tier is empty.
pub async fn get_model_by_tier(
    models: &dyn AiModelRepository,
    tier: &str,
) -> Result<AiModelData, ApplicationError> {
    let rows = models.list_configs().await.map_err(unavailable)?;
    let candidates: Vec<&AiModelData> = rows
        .iter()
        .filter(|row| row.enabled && row.quality_tier == tier)
        .collect();
    match candidates.iter().min_by_key(|row| row.sort_order) {
        Some(row) => Ok((*row).clone()),
        None => Err(ApplicationError::ProviderUnavailable {
            message: format!("No enabled model found for tier '{tier}'."),
            details: serde_json::json!({ "quality_tier": tier }),
        }),
    }
}

/// The outcome of a routing decision, recorded on every draft
/// (`app/ai/router.py::RoutingDecision`).
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingDecision {
    pub model: AiModelData,
    pub requested_tier: String,
    pub actual_tier: String,
    /// `"scenario_match" | "budget_downgrade" | "manual_override"`.
    pub reason: String,
}

/// Downgrade one level; `None` when already at the lowest tier
/// (`router._downgrade_tier`).
fn downgrade_tier(tier: &str) -> Option<&'static str> {
    match tier {
        "premium" => Some("standard"),
        "standard" => Some("economy"),
        _ => None,
    }
}

/// `Router.select` — choose the model for one message type.
///
/// 1. a manual id resolves directly (`manual_override`);
/// 2. otherwise the tier for the message type (override layered on the
///    default table) picks the tier's active model (`scenario_match`);
/// 3. with the budget warning active the tier is downgraded one level
///    (`budget_downgrade`); at economy the request is rejected with 503;
/// 4. a tier gap falls back to the default model so it never blocks
///    generation.
pub async fn select_model(
    models: &dyn AiModelRepository,
    usage: &dyn AiUsageRepository,
    budget: &AiBudgetSettings,
    view: &AiSettingsView,
    message_type: &str,
    manual_model_id: Option<&str>,
    today: &str,
) -> Result<RoutingDecision, ApplicationError> {
    if let Some(manual_model_id) = manual_model_id.filter(|id| !id.is_empty()) {
        let model = get_model(models, view, manual_model_id).await?;
        let tier = model.quality_tier.clone();
        return Ok(RoutingDecision {
            model,
            requested_tier: tier.clone(),
            actual_tier: tier,
            reason: "manual_override".to_string(),
        });
    }

    let requested_tier = requested_tier_for(models, message_type).await?;
    let mut actual_tier = requested_tier.clone();
    let mut reason = "scenario_match".to_string();

    let budget_warning = if budget.daily_budget_cents > 0 {
        let spend = today_spend_cents(usage, today).await?;
        warning_active(budget, spend)
    } else {
        false
    };
    if budget_warning {
        match downgrade_tier(&requested_tier) {
            None => {
                let spend = today_spend_cents(usage, today).await?;
                return Err(ApplicationError::ProviderUnavailable {
                    message: "Daily AI budget near the limit and already at the lowest tier; \
                              draft generation is paused until tomorrow."
                        .to_string(),
                    details: serde_json::json!({
                        "requested_tier": requested_tier,
                        "daily_spend_cents": spend,
                        "daily_budget_cents": budget.daily_budget_cents,
                    }),
                });
            }
            Some(downgraded) => {
                actual_tier = downgraded.to_string();
                reason = "budget_downgrade".to_string();
            }
        }
    }

    match get_model_by_tier(models, &actual_tier).await {
        Ok(model) => Ok(RoutingDecision {
            model,
            requested_tier,
            actual_tier,
            reason,
        }),
        // No enabled model in the (possibly downgraded) tier: fall back to
        // the default model so a tier gap never blocks draft generation.
        Err(tier_error) => match get_default_model(models, view).await? {
            Some(model) => {
                let actual_tier = model.quality_tier.clone();
                Ok(RoutingDecision {
                    model,
                    requested_tier,
                    actual_tier,
                    reason,
                })
            }
            None => Err(tier_error),
        },
    }
}

/// The tier for one message type: the stored override, else the default
/// table (`Router.requested_tier_for`).
async fn requested_tier_for(
    models: &dyn AiModelRepository,
    message_type: &str,
) -> Result<String, ApplicationError> {
    for (message_type_override, tier) in models.routing_overrides().await.map_err(unavailable)? {
        if message_type_override == message_type && QUALITY_TIERS.contains(&tier.as_str()) {
            return Ok(tier);
        }
    }
    match DEFAULT_ROUTING
        .iter()
        .find(|(known, _)| *known == message_type)
    {
        Some((_, tier)) => Ok(tier.to_string()),
        None => Err(ApplicationError::ProviderUnavailable {
            message: format!("Unknown message type '{message_type}'. No routing entry exists."),
            details: serde_json::json!({ "message_type": message_type }),
        }),
    }
}

/// `budget.estimate_cost_cents` — cents (人民币分) from token counts and
/// per-1k rates. Python rounds half-even at an exact `.5`; the parity diff
/// parses the number, so only exact-half values could diverge (same accepted
/// risk as `round4` below).
pub fn estimate_cost_cents(
    prompt_tokens: i64,
    completion_tokens: i64,
    input_cost_per_1k: f64,
    output_cost_per_1k: f64,
) -> i64 {
    let input_cents = (prompt_tokens as f64 / 1000.0) * input_cost_per_1k * 100.0;
    let output_cents = (completion_tokens as f64 / 1000.0) * output_cost_per_1k * 100.0;
    (input_cents + output_cents).round() as i64
}

impl From<AiModelData> for AiModelRead {
    fn from(row: AiModelData) -> Self {
        Self {
            model_id: row.model_id,
            display_name: row.display_name,
            provider_type: row.provider_type,
            base_url: row.base_url,
            api_key_env: row.api_key_env,
            quality_tier: row.quality_tier,
            input_cost_per_1k_tokens: row.input_cost_per_1k_tokens,
            output_cost_per_1k_tokens: row.output_cost_per_1k_tokens,
            max_output_tokens: row.max_output_tokens,
            max_input_tokens: row.max_input_tokens,
            timeout: row.timeout,
            temperature: row.temperature,
            enabled: row.enabled,
            is_default: row.is_default,
            sort_order: row.sort_order,
            api_key_configured: false,
            source: "database".to_string(),
        }
    }
}

/// The effective routing table: defaults layered with the stored overrides.
pub async fn routing_table(
    models: &dyn AiModelRepository,
) -> Result<BTreeMap<String, String>, ApplicationError> {
    let mut table: BTreeMap<String, String> = DEFAULT_ROUTING
        .iter()
        .map(|(message_type, tier)| ((*message_type).to_string(), (*tier).to_string()))
        .collect();
    for (message_type, tier) in models.routing_overrides().await.map_err(unavailable)? {
        // An invalid stored tier is ignored; it must never block routing.
        if QUALITY_TIERS.contains(&tier.as_str()) {
            table.insert(message_type, tier);
        }
    }
    Ok(table)
}

/// `GET /ai/routing`.
pub async fn get_ai_routing(
    models: &dyn AiModelRepository,
) -> Result<BTreeMap<String, String>, ApplicationError> {
    routing_table(models).await
}

/// `PATCH /ai/routing` — persist the overrides for one or more message types.
///
/// Only known message types and valid tiers are accepted (422 with the
/// baseline's `ValidationError` shape). A `None` tier removes the override so
/// the default shows through again.
pub async fn update_ai_routing(
    models: &dyn AiModelRepository,
    patches: Vec<(String, Option<String>)>,
) -> Result<BTreeMap<String, String>, ApplicationError> {
    // The baseline validates the whole payload before it writes anything.
    validate_routing_patch(&patches)?;
    for (message_type, tier) in &patches {
        models
            .replace_routing_override(message_type, tier.as_deref())
            .await
            .map_err(unavailable)?;
    }
    routing_table(models).await
}

/// `GET /ai/usage` — today's totals, `days` of history, all-time per model.
pub async fn get_ai_usage(
    usage: &dyn AiUsageRepository,
    budget: &AiBudgetSettings,
    days: u32,
    today: &str,
) -> Result<UsageOverview, ApplicationError> {
    let rows = usage.list_rows().await.map_err(unavailable)?;

    let today_rows: Vec<&AiUsageRow> = rows.iter().filter(|row| row.usage_date == today).collect();
    let today_spend: i64 = today_rows.iter().map(|row| row.estimated_cost_cents).sum();
    let today_requests: i64 = today_rows.iter().map(|row| row.request_count).sum();
    let remaining = if budget.daily_budget_cents > 0 {
        (budget.daily_budget_cents - today_spend).max(0)
    } else {
        0
    };
    let utilization = if budget.daily_budget_cents > 0 {
        round4(today_spend as f64 / budget.daily_budget_cents as f64)
    } else {
        0.0
    };

    let mut history = Vec::with_capacity(days as usize);
    for offset in 0..days {
        let Some(date) = shift_date(today, offset) else {
            continue;
        };
        let day_rows: Vec<&AiUsageRow> = rows.iter().filter(|row| row.usage_date == date).collect();
        history.push(UsageDay {
            date,
            total_cost_cents: day_rows.iter().map(|row| row.estimated_cost_cents).sum(),
            request_count: day_rows.iter().map(|row| row.request_count).sum(),
            by_model: day_rows
                .iter()
                .map(|row| AiUsageByModel {
                    model_id: row.model_id.clone(),
                    cost_cents: row.estimated_cost_cents,
                    request_count: row.request_count,
                })
                .collect(),
        });
    }

    // All-time per model, in first-seen order (the baseline's dict insertion
    // order over `usage_date`-ordered rows).
    let mut all_time: Vec<AiUsageAllTime> = Vec::new();
    for row in &rows {
        match all_time
            .iter_mut()
            .find(|entry| entry.model_id == row.model_id)
        {
            Some(entry) => {
                entry.total_cost_cents += row.estimated_cost_cents;
                entry.total_requests += row.request_count;
            }
            None => all_time.push(AiUsageAllTime {
                model_id: row.model_id.clone(),
                total_cost_cents: row.estimated_cost_cents,
                total_requests: row.request_count,
            }),
        }
    }

    Ok(UsageOverview {
        today: TodayUsage {
            total_cost_cents: today_spend,
            total_prompt_tokens: today_rows.iter().map(|row| row.prompt_tokens).sum(),
            total_completion_tokens: today_rows.iter().map(|row| row.completion_tokens).sum(),
            request_count: today_requests,
            daily_budget_cents: budget.daily_budget_cents,
            remaining_cents: remaining,
            budget_utilization: utilization,
            tier_downgrade_active: warning_active(budget, today_spend),
        },
        history,
        by_model_all_time: all_time,
    })
}

/// `GET /ai/budget` payload.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetRead {
    pub daily_budget_cents: i64,
    pub warning_threshold: i64,
    pub today_spend_cents: i64,
    pub remaining_cents: i64,
    pub status: String,
}

/// `GET /ai/usage` payload.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageOverview {
    pub today: TodayUsage,
    pub history: Vec<UsageDay>,
    pub by_model_all_time: Vec<AiUsageAllTime>,
}

/// The "today" object inside `UsageRead`.
#[derive(Debug, Clone, PartialEq)]
pub struct TodayUsage {
    pub total_cost_cents: i64,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub request_count: i64,
    pub daily_budget_cents: i64,
    pub remaining_cents: i64,
    pub budget_utilization: f64,
    pub tier_downgrade_active: bool,
}

/// The baseline's `_status_for`.
pub fn budget_status(spend: i64, budget: i64, threshold: i64) -> &'static str {
    if budget <= 0 {
        return "ok";
    }
    if spend >= budget {
        return "exceeded";
    }
    if spend as f64 >= budget as f64 * threshold as f64 / 100.0 {
        return "warning";
    }
    "ok"
}

/// The baseline's `BudgetGuard.warning_active`.
fn warning_active(budget: &AiBudgetSettings, spend: i64) -> bool {
    if budget.daily_budget_cents <= 0 {
        return false;
    }
    spend as f64 >= budget.daily_budget_cents as f64 * budget.warning_threshold as f64 / 100.0
        && spend < budget.daily_budget_cents
}

/// Python's `round(x, 4)` on the utilization ratio (half-even at the 4th
/// decimal; the parity diff parses the number, so only exact-half values
/// could diverge).
fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// `today - offset` in the same `%Y-%m-%d` shape, or `None` when the date
/// arithmetic underflows (which it cannot for `days <= 90`).
fn shift_date(today: &str, offset: u32) -> Option<String> {
    let base = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    base.checked_sub_signed(chrono::Duration::days(offset as i64))
        .map(|shifted| shifted.format("%Y-%m-%d").to_string())
}

/// Validate the routing-patch entries before any write (the baseline
/// validates the whole payload first, then applies it).
pub fn validate_routing_patch(
    patches: &[(String, Option<String>)],
) -> Result<(), ApplicationError> {
    for (message_type, tier) in patches {
        if !DEFAULT_ROUTING
            .iter()
            .any(|(known, _)| known == message_type)
        {
            return Err(ApplicationError::Validation {
                message: format!("Unknown message type '{message_type}'"),
                details: BTreeMap::from([("message_type".to_string(), message_type.clone())]),
            });
        }
        if let Some(tier) = tier {
            if !QUALITY_TIERS.contains(&tier.as_str()) {
                return Err(ApplicationError::Validation {
                    message: format!("Unknown quality tier '{tier}'"),
                    details: BTreeMap::from([
                        ("message_type".to_string(), message_type.clone()),
                        ("tier".to_string(), tier.clone()),
                    ]),
                });
            }
        }
    }
    Ok(())
}

/// `PATCH /ai/budget`'s response construction (the patch itself lives in the
/// adapter: rows first, then the in-process pair).
pub fn budget_read(budget: &AiBudgetSettings, spend: i64) -> BudgetRead {
    BudgetRead {
        daily_budget_cents: budget.daily_budget_cents,
        warning_threshold: budget.warning_threshold,
        today_spend_cents: spend,
        remaining_cents: if budget.daily_budget_cents > 0 {
            (budget.daily_budget_cents - spend).max(0)
        } else {
            0
        },
        status: budget_status(spend, budget.daily_budget_cents, budget.warning_threshold)
            .to_string(),
    }
}

/// Today's total spend across all models (`BudgetGuard.today_spend_cents`).
pub async fn today_spend_cents(
    usage: &dyn AiUsageRepository,
    today: &str,
) -> Result<i64, ApplicationError> {
    let rows = usage.list_rows().await.map_err(unavailable)?;
    Ok(rows
        .iter()
        .filter(|row| row.usage_date == today)
        .map(|row| row.estimated_cost_cents)
        .sum())
}
