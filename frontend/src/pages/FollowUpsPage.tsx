import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useAsync } from '../hooks/useAsync';
import { backend } from '../services/backend';
import { EmptyState, ErrorBox, Loading, LocalTime, StatusBadge } from '../components/ui';
import type { FollowUpTaskStatus, ResultType } from '../types';

const TASK_STATUSES: FollowUpTaskStatus[] = [
  'Pending',
  'Eligible',
  'Drafting',
  'Reviewing',
  'Rejected',
  'Approved',
  'Copied',
  'AwaitingResult',
  'Completed',
  'Cancelled',
  'Expired',
  'Blocked',
];

const RESULT_OPTIONS: { value: ResultType; label: string }[] = [
  { value: 'sent', label: '已发送' },
  { value: 'not_sent', label: '未发送' },
  { value: 'candidate_replied', label: '候选人已回复' },
  { value: 'candidate_no_reply', label: '候选人未回复' },
  { value: 'candidate_declined', label: '候选人拒绝' },
  { value: 'interview', label: '进入面试' },
  { value: 'other', label: '其他' },
];

/**
 * 「推进」按钮的目标状态。
 *
 * 只列出服务端允许手动设置的状态，与后端状态机保持一致
 * （backend/app/domain/follow_up.py 的 ALLOWED_TRANSITIONS 与
 * MANUAL_SETTABLE_STATUSES）。Approved / Copied / Completed 由「批准草稿」
 * 「复制草稿」「记录结果」三个动作写入，直接设置会被服务端拒绝。
 */
const ADVANCE_TARGETS: Partial<Record<FollowUpTaskStatus, FollowUpTaskStatus>> = {
  Pending: 'Eligible',
  Eligible: 'Drafting',
  Drafting: 'Reviewing',
};

/**
 * 「拒绝」按钮的目标状态：只有待审核的草稿可以手动拒绝。
 * 其余状态服务端不接受 Rejected 转换。
 */
const REJECT_TARGETS: Partial<Record<FollowUpTaskStatus, FollowUpTaskStatus>> = {
  Reviewing: 'Rejected',
};

/**
 * 可以记录结果的状态 —— 与后端 RESULT_COMPLETABLE_STATUSES 一致。
 * 终态（Completed / Cancelled / Expired）与 Blocked 不在其中：已关闭的任务
 * 保持关闭，被阻止的任务也不能绕过阻止条件被完成。
 */
const RESULT_COMPLETABLE_STATUSES: FollowUpTaskStatus[] = [
  'Pending',
  'Eligible',
  'Drafting',
  'Reviewing',
  'Approved',
  'Copied',
  'AwaitingResult',
];

/**
 * 可以取消的状态 —— 终态（Completed / Cancelled / Expired）已关闭，
 * 服务端不接受再取消。
 */
const CANCELLABLE_STATUSES: FollowUpTaskStatus[] = [
  'Pending',
  'Eligible',
  'Drafting',
  'Reviewing',
  'Rejected',
  'Approved',
  'Copied',
  'AwaitingResult',
  'Blocked',
];

/** 终态任务（完成/取消/过期）已关闭，不再提供目标编辑。 */
const TERMINAL_STATUSES: FollowUpTaskStatus[] = ['Completed', 'Cancelled', 'Expired'];

/** 每个状态在界面上的一句说明，告诉用户现在该做什么。 */
const STATUS_NOTES: Record<FollowUpTaskStatus, string> = {
  Pending: '还没到跟进时间，但没有被阻止；可以手动标记为可跟进。',
  Eligible: '现在就该跟进。',
  Drafting: '草稿生成中。',
  Reviewing: '草稿待审核：请到 AI 草稿审核批准或拒绝。',
  Rejected: '草稿已被拒绝，任务不再推进。',
  Approved: '话术已获批，等待复制。',
  Copied: '话术已复制，等待记录结果。',
  AwaitingResult: '等待你记录实际的发送结果。',
  Completed: '已记录结果，任务结束。',
  Cancelled: '任务已取消。',
  Expired: '任务已过期。',
  Blocked: '该跟进被策略阻止，不能推进。',
};

export function FollowUpsPage() {
  const [status, setStatus] = useState('');
  const tasks = useAsync(
    () => backend.followUps.list({ status: status || undefined }),
    [status],
  );
  const policy = useAsync(() => backend.followUps.getPolicy(), []);

  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // F3: 跟进目标（objective）编辑——PRD §5.6「修改」。后端 FollowUpTaskUpdate
  // 支持 objective（≤64 字符），显式 null 表示清空。
  const [editingObjective, setEditingObjective] = useState<{ id: string; value: string } | null>(
    null,
  );
  // F5: 记录结果改为两步——先在下拉里选类型，再在确认条里可选填写备注
  // （PRD §5.12「备注」；后端 ResultRecordRequest.note）。
  const [pendingResult, setPendingResult] = useState<{ taskId: string; resultType: ResultType } | null>(
    null,
  );
  const [resultNote, setResultNote] = useState('');

  async function act(taskId: string, action: () => Promise<unknown>, message: string) {
    setBusyId(taskId);
    setError(null);
    setNotice(null);
    try {
      await action();
      setNotice(message);
      tasks.reload();
    } catch (caught: unknown) {
      // ApiError 的 message 就是服务端 error.message 原文，直接展示。
      setError(caught instanceof Error ? caught.message : '操作失败');
    } finally {
      setBusyId(null);
    }
  }

  async function recordResult(taskId: string, resultType: ResultType, note?: string) {
    await act(
      taskId,
      () =>
        backend.followUps.recordResult(taskId, {
          result_type: resultType,
          // 留空 = 不带 note 键（后端 ResultRecordRequest.note 可空）。
          note: note || undefined,
        }),
      '已记录结果。这是唯一能确认消息是否真的发出的操作；重复记录不会重复计数。',
    );
  }

  /** 两步记录流的确认端：把备注（可选）一起提交，然后回到列表态。 */
  async function confirmResult() {
    if (!pendingResult) return;
    const { taskId, resultType } = pendingResult;
    await recordResult(taskId, resultType, resultNote.trim() || undefined);
    // 失败时 act 已经把错误显示出来，确认条保留；只有成功才回到列表态。
    setPendingResult(null);
    setResultNote('');
  }

  async function saveObjective() {
    if (!editingObjective) return;
    const { id, value } = editingObjective;
    await act(
      id,
      () => backend.followUps.update(id, { objective: value.trim() || null }),
      '跟进目标已更新。',
    );
    setEditingObjective(null);
  }

  function startEditObjective(taskId: string, current: string | null) {
    setEditingObjective({ id: taskId, value: current ?? '' });
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>跟进任务</h1>
          <p>Approved 只代表话术获批，Copied 只代表已复制；发送与否由你在这里手动记录。</p>
          <p>任务创建时由策略判定状态：可跟进（Eligible）、未到时间（Pending）或被阻止（Blocked）。</p>
        </div>
      </div>

      {notice && (
        <div className="notice" aria-live="polite">
          {notice}
        </div>
      )}
      {error && <ErrorBox message={error} />}

      {policy.data && (
        <div className="card">
          <div className="card-head">
            <h2>当前跟进策略</h2>
            <span className="muted small">可在 Settings 中调整</span>
          </div>
          <table>
            <tbody>
              <tr>
                <td className="muted">首次跟进等待</td>
                <td>{policy.data.first_follow_up_delay_hours} 小时</td>
              </tr>
              <tr>
                <td className="muted">最大主动跟进次数</td>
                <td>{policy.data.max_active_follow_ups} 次</td>
              </tr>
              <tr>
                <td className="muted">两次跟进最小间隔</td>
                <td>{policy.data.min_interval_hours} 小时</td>
              </tr>
              <tr>
                <td className="muted">任务有效期</td>
                <td>{policy.data.task_validity_days} 天</td>
              </tr>
            </tbody>
          </table>
        </div>
      )}

      <div className="toolbar">
        <select value={status} onChange={(event) => setStatus(event.target.value)}>
          <option value="">全部状态</option>
          {TASK_STATUSES.map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </div>

      {tasks.loading && <Loading />}
      {tasks.error && <ErrorBox message={tasks.error} />}
      {tasks.data && tasks.data.items.length === 0 && (
        <EmptyState message="没有跟进任务" />
      )}
      {tasks.data && tasks.data.items.length > 0 && (
        <div className="card">
          <table>
            <thead>
              <tr>
                <th>候选人</th>
                <th>状态</th>
                <th>目标</th>
                <th>推荐时间</th>
                <th>结果</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {tasks.data.items.map((task) => {
                const advanceTarget = ADVANCE_TARGETS[task.status];
                const rejectTarget = REJECT_TARGETS[task.status];
                return (
                  <tr key={task.id}>
                    <td>
                      <Link to={`/candidates/${task.candidate_id}`}>
                        {task.candidate_name ?? '候选人'}
                      </Link>
                    </td>
                    <td>
                      <StatusBadge status={task.status} />
                      {task.block_reason ? (
                        <div className="warning-box compact">
                          <strong>阻止原因</strong>
                          <div>{task.block_reason}</div>
                        </div>
                      ) : (
                        task.reason && (
                          <div className="muted small mt-4">
                            {task.reason}
                          </div>
                        )
                      )}
                    </td>
                    <td className="muted small">
                      {editingObjective?.id === task.id ? (
                        <>
                          <input
                            aria-label="编辑跟进目标"
                            maxLength={64}
                            value={editingObjective.value}
                            onChange={(event) =>
                              setEditingObjective({ id: task.id, value: event.target.value })
                            }
                          />
                          <div className="actions tight">
                            <button
                              className="primary"
                              disabled={busyId === task.id}
                              onClick={saveObjective}
                            >
                              保存目标
                            </button>
                            <button
                              disabled={busyId === task.id}
                              onClick={() => setEditingObjective(null)}
                            >
                              取消编辑
                            </button>
                          </div>
                        </>
                      ) : (
                        (task.objective ?? '—')
                      )}
                    </td>
                    <td className="muted small">
                      <LocalTime value={task.recommended_at} />
                    </td>
                    <td className="muted small">
                      {task.result_type ? (
                        <>
                          {task.result_type}
                          <div>
                            <LocalTime value={task.result_recorded_at} />
                          </div>
                        </>
                      ) : (
                        '未记录'
                      )}
                    </td>
                    <td>
                      <div className="actions">
                        {!TERMINAL_STATUSES.includes(task.status) &&
                          editingObjective?.id !== task.id && (
                            <button
                              disabled={busyId === task.id}
                              onClick={() => startEditObjective(task.id, task.objective)}
                            >
                              编辑目标
                            </button>
                          )}
                        {advanceTarget && (
                          <button
                            disabled={busyId === task.id}
                            title={`推进到 ${advanceTarget}`}
                            onClick={() =>
                              act(
                                task.id,
                                () =>
                                  backend.followUps.update(task.id, {
                                    status: advanceTarget,
                                  }),
                                `任务状态已推进到 ${advanceTarget}。`,
                              )
                            }
                          >
                            推进
                          </button>
                        )}
                        {rejectTarget && (
                          <button
                            disabled={busyId === task.id}
                            title={`标记为 ${rejectTarget}`}
                            onClick={() =>
                              act(
                                task.id,
                                () =>
                                  backend.followUps.update(task.id, {
                                    status: rejectTarget,
                                  }),
                                '任务已标记为拒绝，不再推进。',
                              )
                            }
                          >
                            拒绝
                          </button>
                        )}
                        {CANCELLABLE_STATUSES.includes(task.status) && (
                          <button
                            disabled={busyId === task.id}
                            onClick={() =>
                              act(
                                task.id,
                                () => backend.followUps.cancel(task.id),
                                '任务已取消。',
                              )
                            }
                          >
                            取消
                          </button>
                        )}
                        {RESULT_COMPLETABLE_STATUSES.includes(task.status) && (
                          <select
                            value=""
                            onChange={(event) => {
                              if (event.target.value) {
                                // 两步流：先选类型，出现确认条后再提交（可带备注）。
                                setPendingResult({
                                  taskId: task.id,
                                  resultType: event.target.value as ResultType,
                                });
                                setResultNote('');
                              }
                            }}
                          >
                            <option value="">记录结果…</option>
                            {RESULT_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                        )}
                      </div>
                      {pendingResult?.taskId === task.id && (
                        <div className="result-strip">
                          <div className="form-row">
                            <label htmlFor={`result-note-${task.id}`}>
                              记录「
                              {RESULT_OPTIONS.find((o) => o.value === pendingResult.resultType)
                                ?.label ?? pendingResult.resultType}
                              」的备注（可选）
                            </label>
                            <textarea
                              id={`result-note-${task.id}`}
                              rows={2}
                              value={resultNote}
                              onChange={(event) => setResultNote(event.target.value)}
                            />
                          </div>
                          <div className="actions">
                            <button
                              className="primary"
                              disabled={busyId === task.id}
                              onClick={confirmResult}
                            >
                              {busyId === task.id ? '记录中…' : '确认记录'}
                            </button>
                            <button
                              disabled={busyId === task.id}
                              onClick={() => setPendingResult(null)}
                            >
                              放弃
                            </button>
                          </div>
                        </div>
                      )}
                      <div className="muted small mt-6">
                        {STATUS_NOTES[task.status]}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
