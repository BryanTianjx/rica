/**
 * 工作台月历卡：本月逐日的沟通条数与跟进建议条数。
 *
 * 数据来自 dashboard 载荷的 `calendar`（后端按 UTC 日期聚合），所以这里
 * 只负责渲染：周一起始的月网格、今日高亮、每格两类事件点与图例。
 * 聚合范围为当前月份，跨月数据后端未提供——不提供翻月切换（诚实边界，
 * 未来加按月参数后再启用）。
 */

import type { DashboardCalendar } from '../../types';

const WEEKDAYS = ['一', '二', '三', '四', '五', '六', '日'];

/** 该月的天数与首日偏移（周一起始：周一=0 … 周日=6）。 */
function monthGrid(month: string): { offset: number; days: number } {
  const [year, mon] = month.split('-').map(Number);
  const first = new Date(Date.UTC(year, mon - 1, 1));
  const days = new Date(Date.UTC(year, mon, 0)).getDate();
  // getDay(): 周日=0 → 转成周一起始偏移
  const offset = (first.getUTCDay() + 6) % 7;
  return { offset, days };
}

/** 今日的 UTC 日期键（与后端聚合口径一致）。 */
function todayKey(): string {
  return new Date().toISOString().slice(0, 10);
}

export function MonthCalendar({ calendar }: { calendar: DashboardCalendar | null }) {
  if (!calendar || !calendar.month) return null;

  const { month, messages_by_day, followups_by_day } = calendar;
  const { offset, days } = monthGrid(month);
  const today = todayKey();
  const cells: Array<{ day: number; key: string } | null> = [
    ...Array.from({ length: offset }, () => null),
    ...Array.from({ length: days }, (_, i) => {
      const key = `${month}-${String(i + 1).padStart(2, '0')}`;
      return { day: i + 1, key };
    }),
  ];
  const hasEvent = (key: string) =>
    (messages_by_day[key] ?? 0) > 0 || (followups_by_day[key] ?? 0) > 0;

  return (
    <div className="card mb-16">
      <div className="card-head">
        <h2>本月动态</h2>
        <span className="muted small">
          <span className="cal-legend">
            <span className="cal-dot info" aria-hidden="true" /> 沟通
            <span className="cal-dot ok" aria-hidden="true" /> 跟进建议
          </span>
          · 按 UTC 日期聚合
        </span>
      </div>
      <div className="cal-grid" role="grid" aria-label={`${month} 动态日历`}>
        {WEEKDAYS.map((d) => (
          <div key={d} className="cal-weekday">
            {d}
          </div>
        ))}
        {cells.map((cell, i) =>
          cell === null ? (
            <div key={`pad-${i}`} className="cal-day cal-pad" />
          ) : (
            <div
              key={cell.key}
              className={
                cell.key === today && hasEvent(cell.key)
                  ? 'cal-day today has-event'
                  : cell.key === today
                    ? 'cal-day today'
                    : hasEvent(cell.key)
                      ? 'cal-day has-event'
                      : 'cal-day'
              }
              title={
                [
                  messages_by_day[cell.key] ? `沟通 ${messages_by_day[cell.key]} 条` : '',
                  followups_by_day[cell.key] ? `跟进建议 ${followups_by_day[cell.key]} 项` : '',
                ]
                  .filter(Boolean)
                  .join(' · ') || undefined
              }
            >
              <span className="cal-num">{cell.day}</span>
              {(messages_by_day[cell.key] ?? 0) > 0 && (
                <span className="cal-dot info" aria-label={`沟通 ${messages_by_day[cell.key]} 条`} />
              )}
              {(followups_by_day[cell.key] ?? 0) > 0 && (
                <span className="cal-dot ok" aria-label={`跟进建议 ${followups_by_day[cell.key]} 项`} />
              )}
            </div>
          ),
        )}
      </div>
    </div>
  );
}
