import { useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useAsync } from '../hooks/useAsync';
import {
  aiApi,
  candidatesApi,
  draftGroupsApi,
  draftsApi,
  followUpsApi,
  importsApi,
  settingsApi,
} from '../services/endpoints';
import { Badge, EmptyState, ErrorBox, Loading, LocalTime, StatusBadge } from '../components/ui';
import { redactionLabel } from '../privacyLabels';
import type { CandidateStatus, MessageType, Tone } from '../types';

const STATUSES = [
  'New',
  'Contacted',
  'ReadNoReply',
  'Replied',
  'Active',
  'Interview',
  'Rejected',
  'Withdrawn',
  'Closed',
];

export function CandidateDetailPage() {
  const { id } = useParams<{ id: string }>();
  const candidateId = id ?? '';

  const candidate = useAsync(() => candidatesApi.get(candidateId), [candidateId]);
  const timeline = useAsync(() => candidatesApi.timeline(candidateId), [candidateId]);
  const context = useAsync(() => candidatesApi.context(candidateId), [candidateId]);
  const decision = useAsync(() => followUpsApi.evaluate(candidateId), [candidateId]);
  // PRIV-07: what leaves the process, to whom. Read-only disclosure.
  const egress = useAsync(() => settingsApi.getAiEgress(), []);

  const [rawText, setRawText] = useState('');
  const [preview, setPreview] = useState<Awaited<ReturnType<typeof importsApi.preview>> | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [instruction, setInstruction] = useState('');
  const [messageType, setMessageType] = useState<MessageType>('reply');
  const [tone, setTone] = useState<Tone>('friendly');
  // A2: PRD §5.9「消息长度」可配置——后端 GenerateRequest.max_length（50–2000）；
  // 留空 = 不限制（键不发送）。
  const [maxLengthInput, setMaxLengthInput] = useState('');
  const [draft, setDraft] = useState<Awaited<ReturnType<typeof draftsApi.generate>> | null>(
    null,
  );

  // ── V2.0 A/B 对比 ──────────────────────────────────────────────
  const models = useAsync(() => aiApi.listModels(), []);
  const [comparisonModels, setComparisonModels] = useState<string[]>([]);
  const [compareOpen, setCompareOpen] = useState(false);
  const [group, setGroup] = useState<Awaited<ReturnType<typeof draftGroupsApi.get>> | null>(
    null,
  );

  function toggleComparisonModel(modelId: string) {
    setComparisonModels((current) =>
      current.includes(modelId)
        ? current.filter((id) => id !== modelId)
        : [...current, modelId],
    );
  }

  const [editingProfile, setEditingProfile] = useState(false);
  const [profileForm, setProfileForm] = useState({
    name: '',
    status: '' as CandidateStatus,
    expected_salary: '',
    expected_location: '',
    job_intention: '',
    notes: '',
  });
  const [profileSaving, setProfileSaving] = useState(false);
  const [profileError, setProfileError] = useState<string | null>(null);

  async function runPreview(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setActionError(null);
    setNotice(null);
    try {
      const session = await importsApi.preview({ candidate_id: candidateId, raw_text: rawText });
      setPreview(session);
    } catch (caught: unknown) {
      setActionError(caught instanceof Error ? caught.message : '解析失败');
    } finally {
      setBusy(false);
    }
  }

  async function confirmImport() {
    if (!preview) return;
    setBusy(true);
    setActionError(null);
    try {
      const result = await importsApi.confirm(preview.id, { candidate_id: candidateId });
      setNotice(
        `已保存 ${result.saved_message_ids.length} 条消息，跳过 ${result.skipped_count} 条（其中重复 ${result.duplicate_count} 条）。`,
      );
      setPreview(null);
      setRawText('');
      timeline.reload();
      candidate.reload();
      // The follow-up decision depends on the message history, so it has to
      // be re-evaluated once the import lands.
      decision.reload();
      context.reload();
    } catch (caught: unknown) {
      setActionError(caught instanceof Error ? caught.message : '保存失败');
    } finally {
      setBusy(false);
    }
  }

  async function rejectImport() {
    if (!preview) return;
    setBusy(true);
    setActionError(null);
    try {
      await importsApi.reject(preview.id);
      setPreview(null);
      setNotice('已放弃本次导入，未写入任何沟通记录。');
    } catch (caught: unknown) {
      setActionError(caught instanceof Error ? caught.message : '操作失败');
    } finally {
      setBusy(false);
    }
  }

  /**
   * A2: 解析「消息长度」输入。留空 → undefined（键不发送，后端按无限制处理）；
   * 非法（非 50–2000 的整数）→ 'invalid'，调用方在发起请求前先报错。
   */
  function parsedMaxLength(): number | undefined | 'invalid' {
    if (!maxLengthInput.trim()) return undefined;
    const value = Number(maxLengthInput);
    if (!Number.isInteger(value) || value < 50 || value > 2000) return 'invalid';
    return value;
  }

  async function generateDraft() {
    const maxLength = parsedMaxLength();
    if (maxLength === 'invalid') {
      setActionError('消息长度需为 50–2000 之间的整数，或留空不限制。');
      return;
    }
    setBusy(true);
    setActionError(null);
    setNotice(null);
    try {
      const created = await draftsApi.generate({
        candidate_id: candidateId,
        message_type: messageType,
        tone,
        user_instruction: instruction || undefined,
        // 留空 = 键不出现在载荷（JSON 语义与内存对象一致，便于测试钉）
        ...(maxLength !== undefined ? { max_length: maxLength } : {}),
      });
      setDraft(created);
    } catch (caught: unknown) {
      setActionError(caught instanceof Error ? caught.message : '生成失败');
    } finally {
      setBusy(false);
    }
  }

  async function generateComparison() {
    if (comparisonModels.length < 2) {
      setActionError('A/B 对比至少需要选择两个模型。');
      return;
    }
    const maxLength = parsedMaxLength();
    if (maxLength === 'invalid') {
      setActionError('消息长度需为 50–2000 之间的整数，或留空不限制。');
      return;
    }
    setBusy(true);
    setActionError(null);
    setNotice(null);
    try {
      const created = await draftsApi.generateComparison({
        candidate_id: candidateId,
        message_type: messageType,
        tone,
        user_instruction: instruction || undefined,
        ...(maxLength !== undefined ? { max_length: maxLength } : {}),
        comparison_models: comparisonModels,
      });
      const groupId = created[0]?.draft_group_id;
      if (groupId) {
        setGroup(await draftGroupsApi.get(groupId));
      }
      setNotice(`已用 ${comparisonModels.length} 个模型生成对比草稿。`);
    } catch (caught: unknown) {
      setActionError(caught instanceof Error ? caught.message : '对比生成失败');
    } finally {
      setBusy(false);
    }
  }

  async function selectFromGroup(draftId: string) {
    if (!group) return;
    setBusy(true);
    setActionError(null);
    try {
      setGroup(await draftGroupsApi.select(group.draft_group_id, draftId));
      setNotice('已选中该草稿，其余对比草稿已自动拒绝。');
    } catch (caught: unknown) {
      setActionError(caught instanceof Error ? caught.message : '选择失败');
    } finally {
      setBusy(false);
    }
  }

  async function createTask() {
    setBusy(true);
    setActionError(null);
    try {
      await followUpsApi.create({
        candidate_id: candidateId,
        objective: decision.data?.suggested_objective ?? undefined,
      });
      setNotice('已创建跟进任务。');
      decision.reload();
    } catch (caught: unknown) {
      setActionError(caught instanceof Error ? caught.message : '创建失败');
    } finally {
      setBusy(false);
    }
  }

  function startEditProfile() {
    if (!person) return;
    setProfileForm({
      name: person.name,
      status: person.status,
      expected_salary: person.expected_salary ?? '',
      expected_location: person.expected_location ?? '',
      job_intention: person.job_intention ?? '',
      notes: person.notes ?? '',
    });
    setEditingProfile(true);
    setProfileError(null);
  }

  function cancelEditProfile() {
    setEditingProfile(false);
    setProfileError(null);
  }

  async function saveProfile() {
    setProfileSaving(true);
    setProfileError(null);
    try {
      await candidatesApi.update(candidateId, {
        name: profileForm.name || undefined,
        status: profileForm.status,
        // The clearable optional profile fields send explicit `null` when
        // emptied: `undefined` would drop the key and the backend's
        // exclude_unset PATCH would keep the old value, so clearing a field
        // in the form would silently do nothing.
        expected_salary: profileForm.expected_salary || null,
        expected_location: profileForm.expected_location || null,
        job_intention: profileForm.job_intention || null,
        notes: profileForm.notes || null,
      });
      setEditingProfile(false);
      candidate.reload();
    } catch (caught: unknown) {
      setProfileError(caught instanceof Error ? caught.message : '保存失败');
    } finally {
      setProfileSaving(false);
    }
  }

  if (candidate.loading) return <Loading />;
  if (candidate.error) return <ErrorBox message={candidate.error} />;
  if (!candidate.data) return <EmptyState message="候选人不存在" />;

  const person = candidate.data;

  return (
    <>
      <div className="page-head">
        <div>
          <h1>
            {person.name} <StatusBadge status={person.status} />
          </h1>
          <p>
            {person.position_name ?? '未关联岗位'} · 来源 {person.source} ·{' '}
            {person.message_count} 条消息
          </p>
        </div>
        <div className="actions">
          <Link className="btn" to="/candidates">
            返回列表
          </Link>
        </div>
      </div>

      {actionError && <ErrorBox message={actionError} />}
      {notice && (
        <div className="notice" aria-live="polite">
          {notice}
        </div>
      )}

      <div className="grid cols-2">
        <div className="card">
          <div className="card-head">
            <h2>档案</h2>
            {!editingProfile && (
              <button onClick={startEditProfile}>编辑</button>
            )}
          </div>
          {profileError && <ErrorBox message={profileError} />}
          {editingProfile ? (
            <table>
              <tbody>
                <tr>
                  <td className="muted">姓名 *</td>
                  <td>
                    <input
                      value={profileForm.name}
                      onChange={(event) =>
                        setProfileForm({ ...profileForm, name: event.target.value })
                      }
                    />
                  </td>
                </tr>
                <tr>
                  <td className="muted">状态</td>
                  <td>
                    <select
                      value={profileForm.status}
                      onChange={(event) =>
                        setProfileForm({ ...profileForm, status: event.target.value as CandidateStatus })
                      }
                    >
                      {STATUSES.map((value) => (
                        <option key={value} value={value}>
                          {value}
                        </option>
                      ))}
                    </select>
                  </td>
                </tr>
                <tr>
                  <td className="muted">期望薪资</td>
                  <td>
                    <input
                      value={profileForm.expected_salary}
                      onChange={(event) =>
                        setProfileForm({ ...profileForm, expected_salary: event.target.value })
                      }
                    />
                  </td>
                </tr>
                <tr>
                  <td className="muted">期望地点</td>
                  <td>
                    <input
                      value={profileForm.expected_location}
                      onChange={(event) =>
                        setProfileForm({ ...profileForm, expected_location: event.target.value })
                      }
                    />
                  </td>
                </tr>
                <tr>
                  <td className="muted">求职意向</td>
                  <td>
                    <input
                      value={profileForm.job_intention}
                      onChange={(event) =>
                        setProfileForm({ ...profileForm, job_intention: event.target.value })
                      }
                    />
                  </td>
                </tr>
                <tr>
                  <td className="muted">备注</td>
                  <td>
                    <textarea
                      value={profileForm.notes}
                      onChange={(event) =>
                        setProfileForm({ ...profileForm, notes: event.target.value })
                      }
                      rows={3}
                    />
                  </td>
                </tr>
              </tbody>
            </table>
          ) : (
            <table>
              <tbody>
                <tr>
                  <td className="muted">经验摘要</td>
                  <td>{person.experience_summary ?? <span className="muted">未知</span>}</td>
                </tr>
                <tr>
                  <td className="muted">技能摘要</td>
                  <td>{person.skill_summary ?? <span className="muted">未知</span>}</td>
                </tr>
                <tr>
                  <td className="muted">期望薪资</td>
                  <td>{person.expected_salary ?? <span className="muted">未知</span>}</td>
                </tr>
                <tr>
                  <td className="muted">期望地点</td>
                  <td>{person.expected_location ?? <span className="muted">未知</span>}</td>
                </tr>
                <tr>
                  <td className="muted">求职意向</td>
                  <td>{person.job_intention ?? <span className="muted">未知</span>}</td>
                </tr>
                <tr>
                  <td className="muted">备注</td>
                  <td>{person.notes ?? <span className="muted">—</span>}</td>
                </tr>
              </tbody>
            </table>
          )}
          {editingProfile && (
            <div className="actions mt-10">
              <button
                className="primary"
                onClick={saveProfile}
                disabled={profileSaving || !profileForm.name.trim()}
              >
                {profileSaving ? '保存中…' : '保存'}
              </button>
              <button onClick={cancelEditProfile} disabled={profileSaving}>
                取消
              </button>
            </div>
          )}
        </div>

        <div className="card">
          <div className="card-head">
            <h2>跟进判断</h2>
            {decision.data && (
              <Badge
                tone={
                  decision.data.outcome === 'eligible'
                    ? 'ok'
                    : decision.data.outcome === 'blocked'
                      ? 'bad'
                      : 'warn'
                }
              >
                {decision.data.outcome}
              </Badge>
            )}
          </div>
          {decision.loading && <Loading />}
          {decision.data && (
            <>
              {decision.data.reasons.length > 0 && (
                <ul className="plain small">
                  {decision.data.reasons.map((reason) => (
                    <li key={reason}>· {reason}</li>
                  ))}
                </ul>
              )}
              {decision.data.block_reasons.length > 0 && (
                <div className="warning-box">
                  <strong>阻止原因</strong>
                  <ul className="plain small">
                    {decision.data.block_reasons.map((reason) => (
                      <li key={reason}>· {reason}</li>
                    ))}
                  </ul>
                </div>
              )}
              {decision.data.risks.length > 0 && (
                <div className="warning-box">
                  <strong>风险提示</strong>
                  <ul className="plain small">
                    {decision.data.risks.map((risk) => (
                      <li key={risk}>· {risk}</li>
                    ))}
                  </ul>
                </div>
              )}
              {decision.data.recommended_at && (
                <p className="muted small">
                  推荐时间：<LocalTime value={decision.data.recommended_at} />
                </p>
              )}
              <div className="actions">
                <button onClick={createTask} disabled={busy}>
                  创建跟进任务
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      <div className="card">
        <div className="card-head">
          <h2>手动导入沟通记录</h2>
          <span className="muted small">粘贴 BOSS 聊天文本 → 预览 → 确认后才会写入</span>
        </div>
        <form onSubmit={runPreview}>
          <div className="form-row">
            <label htmlFor="raw-text">聊天文本</label>
            <textarea
              id="raw-text"
              value={rawText}
              onChange={(event) => setRawText(event.target.value)}
              placeholder={'10:23 我：您好，看到您投递了后端工程师岗位，方便聊聊吗？\n10:25 候选人：您好，我目前在职，主要做 Python 后端。'}
              rows={6}
            />
          </div>
          <div className="actions">
            <button className="primary" type="submit" disabled={busy || !rawText.trim()}>
              解析预览
            </button>
          </div>
        </form>

        {preview && (
          <>
            <h3 className="mt-18">预览（尚未写入）</h3>
            <table>
              <thead>
                <tr>
                  <th>#</th>
                  <th>发送方</th>
                  <th>时间</th>
                  <th>内容</th>
                  <th>状态</th>
                </tr>
              </thead>
              <tbody>
                {preview.records.map((record) => (
                  <tr key={record.id}>
                    <td className="muted">{record.sequence}</td>
                    <td>
                      <StatusBadge status={record.sender_type ?? 'Unknown'} />
                    </td>
                    <td className="muted small">
                      <LocalTime value={record.message_time} />
                    </td>
                    <td>{record.content}</td>
                    <td>
                      <StatusBadge status={record.status} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="actions mt-12">
              <button className="primary" onClick={confirmImport} disabled={busy}>
                确认保存
              </button>
              <button onClick={rejectImport} disabled={busy}>
                放弃
              </button>
              <span className="muted small">疑似重复 {preview.detected_duplicate_count} 条</span>
            </div>
          </>
        )}
      </div>

      <div className="card">
        <div className="card-head">
          <h2>沟通时间线</h2>
        </div>
        {timeline.loading && <Loading />}
        {timeline.data && timeline.data.length === 0 && (
          <EmptyState message="还没有沟通记录" />
        )}
        {timeline.data?.map((entry) => (
          <div
            key={entry.id}
            className={`message ${entry.sender_type?.toLowerCase() ?? ''}`}
          >
            <div className="message-head">
              <strong>{entry.sender_type ?? 'Unknown'}</strong>
              <span>
                <LocalTime value={entry.message_time ?? entry.created_at} />
              </span>
              {entry.source && entry.source !== 'manual' && (
                <span className="muted small">{entry.source}</span>
              )}
            </div>
            <div className="message-body">{entry.content}</div>
          </div>
        ))}
      </div>

      <div className="card">
        <div className="card-head">
          <h2>AI 话术生成</h2>
          <span className="muted small">AI 只起草；发送由你在 BOSS 直聘手动完成</span>
        </div>
        {egress.data && (
          <div className="notice" aria-live="polite" data-testid="ai-egress-notice">
            出网告知：点击生成后，上方脱敏过的候选人上下文会发送至{' '}
            <strong>{egress.data.provider}</strong>（模型 {egress.data.model}
            ，端点 <span className="mono">{egress.data.base_url}</span>）。脱敏项：
            {egress.data.masked_fields.map(redactionLabel).join('、')}。
            {egress.data.consent_granted
              ? ' 可在设置页随时关闭 AI 出网。'
              : ' AI 出网当前已关闭，生成会被拒绝。'}
          </div>
        )}
        <div className="form-row two">
          <div>
            <label htmlFor="msg-type">沟通目标</label>
            <select
              id="msg-type"
              value={messageType}
              onChange={(event) => setMessageType(event.target.value as MessageType)}
            >
              <option value="greeting">首次招呼</option>
              <option value="reply">候选人回复</option>
              <option value="follow_up_question">追问</option>
              <option value="position_intro">岗位介绍</option>
              <option value="salary_talk">薪资沟通</option>
              <option value="interview_invite">面试邀约</option>
              <option value="post_interview">面试后跟进</option>
              <option value="polite_follow_up">礼貌跟进</option>
              <option value="decline_ack">婉拒致谢</option>
            </select>
          </div>
          <div>
            <label htmlFor="tone">语气</label>
            <select
              id="tone"
              value={tone}
              onChange={(event) => setTone(event.target.value as Tone)}
            >
              <option value="friendly">友好</option>
              <option value="formal">正式</option>
              <option value="concise">简洁</option>
              <option value="warm">亲和</option>
            </select>
          </div>
        </div>
        <div className="form-row">
          <label htmlFor="instruction">补充要求（唯一会被执行的指令来源）</label>
          <textarea
            id="instruction"
            value={instruction}
            onChange={(event) => setInstruction(event.target.value)}
            rows={3}
            placeholder="例如：询问对方是否方便本周三下午线上面试。"
          />
        </div>
        <div className="form-row">
          <label htmlFor="msg-max-length">消息长度（可选，50–2000 字符，留空不限制）</label>
          <input
            id="msg-max-length"
            type="number"
            min={50}
            max={2000}
            value={maxLengthInput}
            onChange={(event) => setMaxLengthInput(event.target.value)}
          />
        </div>
        <div className="actions">
          <button className="primary" onClick={generateDraft} disabled={busy}>
            {busy ? '生成中…' : '生成草稿'}
          </button>
          <Link className="btn" to="/drafts">
            前往审核中心
          </Link>
        </div>

        {draft && (
          <div className="mt-16">
            <h3>生成的草稿</h3>
            <div className="message-body mb-10">
              {draft.message_text}
            </div>
            {draft.alternatives.length > 0 && (
              <>
                <h3>备选</h3>
                <ul className="plain small">
                  {draft.alternatives.map((alternative, index) => (
                    <li key={index}>{alternative}</li>
                  ))}
                </ul>
              </>
            )}
            <div className="actions mt-10">
              <StatusBadge status={draft.validation_status} />
              {draft.context_references.map((reference) => (
                <span key={reference} className="muted small">
                  {reference}
                </span>
              ))}
            </div>
            {draft.uncertainties.length > 0 && (
              <div className="warning-box mt-10">
                <strong>不确定事项</strong>
                <ul className="plain small">
                  {draft.uncertainties.map((item) => (
                    <li key={item}>· {item}</li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}

        {/* ── V2.0 A/B 对比 ─────────────────────────────────────────── */}
        <div className="form-row mt-20">
          <div className="actions">
            <button type="button" onClick={() => setCompareOpen((open) => !open)}>
              {compareOpen ? '收起 A/B 对比' : 'A/B 多模型对比'}
            </button>
            {comparisonModels.length > 0 && (
              <span className="muted small">已选 {comparisonModels.length} 个模型</span>
            )}
          </div>
        </div>

        {compareOpen && (
          <div className="card mt-8">
            <div className="card-head">
              <h3>选择对比模型</h3>
              <span className="muted small">
                同一上下文由多个模型各生成一份草稿，并排比较后只保留一份
              </span>
            </div>
            {models.loading && <Loading />}
            {models.data && models.data.models.filter((m) => m.enabled).length === 0 && (
              <EmptyState message="还没有可用的 AI 模型，请先在设置中配置。" />
            )}
            {models.data && models.data.models.filter((m) => m.enabled).length > 0 && (
              <div className="actions">
                {models.data.models
                  .filter((m) => m.enabled)
                  .map((model) => (
                    <button
                      key={model.model_id}
                      type="button"
                      className={comparisonModels.includes(model.model_id) ? 'primary' : ''}
                      onClick={() => toggleComparisonModel(model.model_id)}
                    >
                      {model.display_name}
                      <span className="muted small"> · {model.quality_tier}</span>
                    </button>
                  ))}
              </div>
            )}
            <div className="actions mt-12">
              <button
                className="primary"
                onClick={generateComparison}
                disabled={busy || comparisonModels.length < 2}
              >
                {busy ? '生成中…' : `生成 ${comparisonModels.length} 份对比草稿`}
              </button>
              {comparisonModels.length < 2 && (
                <span className="muted small">至少选择两个模型</span>
              )}
            </div>
          </div>
        )}

        {group && (
          <div className="mt-16">
            <h3>A/B 对比结果</h3>
            <table>
              <thead>
                <tr>
                  <th>模型</th>
                  <th>草稿内容</th>
                  <th>层级</th>
                  <th>Token</th>
                  <th>成本</th>
                  <th>状态</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {group.drafts.map((item) => (
                  <tr key={item.id}>
                    <td>{item.model ?? '—'}</td>
                    <td>
                      <div className="message-body">{item.message_text}</div>
                      {item.uncertainties.length > 0 && (
                        <div className="warning-box compact">
                          <ul className="plain small">
                            {item.uncertainties.map((item2) => (
                              <li key={item2}>· {item2}</li>
                            ))}
                          </ul>
                        </div>
                      )}
                    </td>
                    <td className="muted small">{item.routing_tier ?? '—'}</td>
                    <td className="muted small">
                      {item.prompt_tokens ?? 0}+{item.completion_tokens ?? 0}
                    </td>
                    <td className="muted small">
                      {item.cost_cents === null || item.cost_cents === undefined
                        ? '—'
                        : `${item.cost_cents} 分`}
                    </td>
                    <td>
                      <Badge>{item.status}</Badge>
                    </td>
                    <td>
                      {item.status === 'generated' && (
                        <button
                          type="button"
                          onClick={() => selectFromGroup(item.id)}
                          disabled={busy}
                        >
                          选中
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="muted small mt-8">
              组 ID：{group.draft_group_id}
            </div>
          </div>
        )}
      </div>

      <div className="card">
        <div className="card-head">
          <h2>AI 上下文预览</h2>
          <span className="muted small">模型实际会看到的候选人与岗位信息</span>
        </div>
        {context.loading && <Loading />}
        {context.data && (
          <>
            {context.data.redaction_applied.length > 0 ? (
              <div className="notice" aria-live="polite">
                已脱敏字段：{context.data.redaction_applied.map(redactionLabel).join('、')}
              </div>
            ) : (
              <div className="notice" aria-live="polite">
                本次上下文未包含姓名、电话等需要脱敏的身份字段。
              </div>
            )}
            {context.data.unknowns.length > 0 && (
              <div className="warning-box">
                <strong>未知信息（不得臆造）</strong>
                <ul className="plain small">
                  {context.data.unknowns.map((item) => (
                    <li key={item}>· {item}</li>
                  ))}
                </ul>
              </div>
            )}
            {context.data.conflicts.length > 0 && (
              <div className="warning-box">
                <strong>信息冲突，需你确认</strong>
                <ul className="plain small">
                  {context.data.conflicts.map((item) => (
                    <li key={item}>· {item}</li>
                  ))}
                </ul>
              </div>
            )}
            <table>
              <tbody>
                <tr>
                  <td className="muted">候选人</td>
                  <td className="mono">
                    {JSON.stringify(context.data.candidate_summary)}
                  </td>
                </tr>
                <tr>
                  <td className="muted">岗位</td>
                  <td className="mono">
                    {JSON.stringify(context.data.position_summary)}
                  </td>
                </tr>
              </tbody>
            </table>
          </>
        )}
      </div>
    </>
  );
}
