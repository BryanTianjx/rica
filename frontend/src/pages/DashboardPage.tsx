import { Link } from 'react-router-dom';
import { useAsync } from '../hooks/useAsync';
import { backend } from '../services/backend';
import { EmptyState, ErrorBox, Loading, LocalTime, Stat, StatusBadge } from '../components/ui';
import { MonthCalendar } from '../components/dashboard/MonthCalendar';

export function DashboardPage() {
  const { data, error, loading, reload } = useAsync(() => backend.dashboard.get(), []);

  if (loading) return <Loading />;
  if (error) return <ErrorBox message={error} />;
  if (!data) return <EmptyState message="没有可显示的数据" />;

  const activeStatuses = Object.entries(data.candidate_status_counts).filter(
    ([, count]) => count > 0,
  );

  return (
    <>
      <div className="page-head">
        <div>
          <h1>工作台</h1>
          <p>所有统计均来自本地数据库实时计算。</p>
        </div>
        <div className="actions">
          <button onClick={reload}>刷新</button>
          <Link className="btn primary" to="/candidates">
            新增候选人
          </Link>
        </div>
      </div>

      <div className="grid cols-4 mb-16">
        <Stat
          value={data.candidate_total}
          label="候选人总数"
        />
        <Stat
          value={data.active_follow_up_count}
          label="进行中的跟进"
          hint="待处理 / 可跟进 / 草稿中 / 待审核"
        />
        <Stat value={data.awaiting_result_count} label="待记录结果" />
        <Stat value={data.pending_draft_count} label="待审核草稿" />
      </div>

      <MonthCalendar calendar={data.calendar ?? null} />

      <div className="grid cols-2">
        <div className="card">
          <div className="card-head">
            <h2>候选人状态</h2>
            <span className="muted small">在职岗位 {data.open_position_count}/{data.position_total}</span>
          </div>
          {activeStatuses.length === 0 ? (
            <EmptyState message="还没有候选人" />
          ) : (
            <table>
              <tbody>
                {activeStatuses.map(([status, count]) => (
                  <tr key={status}>
                    <td>
                      <StatusBadge status={status} />
                    </td>
                    <td className="text-right num">{count}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="card">
          <div className="card-head">
            <h2>待跟进</h2>
            <Link to="/follow-ups" className="small">
              全部跟进 →
            </Link>
          </div>
          {data.pending_follow_ups.length === 0 ? (
            <EmptyState message="没有待跟进的任务" />
          ) : (
            <ul className="plain">
              {data.pending_follow_ups.map((task) => (
                <li key={task.id}>
                  <Link to="/follow-ups">{task.candidate_name ?? '候选人'}</Link>{' '}
                  <StatusBadge status={task.status} />{' '}
                  <span className="muted small">
                    {task.recommended_at ? (
                      <>
                        建议 <LocalTime value={task.recommended_at} />
                      </>
                    ) : (
                      task.objective ?? '未设定目标'
                    )}
                  </span>
                  {task.block_reason && (
                    <div className="warning-box compact">阻止原因：{task.block_reason}</div>
                  )}
                </li>
              ))}
            </ul>
          )}
          <p className="muted small mt-10 mb-0">
            此处只统计待处理、可跟进、草稿中与待审核的任务；被策略阻止（Blocked）的任务不在其中，
            请到 Follow-ups 查看阻止原因。
          </p>
        </div>
      </div>

      <div className="card">
        <div className="card-head">
          <h2>近期沟通</h2>
        </div>
        {data.recent_messages.length === 0 ? (
          <EmptyState message="还没有沟通记录" />
        ) : (
          <table>
            <thead>
              <tr>
                <th>发送方</th>
                <th>内容</th>
                <th>时间</th>
              </tr>
            </thead>
            <tbody>
              {data.recent_messages.map((message) => (
                <tr key={message.id}>
                  <td>
                    <StatusBadge status={message.sender_type ?? 'Unknown'} />
                  </td>
                  <td>{message.content.slice(0, 120)}</td>
                  <td className="muted small">
                    <LocalTime value={message.message_time ?? message.created_at} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}
