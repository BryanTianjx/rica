import { useState } from 'react';
import { useAsync } from '../hooks/useAsync';
import { aiApi, followUpsApi, settingsApi } from '../services/endpoints';
import { Badge, EmptyState, ErrorBox, Loading, LocalTime } from '../components/ui';
import { ConfirmBar } from '../components/ConfirmBar';
import { AiModelConfigPanel } from '../components/AiModelConfigPanel';
import { redactionLabel } from '../privacyLabels';

type PolicyDraft = {
  first_follow_up_delay_hours: number;
  max_active_follow_ups: number;
  min_interval_hours: number;
  task_validity_days: number;
};

export function SettingsPage() {
  const settings = useAsync(() => settingsApi.get(), []);
  const policy = useAsync(() => followUpsApi.getPolicy(), []);
  const backups = useAsync(() => settingsApi.listBackups(), []);
  const models = useAsync(() => aiApi.listModels(), []);
  // PRIV-07: where AI context goes, what is masked, and the egress switch.
  const egress = useAsync(() => settingsApi.getAiEgress(), []);
  // G409-GAP: 审计记录查看入口（OFF-017「断网下可查看审计记录」的 UI 面）。
  const [auditEventType, setAuditEventType] = useState('');
  const [auditEntityType, setAuditEntityType] = useState('');
  const audit = useAsync(
    () =>
      settingsApi.listAuditEvents({
        event_type: auditEventType || undefined,
        entity_type: auditEntityType || undefined,
        limit: 100,
      }),
    [auditEventType, auditEntityType],
  );

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // AI key 桌面端配置（G409-GAP 后续）：输入只写一次，保存后立即清空输入框；
  // 状态以 settings.ai_enabled 为准，接口永不回显完整密钥。
  const [aiKeyInput, setAiKeyInput] = useState('');
  // 密钥写入范围：'' = 安装级槽位（所有模型共用），否则写该模型的独立槽位。
  const [aiKeyTargetId, setAiKeyTargetId] = useState('');

  const keyTargetModel = aiKeyTargetId
    ? models.data?.models.find((model) => model.model_id === aiKeyTargetId) ?? null
    : null;
  // 选中的模型被删除（或列表刷新掉了）时回落到安装级，避免把密钥写进一个
  // 已经不存在的模型的槽位。
  const aiKeyTarget = keyTargetModel ? keyTargetModel.model_id : '';
  const keyConfigured = keyTargetModel
    ? keyTargetModel.api_key_configured
    : Boolean(settings.data?.ai_enabled);

  async function saveAiKey() {
    if (!aiKeyInput.trim()) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    const key = aiKeyInput.trim();
    try {
      // 不带 model_id 时写安装级槽位（保持既有行为与调用形状）。
      const result = aiKeyTarget
        ? await settingsApi.setAiKey(key, aiKeyTarget)
        : await settingsApi.setAiKey(key);
      setNotice(
        aiKeyTarget
          ? `API Key 已保存到模型「${aiKeyTarget}」的独立槽位（尾号 ${result.key_masked ?? '—'}），该模型优先使用它。`
          : `API Key 已保存并生效（尾号 ${result.key_masked ?? '—'}）。AI 功能${
              result.ai_enabled ? '已启用' : '仍未启用'
            }。`,
      );
      setAiKeyInput('');
      settings.reload();
      models.reload();
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '保存失败');
    } finally {
      setBusy(false);
    }
  }

  async function clearAiKey() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (aiKeyTarget) {
        await settingsApi.clearAiKey(aiKeyTarget);
        setNotice(`API Key 已清除（模型「${aiKeyTarget}」的独立槽位）。`);
      } else {
        await settingsApi.clearAiKey();
        setNotice('API Key 已清除，AI 功能停用。');
      }
      setAiKeyInput('');
      settings.reload();
      models.reload();
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '清除失败');
    } finally {
      setBusy(false);
    }
  }

  const [draftPolicy, setDraftPolicy] = useState<PolicyDraft | null>(null);

  /** Merge one edited field into the pending policy draft. */
  function updatePolicyField<K extends keyof PolicyDraft>(
    key: K,
    value: PolicyDraft[K],
  ) {
    const current = policy.data;
    const base: PolicyDraft = draftPolicy ?? {
      first_follow_up_delay_hours: current?.first_follow_up_delay_hours ?? 24,
      max_active_follow_ups: current?.max_active_follow_ups ?? 2,
      min_interval_hours: current?.min_interval_hours ?? 48,
      task_validity_days: current?.task_validity_days ?? 7,
    };
    setDraftPolicy({ ...base, [key]: value });
  }

  async function savePolicy() {
    if (!draftPolicy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await followUpsApi.updatePolicy(draftPolicy);
      setNotice('跟进策略已更新。');
      policy.reload();
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '保存失败');
    } finally {
      setBusy(false);
    }
  }

  /** Flip the outbound-AI switch; the backend audit-logs the change. */
  async function toggleEgress(granted: boolean) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await settingsApi.setAiEgressConsent(granted);
      setNotice(granted ? 'AI 出网已开启。' : 'AI 出网已关闭，草稿生成会被拒绝。');
      egress.reload();
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '保存失败');
    } finally {
      setBusy(false);
    }
  }

  async function createBackup() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const record = await settingsApi.createBackup();
      setNotice(`已创建备份 ${record.name}（${record.size_bytes} 字节）。`);
      backups.reload();
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '备份失败');
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>系统设置</h1>
          <p>跟进策略、AI 接入状态与本地数据备份。</p>
        </div>
      </div>

      {notice && (
        <div className="notice" aria-live="polite">
          {notice}
        </div>
      )}
      {error && <ErrorBox message={error} />}

      <div className="settings-layout">
        <nav className="settings-nav" aria-label="设置分区">
          <a href="#sec-ai">AI 接入</a>
          <a href="#sec-privacy">隐私与 AI 出网</a>
          <a href="#sec-policy">跟进策略</a>
          <a href="#sec-backup">本地备份</a>
          <a href="#sec-audit">审计记录</a>
        </nav>

        <div className="settings-main">
          <section id="sec-ai" className="settings-section">
            <div className="card">
              <div className="card-head">
                <h2>AI 接入</h2>
                <span className="muted small">
                  Provider / 端点 / 模型 / Key 都在这里配置；保存后立即生效
                </span>
              </div>
              {settings.loading && <Loading />}
              {settings.error && <ErrorBox message={settings.error} />}
              {settings.data && (
                <>
                  <p>
                    状态：{' '}
                    {settings.data.ai_enabled ? (
                      <Badge tone="ok">已启用</Badge>
                    ) : (
                      <Badge tone="warn">未配置</Badge>
                    )}
                  </p>
                  {/*
                   * 模型配置面板：当前生效配置 + 编辑表单 + 已配置模型表 + 场景路由。
                   * 数据由本页统一加载，面板负责写入并回调 reload，避免两处各自请求。
                   */}
                  <AiModelConfigPanel
                    models={models.data}
                    loading={models.loading}
                    error={models.error}
                    onReload={models.reload}
                  />

                  <div className="subpanel">
                    <h3>API Key</h3>
                    <div className="form-row two">
                      <div>
                        <label htmlFor="ai-key-target">作用范围</label>
                        <select
                          id="ai-key-target"
                          value={aiKeyTarget}
                          onChange={(event) => setAiKeyTargetId(event.target.value)}
                        >
                          <option value="">安装级 — 所有模型共用</option>
                          {(models.data?.models ?? [])
                            .filter((model) => model.source === 'database')
                            .map((model) => (
                              <option key={model.model_id} value={model.model_id}>
                                {model.display_name}（{model.model_id}）
                              </option>
                            ))}
                        </select>
                      </div>
                      <div>
                        <label htmlFor="ai-api-key">API Key</label>
                        <input
                          id="ai-api-key"
                          type="password"
                          placeholder={
                            keyConfigured ? '已配置（输入新值可替换）' : '粘贴 AI 服务的 API Key'
                          }
                          value={aiKeyInput}
                          onChange={(event) => setAiKeyInput(event.target.value)}
                        />
                      </div>
                    </div>
                    <div className="actions">
                      <button
                        className="primary"
                        disabled={busy || !aiKeyInput.trim()}
                        onClick={saveAiKey}
                      >
                        保存密钥
                      </button>
                      {(aiKeyTarget || settings.data.ai_enabled) && (
                        <button disabled={busy} onClick={clearAiKey}>
                          清除密钥
                        </button>
                      )}
                    </div>
                    <div className="muted small mt-10">
                      当前密钥状态：
                      {keyConfigured ? '已配置（接口与界面均不回显完整密钥）' : '未配置'}
                    </div>
                    <div className="muted small">
                      密钥写入本机凭据库（Windows DPAPI / macOS Keychain），审计只记录尾号；
                      不带模型的密钥为安装级，对所有未单独配置密钥的模型生效。
                    </div>
                  </div>

                  {!settings.data.ai_enabled && (
                    <div className="warning-box mt-12">
                      未配置 AI API Key，AI 生成功能不可用（单独配置了密钥的模型除外）；
                      其余管理功能正常可用。Key 存放在本机凭据库，不会经由 API 返回。
                    </div>
                  )}
                </>
              )}
            </div>
          </section>

          <section id="sec-privacy" className="settings-section">
            <div className="card">
              <div className="card-head">
                <h2>隐私与 AI 出网</h2>
                <span className="muted small">数据去哪里、脱敏了什么、如何停止</span>
              </div>
              {egress.loading && <Loading />}
              {egress.error && <ErrorBox message={egress.error} />}
              {egress.data && (
                <>
                  <p className="muted small">{egress.data.notice}</p>
                  <div className="kv mb-12">
                    <div>
                      <span className="k">供应商</span>
                      <span className="v">{egress.data.provider}</span>
                    </div>
                    <div>
                      <span className="k">模型</span>
                      <span className="v">{egress.data.model}</span>
                    </div>
                    <div>
                      <span className="k">端点</span>
                      <span className="v mono">{egress.data.base_url}</span>
                    </div>
                    <div>
                      <span className="k">出网前脱敏</span>
                      <span className="v">
                        {egress.data.masked_fields.map(redactionLabel).join('、')}
                      </span>
                    </div>
                    <div>
                      <span className="k">候选人姓名</span>
                      <span className="v">
                        {egress.data.include_candidate_name
                          ? '随上下文发送（已显式开启 AI_INCLUDE_CANDIDATE_NAME）'
                          : '以 [NAME] 占位，不发送'}
                      </span>
                    </div>
                    <div>
                      <span className="k">当前状态</span>
                      <span className="v">
                        {egress.data.consent_granted ? (
                          <Badge tone="ok">允许出网</Badge>
                        ) : (
                          <Badge tone="warn">已关闭</Badge>
                        )}
                      </span>
                    </div>
                  </div>
                  <div className="actions">
                    <button
                      type="button"
                      onClick={() => toggleEgress(!egress.data!.consent_granted)}
                      disabled={busy}
                    >
                      {egress.data.consent_granted ? '关闭 AI 出网' : '允许 AI 出网'}
                    </button>
                  </div>
                  <p className="muted small mt-10">
                    关闭后，后端会在构建上下文之前拒绝生成，任何候选人字段都不会发往供应商。
                    每次发送（<span className="mono">ai.egress</span>）与每次开关变更都会写入审计事件。
                  </p>
                </>
              )}
            </div>
          </section>

          <section id="sec-policy" className="settings-section">
            <div className="card">
              <div className="card-head">
                <h2>跟进策略</h2>
                <span className="muted small">策略由后端评估；此处只改参数</span>
              </div>
              {policy.loading && <Loading />}
              {policy.data && (
                <div className="form-row two">
                  <div>
                    <label htmlFor="p-delay">首次跟进等待（小时）</label>
                    <input
                      id="p-delay"
                      type="number"
                      min={0}
                      value={
                        draftPolicy?.first_follow_up_delay_hours ??
                        policy.data.first_follow_up_delay_hours
                      }
                      onChange={(event) =>
                        updatePolicyField('first_follow_up_delay_hours', Number(event.target.value))
                      }
                    />
                  </div>
                  <div>
                    <label htmlFor="p-max">最大主动跟进次数</label>
                    <input
                      id="p-max"
                      type="number"
                      min={1}
                      value={
                        draftPolicy?.max_active_follow_ups ?? policy.data.max_active_follow_ups
                      }
                      onChange={(event) =>
                        updatePolicyField('max_active_follow_ups', Number(event.target.value))
                      }
                    />
                  </div>
                  <div>
                    <label htmlFor="p-interval">两次跟进最小间隔（小时）</label>
                    <input
                      id="p-interval"
                      type="number"
                      min={0}
                      value={
                        draftPolicy?.min_interval_hours ?? policy.data.min_interval_hours
                      }
                      onChange={(event) =>
                        updatePolicyField('min_interval_hours', Number(event.target.value))
                      }
                    />
                  </div>
                  <div>
                    <label htmlFor="p-validity">任务有效期（天）</label>
                    <input
                      id="p-validity"
                      type="number"
                      min={1}
                      value={
                        draftPolicy?.task_validity_days ?? policy.data.task_validity_days
                      }
                      onChange={(event) =>
                        updatePolicyField('task_validity_days', Number(event.target.value))
                      }
                    />
                  </div>
                </div>
              )}
              <div className="actions">
                <button className="primary" onClick={savePolicy} disabled={busy || !draftPolicy}>
                  保存策略
                </button>
              </div>
            </div>
          </section>

          <section id="sec-backup" className="settings-section">
            <div className="card">
              <div className="card-head">
                <h2>本地备份</h2>
                <button onClick={createBackup} disabled={busy}>
                  立即备份
                </button>
              </div>
              {backups.loading && <Loading />}
              {backups.data && backups.data.length === 0 && (
                <EmptyState message="还没有备份" />
              )}
              {backups.data && backups.data.length > 0 && (
                <table>
                  <thead>
                    <tr>
                      <th>文件</th>
                      <th>大小</th>
                      <th>创建时间</th>
                      <th>操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    {backups.data.map((backup) => (
                      <tr key={backup.name}>
                        <td className="mono">{backup.name}</td>
                        <td className="muted small num">{backup.size_bytes} 字节</td>
                        <td className="muted small">
                          <LocalTime value={backup.created_at} />
                        </td>
                        <td>
                          <RestoreButton
                            backup={backup}
                            busy={busy}
                            onRestore={async () => {
                              setBusy(true);
                              setError(null);
                              setNotice(null);
                              try {
                                const result = await settingsApi.restore(backup.name);
                                setNotice(
                                  `已从 ${result.restored_from} 恢复数据库。前一状态已保存为 ${result.previous_copy}。`,
                                );
                                backups.reload();
                              } catch (caught: unknown) {
                                setError(caught instanceof Error ? caught.message : '恢复失败');
                              } finally {
                                setBusy(false);
                              }
                            }}
                          />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </section>

          <section id="sec-audit" className="settings-section">
            <div className="card">
              <div className="card-head">
                <h2>审计记录</h2>
                <span className="muted small">最新在前；仅记录元数据，不含聊天正文与密钥</span>
              </div>
              <div className="form-row two">
                <div>
                  <label htmlFor="audit-event-type">事件类型筛选</label>
                  <input
                    id="audit-event-type"
                    value={auditEventType}
                    placeholder="如 candidate.updated，留空 = 全部"
                    onChange={(event) => setAuditEventType(event.target.value)}
                  />
                </div>
                <div>
                  <label htmlFor="audit-entity-type">对象类型筛选</label>
                  <input
                    id="audit-entity-type"
                    value={auditEntityType}
                    placeholder="如 candidate，留空 = 全部"
                    onChange={(event) => setAuditEntityType(event.target.value)}
                  />
                </div>
              </div>
              {audit.loading && <Loading />}
              {audit.error && <ErrorBox message={audit.error} />}
              {audit.data && audit.data.length === 0 && (
                <EmptyState message="没有匹配的审计记录" />
              )}
              {audit.data && audit.data.length > 0 && (
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th>时间</th>
                        <th>事件类型</th>
                        <th>对象</th>
                        <th>操作者</th>
                        <th>摘要</th>
                      </tr>
                    </thead>
                    <tbody>
                      {audit.data.map((event) => (
                        <tr key={event.id}>
                          <td className="muted small">
                            <LocalTime value={event.created_at} />
                          </td>
                          <td>
                            <Badge tone="info">{event.event_type}</Badge>
                          </td>
                          <td className="mono muted small">
                            {event.entity_type ?? '—'}
                            {event.entity_id ? ` · ${event.entity_id.slice(0, 8)}` : ''}
                          </td>
                          <td className="muted small">{event.actor}</td>
                          <td className="muted small">{event.summary ?? '—'}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          </section>
        </div>
      </div>
    </>
  );
}

function RestoreButton({
  backup,
  busy,
  onRestore,
}: {
  backup: { name: string };
  busy: boolean;
  onRestore: () => Promise<void>;
}) {
  const [confirming, setConfirming] = useState(false);

  if (confirming) {
    return (
      <ConfirmBar
        message={
          <>
            确定要从 <strong>{backup.name}</strong>{' '}
            恢复？当前数据会先被保存为安全副本，但恢复后仍需手动确认。
          </>
        }
        onConfirm={onRestore}
        onCancel={() => setConfirming(false)}
        busy={busy}
        cancelDisabled={busy}
        confirmLabel="确认恢复"
        busyLabel="恢复中…"
      />
    );
  }

  return (
    <button className="danger" onClick={() => setConfirming(true)} disabled={busy}>
      恢复此备份
    </button>
  );
}
