/**
 * 任务面板：AI 任务创建 / 运行 / 取消 / 审计时间线。
 *
 * 后端没有任务列表端点（只有 GET /tasks/{id}）：已知任务 id 从审计事件
 * （entity_type=browser_task）里取，再拉详情。取消运行中的任务不可逆，
 * 走 ConfirmBar 二次确认。
 */

import { useState } from 'react';
import { browserApi } from '../../services/endpoints';
import { Badge, EmptyState, ErrorBox, Loading, LocalTime } from '../ui';
import { ConfirmBar } from '../ConfirmBar';
import { useAsync } from '../../hooks/useAsync';
import { taskTone, statusLabel } from './browserStatus';
import type { BrowserAuditEvent, BrowserTask } from '../../types';

function errText(exc: unknown): string {
  return exc instanceof Error ? exc.message : String(exc);
}

export function TasksPanel() {
  const list = useAsync(
    async () => {
      const audit = await browserApi.audit({ limit: 200 });
      const ids: string[] = [];
      for (const ev of audit.events) {
        if (ev.entity_type === 'browser_task' && ev.entity_id && !ids.includes(ev.entity_id)) {
          ids.push(ev.entity_id);
        }
      }
      const details = await Promise.all(
        ids.map(async (id) => {
          try {
            const data = await browserApi.getTask(id);
            return data.task;
          } catch {
            return null;
          }
        }),
      );
      return details.filter((t): t is BrowserTask => t !== null);
    },
    [],
  );
  const [intent, setIntent] = useState('');
  const [sessionId, setSessionId] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmCancel, setConfirmCancel] = useState<BrowserTask | null>(null);
  const [timeline, setTimeline] = useState<{ task: BrowserTask; events: BrowserAuditEvent[] } | null>(null);
  const [loadingTimeline, setLoadingTimeline] = useState<string | null>(null);

  const tasks = list.data ?? null;

  const create = async () => {
    if (!sessionId.trim()) {
      setError('需要一个会话 ID（先在会话操作台新建会话）。');
      return;
    }
    if (!intent.trim()) {
      setError('任务意图不能为空。');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      // auto_run=false：显式创建后由用户点「运行」，避免创建即执行。
      const data = await browserApi.createTask({
        session_id: sessionId.trim(),
        intent: intent.trim(),
        auto_run: false,
      });
      setNotice(`任务 ${data.task.task_id} 已创建（待运行）。`);
      setIntent('');
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  const run = async (task: BrowserTask) => {
    setBusy(true);
    setError(null);
    try {
      await browserApi.runTask(task.task_id);
      setNotice(`任务 ${task.task_id} 已运行。`);
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  const cancel = async (task: BrowserTask) => {
    setBusy(true);
    try {
      await browserApi.cancelTask(task.task_id, 'user');
      setNotice(`任务 ${task.task_id} 已请求取消。`);
      setConfirmCancel(null);
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  const loadTimeline = async (task: BrowserTask) => {
    setLoadingTimeline(task.task_id);
    try {
      const audit = await browserApi.taskAudit(task.task_id);
      setTimeline({ task, events: audit.events });
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setLoadingTimeline(null);
    }
  };

  if (tasks === null && !list.error) {
    return <Loading label="读取任务…" />;
  }

  const cancellable = (t: BrowserTask) =>
    t.state === 'running' || t.state === 'pending' || t.state === 'awaiting_approval';

  return (
    <div>
      <div className="toolbar">
        <input
          value={sessionId}
          onChange={(e) => setSessionId(e.target.value)}
          aria-label="会话 ID"
          placeholder="会话 ID（ses_…）"
        />
        <input
          value={intent}
          onChange={(e) => setIntent(e.target.value)}
          aria-label="任务意图"
          placeholder="任务意图，如「打开岗位列表并统计条数」"
        />
        <button type="button" className="primary" onClick={() => void create()} disabled={busy}>
          创建任务
        </button>
      </div>

      {list.error && <ErrorBox message={`任务读取失败：${list.error}`} />}
      {error && <ErrorBox message={error} />}
      {notice && <div className="notice">{notice}</div>}

      {tasks !== null && tasks.length === 0 ? (
        <EmptyState message="还没有 AI 任务。" />
      ) : (
        <table>
          <thead>
            <tr>
              <th>任务</th>
              <th>状态</th>
              <th>步骤</th>
              <th>取消请求</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {(tasks ?? []).map((t) => (
              <tr key={t.task_id}>
                <td>
                  <span className="mono small">{t.task_id}</span>
                  <br />
                  <span className="small">{t.intent}</span>
                </td>
                <td>
                  <Badge tone={taskTone(t.state)}>{statusLabel(t.state)}</Badge>
                  {t.error_message && <span className="muted small"> {t.error_code ?? ''}</span>}
                </td>
                <td>
                  {t.executed_steps}/{t.steps_count}
                </td>
                <td>
                  <LocalTime value={t.cancel_requested_at} />
                </td>
                <td>
                  <span className="actions">
                    {t.state === 'pending' && (
                      <button
                        type="button"
                        className="primary"
                        onClick={() => void run(t)}
                        disabled={busy}
                      >
                        运行
                      </button>
                    )}
                    {cancellable(t) && (
                      <button type="button" onClick={() => setConfirmCancel(t)} disabled={busy}>
                        取消
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => void loadTimeline(t)}
                      disabled={loadingTimeline === t.task_id}
                    >
                      审计
                    </button>
                  </span>
                  {confirmCancel?.task_id === t.task_id && (
                    <ConfirmBar
                      message={`取消运行中的任务 ${t.task_id}？已执行步骤保留，未执行步骤作废。`}
                      onConfirm={() => void cancel(t)}
                      onCancel={() => setConfirmCancel(null)}
                      busy={busy}
                      confirmLabel="确认取消"
                    />
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {timeline && (
        <>
          <h4>任务 {timeline.task.task_id} 审计时间线</h4>
          {timeline.events.length === 0 ? (
            <EmptyState message="该任务还没有审计事件。" />
          ) : (
            <table>
              <thead>
                <tr>
                  <th>时间</th>
                  <th>事件</th>
                  <th>摘要</th>
                </tr>
              </thead>
              <tbody>
                {timeline.events.map((ev) => (
                  <tr key={ev.event_id}>
                    <td>
                      <LocalTime value={ev.created_at} />
                    </td>
                    <td className="mono small">{ev.event_type}</td>
                    <td className="small">{ev.summary ?? ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
    </div>
  );
}
