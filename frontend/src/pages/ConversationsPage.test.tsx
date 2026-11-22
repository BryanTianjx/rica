import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { ConversationsPage } from '../pages/ConversationsPage';
import { candidatesApi, conversationsApi, messagesApi, positionsApi } from '../services/endpoints';
import type { Candidate, Conversation, Message, Page, Position } from '../types';

// ── 测试辅助 ────────────────────────────────────────────────────────────

function pageOf<T>(items: T[]): Page<T> {
  return { items, total: items.length, offset: 0, limit: 50, has_more: false };
}

function makePosition(overrides: Partial<Position> = {}): Position {
  return {
    id: 'pos-1',
    name: 'Java 开发',
    description: null,
    location: null,
    skill_requirements: null,
    experience_requirements: null,
    education_requirements: null,
    salary_min: null,
    salary_max: null,
    notes: null,
    status: 'Open',
    archived: false,
    created_at: '2026-11-02T13:00:00.000000Z',
    updated_at: '2026-11-02T13:00:00.000000Z',
    candidate_count: 1,
    ...overrides,
  };
}

function makeCandidate(overrides: Partial<Candidate> = {}): Candidate {
  return {
    id: 'cand-1',
    name: '张三',
    source: 'manual',
    position_id: null,
    experience_summary: null,
    skill_summary: null,
    expected_salary: null,
    expected_location: null,
    job_intention: null,
    notes: null,
    status: 'Active',
    archived: false,
    created_at: '2026-11-02T13:00:00.000000Z',
    updated_at: '2026-11-02T13:00:00.000000Z',
    position_name: null,
    message_count: 2,
    active_follow_up_count: 0,
    ...overrides,
  };
}

function makeConversation(overrides: Partial<Conversation> = {}): Conversation {
  return {
    id: 'conv-1',
    candidate_id: 'cand-1',
    channel: 'wechat',
    title: null,
    is_active: true,
    last_message_at: '2026-11-02T13:35:16.738715Z',
    created_at: '2026-11-02T13:00:00.000000Z',
    updated_at: '2026-11-02T13:35:16.738715Z',
    message_count: 2,
    position_id: null,
    position_name: null,
    ...overrides,
  };
}

function makeMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: 'msg-1',
    candidate_id: 'cand-1',
    conversation_id: 'conv-1',
    sender_type: 'Candidate',
    content: '您好，想和您聊聊。',
    message_time: '2026-11-02T13:35:16.738715Z',
    channel: 'wechat',
    source: 'manual',
    created_at: '2026-11-02T13:35:16.738715Z',
    ...overrides,
  };
}

/** Tab forward until `element` receives focus; fails the test if unreachable. */
async function tabTo(
  user: ReturnType<typeof userEvent.setup>,
  element: HTMLElement,
  maxTabs = 10,
) {
  for (let i = 0; i < maxTabs; i += 1) {
    await user.tab();
    if (document.activeElement === element) return;
  }
  throw new Error('元素无法通过 Tab 聚焦');
}

// The overrides are complete Conversation objects (every call site passes
// makeConversation(...) results), so the array must be Conversation[] —
// declaring Partial<Conversation>[] widened the spread to
// (Conversation | Partial<Conversation>)[], which pageOf then inferred as
// Page<Partial<Conversation>> and failed to assign to Page<Conversation>
// (TS2345, surfaced by `npm run build` -> tsc -b, not by vitest).
async function renderPage(conversationOverrides: Conversation[] = []) {
  const positionsList = vi
    .spyOn(positionsApi, 'list')
    .mockResolvedValue(pageOf([makePosition()]));
  vi.spyOn(candidatesApi, 'list').mockResolvedValue(
    pageOf([
      makeCandidate(),
      makeCandidate({ id: 'cand-2', name: '李四', message_count: 1 }),
    ]),
  );
  vi.spyOn(conversationsApi, 'list').mockResolvedValue(
    pageOf([
      makeConversation(),
      makeConversation({
        id: 'conv-2',
        candidate_id: 'cand-2',
        is_active: false,
        message_count: 1,
      }),
      ...conversationOverrides.map((override) => makeConversation(override)),
    ]),
  );
  vi.spyOn(messagesApi, 'list').mockResolvedValue(pageOf([makeMessage()]));

  render(
    <MemoryRouter>
      <ConversationsPage />
    </MemoryRouter>,
  );
  await screen.findByText('张三');
  return { positionsList };
}

afterEach(() => {
  vi.restoreAllMocks();
});

// ── 测试 ────────────────────────────────────────────────────────────────

describe('ConversationsPage — 键盘可达性', () => {
  it('会话行渲染为真实 button，且可通过 Tab 聚焦', async () => {
    const user = userEvent.setup();
    await renderPage();

    const row = screen.getByRole('button', { name: /张三/ });
    expect(row.tagName).toBe('BUTTON');
    expect(row).toHaveClass('row-link');

    await tabTo(user, row);
    expect(row).toHaveFocus();
  });

  it('聚焦会话行后按 Enter 选中会话并加载时间线', async () => {
    const user = userEvent.setup();
    await renderPage();

    const row = screen.getByRole('button', { name: /张三/ });
    await tabTo(user, row);
    await user.keyboard('{Enter}');

    expect(await screen.findByText('时间线')).toBeInTheDocument();
    expect(screen.getByText('您好，想和您聊聊。')).toBeInTheDocument();
    // 页面保持挂载：焦点留在原行按钮上，选中态通过 aria-current 暴露给辅助技术
    expect(row).toHaveFocus();
    expect(row).toHaveAttribute('aria-current', 'true');
  });

  it('聚焦会话行后按 Space 同样选中会话', async () => {
    const user = userEvent.setup();
    await renderPage();

    const row = screen.getByRole('button', { name: /李四/ });
    await tabTo(user, row);
    await user.keyboard(' ');

    expect(await screen.findByText('时间线')).toBeInTheDocument();
  });
});

describe('ConversationsPage — 切换会话的加载态', () => {
  it('新会话请求返回前不显示上一个会话的时间线，只显示加载态', async () => {
    const user = userEvent.setup();
    vi.spyOn(positionsApi, 'list').mockResolvedValue(pageOf([makePosition()]));
    vi.spyOn(candidatesApi, 'list').mockResolvedValue(
      pageOf([
        makeCandidate(),
        makeCandidate({ id: 'cand-2', name: '李四', message_count: 1 }),
      ]),
    );
    vi.spyOn(conversationsApi, 'list').mockResolvedValue(
      pageOf([
        makeConversation(),
        makeConversation({
          id: 'conv-2',
          candidate_id: 'cand-2',
          is_active: false,
          message_count: 1,
        }),
      ]),
    );

    // 会话 B 的请求被挂起，用于观察「尚未返回」的中间态。
    let releaseSecond: (() => void) | undefined;
    vi.spyOn(messagesApi, 'list').mockImplementation(
      (params: { candidate_id?: string; conversation_id?: string; limit?: number } = {}) => {
        if (params.conversation_id === 'conv-2') {
          return new Promise((resolve) => {
            releaseSecond = () =>
              resolve(
                pageOf([
                  makeMessage({
                    id: 'msg-2',
                    candidate_id: 'cand-2',
                    conversation_id: 'conv-2',
                    content: '第二个会话的消息',
                  }),
                ]),
              );
          }) as never;
        }
        return Promise.resolve(pageOf([makeMessage({ content: '第一个会话的消息' })]));
      },
    );

    render(
      <MemoryRouter>
        <ConversationsPage />
      </MemoryRouter>,
    );

    await user.click(await screen.findByRole('button', { name: /张三/ }));
    expect(await screen.findByText('第一个会话的消息')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /李四/ }));

    // 回归守卫：useAsync 会保留旧 data，详情列必须用 loading 门控挡掉它。
    expect(screen.queryByText('第一个会话的消息')).not.toBeInTheDocument();
    expect(screen.getByText('加载中…')).toBeInTheDocument();

    await act(async () => {
      releaseSecond?.();
    });

    expect(await screen.findByText('第二个会话的消息')).toBeInTheDocument();
    expect(screen.queryByText('第一个会话的消息')).not.toBeInTheDocument();
  });
});

describe('ConversationsPage — 岗位筛选与岗位列（G4-03 硬化）', () => {
  it('岗位下拉请求显式带 limit=200，不受后端默认 50 截断', async () => {
    const { positionsList } = await renderPage();

    expect(positionsList).toHaveBeenCalledTimes(1);
    // QA-002：不带 limit 时后端默认 limit=50，岗位超过 50 个会被截断。
    expect(positionsList).toHaveBeenCalledWith({ limit: 200 });
  });

  it('会话行显示候选人的岗位名，无岗位时显示占位符', async () => {
    await renderPage([
      makeConversation({
        id: 'conv-3',
        candidate_id: 'cand-2',
        position_id: 'pos-1',
        position_name: 'Java 开发',
      }),
    ]);

    // 「Java 开发」同时出现在筛选下拉的 option 和会话行里；
    // 断言其中至少有一处渲染在会话行按钮内部。
    const inRow = screen
      .getAllByText('Java 开发')
      .filter((element) => element.closest('button.row-link'));
    expect(inRow.length).toBeGreaterThanOrEqual(1);
    // 张三的会话无岗位（fixture 默认 null），行内显示占位符。
    expect(screen.getAllByText('—').length).toBeGreaterThanOrEqual(1);
  });

  it('选择岗位后只显示该岗位下的会话，切回「全部岗位」恢复', async () => {
    const user = userEvent.setup();
    await renderPage([
      makeConversation({
        id: 'conv-3',
        candidate_id: 'cand-2',
        position_id: 'pos-1',
        position_name: 'Java 开发',
      }),
    ]);

    const filter = await screen.findByLabelText('按岗位筛选');
    await user.selectOptions(filter, 'pos-1');

    // pos-1 只挂了李四的会话：张三的行被过滤掉。
    expect(screen.queryByRole('button', { name: /张三/ })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /李四/ })).toBeInTheDocument();

    await user.selectOptions(filter, '');

    // 李四名下有两个会话（conv-2 无岗位、conv-3 在 pos-1），恢复后都在。
    expect(screen.getByRole('button', { name: /张三/ })).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: /李四/ }).length).toBe(2);
  });

  it('岗位列表加载失败时渲染 ErrorBox，会话列表仍可用', async () => {
    vi.spyOn(positionsApi, 'list').mockRejectedValue(new Error('岗位服务不可用'));
    vi.spyOn(candidatesApi, 'list').mockResolvedValue(
      pageOf([makeCandidate()]),
    );
    vi.spyOn(conversationsApi, 'list').mockResolvedValue(pageOf([makeConversation()]));
    vi.spyOn(messagesApi, 'list').mockResolvedValue(pageOf([makeMessage()]));

    render(
      <MemoryRouter>
        <ConversationsPage />
      </MemoryRouter>,
    );

    // QA-005：/positions 失败不能静默降级成只剩「全部岗位」，要渲染错误。
    expect(await screen.findByRole('alert')).toHaveTextContent('岗位服务不可用');
    // 会话列表不受岗位失败影响，仍然渲染。
    expect(await screen.findByRole('button', { name: /张三/ })).toBeInTheDocument();
    // 筛选下拉被错误提示取代，不再渲染。
    expect(screen.queryByLabelText('按岗位筛选')).not.toBeInTheDocument();
  });
});

// ── 手动新增消息：时间与渠道（PRD §5.4 C4/C5） ───────────────────────────

describe('ConversationsPage — 手动新增消息的时间与渠道', () => {
  async function openComposer() {
    await renderPage();
    fireEvent.click(await screen.findByRole('button', { name: /张三/ }));
    expect(await screen.findByText('手动新增消息')).toBeInTheDocument();
  }

  it('补录历史时间与渠道后提交，载荷携带 message_time 与 channel', async () => {
    const mockCreate = vi
      .spyOn(messagesApi, 'create')
      .mockResolvedValue(makeMessage());
    await openComposer();

    const user = userEvent.setup();
    await user.selectOptions(screen.getByLabelText('渠道'), 'phone');
    await user.type(screen.getByLabelText(/时间（可选/), '2026-11-15T10:30');
    await user.type(screen.getByLabelText('内容'), '电话沟通过一轮');
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(mockCreate).toHaveBeenCalledWith(
        expect.objectContaining({
          sender_type: 'Recruiter',
          content: '电话沟通过一轮',
          channel: 'phone',
          // datetime-local 的本地时间被规范化为 ISO（UTC）
          message_time: new Date('2026-11-15T10:30').toISOString(),
        }),
      );
    });
  });

  it('不填时间与渠道时按原行为提交：channel=boss，message_time 为提交时刻', async () => {
    const mockCreate = vi
      .spyOn(messagesApi, 'create')
      .mockResolvedValue(makeMessage());
    await openComposer();

    const user = userEvent.setup();
    await user.type(screen.getByLabelText('内容'), '默认渠道消息');
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(mockCreate).toHaveBeenCalledWith(
        expect.objectContaining({ channel: 'boss' }),
      );
    });
    const payload = mockCreate.mock.calls[0][0];
    expect(payload.message_time).toBeTruthy();
    // 留空 = 当前时间：与 now 的偏差在容差内（而非 1970 之类的哨兵值）
    expect(Date.parse(payload.message_time as string)).toBeGreaterThan(
      Date.now() - 60_000,
    );
  });
});
