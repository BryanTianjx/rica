/**
 * 设置页 AI 模型配置面板 —— V2.0 自配置（Provider / 端点 / 模型 ID / 层级 /
 * 超时 / 温度 / 最大输出 tokens），以及已配置模型的「设默认 / 编辑 / 删除」。
 *
 * 后端契约（t_f728bf55，全局前缀 `/api`）：
 * * 列表是 `{models, routing}`；`source="settings"` 表示 `ai_model_configs`
 *   为空时后端从 `.env` 构造的回落行 —— 这一类行不能 PUT/DELETE（会 404），
 *   只能在表单里保存一次、写成数据库配置。
 * * `model_id` 是行标识（同时也是凭据槽位名），**不可更新**：改 ID 等于新建
 *   配置。URL 里的 ID 必须百分号编码（含 `/` 与 `:`）。
 * * 运行时默认模型 = 第一个「已启用且 is_default」的行，没有则回落排序最前的
 *   启用行（与 `ModelRegistry.get_default_model` 一致）。本面板据此显示
 *   「当前生效配置」，并明确告诉用户生效值来自数据库还是 `.env`。
 * * 响应永不含 key，只有 `api_key_configured`。
 */

import { useState } from 'react';
import { backend } from '../services/backend';
import { ApiError } from '../services/backend';
import { Badge, EmptyState, ErrorBox, Loading } from './ui';
import { ConfirmBar } from './ConfirmBar';
import type { AiModel, AiModelsResponse, AiModelWrite, AiQualityTier } from '../types';

/**
 * 供应商预设：每个供应商的端点都是 OpenAI 兼容线协议（后端统一分发到
 * OpenAICompatibleProvider），选预设自动填默认端点——可手动改。value 与后端
 * SUPPORTED_PROVIDER_TYPES 白名单一一对应。
 */
const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    value: 'z.ai',
    label: 'z.ai（智谱 GLM）',
    baseUrl: 'https://api.z.ai/api/paas/v4',
    models: ['glm-4.6', 'glm-4.5', 'glm-4.5-air'],
    keyUrl: 'https://z.ai',
  },
  {
    value: 'minimax-cn',
    label: 'MiniMax（中国）',
    // 官方文档（platform.minimax.cn → Chat Completions API）：
    // POST https://api.minimax.cn/v1/chat/completions，Authorization: Bearer <key>
    baseUrl: 'https://api.minimax.cn/v1',
    models: [
      'MiniMax-M3',
      'MiniMax-M2.7',
      'MiniMax-M2.7-highspeed',
      'MiniMax-M2.5',
      'MiniMax-M2.5-highspeed',
      'MiniMax-M2.1',
      'MiniMax-M2.1-highspeed',
      'MiniMax-M2',
    ],
    keyHint: '在 platform.minimax.cn 控制台「账户管理 > 接口密钥」创建（按量付费）或使用 M Plan 订阅 Key。',
    keyUrl: 'https://platform.minimax.cn',
  },
  {
    value: 'opencode',
    label: 'opencode',
    baseUrl: 'https://opencode.ai/zen/v1',
    models: [],
  },
  {
    value: 'openai_compatible',
    label: '自定义（OpenAI 兼容端点）',
    baseUrl: '',
    models: [],
  },
];

type ProviderPreset = {
  value: string;
  label: string;
  baseUrl: string;
  /** 模型 ID 建议清单（随供应商更新）；未维护的供应商为空数组=纯手填。 */
  models: string[];
  /** 该供应商 API Key 的官方获取指引（无则不显示）。 */
  keyHint?: string;
  keyUrl?: string;
};

/** Legacy 单值常量的调用点全部改走预设表。 */
const PROVIDER_TYPES = PROVIDER_PRESETS.map((preset) => preset.value);
const QUALITY_TIERS: AiQualityTier[] = ['economy', 'standard', 'premium'];

/** 表单值全部保留成字符串，数字在提交前才解析，用户输入不会被吞掉。 */
type ModelDraft = {
  model_id: string;
  display_name: string;
  provider_type: string;
  base_url: string;
  quality_tier: AiQualityTier;
  timeout: string;
  temperature: string;
  max_output_tokens: string;
  enabled: boolean;
  is_default: boolean;
};

function draftFromModel(model: AiModel): ModelDraft {
  return {
    model_id: model.model_id,
    display_name: model.display_name,
    provider_type: model.provider_type,
    base_url: model.base_url,
    quality_tier: model.quality_tier,
    timeout: String(model.timeout),
    temperature: String(model.temperature),
    max_output_tokens: String(model.max_output_tokens),
    enabled: model.enabled,
    is_default: model.is_default,
  };
}

/** 新建表单：库里还没有配置时，第一个模型默认就是生效模型。 */
function emptyDraft(isFirstModel: boolean): ModelDraft {
  return {
    model_id: '',
    display_name: '',
    provider_type: PROVIDER_TYPES[0],
    base_url: PROVIDER_PRESETS[0].baseUrl,
    quality_tier: 'standard',
    timeout: '60',
    temperature: '0.4',
    max_output_tokens: '1500',
    enabled: true,
    is_default: isFirstModel,
  };
}

function presetOf(providerType: string) {
  return PROVIDER_PRESETS.find((preset) => preset.value === providerType) ?? null;
}

/** 当前供应商的 API Key 获取提示（无官方指引的供应商返回 null）。 */
function keyHintFor(providerType: string): string | null {
  return presetOf(providerType)?.keyHint ?? null;
}

function keyUrlFor(providerType: string): string | null {
  return presetOf(providerType)?.keyUrl ?? null;
}

/** 模型 ID 建议清单随供应商配置更新；未维护清单的供应商返回空（纯手填）。 */
function modelOptionsFor(providerType: string): string[] {
  return presetOf(providerType)?.models ?? [];
}

/** 数值字段的可读校验；返回 null 表示通过。 */
function numberIssue(
  label: string,
  raw: string,
  min: number,
  max: number,
  integer: boolean,
): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return `${label}不能为空。`;
  const value = Number(trimmed);
  if (!Number.isFinite(value)) return `${label}必须是数字。`;
  if (integer && !Number.isInteger(value)) return `${label}必须是整数。`;
  if (value < min || value > max) return `${label} 必须在 ${min}–${max} 之间。`;
  return null;
}

/**
 * 客户端校验（与后端 ModelCreate/ModelUpdate 的边界一致）。
 *
 * 后端没有 extra=forbid，字段名写错会被静默丢弃，所以这里宁可先拦住明显错误，
 * 也不让无效值悄悄落到数据库。
 */
function validateDraft(draft: ModelDraft): string | null {
  const modelId = draft.model_id.trim();
  if (!modelId) return '模型 ID 不能为空。';
  if (/\s/.test(modelId)) return '模型 ID 不能包含空格或换行。';
  if (modelId.length > 128) return '模型 ID 最长 128 个字符。';
  if (!(PROVIDER_TYPES as readonly string[]).includes(draft.provider_type)) {
    return '供应商必须是预设之一：z.ai / minimax-cn / opencode / 自定义。';
  }
  const baseUrl = draft.base_url.trim();
  if (!baseUrl) return '端点不能为空。';
  if (!/^https?:\/\//i.test(baseUrl)) return '端点必须以 http:// 或 https:// 开头。';
  const timeoutIssue = numberIssue('超时（秒）', draft.timeout, 1, 600, true);
  if (timeoutIssue) return timeoutIssue;
  const temperatureIssue = numberIssue('采样温度', draft.temperature, 0, 2, false);
  if (temperatureIssue) return temperatureIssue;
  const outputIssue = numberIssue('最大输出 tokens', draft.max_output_tokens, 1, 200000, true);
  if (outputIssue) return outputIssue;
  if (draft.is_default && !draft.enabled) {
    return '禁用的模型不能设为默认：请先勾选「启用」。';
  }
  return null;
}

/** 只在校验通过后调用；字段名逐个对齐后端 ModelWrite。 */
function buildPayload(draft: ModelDraft): AiModelWrite {
  return {
    display_name: draft.display_name.trim() || draft.model_id.trim(),
    provider_type: draft.provider_type,
    base_url: draft.base_url.trim(),
    quality_tier: draft.quality_tier,
    timeout: Number(draft.timeout.trim()),
    temperature: Number(draft.temperature.trim()),
    max_output_tokens: Number(draft.max_output_tokens.trim()),
    enabled: draft.enabled,
    is_default: draft.is_default,
  };
}

/** 后端错误码 → 可读中文；原始 message 附在括号里便于排障。 */
const ERROR_HINTS: Record<string, string> = {
  ai_model_exists: '该模型 ID 已存在：请直接编辑已配置的模型，或换一个 ID。',
  model_disabled: '禁用的模型不能作为默认模型：请先勾选「启用」。',
  invalid_base_url: '端点必须以 http:// 或 https:// 开头。',
  unsupported_provider_type: '不支持的供应商；可选 z.ai / minimax-cn / opencode / 自定义。',
  invalid_quality_tier: '层级只能是 economy / standard / premium。',
  validation_error: '字段超出允许范围，请检查数值上下界。',
  not_found: '该模型配置已不存在；列表已刷新，请重试。',
  admin_required: '当前账户没有管理 AI 配置的权限。',
};

function readableError(caught: unknown): string {
  if (caught instanceof ApiError) {
    const hint = ERROR_HINTS[caught.code];
    return hint ? `${hint}（${caught.code}: ${caught.message}）` : `${caught.message}（${caught.code}）`;
  }
  return caught instanceof Error ? caught.message : '操作失败';
}

/** 与后端 `ModelRegistry.get_default_model` 同序：启用且显式的默认 → 排序最前的启用行。 */
function effectiveModelOf(models: AiModel[]): AiModel | null {
  const explicitDefaults = models.filter((model) => model.is_default && model.enabled);
  if (explicitDefaults.length > 0) return explicitDefaults[0];
  return models.find((model) => model.enabled) ?? null;
}

export function AiModelConfigPanel({
  models,
  loading,
  error,
  onReload,
}: {
  models: AiModelsResponse | null;
  loading: boolean;
  error: string | null;
  onReload: () => void;
}) {
  const [draft, setDraft] = useState<ModelDraft | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [rowBusy, setRowBusy] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const rows = models?.models ?? [];
  const databaseRows = rows.filter((model) => model.source === 'database');
  const effective = effectiveModelOf(rows);
  // 编辑目标按全量列表找（含 .env 回落行）：回落行不能 PUT，但表单需要提示
  // 「保存后会写入数据库」。
  const editingRow = editingId
    ? rows.find((model) => model.model_id === editingId) ?? null
    : null;
  const idChanged = !!editingRow && draft !== null && draft.model_id.trim() !== editingRow.model_id;

  function patch(patchValue: Partial<ModelDraft>) {
    setDraft((current) => (current ? { ...current, ...patchValue } : current));
  }

  function startEdit(model: AiModel) {
    setDraft(draftFromModel(model));
    setEditingId(model.model_id);
    setFormError(null);
    setNotice(null);
    setActionError(null);
  }

  function startCreate() {
    setDraft(emptyDraft(databaseRows.length === 0));
    setEditingId(null);
    setFormError(null);
    setNotice(null);
    setActionError(null);
  }

  function cancelEdit() {
    setDraft(null);
    setEditingId(null);
    setFormError(null);
  }

  async function submitDraft(event: React.FormEvent) {
    event.preventDefault();
    if (!draft) return;
    const issue = validateDraft(draft);
    if (issue) {
      setFormError(issue);
      setNotice(null);
      return;
    }
    const modelId = draft.model_id.trim();
    const payload = buildPayload(draft);
    // 已存在的数据库行 → 更新；新 ID（或 .env 回落行）→ 新建。
    const existing = databaseRows.find((model) => model.model_id === modelId);
    setSaving(true);
    setFormError(null);
    setNotice(null);
    setActionError(null);
    try {
      if (existing) {
        await backend.ai.updateModel(existing.model_id, payload);
        setNotice(`模型配置「${modelId}」已更新。`);
      } else {
        await backend.ai.createModel({ model_id: modelId, ...payload });
        setNotice(`模型配置「${modelId}」已创建。`);
      }
      setDraft(null);
      setEditingId(null);
      onReload();
    } catch (caught: unknown) {
      setFormError(readableError(caught));
    } finally {
      setSaving(false);
    }
  }

  async function makeDefault(model: AiModel) {
    setRowBusy(model.model_id);
    setActionError(null);
    setNotice(null);
    try {
      await backend.ai.setDefaultModel(model.model_id);
      setNotice(`已把「${model.model_id}」设为默认生效模型。`);
      onReload();
    } catch (caught: unknown) {
      setActionError(readableError(caught));
    } finally {
      setRowBusy(null);
    }
  }

  async function confirmDelete(modelId: string) {
    setRowBusy(modelId);
    setActionError(null);
    setNotice(null);
    try {
      await backend.ai.deleteModel(modelId);
      if (editingId === modelId) {
        setDraft(null);
        setEditingId(null);
      }
      setNotice(`模型配置「${modelId}」已删除。`);
      onReload();
    } catch (caught: unknown) {
      setActionError(readableError(caught));
    } finally {
      setDeletingId(null);
      setRowBusy(null);
    }
  }

  return (
    <div>
      {notice && (
        <div className="notice" aria-live="polite">
          {notice}
        </div>
      )}
      {actionError && <ErrorBox message={actionError} />}
      {loading && <Loading />}
      {error && <ErrorBox message={error} />}

      {models && (
        <>
          <h3>当前生效配置</h3>
          {effective ? (
            <table>
              <tbody>
                <tr>
                  <td className="muted">Provider</td>
                  <td>{effective.provider_type}</td>
                </tr>
                <tr>
                  <td className="muted">模型</td>
                  <td className="mono">{effective.model_id}</td>
                </tr>
                <tr>
                  <td className="muted">端点</td>
                  <td className="mono">{effective.base_url}</td>
                </tr>
                <tr>
                  <td className="muted">生效来源</td>
                  <td>
                    {effective.source === 'database' ? (
                      <Badge tone="info">数据库配置</Badge>
                    ) : (
                      <Badge>.env 回落</Badge>
                    )}
                  </td>
                </tr>
              </tbody>
            </table>
          ) : (
            <EmptyState message="还没有启用的 AI 模型" />
          )}
          <p className="muted small">
            {effective === null
              ? '没有任何启用的模型：草稿生成会失败。请新增一条配置，或启用已有配置。'
              : effective.source === 'database'
                ? `草稿生成使用数据库里的默认模型「${effective.model_id}」。修改 backend/.env 的 AI_MODEL / AI_BASE_URL 不会改变这里的生效值 —— 要换模型请编辑上方配置，或在下方列表点「设为默认」。${
                    effective.is_default
                      ? ''
                      : '当前没有显式默认模型，按排序回落到第一个启用模型。'
                  }`
                : '数据库里还没有模型配置，当前生效值来自 .env 回落（AI_MODEL / AI_BASE_URL / AI_PROVIDER）。在这里保存一次会写入数据库，并从此优先于 .env。'}
          </p>
          <div className="actions">
            {effective && (
              <button type="button" onClick={() => startEdit(effective)}>
                编辑生效配置
              </button>
            )}
            <button type="button" onClick={startCreate}>
              新增模型配置
            </button>
          </div>

          {draft && (
            <form onSubmit={submitDraft} noValidate>
              {/*
               * noValidate：数字输入带 min/max 只为给浏览器微调按钮一个范围提示，
               * 真正的拦截与中文提示由 validateDraft 统一给出 —— 否则浏览器原生
               * 约束校验会先拦下提交，用户看不到站点自己的可读错误。
               */}
              <h3>{editingId ? '编辑模型配置' : '新增模型配置'}</h3>
              {editingRow?.source === 'settings' && (
                <p className="muted small">
                  该配置来自 .env 回落，还不能直接修改：保存后会写入数据库，并优先于
                  .env。
                </p>
              )}
              {idChanged && (
                <p className="muted small">
                  模型 ID 已改变：保存时会新建一条配置，原有配置保持不变。
                </p>
              )}
              <div className="form-row two">
                <div>
                  <label htmlFor="ai-form-provider">Provider *</label>
                  <select
                    id="ai-form-provider"
                    value={draft.provider_type}
                    onChange={(event) => {
                      const preset = PROVIDER_PRESETS.find((p) => p.value === event.target.value);
                      if (!preset) return;
                      // 端点为空或仍是另一个预设的默认值时才覆盖，避免抹掉手改端点。
                      const current = draft.base_url.trim();
                      const isPresetDefault =
                        current === '' ||
                        PROVIDER_PRESETS.some((p) => p.baseUrl !== '' && p.baseUrl === current);
                      // 模型 ID 同步供应商预设清单：为空时填首个建议，已填则保留。
                      const modelId =
                        draft.model_id.trim() === '' && preset.models.length > 0
                          ? preset.models[0]
                          : draft.model_id;
                      patch({
                        provider_type: preset.value,
                        base_url: isPresetDefault ? preset.baseUrl : draft.base_url,
                        model_id: modelId,
                      });
                    }}
                  >
                    {PROVIDER_PRESETS.map((preset) => (
                      <option key={preset.value} value={preset.value}>
                        {preset.label}
                      </option>
                    ))}
                  </select>
                  {keyHintFor(draft.provider_type) && (
                    <p className="muted small mt-4">
                      API Key 获取：{keyHintFor(draft.provider_type)}
                      {keyUrlFor(draft.provider_type) && (
                        <>
                          {' '}
                          <a href={keyUrlFor(draft.provider_type)!} target="_blank" rel="noreferrer">
                            打开控制台 ↗
                          </a>
                        </>
                      )}
                    </p>
                  )}
                </div>
                <div>
                  <label htmlFor="ai-form-model-id">模型 ID *</label>
                  <input
                    id="ai-form-model-id"
                    className="mono"
                    value={draft.model_id}
                    list="ai-form-model-options"
                    placeholder="输入或从建议中选择"
                    onChange={(event) => patch({ model_id: event.target.value })}
                  />
                  <datalist id="ai-form-model-options">
                    {modelOptionsFor(draft.provider_type).map((modelId) => (
                      <option key={modelId} value={modelId} />
                    ))}
                  </datalist>
                </div>
              </div>
              <div className="form-row">
                <label htmlFor="ai-form-base-url">端点（Base URL）*</label>
                <input
                  id="ai-form-base-url"
                  className="mono"
                  value={draft.base_url}
                  placeholder="https://api.openai.com/v1"
                  onChange={(event) => patch({ base_url: event.target.value })}
                />
              </div>
              <div className="form-row two">
                <div>
                  <label htmlFor="ai-form-display-name">显示名</label>
                  <input
                    id="ai-form-display-name"
                    value={draft.display_name}
                    placeholder="留空则用模型 ID"
                    onChange={(event) => patch({ display_name: event.target.value })}
                  />
                </div>
                <div>
                  <label htmlFor="ai-form-tier">层级</label>
                  <select
                    id="ai-form-tier"
                    value={draft.quality_tier}
                    onChange={(event) =>
                      patch({ quality_tier: event.target.value as AiQualityTier })
                    }
                  >
                    {QUALITY_TIERS.map((value) => (
                      <option key={value} value={value}>
                        {value}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
              <div className="form-row two">
                <div>
                  <label htmlFor="ai-form-timeout">超时（秒，1–600）</label>
                  <input
                    id="ai-form-timeout"
                    type="number"
                    min={1}
                    max={600}
                    value={draft.timeout}
                    onChange={(event) => patch({ timeout: event.target.value })}
                  />
                </div>
                <div>
                  <label htmlFor="ai-form-temperature">采样温度（0–2）</label>
                  <input
                    id="ai-form-temperature"
                    type="number"
                    step="0.1"
                    min={0}
                    max={2}
                    value={draft.temperature}
                    onChange={(event) => patch({ temperature: event.target.value })}
                  />
                </div>
              </div>
              <div className="form-row">
                <label htmlFor="ai-form-max-output">最大输出 tokens（1–200000）</label>
                <input
                  id="ai-form-max-output"
                  type="number"
                  min={1}
                  max={200000}
                  value={draft.max_output_tokens}
                  onChange={(event) => patch({ max_output_tokens: event.target.value })}
                />
              </div>
              <div className="form-row two">
                <div>
                  <label htmlFor="ai-form-enabled">
                    <input
                      id="ai-form-enabled"
                      type="checkbox"
                      checked={draft.enabled}
                      onChange={(event) => patch({ enabled: event.target.checked })}
                    />
                    启用（可被草稿生成使用）
                  </label>
                </div>
                <div>
                  <label htmlFor="ai-form-default">
                    <input
                      id="ai-form-default"
                      type="checkbox"
                      checked={draft.is_default}
                      onChange={(event) => patch({ is_default: event.target.checked })}
                    />
                    设为默认模型（草稿生成使用）
                  </label>
                </div>
              </div>
              {formError && <ErrorBox message={formError} />}
              <div className="actions">
                <button className="primary" type="submit" disabled={saving}>
                  {saving ? '保存中…' : '保存配置'}
                </button>
                <button type="button" onClick={cancelEdit} disabled={saving}>
                  取消
                </button>
              </div>
            </form>
          )}

          <h3>已配置模型</h3>
          {rows.length === 0 ? (
            <EmptyState message="还没有配置 AI 模型" />
          ) : (
            <table aria-label="已配置模型">
              <thead>
                <tr>
                  <th>模型</th>
                  <th>层级</th>
                  <th>来源</th>
                  <th>默认</th>
                  <th>Key</th>
                  <th>状态</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((model) => (
                  <tr key={`${model.source}:${model.model_id}`}>
                    <td>
                      {model.display_name}
                      <div className="muted mono small">{model.model_id}</div>
                    </td>
                    <td>{model.quality_tier}</td>
                    <td>
                      {model.source === 'database' ? (
                        <Badge tone="info">数据库</Badge>
                      ) : (
                        <Badge>.env</Badge>
                      )}
                    </td>
                    <td>{model.is_default ? <Badge tone="ok">默认</Badge> : '—'}</td>
                    <td>
                      {model.api_key_configured ? (
                        <Badge tone="ok">已配置</Badge>
                      ) : (
                        <Badge tone="warn">未配置</Badge>
                      )}
                    </td>
                    <td>
                      {model.enabled ? (
                        <Badge tone="ok">启用</Badge>
                      ) : (
                        <Badge tone="warn">禁用</Badge>
                      )}
                    </td>
                    <td>
                      {deletingId === model.model_id ? (
                        <ConfirmBar
                          message={
                            model.is_default ? (
                              <>
                                「<strong>{model.model_id}</strong>」是当前默认模型：删除后
                                不会自动指定新的默认模型。确定删除？
                              </>
                            ) : (
                              <>
                                确定删除模型配置「<strong>{model.model_id}</strong>」？
                              </>
                            )
                          }
                          onConfirm={() => confirmDelete(model.model_id)}
                          onCancel={() => setDeletingId(null)}
                          busy={rowBusy === model.model_id}
                          confirmLabel="确认删除"
                          busyLabel="删除中…"
                        />
                      ) : (
                        <div className="actions">
                          <button
                            type="button"
                            onClick={() => makeDefault(model)}
                            disabled={
                              rowBusy !== null ||
                              model.source !== 'database' ||
                              model.is_default ||
                              !model.enabled
                            }
                            title={
                              model.source !== 'database'
                                ? '.env 回落配置不是数据库记录，保存后才可设为默认'
                                : !model.enabled
                                  ? '禁用的模型不能设为默认'
                                  : model.is_default
                                    ? '已经是默认模型'
                                    : '设为默认生效模型'
                            }
                          >
                            设为默认
                          </button>
                          <button
                            type="button"
                            onClick={() => startEdit(model)}
                            disabled={rowBusy !== null}
                          >
                            编辑
                          </button>
                          <button
                            type="button"
                            className="danger"
                            onClick={() => setDeletingId(model.model_id)}
                            disabled={rowBusy !== null || model.source !== 'database'}
                            title={
                              model.source !== 'database'
                                ? '.env 回落配置不是数据库记录，无法删除'
                                : '删除这条模型配置'
                            }
                          >
                            删除
                          </button>
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}

          {Object.keys(models.routing).length > 0 && (
            <>
              <h3>场景路由</h3>
              <p className="muted">
                按消息类型选择生成模型层级。空值表示使用默认（economy/standard/premium）。
              </p>
              <table>
                <thead>
                  <tr>
                    <th>消息类型</th>
                    <th>层级</th>
                  </tr>
                </thead>
                <tbody>
                  {Object.entries(models.routing).map(([type, tier]) => (
                    <tr key={type}>
                      <td>{type}</td>
                      <td>{tier}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </>
      )}
    </div>
  );
}
