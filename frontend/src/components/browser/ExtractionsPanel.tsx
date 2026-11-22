/**
 * 候选人提取面板：提取列表、字段修订（before/after）、审批、落档到候选人。
 *
 * 硬性要求（docs/plan/02-frontend.md）：落档前必须展示字段摘要（改动项 + 来源），
 * 因为这一步写进正式业务数据 —— 摘要 + ConfirmBar 双确认；重复判定来自后端
 * duplicate 字段，越过重复必须显式勾选 confirm_duplicate。
 */

import { useMemo, useState } from 'react';
import { browserApi } from '../../services/endpoints';
import { ApiError } from '../../services/api';
import { Badge, EmptyState, ErrorBox, Loading } from '../ui';
import { ConfirmBar } from '../ConfirmBar';
import { useAsync } from '../../hooks/useAsync';
import { extractionTone, statusLabel } from './browserStatus';
import type { BrowserExtraction, BrowserExtractionFields } from '../../types';

const FIELD_LABELS: Array<{ key: keyof BrowserExtractionFields; label: string }> = [
  { key: 'name', label: '姓名' },
  { key: 'job_intention', label: '求职意向' },
  { key: 'expected_salary', label: '期望薪资' },
  { key: 'expected_location', label: '期望地点' },
  { key: 'experience_summary', label: '经验摘要' },
  { key: 'skill_summary', label: '技能摘要' },
  { key: 'note', label: '备注' },
];

function errText(exc: unknown): string {
  if (exc instanceof ApiError) return `${exc.code}: ${exc.message}`;
  return String(exc);
}

export function ExtractionsPanel() {
  const list = useAsync(async () => (await browserApi.listExtractions()).extractions, []);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<BrowserExtractionFields>({});
  const [original, setOriginal] = useState<Record<string, string | null>>({});
  const [confirmDuplicate, setConfirmDuplicate] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const items = list.data ?? null;

  const selected = useMemo(
    () => items?.find((x) => x.id === selectedId) ?? null,
    [items, selectedId],
  );

  const select = (item: BrowserExtraction) => {
    setSelectedId(item.id);
    setOriginal({ ...item.fields });
    setDraft({});
    setConfirming(false);
    setConfirmDuplicate(false);
    setNotice(null);
    setError(null);
  };

  const changedKeys = FIELD_LABELS.filter(
    ({ key }) => draft[key] !== undefined && (draft[key] ?? null) !== (original[key] ?? null),
  ).map(({ key }) => key);

  const saveDraft = async () => {
    if (!selected || changedKeys.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      const patch: BrowserExtractionFields = {};
      for (const key of changedKeys) patch[key] = draft[key] ?? null;
      await browserApi.patchExtraction(selected.id, patch);
      setNotice('修订已保存（before/after 已记入修订历史）。');
      const data = await browserApi.getExtraction(selected.id);
      setOriginal({ ...data.extraction.fields });
      setDraft({});
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  const review = async (decision: 'approve' | 'reject') => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      await browserApi.reviewExtraction(selected.id, decision);
      setNotice(decision === 'approve' ? '提取已批准，可落档。' : '提取已驳回。');
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  /** 落档到候选人：正式业务数据写入，摘要 + 二次确认。 */
  const confirmToCandidate = async () => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const importSessionId = selected.import_session_id;
      if (!importSessionId) {
        setError('该提取没有关联的导入会话（import_session_id），无法走落档端点。');
        return;
      }
      const result = await browserApi.confirmExtraction(importSessionId, {
        confirm_duplicate: confirmDuplicate,
      });
      setNotice(
        `已落档为候选人 ${result.candidate_id}（写入字段：${result.fields_written.join('、') || '无'}）。`,
      );
      setConfirming(false);
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  if (items === null && !list.error) {
    return <Loading label="读取提取列表…" />;
  }

  const duplicateBlocked = selected?.duplicate != null && !confirmDuplicate;

  return (
    <div>
      {list.error && <ErrorBox message={`提取读取失败：${list.error}`} />}
      {error && <ErrorBox message={error} />}
      {notice && <div className="notice">{notice}</div>}

      {items !== null && items.length === 0 ? (
        <EmptyState message="还没有候选人提取（提取由浏览器任务在页面上完成）。" />
      ) : (
        <div className="grid cols-2">
          <div>
            <table>
              <thead>
                <tr>
                  <th>提取</th>
                  <th>状态</th>
                  <th>来源</th>
                </tr>
              </thead>
              <tbody>
                {(items ?? []).map((x) => (
                  <tr key={x.id}>
                    <td>
                      <button
                        type="button"
                        className="link-btn"
                        onClick={() => select(x)}
                      >
                        {x.fields.name ?? x.id.slice(0, 8)}
                      </button>
                    </td>
                    <td>
                      <Badge tone={extractionTone(x.status)}>{statusLabel(x.status)}</Badge>
                    </td>
                    <td>
                      <span className="muted small">{x.provenance.source_domain}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div>
            {selected === null ? (
              <EmptyState message="左侧选择一条提取进行审核。" />
            ) : (
              <>
                <p className="muted small">
                  来源：{selected.provenance.source_url} · 快照 {selected.provenance.snapshot_node_count} 节点
                  · 脱敏命中 {selected.mask_hits}
                  {selected.duplicate ? ` · 重复判定：${selected.duplicate.kind}` : ''}
                </p>
                <table>
                  <thead>
                    <tr>
                      <th>字段</th>
                      <th>提取值</th>
                      <th>修订</th>
                    </tr>
                  </thead>
                  <tbody>
                    {FIELD_LABELS.map(({ key, label }) => (
                      <tr key={key}>
                        <td>{label}</td>
                        <td>
                          <span className="small">
                            {original[key] ?? selected.fields[key] ?? '—'}
                          </span>
                          {draft[key] !== undefined && draft[key] !== selected.fields[key] && (
                            <span className="muted small">（改前：{selected.fields[key] ?? '—'}）</span>
                          )}
                        </td>
                        <td>
                          <input
                            value={draft[key] ?? ''}
                            onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
                            aria-label={`修订 ${label}`}
                          />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <div className="toolbar">
                  <button
                    type="button"
                    onClick={() => void saveDraft()}
                    disabled={busy || changedKeys.length === 0}
                  >
                    保存修订（{changedKeys.length}）
                  </button>
                  <button
                    type="button"
                    className="primary"
                    onClick={() => void review('approve')}
                    disabled={busy || selected.status !== 'draft'}
                  >
                    审批通过
                  </button>
                  <button
                    type="button"
                    onClick={() => void review('reject')}
                    disabled={busy || selected.status !== 'draft'}
                  >
                    驳回
                  </button>
                </div>

                {selected.duplicate && (
                  <p className="warning-box compact">
                    重复判定：{selected.duplicate.kind}
                    {selected.duplicate.candidate_id ? `（已有候选人 ${selected.duplicate.candidate_id}）` : ''}
                    。落档必须显式确认越过重复。
                  </p>
                )}
                {confirming && (
                  <ConfirmBar
                    message={
                      <span>
                        落档为正式候选人？
                        <b>写入字段：</b>
                        {FIELD_LABELS.filter(({ key }) => selected.fields[key] != null)
                          .map(({ key, label }) => `${label}=${String(selected.fields[key])}`)
                          .join('；') || '（无）'}
                        <b>来源：</b>
                        {selected.provenance.source_url}
                        {duplicateBlocked ? '（将勾选「确认越过重复」）' : ''}
                      </span>
                    }
                    onConfirm={() => void confirmToCandidate()}
                    onCancel={() => setConfirming(false)}
                    busy={busy}
                    confirmLabel="确认落档"
                  />
                )}
                <div className="toolbar">
                  <label>
                    <input
                      type="checkbox"
                      checked={confirmDuplicate}
                      onChange={(e) => setConfirmDuplicate(e.target.checked)}
                      disabled={!selected.duplicate}
                    />{' '}
                    确认越过重复判定
                  </label>
                  <button
                    type="button"
                    className="primary"
                    onClick={() => setConfirming(true)}
                    disabled={busy || selected.status !== 'approved' || duplicateBlocked}
                    title={
                      selected.status !== 'approved'
                        ? '先审批通过才能落档'
                        : duplicateBlocked
                          ? '存在重复判定：先确认越过重复'
                          : undefined
                    }
                  >
                    落档到候选人
                  </button>
                </div>

                {selected.revisions.length > 0 && (
                  <>
                    <h4>修订历史</h4>
                    <table>
                      <tbody>
                        {selected.revisions
                          .slice()
                          .reverse()
                          .map((rev) => (
                            <tr key={rev.revision_no}>
                              <td>#{rev.revision_no}</td>
                              <td className="small">
                                {rev.changes.map((c) => (
                                  <div key={String(c.field)}>
                                    {String(c.field)}：{String(c.before ?? '—')} →{' '}
                                    {String(c.after ?? '—')}
                                  </div>
                                ))}
                              </td>
                            </tr>
                          ))}
                      </tbody>
                    </table>
                  </>
                )}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
