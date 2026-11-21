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

use crate::error::ApplicationError;
use crate::ports::{AiModelRepository, AiUsageRepository};
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
/// Key presence: the model's env var, or the V1.0 installation key — the
/// credential-store slot is part of the ai-key decision and stays deferred.
pub async fn list_ai_models(
    models: &dyn AiModelRepository,
    view: &AiSettingsView,
) -> Result<AiModelsOverview, ApplicationError> {
    let rows = models.list_configs().await.map_err(unavailable)?;
    let fallback_needed = rows.is_empty() && !view.ai_model.is_empty();
    let mut items: Vec<AiModelRead> = rows
        .into_iter()
        .map(|row| {
            // An invalid stored tier degrades to economy on load, never a
            // failure (registry._load).
            let quality_tier = if QUALITY_TIERS.contains(&row.quality_tier.as_str()) {
                row.quality_tier.clone()
            } else {
                "economy".to_string()
            };
            AiModelRead {
                api_key_configured: api_key_resolved(&row.api_key_env, view.ai_enabled),
                source: "database".to_string(),
                quality_tier,
                ..AiModelRead::from(row)
            }
        })
        .collect();
    if fallback_needed {
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
            api_key_configured: view.ai_enabled,
            source: "settings".to_string(),
        });
    }
    let routing = routing_table(models).await?;
    Ok(AiModelsOverview {
        models: items,
        routing,
    })
}

/// Key presence: the model's `api_key_env` var, or the V1.0 installation key.
fn api_key_resolved(api_key_env: &str, v1_key_present: bool) -> bool {
    if !api_key_env.is_empty()
        && std::env::var(api_key_env)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    {
        return true;
    }
    v1_key_present
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
