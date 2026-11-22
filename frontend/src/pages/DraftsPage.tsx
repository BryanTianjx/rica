import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useAsync } from '../hooks/useAsync';
import { draftsApi } from '../services/endpoints';
import { Badge, EmptyState, ErrorBox, Loading, LocalTime, StatusBadge } from '../components/ui';
import type { Draft } from '../types';

export function DraftsPage() {
  const drafts = useAsync(() => draftsApi.list(), []);
  const [editedForId, setEditedForId] = useState<string | null>(null);
  const [editedText, setEditedText] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const active: Draft | undefined =
    drafts.data?.items.find((draft) => draft.id === editedForId) ?? drafts.data?.items[0];
  const isEdited = active !== undefined && editedForId === active.id;

  /**
   * Single selection entry point shared by mouse and keyboard activation.
   * Initialising the edit buffer here is what makes an intentionally cleared
   * draft text stay empty instead of falling back to the original message.
   */
  function select(draft: Draft) {
    setEditedForId(draft.id);
    setEditedText(draft.message_text);
  }

  async function act(action: () => Promise<unknown>, message: string) {
    if (!active) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await action();
      setNotice(message);
      drafts.reload();
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '操作失败');
    } finally {
      setBusy(false);
    }
  }

  async function saveEdit() {
    if (!active) return;
    await act(
      () => draftsApi.update(active.id, { message_text: editedText }),
      '已保存修改。',
    );
  }

  async function approve() {
    if (!active) return;
    await act(() => draftsApi.approve(active.id), '已批准。批准不代表消息已发送。');
  }

  async function reject() {
    if (!active) return;
    await act(() => draftsApi.reject(active.id), '已拒绝该草稿。');
  }

  async function copy() {
    if (!active) return;
    await act(async () => {
      const result = await draftsApi.copy(active.id);
      await navigator.clipboard?.writeText(result.message_text);
    }, '已复制到剪贴板。复制不代表消息已发送 —— 请在平台手动发送后到 Follow-ups 记录结果。');
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>AI 草稿审核</h1>
          <p>审核、编辑、批准或拒绝草稿。批准与复制都不会发送任何消息。</p>
        </div>
      </div>

      {notice && (
        <div className="notice" aria-live="polite">
          {notice}
        </div>
      )}
      {error && <ErrorBox message={error} />}

      {drafts.loading && <Loading />}
      {drafts.error && <ErrorBox message={drafts.error} />}
      {drafts.data && drafts.data.items.length === 0 && (
        <EmptyState message="还没有草稿。到候选人详情页生成一个吧。" />
      )}

      {drafts.data && drafts.data.items.length > 0 && (
        <div className="grid cols-2">
          <div className="card">
            <div className="card-head">
              <h2>草稿列表</h2>
            </div>
            <table>
              <tbody>
                {drafts.data.items.map((draft) => (
                  <tr key={draft.id}>
                    <td>
                      <button
                        type="button"
                        className="row-link"
                        onClick={() => select(draft)}
                        aria-current={active?.id === draft.id ? 'true' : undefined}
                      >
                        <span className="row-link-label">{draft.message_type}</span>
                        <span className="row-link-meta">
                          <LocalTime value={draft.created_at} />
                        </span>
                      </button>
                      <Link className="small" to={`/candidates/${draft.candidate_id}`}>
                        候选人页
                      </Link>
                    </td>
                    <td>
                      <StatusBadge status={draft.status} />
                    </td>
                    <td>
                      <StatusBadge status={draft.validation_status} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {active && (
            <div className="card">
              <div className="card-head">
                <h2>草稿详情</h2>
                <div className="actions">
                  <StatusBadge status={active.status} />
                  <StatusBadge status={active.validation_status} />
                </div>
              </div>

              {active.objective && (
                <p className="muted small">目标：{active.objective}</p>
              )}

              <div className="form-row">
                <label htmlFor="draft-text">话术内容（可直接编辑）</label>
                <textarea
                  id="draft-text"
                  value={isEdited ? editedText : (active?.message_text ?? '')}
                  onChange={(event) => {
                    setEditedText(event.target.value);
                    setEditedForId(active?.id ?? null);
                  }}
                  rows={7}
                />
              </div>

              <div className="actions">
                <button onClick={saveEdit} disabled={busy}>
                  保存修改
                </button>
                <button className="primary" onClick={approve} disabled={busy}>
                  批准
                </button>
                <button onClick={copy} disabled={busy}>
                  复制
                </button>
                <button className="danger" onClick={reject} disabled={busy}>
                  拒绝
                </button>
              </div>

              {active.alternatives.length > 0 && (
                <>
                  <h3 className="mt-18">备选话术</h3>
                  <ul className="plain small">
                    {active.alternatives.map((alternative, index) => (
                      <li key={index}>{alternative}</li>
                    ))}
                  </ul>
                </>
              )}

              {active.context_references.length > 0 && (
                <>
                  <h3 className="mt-18">依据</h3>
                  <div className="actions">
                    {active.context_references.map((reference) => (
                      <Badge key={reference} tone="info">
                        {reference}
                      </Badge>
                    ))}
                  </div>
                </>
              )}

              {active.uncertainties.length > 0 && (
                <div className="warning-box mt-14">
                  <strong>不确定事项</strong>
                  <ul className="plain small">
                    {active.uncertainties.map((item) => (
                      <li key={item}>· {item}</li>
                    ))}
                  </ul>
                </div>
              )}

              {active.warnings.length > 0 && (
                <div className="warning-box mt-10">
                  <strong>风险提示</strong>
                  <ul className="plain small">
                    {active.warnings.map((item) => (
                      <li key={item}>· {item}</li>
                    ))}
                  </ul>
                </div>
              )}

              {active.validation_messages.length > 0 && (
                <div className="warning-box mt-10">
                  <strong>内容检查</strong>
                  <ul className="plain small">
                    {active.validation_messages.map((item) => (
                      <li key={item}>· {item}</li>
                    ))}
                  </ul>
                  <div className="muted small">
                    校验通过不代表内容事实正确，仍需人工审核。
                  </div>
                </div>
              )}

              <p className="muted small mt-14">
                模型 {active.model ?? '—'} · Prompt {active.prompt_version ?? '—'} ·
                生成于 <LocalTime value={active.created_at} />
              </p>
            </div>
          )}
        </div>
      )}
    </>
  );
}
