import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { FollowUpsPage } from '../pages/FollowUpsPage';
import { backend } from '../services/backend';
import type { FollowUpTaskStatus, ResultType } from '../types';

// ── Test helpers ───────────────────────────────────────────────────────

async function renderPage(tasks: any[]) {
  const originalList = backend.followUps.list;
  const mockList = vi.fn<typeof originalList>();
  mockList.mockResolvedValue({
    items: tasks,
    total: tasks.length,
    offset: 0,
    limit: 50,
    has_more: false,
  });
  vi.spyOn(backend.followUps, 'list').mockImplementation(mockList);
  render(
    <MemoryRouter>
      <FollowUpsPage />
    </MemoryRouter>,
  );
  // Wait for the async loader to resolve
  return (screen as any).findByRole('row', { name: /张三/ });
}

function makeTask(overrides: {
  status?: FollowUpTaskStatus;
  block_reason?: string;
  result_type?: ResultType | null;
  id?: string;
  candidate_name?: string;
} = {}) {
  return {
    id: overrides.id ?? 'task-1',
    candidate_id: 'cand-1',
    candidate_name: overrides.candidate_name ?? '张三',
    status: overrides.status ?? 'Pending',
    position_id: 'pos-1',
    objective: '首次招呼',
    reason: null,
    block_reason: overrides.block_reason ?? null,
    recommended_at: '2026-11-02T13:35:16.738715Z',
    expires_at: null,
    last_follow_up_at: null,
    follow_up_count: 0,
    completed_at: null,
    result_type: overrides.result_type ?? null,
    result_note: null,
    result_recorded_at: null,
    created_at: '2026-11-02T13:00:00.000000Z',
    updated_at: '2026-11-02T13:00:00.000000Z',
  };
}

// ── Tests ──────────────────────────────────────────────────────────────

describe('FollowUpsPage — 推进按钮', () => {
  it('Pending 状态下显示推进按钮', async () => {
    await renderPage([makeTask({ status: 'Pending' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const buttons = row.querySelectorAll('button') as NodeListOf<HTMLElement>;
    const advanceBtns = Array.from(buttons).filter((b) => b.textContent === '推进');
    expect(advanceBtns).toHaveLength(1);
  });

  it('Eligible 状态下显示推进按钮', async () => {
    await renderPage([makeTask({ status: 'Eligible' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const buttons = row.querySelectorAll('button') as NodeListOf<HTMLElement>;
    const advanceBtns = Array.from(buttons).filter((b) => b.textContent === '推进');
    expect(advanceBtns).toHaveLength(1);
  });

  it('Drafting 状态下显示推进按钮', async () => {
    await renderPage([makeTask({ status: 'Drafting' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const buttons = row.querySelectorAll('button') as NodeListOf<HTMLElement>;
    const advanceBtns = Array.from(buttons).filter((b) => b.textContent === '推进');
    expect(advanceBtns).toHaveLength(1);
  });

  it('Blocked 状态下不显示推进按钮', async () => {
    await renderPage([makeTask({ status: 'Blocked', block_reason: 'Candidate refused' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const buttons = row.querySelectorAll('button') as NodeListOf<HTMLElement>;
    const advanceBtns = Array.from(buttons).filter((b) => b.textContent === '推进');
    expect(advanceBtns).toHaveLength(0);
  });

  it('Rejected 状态下不显示推进按钮', async () => {
    await renderPage([makeTask({ status: 'Rejected' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const buttons = row.querySelectorAll('button') as NodeListOf<HTMLElement>;
    const advanceBtns = Array.from(buttons).filter((b) => b.textContent === '推进');
    expect(advanceBtns).toHaveLength(0);
  });

  it('Approved 状态下不显示推进按钮', async () => {
    await renderPage([makeTask({ status: 'Approved' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const buttons = row.querySelectorAll('button') as NodeListOf<HTMLElement>;
    const advanceBtns = Array.from(buttons).filter((b) => b.textContent === '推进');
    expect(advanceBtns).toHaveLength(0);
  });

  it('Completed 状态下不显示推进按钮', async () => {
    await renderPage([makeTask({ status: 'Completed' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const buttons = row.querySelectorAll('button') as NodeListOf<HTMLElement>;
    const advanceBtns = Array.from(buttons).filter((b) => b.textContent === '推进');
    expect(advanceBtns).toHaveLength(0);
  });
});

describe('FollowUpsPage — 记录结果下拉框', () => {
  it('Pending 状态下显示记录结果下拉框', async () => {
    await renderPage([makeTask({ status: 'Pending' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const select = row.querySelector('select');
    expect(select).toBeInTheDocument();
    expect(select).toHaveTextContent('记录结果…');
  });

  it('Completed 状态下不显示记录结果下拉框', async () => {
    await renderPage([makeTask({ status: 'Completed', result_type: 'sent' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const select = row.querySelector('select');
    expect(select).not.toBeInTheDocument();
  });

  it('Blocked 状态下不显示记录结果下拉框', async () => {
    await renderPage([makeTask({ status: 'Blocked', block_reason: 'Candidate refused' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const select = row.querySelector('select');
    expect(select).not.toBeInTheDocument();
  });

  it('Completed 状态下显示已记录结果', async () => {
    await renderPage([makeTask({ status: 'Completed', result_type: 'sent' })]);
    const cell = (screen as any).getByText('sent');
    expect(cell).toBeInTheDocument();
  });
});

describe('FollowUpsPage — 阻止原因展示', () => {
  it('Blocked 状态下显示阻止原因', async () => {
    await renderPage([makeTask({ status: 'Blocked', block_reason: 'Candidate refused contact' })]);
    const row = (screen as any).getByRole('row', { name: /张三/ });
    const warningBox = row.querySelector('.warning-box');
    expect(warningBox).toBeInTheDocument();
    expect(warningBox).toHaveTextContent('阻止原因');
    expect(warningBox).toHaveTextContent('Candidate refused contact');
  });

  it('非 Blocked 状态不显示阻止原因', async () => {
    await renderPage([makeTask({ status: 'Pending' })]);
    const rows = await (screen as any).findAllByRole('row', { name: /张三/ });
    const warningBoxes = rows[0].querySelectorAll('.warning-box') as NodeListOf<Element>;
    expect(warningBoxes).toHaveLength(0);
  });
});

describe('FollowUpsPage — 错误显示', () => {
  it('API 错误显示错误消息', async () => {
    const originalList = backend.followUps.list;
    const mockList = vi.fn<typeof originalList>();
    mockList.mockRejectedValue(new Error('Request failed with HTTP 403'));
    vi.spyOn(backend.followUps, 'list').mockImplementation(mockList);
    render(<MemoryRouter><FollowUpsPage /></MemoryRouter>);
    await (screen as any).findByText('Request failed with HTTP 403');
    vi.restoreAllMocks();
  });
});

// ── F3: 跟进目标编辑（PRD §5.6「修改」） ────────────────────────────────

describe('FollowUpsPage — 编辑跟进目标', () => {
  async function openRow(status: FollowUpTaskStatus = 'Pending') {
    vi.spyOn(backend.followUps, 'list').mockResolvedValue({
      items: [makeTask({ status })],
      total: 1,
      offset: 0,
      limit: 50,
      has_more: false,
    });
    render(
      <MemoryRouter>
        <FollowUpsPage />
      </MemoryRouter>,
    );
    return (screen as any).findByRole('row', { name: /张三/ });
  }

  it('非终态任务显示编辑目标按钮，保存后提交新 objective', async () => {
    const mockUpdate = vi.spyOn(backend.followUps, 'update').mockResolvedValue(makeTask());
    await openRow('Pending');

    fireEvent.click(screen.getByRole('button', { name: '编辑目标' }));
    const input = screen.getByLabelText('编辑跟进目标');
    expect(input).toHaveValue('首次招呼');
    fireEvent.change(input, { target: { value: '约本周三线上面试' } });
    fireEvent.click(screen.getByRole('button', { name: '保存目标' }));

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith('task-1', { objective: '约本周三线上面试' });
    });
  });

  it('清空目标后保存提交显式 null（清空真正生效）', async () => {
    const mockUpdate = vi.spyOn(backend.followUps, 'update').mockResolvedValue(makeTask());
    await openRow('Eligible');

    fireEvent.click(screen.getByRole('button', { name: '编辑目标' }));
    fireEvent.change(screen.getByLabelText('编辑跟进目标'), { target: { value: '  ' } });
    fireEvent.click(screen.getByRole('button', { name: '保存目标' }));

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith('task-1', { objective: null });
    });
    const payload = mockUpdate.mock.calls[0][1];
    expect(payload).toHaveProperty('objective', null);
  });

  it('终态任务（Completed）不显示编辑目标按钮', async () => {
    const row = await openRow('Completed');
    const buttons = row.querySelectorAll('button') as NodeListOf<HTMLElement>;
    const editBtns = Array.from(buttons).filter((b) => b.textContent === '编辑目标');
    expect(editBtns).toHaveLength(0);
  });
});

// ── F5: 两步记录结果 + 备注（PRD §5.12） ────────────────────────────────

describe('FollowUpsPage — 记录结果两步流与备注', () => {
  function getRowSelect(): HTMLSelectElement {
    const row = (screen as any).getByRole('row', { name: /张三/ });
    return row.querySelector('select') as HTMLSelectElement;
  }
  async function openPendingRow() { 
    vi.spyOn(backend.followUps, 'list').mockResolvedValue({
      items: [makeTask({ status: 'Pending' })],
      total: 1,
      offset: 0,
      limit: 50,
      has_more: false,
    });
    render(
      <MemoryRouter>
        <FollowUpsPage />
      </MemoryRouter>,
    );
    await (screen as any).findByRole('row', { name: /张三/ });
  }

  it('选择结果后出现确认条；填备注确认，载荷携带 result_type 与 note', async () => {
    const mockRecord = vi
      .spyOn(backend.followUps, 'recordResult')
      .mockResolvedValue(makeTask({ status: 'Completed', result_type: 'sent' }));
    await openPendingRow();

    fireEvent.change(getRowSelect(), { target: { value: 'sent' } });
    // 确认条出现，含所选拟发送结果类型
    expect(await screen.findByLabelText(/记录「已发送」的备注/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText(/记录「已发送」的备注/), {
      target: { value: '已在 BOSS 发出，等待回复' },
    });
    fireEvent.click(screen.getByRole('button', { name: '确认记录' }));

    await waitFor(() => {
      expect(mockRecord).toHaveBeenCalledWith('task-1', {
        result_type: 'sent',
        note: '已在 BOSS 发出，等待回复',
      });
    });
  });

  it('备注留空确认时不带 note 键（JSON 丢弃 undefined）', async () => {
    const mockRecord = vi
      .spyOn(backend.followUps, 'recordResult')
      .mockResolvedValue(makeTask({ status: 'Completed', result_type: 'other' }));
    await openPendingRow();

    fireEvent.change(getRowSelect(), { target: { value: 'other' } });
    fireEvent.click(screen.getByRole('button', { name: '确认记录' }));

    await waitFor(() => {
      expect(mockRecord).toHaveBeenCalledWith('task-1', { result_type: 'other' });
    });
    const payload = mockRecord.mock.calls[0][1];
    expect(payload.note).toBeUndefined();
  });

  it('确认条取消后不提交记录', async () => {
    const mockRecord = vi.spyOn(backend.followUps, 'recordResult');
    await openPendingRow();

    fireEvent.change(getRowSelect(), { target: { value: 'sent' } });
    fireEvent.click(await screen.findByRole('button', { name: '放弃' }));

    expect(mockRecord).not.toHaveBeenCalled();
    // 确认条收回
    expect(screen.queryByLabelText(/记录「已发送」的备注/)).not.toBeInTheDocument();
  });
});
