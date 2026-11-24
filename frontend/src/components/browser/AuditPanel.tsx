/**
 * Phase 5 — 浏览器操作审计独立面板。
 * 直接消费真实 `/browser/v1/audit` 契约；不做任何模拟数据。
 * 人工边界：这里只呈现事件，不提供绕过审批的操作。
 */
import { useAsync } from '../../hooks/useAsync';
import { backend } from '../../services/backend';
import { EmptyState, ErrorBox, Loading, LocalTime } from '../ui';
import { useState } from 'react';

const LIMIT = 100;

export function AuditPanel() {
  const [taskId, setTaskId] = useState('');
  const [sessionId, setSessionId] = useState('');
  const audit = useAsync(
    () =>
      backend.browser.audit({
        limit: LIMIT,
        task_id: taskId.trim() || undefined,
        session_id: sessionId.trim() || undefined,
      }),
    [taskId, sessionId],
  );

  return (
    <div>
      <div className="form-row two">
        <div>
          <label htmlFor="audit-task-id">任务 ID（可选）</label>
          <input
            id="audit-task-id"
            value={taskId}
            onChange={(event) => setTaskId(event.target.value)}
            placeholder="按任务筛选"
          />
        </div>
        <div>
          <label htmlFor="audit-session-id">会话 ID（可选）</label>
          <input
            id="audit-session-id"
            value={sessionId}
            onChange={(event) => setSessionId(event.target.value)}
            placeholder="按会话筛选"
          />
        </div>
      </div>

      {audit.loading && <Loading />}
      {audit.error && <ErrorBox message={`审计读取失败：${audit.error}`} />}
      {audit.data && audit.data.events.length === 0 && (
        <EmptyState message="没有匹配的浏览器操作审计事件" />
      )}
      {audit.data && audit.data.events.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>时间</th>
              <th>事件类型</th>
              <th>对象</th>
              <th>摘要</th>
            </tr>
          </thead>
          <tbody>
            {audit.data.events.map((event) => (
              <tr key={event.event_id}>
                <td><LocalTime value={event.created_at} /></td>
                <td>{event.event_type}</td>
                <td>{event.entity_type}{event.entity_id ? ` / ${event.entity_id}` : ''}</td>
                <td>{event.summary ?? '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
