/**
 * 审批面板：待审批高风险操作列表 + 批准 / 驳回。
 *
 * 后端没有「待审批列表」端点（只有 POST /approvals、GET /approvals/{id}、
 * decide）：待审批项从 /audit 的 browser.approval.requested 事件里取
 * entity_id，再逐条 GET /approvals/{id} 拿完整详情。列表来源是真实审计数据，
 * 已决（approved/rejected/expired）的条目原样展示但不重复给按钮。
 */

import { useState } from 'react';
import { backend } from '../../services/backend';
import { Badge, EmptyState, ErrorBox, Loading } from '../ui';
import { useAsync } from '../../hooks/useAsync';
import { approvalTone, riskTone, statusLabel } from './browserStatus';
import type { BrowserApproval, BrowserAuditEvent } from '../../types';

function errText(exc: unknown): string {
  return exc instanceof Error ? exc.message : String(exc);
}

export function ApprovalsPanel() {
  // 后端没有待审批列表端点：从 /audit 的 browser.approval.requested 事件取 id，
  // 逐条拉详情；已决条目同样保留展示。
  const list = useAsync(
    async () => {
      const audit = await backend.browser.audit({ limit: 200 });
      const events: BrowserAuditEvent[] = audit.events;
      const ids: string[] = [];
      const decidedTypes = new Set(['browser.approval.decided']);
      const requested = new Map<string, BrowserAuditEvent>();
      for (const ev of events) {
        if (ev.entity_type !== 'browser_approval' || !ev.entity_id) continue;
        if (!ids.includes(ev.entity_id)) ids.push(ev.entity_id);
        if (decidedTypes.has(ev.event_type)) requested.delete(ev.entity_id);
        else if (!requested.has(ev.entity_id)) requested.set(ev.entity_id, ev);
      }
      const details = await Promise.all(
        ids.map(async (id) => {
          try {
            const data = await backend.browser.getApproval(id);
            return data.approval;
          } catch {
            // 审批行可能已过期清理：跳过，不把整张面板打挂
            return null;
          }
        }),
      );
      return details.filter((a): a is BrowserApproval => a !== null);
    },
    [],
  );
  const [note, setNote] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const approvals = list.data ?? null;

  const decide = async (approval: BrowserApproval, decision: 'approve' | 'reject') => {
    setBusy(true);
    setError(null);
    try {
      await backend.browser.decideApproval(approval.approval_id, decision, note.trim() || undefined);
      setNotice(`审批 ${approval.approval_id} 已${decision === 'approve' ? '批准' : '驳回'}。`);
      setNote('');
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  if (approvals === null && !list.error) {
    return <Loading label="读取待审批项…" />;
  }

  return (
    <div>
      {list.error && <ErrorBox message={`审批读取失败：${list.error}`} />}
      {error && <ErrorBox message={error} />}
      {notice && <div className="notice">{notice}</div>}
      <p className="muted small">
        L4（不可逆）动作不批准就永远执行不了；批准只对动作摘要（digest）有效，
        且一次性消耗。列表来自审计事件，已决条目只读保留。
      </p>

      {approvals !== null && approvals.length === 0 ? (
        <EmptyState message="当前没有待审批项。" />
      ) : (
        <table>
          <thead>
            <tr>
              <th>类型</th>
              <th>风险</th>
              <th>摘要</th>
              <th>状态</th>
              <th>过期</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {(approvals ?? []).map((a) => (
              <tr key={a.approval_id}>
                <td>{a.request_type}</td>
                <td>
                  <Badge tone={riskTone(a.risk_level)}>{a.risk_level}</Badge>
                </td>
                <td>
                  <span className="small">{a.summary ?? '（无摘要）'}</span>
                  {a.action_digest && (
                    <span className="mono muted small"> {a.action_digest.slice(0, 12)}…</span>
                  )}
                </td>
                <td>
                  <Badge tone={approvalTone(a.state)}>{statusLabel(a.state)}</Badge>
                </td>
                <td>
                  <span className="muted small">{a.expires_at ?? '—'}</span>
                </td>
                <td>
                  {a.state === 'pending' ? (
                    <span className="actions">
                      <button
                        type="button"
                        className="primary"
                        onClick={() => void decide(a, 'approve')}
                        disabled={busy}
                      >
                        批准
                      </button>
                      <button type="button" onClick={() => void decide(a, 'reject')} disabled={busy}>
                        驳回
                      </button>
                    </span>
                  ) : (
                    <span className="muted small">已决（{a.decided_at ?? '—'}）</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div className="toolbar">
        <input
          value={note}
          onChange={(e) => setNote(e.target.value)}
          aria-label="审批备注"
          placeholder="审批备注（随决定写入审计）"
        />
      </div>
    </div>
  );
}
