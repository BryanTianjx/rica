import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { DashboardPage } from '../pages/DashboardPage';
import { backend } from '../services/backend';
import type { Dashboard } from '../types';

// ── 测试辅助 ────────────────────────────────────────────────────────────

/** 构造一份默认 Dashboard 数据，可用 overrides 局部覆盖。 */
function makeDashboard(overrides: Partial<Dashboard> = {}): Dashboard {
  return {
    candidate_total: 5,
    candidate_status_counts: { New: 2, Active: 3 },
    active_follow_up_count: 4,
    pending_follow_ups: [],
    awaiting_result_count: 1,
    pending_draft_count: 2,
    recent_messages: [],
    position_total: 3,
    open_position_count: 2,
    calendar: null,
    ...overrides,
  };
}

/** 渲染 Dashboard 并等待数据加载完成。 */
async function renderPage(data: Dashboard) {
  vi.spyOn(backend.dashboard, 'get').mockResolvedValue(data);
  render(
    <MemoryRouter>
      <DashboardPage />
    </MemoryRouter>,
  );
  // 等待标题出现即代表数据已加载
  return screen.findByRole('heading', { name: '工作台' });
}

// ── 测试 ────────────────────────────────────────────────────────────────

describe('DashboardPage — 统计卡片', () => {
  it('渲染候选人总数、进行中跟进、待记录结果、待审核草稿四个统计卡片', async () => {
    await renderPage(makeDashboard());
    // 四个统计标签都存在
    expect(screen.getByText('候选人总数')).toBeInTheDocument();
    expect(screen.getByText('进行中的跟进')).toBeInTheDocument();
    expect(screen.getByText('待记录结果')).toBeInTheDocument();
    expect(screen.getByText('待审核草稿')).toBeInTheDocument();
    // 对应的值（通过 stat 容器定位，避免数字冲突）
    const labels = screen.getAllByText(/候选人总数|进行中的跟进|待记录结果|待审核草稿/);
    expect(labels).toHaveLength(4);
    // 候选人总数 = 5
    const totalStat = screen.getByText('候选人总数').closest('.stat');
    expect(totalStat).toHaveTextContent('5');
    // 进行中跟进 = 4
    const followStat = screen.getByText('进行中的跟进').closest('.stat');
    expect(followStat).toHaveTextContent('4');
    // 待记录结果 = 1
    const resultStat = screen.getByText('待记录结果').closest('.stat');
    expect(resultStat).toHaveTextContent('1');
  });

  it('进行中跟进卡片显示提示语', async () => {
    await renderPage(makeDashboard());
    expect(
      screen.getByText('待处理 / 可跟进 / 草稿中 / 待审核'),
    ).toBeInTheDocument();
  });
});

describe('DashboardPage — 候选人状态列表', () => {
  it('展示数量大于零的候选人状态', async () => {
    await renderPage(
      makeDashboard({
        candidate_status_counts: { New: 2, Active: 3, Rejected: 0 },
      }),
    );
    expect(screen.getByText('New')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
    // Rejected 数量为 0，不应显示
    expect(screen.queryByText('Rejected')).not.toBeInTheDocument();
  });

  it('没有候选人时显示空状态提示', async () => {
    await renderPage(
      makeDashboard({ candidate_status_counts: {} }),
    );
    expect(screen.getByText('还没有候选人')).toBeInTheDocument();
  });

  it('显示在职岗位与岗位总数', async () => {
    await renderPage(
      makeDashboard({ open_position_count: 2, position_total: 5 }),
    );
    expect(screen.getByText(/在职岗位/)).toHaveTextContent('在职岗位 2/5');
  });
});

describe('DashboardPage — 待跟进列表', () => {
  it('展示待跟进任务并渲染候选人姓名', async () => {
    await renderPage(
      makeDashboard({
        pending_follow_ups: [
          {
            id: 'task-1',
            candidate_id: 'cand-1',
            candidate_name: '张三',
            position_id: null,
            status: 'Pending',
            objective: '首次招呼',
            reason: null,
            block_reason: null,
            recommended_at: '2026-11-02T13:35:16.738715Z',
            expires_at: null,
            last_follow_up_at: null,
            follow_up_count: 0,
            completed_at: null,
            result_type: null,
            result_note: null,
            result_recorded_at: null,
            created_at: '2026-11-02T13:00:00.000000Z',
            updated_at: '2026-11-02T13:00:00.000000Z',
          },
        ],
      }),
    );
    expect(await screen.findByText('张三')).toBeInTheDocument();
  });

  it('没有待跟进任务时显示空状态', async () => {
    await renderPage(makeDashboard({ pending_follow_ups: [] }));
    expect(screen.getByText('没有待跟进的任务')).toBeInTheDocument();
  });

  it('待跟进任务带阻止原因时显示 warning-box', async () => {
    await renderPage(
      makeDashboard({
        pending_follow_ups: [
          {
            id: 'task-1',
            candidate_id: 'cand-1',
            candidate_name: '李四',
            position_id: null,
            status: 'Pending',
            objective: null,
            reason: null,
            block_reason: '候选人拒绝联系',
            recommended_at: null,
            expires_at: null,
            last_follow_up_at: null,
            follow_up_count: 0,
            completed_at: null,
            result_type: null,
            result_note: null,
            result_recorded_at: null,
            created_at: '2026-11-02T13:00:00.000000Z',
            updated_at: '2026-11-02T13:00:00.000000Z',
          },
        ],
      }),
    );
    expect(await screen.findByText('李四')).toBeInTheDocument();
    // warning-box compact 内含阻止原因文本
    const warningBox = document.querySelector('.warning-box.compact');
    expect(warningBox).not.toBeNull();
    expect(warningBox).toHaveTextContent('阻止原因');
    expect(warningBox).toHaveTextContent('候选人拒绝联系');
  });
});

describe('DashboardPage — 近期沟通', () => {
  it('展示近期沟通记录表格', async () => {
    await renderPage(
      makeDashboard({
        recent_messages: [
          {
            id: 'msg-1',
            candidate_id: 'cand-1',
            conversation_id: 'conv-1',
            sender_type: 'Recruiter',
            content: '您好，看到您的简历想和您聊聊。',
            message_time: '2026-11-02T13:35:16.738715Z',
            channel: 'wechat',
            source: 'manual',
            created_at: '2026-11-02T13:35:16.738715Z',
          },
        ],
      }),
    );
    expect(await screen.findByText(/您好，看到您的简历/)).toBeInTheDocument();
    expect(screen.getByText('发送方')).toBeInTheDocument();
    expect(screen.getByText('内容')).toBeInTheDocument();
    expect(screen.getByText('时间')).toBeInTheDocument();
  });

  it('没有沟通记录时显示空状态', async () => {
    await renderPage(makeDashboard({ recent_messages: [] }));
    expect(screen.getByText('还没有沟通记录')).toBeInTheDocument();
  });
});

describe('DashboardPage — 刷新与错误', () => {
  it('点击刷新按钮触发 reload', async () => {
    const mockGet = vi
      .spyOn(backend.dashboard, 'get')
      .mockResolvedValue(makeDashboard());
    await renderPage(makeDashboard());
    const refreshBtn = screen.getByRole('button', { name: '刷新' });
    fireEvent.click(refreshBtn);
    // 初始加载 1 次 + 刷新 1 次 = 至少 2 次
    expect(mockGet.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it('API 错误时显示错误信息', async () => {
    vi.spyOn(backend.dashboard, 'get').mockRejectedValue(
      new Error('Request failed with HTTP 500'),
    );
    render(
      <MemoryRouter>
        <DashboardPage />
      </MemoryRouter>,
    );
    expect(await screen.findByText('Request failed with HTTP 500')).toBeInTheDocument();
  });
});

describe('DashboardPage — 本月动态日历', () => {
  it('渲染本月网格，今日高亮并显示逐日事件点', async () => {
    const today = new Date().toISOString().slice(0, 10);
    const month = today.slice(0, 7);
    const calendar = {
      month,
      messages_by_day: { [today]: 3 },
      followups_by_day: {},
    };
    vi.spyOn(backend.dashboard, 'get').mockResolvedValue(makeDashboard({ calendar }));
    await renderPage(makeDashboard({ calendar }));

    expect(screen.getByRole('grid', { name: `${month} 动态日历` })).toBeInTheDocument();
    // 今日格子带高亮与事件点（可访问名=事件计数）
    const todayCell = screen.getByText(String(Number(today.slice(8)))).closest('.cal-day');
    expect(todayCell).toBeTruthy();
    expect(todayCell?.classList.contains('today')).toBe(true);
    expect(todayCell?.querySelector('.cal-dot.info')).toBeTruthy();
  });

  it('calendar 为空时整个日历卡不渲染', async () => {
    vi.spyOn(backend.dashboard, 'get').mockResolvedValue(makeDashboard({ calendar: null }));
    await renderPage(makeDashboard({ calendar: null }));
    expect(screen.queryByText('本月动态')).toBeNull();
  });
});