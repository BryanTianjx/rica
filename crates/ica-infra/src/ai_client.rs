//! The real AI provider adapter (the W5 item this closes): an
//! OpenAI-compatible chat-completions client behind the [`DraftGenerator`]
//! port — `app/ai/service.py::generate_draft` +
//! `app/ai/providers/base.py::OpenAICompatibleProvider`, one model at a time
//! or the whole A/B comparison group.
//!
//! Flow per draft (the baseline's `_generate_single_draft`): budget gate →
//! three-level key resolution → HTTP call → egress audit → JSON extraction →
//! content validation → draft + usage + audit persisted in one transaction.
//! Every failure path answers the baseline's exact `AIProviderError` text.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use ica_application::ai_context::{build_context, render_context, GenerateDraftRequest};
use ica_application::ai_key::resolve_model_api_key;
use ica_application::ai_registry::{
    estimate_cost_cents, get_model, select_model, AiBudgetSettings, AiSettingsView, RoutingDecision,
};
use ica_application::error::ApplicationError;
use ica_application::ports::{
    CandidateRepository, DraftGeneration, DraftGenerator, PositionRepository, SecretStore,
};
use ica_application::prompts::{build_user_prompt, PROMPT_VERSION, SYSTEM_PROMPT};
use ica_application::validators::validate_draft;
use ica_domain::{Candidate, GeneratedDraft, Position};

use crate::audit::{insert_audit, AuditEntry};
use crate::json_fmt::python_json;
use crate::store::{new_uuid, to_sql_timestamp, SqliteStore};

fn unavailable(error: ica_application::ports::RepositoryError) -> ApplicationError {
    error.application_error()
}

/// The synthetic local identity (`app/application/recruiting._owner_tenant`
/// on the local path — the same constants the other adapters stamp).
const LOCAL_OWNER_ID: &str = "00000000-0000-0000-0000-000000000001";
const LOCAL_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";

/// The OpenAI-compatible generator. One instance per backend: it borrows the
/// store for registry reads and persistence, the OS credential store for key
/// resolution, the settings view for the V1.0 fallback and the process-local
/// budget pair.
pub struct AiDraftGenerator {
    store: SqliteStore,
    secrets: Arc<dyn SecretStore>,
    view: AiSettingsView,
    budget: Arc<RwLock<AiBudgetSettings>>,
    http: reqwest::Client,
    /// A process-stable session id for the `opencode` provider gateways: they
    /// require `x-opencode-session` on every chat call (measured: a missing
    /// header is a 400 `MissingSessionID`), one stable id per conversation.
    /// The adapter sends one per process — the closest a batch API gets.
    session_id: String,
}

impl AiDraftGenerator {
    pub fn new(
        store: SqliteStore,
        secrets: Arc<dyn SecretStore>,
        view: AiSettingsView,
        budget: Arc<RwLock<AiBudgetSettings>>,
    ) -> Self {
        Self {
            store,
            secrets,
            view,
            budget,
            http: reqwest::Client::new(),
            session_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// The configured daily budget (the process-local pair the baseline's
    /// `app_settings` singleton mirrors).
    fn budget_snapshot(&self) -> AiBudgetSettings {
        *self.budget.read().expect("budget lock")
    }

    /// `_generate_single_draft`: one model, one provider call, one persisted
    /// draft. `group_id` attaches the A/B group; `None` for the single path.
    #[allow(clippy::too_many_arguments)]
    async fn generate_one(
        &self,
        request: &GenerateDraftRequest,
        candidate: &Candidate,
        position: Option<&Position>,
        redaction_applied: &[String],
        context_text: &str,
        decision: &RoutingDecision,
        group_id: Option<&str>,
    ) -> Result<GeneratedDraft, ApplicationError> {
        let model = &decision.model;

        // Budget gate: reject (503) once today's spend has hit the cap.
        let budget = self.budget_snapshot();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        if budget.daily_budget_cents > 0 {
            let spend = ica_application::today_spend_cents(&self.store, &today).await?;
            if spend >= budget.daily_budget_cents {
                return Err(ApplicationError::ProviderUnavailable {
                    message: "Daily AI budget exceeded; draft generation is paused until \
                              tomorrow."
                        .to_string(),
                    details: serde_json::json!({
                        "daily_budget_cents": budget.daily_budget_cents,
                        "today_spend_cents": spend,
                    }),
                });
            }
        }

        // Three-level key resolution; a missing key is the baseline's exact
        // no-key 503 (`build_provider_for_model` → provider constructor).
        let api_key =
            resolve_model_api_key(self.secrets.as_ref(), &model.api_key_env, &model.model_id)
                .await
                .map_err(unavailable)?
                .ok_or_else(|| ApplicationError::ProviderUnavailable {
                    message: format!(
                        "No API key found for model '{}' in env var '{}'",
                        model.model_id, model.api_key_env
                    ),
                    details: serde_json::json!({}),
                })?;

        let response = call_provider(
            &self.http,
            model.base_url.trim_end_matches('/'),
            &api_key,
            &model.model_id,
            &model.provider_type,
            &self.session_id,
            model.timeout,
            model.temperature,
            model.max_output_tokens,
            &build_user_prompt(
                context_text,
                request.message_type,
                request.tone,
                request.user_instruction.as_deref(),
                request.include_alternatives,
                request.max_length,
            ),
        )
        .await?;

        // Egress evidence: the redacted context has now left the process.
        // Recorded next to the draft so "what was sent, to whom, with which
        // fields masked" is answerable from the audit trail; the summary
        // deliberately carries the candidate id, never the name.
        let egress_audit = AuditEntry::new(
            "ai.egress",
            "candidate",
            &candidate.id,
            format!(
                "Sent redacted AI context for candidate '{}' to {} ({})",
                candidate.id, response.provider, response.model
            ),
        )
        .with_metadata(serde_json::json!({
            "provider": response.provider,
            "model": response.model,
            "redaction_applied": redaction_applied,
            "candidate_name_masked": redaction_applied.iter().any(|entry| entry == "name"),
        }));

        let payload_json = extract_json(&response.content)?;

        let message_text = payload_json
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if message_text.is_empty() {
            return Err(ApplicationError::ProviderUnavailable {
                message: "The AI response did not contain a message body".to_string(),
                details: serde_json::json!({ "candidate_id": candidate.id }),
            });
        }

        let context_references = as_str_list(payload_json.get("context_references"));
        let uncertainties = as_str_list(payload_json.get("uncertainties"));
        let warnings = as_str_list(payload_json.get("warnings"));
        let alternatives = as_str_list(payload_json.get("alternatives"));
        let objective = payload_json
            .get("objective")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let objective = if objective.is_empty() {
            None
        } else {
            Some(objective)
        };
        let quality_status = payload_json
            .get("quality_status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let quality_status = if quality_status.is_empty() {
            None
        } else {
            Some(quality_status)
        };

        let (validation_status, validation_messages) = validate_draft(
            &message_text,
            candidate.status,
            position.map(|position| position.status.as_str()),
            &uncertainties,
            &warnings,
            quality_status.as_deref(),
            &context_references,
        );

        // Token usage and estimated cost for this draft.
        let prompt_tokens = usage_int(&response.raw_usage, "prompt_tokens");
        let completion_tokens = usage_int(&response.raw_usage, "completion_tokens");
        let cost = estimate_cost_cents(
            prompt_tokens,
            completion_tokens,
            model.input_cost_per_1k_tokens,
            model.output_cost_per_1k_tokens,
        );

        let routing_decision_json = python_json(&serde_json::json!({
            "requested_tier": decision.requested_tier,
            "actual_tier": decision.actual_tier,
            "reason": decision.reason,
            "model_id": model.model_id,
        }));

        let draft = GeneratedDraft {
            id: new_uuid(),
            candidate_id: candidate.id.clone(),
            task_id: request.task_id.clone(),
            message_type: request.message_type.as_str().to_string(),
            tone: request.tone.as_str().to_string(),
            status: ica_domain::DraftStatus::Generated,
            objective,
            message_text,
            alternatives_json: python_json(&serde_json::json!(alternatives)),
            context_references_json: python_json(&serde_json::json!(context_references)),
            uncertainties_json: python_json(&serde_json::json!(uncertainties)),
            warnings_json: python_json(&serde_json::json!(warnings)),
            validation_status: validation_status.as_str().to_string(),
            validation_messages_json: python_json(&serde_json::json!(validation_messages)),
            quality_status,
            provider: Some(response.provider.clone()),
            model: Some(response.model.clone()),
            prompt_version: Some(PROMPT_VERSION.to_string()),
            is_edited: false,
            approved_at: None,
            copied_at: None,
            rejected_at: None,
            draft_group_id: group_id.map(str::to_string),
            cost_cents: Some(cost),
            prompt_tokens: Some(prompt_tokens),
            completion_tokens: Some(completion_tokens),
            routing_decision_json: Some(routing_decision_json),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let generated_audit = AuditEntry::new(
            "draft.generated",
            "draft",
            &draft.id,
            format!("Generated a '{}' draft", draft.message_type),
        )
        .with_metadata(serde_json::json!({
            "provider": response.provider,
            "model": response.model,
            "prompt_version": PROMPT_VERSION,
            "validation_status": draft.validation_status,
            "redaction_applied": redaction_applied,
            "routing_tier": decision.actual_tier,
            "routing_reason": decision.reason,
            "cost_cents": cost,
        }));

        // Persist draft + usage + both audits in one transaction (the
        // baseline's flush-then-commit). Usage is keyed by the CONFIG model
        // id (`guard.record_usage(config.model_id, …)`), not the echo.
        self.persist(&model.model_id, &draft, &egress_audit, &generated_audit)
            .await
            .map_err(unavailable)?;

        Ok(draft)
    }

    /// One transaction: the egress audit, the draft row, the usage upsert and
    /// the generation audit — the baseline commits them together.
    async fn persist(
        &self,
        model_id: &str,
        draft: &GeneratedDraft,
        egress_audit: &AuditEntry<'_>,
        generated_audit: &AuditEntry<'_>,
    ) -> Result<(), ica_application::ports::RepositoryError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(crate::store::repo_error)?;
        insert_audit(&mut tx, clone_entry(egress_audit))
            .await
            .map_err(crate::store::repo_error)?;
        sqlx::query(
            "INSERT INTO generated_drafts (id, candidate_id, task_id, message_type, tone, \
             status, objective, message_text, alternatives_json, context_references_json, \
             uncertainties_json, warnings_json, validation_status, validation_messages_json, \
             quality_status, provider, model, prompt_version, is_edited, approved_at, \
             copied_at, rejected_at, draft_group_id, routing_decision_json, cost_cents, \
             prompt_tokens, completion_tokens, owner_id, tenant_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
             ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&draft.id)
        .bind(&draft.candidate_id)
        .bind(&draft.task_id)
        .bind(&draft.message_type)
        .bind(&draft.tone)
        .bind(draft.status.as_str())
        .bind(&draft.objective)
        .bind(&draft.message_text)
        .bind(&draft.alternatives_json)
        .bind(&draft.context_references_json)
        .bind(&draft.uncertainties_json)
        .bind(&draft.warnings_json)
        .bind(&draft.validation_status)
        .bind(&draft.validation_messages_json)
        .bind(&draft.quality_status)
        .bind(&draft.provider)
        .bind(&draft.model)
        .bind(&draft.prompt_version)
        .bind(draft.is_edited)
        .bind(draft.approved_at.map(|t| to_sql_timestamp(&t)))
        .bind(draft.copied_at.map(|t| to_sql_timestamp(&t)))
        .bind(draft.rejected_at.map(|t| to_sql_timestamp(&t)))
        .bind(&draft.draft_group_id)
        .bind(&draft.routing_decision_json)
        .bind(draft.cost_cents)
        .bind(draft.prompt_tokens)
        .bind(draft.completion_tokens)
        .bind(LOCAL_OWNER_ID)
        .bind(LOCAL_TENANT_ID)
        .bind(to_sql_timestamp(&draft.created_at))
        .bind(to_sql_timestamp(&draft.updated_at))
        .execute(&mut *tx)
        .await
        .map_err(crate::store::repo_error)?;
        // The usage upsert rides the SAME transaction (the baseline records
        // usage inside the draft's transaction; a pool connection here would
        // deadlock on this transaction's own write lock).
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        crate::ai_models::upsert_usage(
            &mut tx,
            &today,
            model_id,
            draft.prompt_tokens.unwrap_or(0),
            draft.completion_tokens.unwrap_or(0),
            draft.cost_cents.unwrap_or(0),
        )
        .await
        .map_err(crate::store::repo_error)?;
        insert_audit(&mut tx, clone_entry(generated_audit))
            .await
            .map_err(crate::store::repo_error)?;
        tx.commit().await.map_err(crate::store::repo_error)
    }
}

/// Rebuild an audit entry with a `'static` lifetime for the insert (the
/// event/entity fields are literals at the call sites; `insert_audit` stamps
/// its own `created_at`).
fn clone_entry(entry: &AuditEntry<'_>) -> AuditEntry<'static> {
    AuditEntry {
        event_type: leak_str(entry.event_type),
        entity_type: entry.entity_type.map(leak_str),
        entity_id: entry.entity_id.map(leak_str),
        summary: entry.summary.clone(),
        metadata: entry.metadata.clone(),
    }
}

/// The audit entry's static fields are literals at the call sites, so a
/// per-call leak is bounded and equivalent to the baseline's constants.
fn leak_str(value: &str) -> &'static str {
    Box::leak(value.to_string().into_boxed_str())
}

/// The full cause chain of a reqwest error, rendered like httpx's exception
/// text — the top-level Display alone ("error sending request for url …")
/// hides whether it was DNS, connect, TLS or the proxy that failed.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// The provider response (`AIResponse`): the message text, the echoed model
/// and the raw usage object.
struct ProviderResponse {
    content: String,
    model: String,
    provider: String,
    raw_usage: serde_json::Value,
}

/// `OpenAICompatibleProvider.generate` — one chat-completions POST with the
/// model config's temperature/max tokens/timeout, and the baseline's exact
/// error texts on every failure path. The `opencode` provider type adds the
/// gateway's mandated `x-opencode-session` header and its own user agent
/// (the baseline client sends only Bearer + Content-Type; the go gateway
/// rejects anything else with 400 `MissingSessionID`).
#[allow(clippy::too_many_arguments)]
async fn call_provider(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model_id: &str,
    provider_type: &str,
    session_id: &str,
    timeout_secs: i64,
    temperature: f64,
    max_tokens: i64,
    user_prompt: &str,
) -> Result<ProviderResponse, ApplicationError> {
    let provider_name = format!("openai_compatible:{model_id}");
    let shape = |message: &str| ApplicationError::ProviderUnavailable {
        message: message.to_string(),
        details: serde_json::json!({ "provider": provider_name, "model": model_id }),
    };

    let payload = serde_json::json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_prompt },
        ],
        "temperature": temperature,
        "max_tokens": max_tokens,
        "response_format": { "type": "json_object" },
    });

    let mut request = http
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(api_key)
        .timeout(Duration::from_secs(timeout_secs.max(1) as u64))
        .json(&payload);
    if provider_type == "opencode" {
        request = request
            .header("x-opencode-session", session_id)
            .header("User-Agent", "ica-personal/1.0");
    }
    let response = request.send().await;
    let response = match response {
        Ok(response) => response,
        Err(error) if error.is_timeout() => {
            return Err(ApplicationError::ProviderUnavailable {
                message: format!("The AI provider timed out after {timeout_secs}s"),
                details: serde_json::json!({ "provider": provider_name, "model": model_id }),
            });
        }
        Err(error) => {
            return Err(ApplicationError::ProviderUnavailable {
                message: format!("Could not reach the AI provider: {}", error_chain(&error)),
                details: serde_json::json!({ "provider": provider_name, "model": model_id }),
            });
        }
    };

    let status = response.status().as_u16();
    if status >= 400 {
        return Err(ApplicationError::ProviderUnavailable {
            message: format!("The AI provider returned HTTP {status}"),
            details: serde_json::json!({
                "provider": provider_name,
                "model": model_id,
                "status_code": status,
            }),
        });
    }

    let body: serde_json::Value = match response.json().await {
        Ok(body) => body,
        Err(_) => {
            return Err(shape(
                "The AI provider returned an unexpected response shape",
            ))
        }
    };
    let content = body
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .ok_or_else(|| shape("The AI provider returned an unexpected response shape"))?;
    // Python: `content or ""` — an explicit null content is an empty message.
    let content = match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        _ => {
            return Err(shape(
                "The AI provider returned an unexpected response shape",
            ))
        }
    };
    let model = body
        .get("model")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(model_id)
        .to_string();
    let raw_usage = body.get("usage").cloned().unwrap_or(serde_json::json!({}));

    Ok(ProviderResponse {
        content,
        model,
        provider: provider_name,
        raw_usage,
    })
}

/// `extract_json` — models wrap JSON in code fences, reasoning blocks or
/// prose, so the payload is located rather than assumed to be the whole
/// response. Reasoning models put `<think>…</think>` before the JSON; the
/// think text may contain braces, so it is removed first.
fn extract_json(text: &str) -> Result<serde_json::Value, ApplicationError> {
    let think = regex::Regex::new(r"(?is)<think>.*?</think>").expect("think pattern");
    let cleaned = think.replace_all(text, "").trim().to_string();
    let mut cleaned = cleaned.as_str();
    if cleaned.starts_with("```") {
        let stripped = cleaned.trim_matches('`');
        let stripped = if stripped.len() >= 4 && stripped[..4].eq_ignore_ascii_case("json") {
            &stripped[4..]
        } else {
            stripped
        };
        cleaned = stripped.trim();
    }

    let parsed = match serde_json::from_str::<serde_json::Value>(cleaned) {
        Ok(parsed) => parsed,
        Err(_) => {
            let start = cleaned.find('{');
            let end = cleaned.rfind('}');
            let (Some(start), Some(end)) = (start, end) else {
                return Err(ApplicationError::ProviderUnavailable {
                    message: "The AI response did not contain a JSON object".to_string(),
                    details: serde_json::json!({ "preview": preview(cleaned) }),
                });
            };
            if end <= start {
                return Err(ApplicationError::ProviderUnavailable {
                    message: "The AI response did not contain a JSON object".to_string(),
                    details: serde_json::json!({ "preview": preview(cleaned) }),
                });
            }
            match serde_json::from_str::<serde_json::Value>(&cleaned[start..=end]) {
                Ok(parsed) => parsed,
                Err(_) => {
                    return Err(ApplicationError::ProviderUnavailable {
                        message: "The AI response contained malformed JSON".to_string(),
                        details: serde_json::json!({
                            "preview": preview(&cleaned[start..(start + 200).min(cleaned.len())]),
                        }),
                    });
                }
            }
        }
    };
    if !parsed.is_object() {
        return Err(ApplicationError::ProviderUnavailable {
            message: "The AI response JSON was not an object".to_string(),
            details: serde_json::json!({ "type": python_type_name(&parsed) }),
        });
    }
    Ok(parsed)
}

/// `cleaned[:200]` — byte-slice clamped to a char boundary like Python's
/// string slice.
fn preview(text: &str) -> String {
    let mut end = text.len().min(200);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// `type(parsed).__name__` for the not-an-object error's details.
fn python_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "NoneType",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(number) => {
            if number.is_f64() {
                "float"
            } else {
                "int"
            }
        }
        serde_json::Value::String(_) => "str",
        serde_json::Value::Array(_) => "list",
        serde_json::Value::Object(_) => "dict",
    }
}

/// `_as_str_list` — coerce a model-supplied field into a list of strings.
fn as_str_list(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::String(text)) => {
            if text.trim().is_empty() {
                Vec::new()
            } else {
                vec![text.clone()]
            }
        }
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                serde_json::Value::String(text) => text.trim().to_string(),
                other => other.to_string(),
            })
            .filter(|text| !text.is_empty())
            .collect(),
        Some(other) => vec![other.to_string()],
    }
}

/// `int(usage.get(key) or 0)` — numeric or digit-string usage fields.
fn usage_int(usage: &serde_json::Value, key: &str) -> i64 {
    match usage.get(key) {
        Some(serde_json::Value::Number(number)) => number.as_i64().unwrap_or(0),
        Some(serde_json::Value::String(text)) => text.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

#[async_trait::async_trait]
impl DraftGenerator for AiDraftGenerator {
    async fn generate(
        &self,
        request: GenerateDraftRequest,
    ) -> Result<DraftGeneration, ApplicationError> {
        // The shared context is built ONCE for both paths (one context per
        // comparison group). V1.0 semantics: no token truncation.
        let context =
            build_context(&self.store, &self.store, &self.store, &request.candidate_id).await?;
        let context_text = render_context(&context);

        // The rows the validator re-reads (candidate status / position state).
        // `find` is ambiguous on the store (both repos name it): the calls are
        // fully qualified.
        let candidate: Candidate = CandidateRepository::find(&self.store, &request.candidate_id)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| ApplicationError::not_found("Candidate", &request.candidate_id))?;
        let position = match &candidate.position_id {
            Some(position_id) => PositionRepository::find(&self.store, position_id)
                .await
                .map_err(unavailable)?,
            None => None,
        };

        if !request.comparison_models.is_empty() {
            // A/B comparison: every model is a user choice, so the routing
            // reason is `manual_override` regardless of the budget state.
            // Unknown or disabled models raise immediately, so a partially
            // invalid group never slips through.
            let mut drafts = Vec::with_capacity(request.comparison_models.len());
            let group_id = new_uuid();
            for model_id in &request.comparison_models {
                let model = get_model(&self.store, &self.view, model_id).await?;
                let tier = model.quality_tier.clone();
                let decision = RoutingDecision {
                    requested_tier: tier.clone(),
                    actual_tier: tier,
                    reason: "manual_override".to_string(),
                    model,
                };
                drafts.push(
                    self.generate_one(
                        &request,
                        &candidate,
                        position.as_ref(),
                        &context.redaction_applied,
                        &context_text,
                        &decision,
                        Some(&group_id),
                    )
                    .await?,
                );
            }
            return Ok(DraftGeneration::Group(drafts));
        }

        // Single-model path (V1.0 compatible): the routing decision records
        // whether it was a scenario match, a manual override, or a budget
        // downgrade.
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let decision = select_model(
            &self.store,
            &self.store,
            &self.budget_snapshot(),
            &self.view,
            request.message_type.as_str(),
            request.model_id.as_deref(),
            &today,
        )
        .await?;
        let draft = self
            .generate_one(
                &request,
                &candidate,
                position.as_ref(),
                &context.redaction_applied,
                &context_text,
                &decision,
                None,
            )
            .await?;
        Ok(DraftGeneration::Single(Box::new(draft)))
    }
}
