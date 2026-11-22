import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { backend } from '../../services/backend';
import { TasksPanel } from './TasksPanel';
import type { BrowserAuditEvent, BrowserTask } from '../../types';

function makeTask(overrides: Partial<BrowserTask> = {}): BrowserTask {
  return {
    task_id: 'task_1',
    session_id: 'ses_abc',
    intent: '打开岗位列表并统计条数',
    state: 'pending',
    risk_level: 'L2',
    plan_sha256: null,
    steps_count: 2,
    executed_steps: 0,
    current_step: null,
    cancel_requested_at: null,
    error_code: null,
    error_message: null,
    steps: [],
    ...overrides,
  };
}

function makeEvent(overrides: Partial<BrowserAuditEvent> = {}): BrowserAuditEvent {
  return {
    event_id: 'ev_1',
    event_type: 'browser.task.created',
    entity_type: 'browser_task',
    entity_id: 'task_1',
    summary: 'task created',
    metadata: {},
    created_at: '2026-11-18T15:40:00Z',
    ...overrides,
  };
}

describe('TasksPanel', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('creates a task with auto_run=false and shows it after refresh', async () => {
    const createSpy = vi
      .spyOn(backend.browser, 'createTask')
      .mockResolvedValue({ task: makeTask() });
    vi.spyOn(backend.browser, 'audit').mockResolvedValue({ events: [makeEvent()] });
    vi.spyOn(backend.browser, 'getTask').mockResolvedValue({ task: makeTask() });

    render(<TasksPanel />);
    await waitFor(() => expect(screen.getByLabelText('会话 ID')).toBeTruthy());
    fireEvent.change(screen.getByLabelText('会话 ID'), { target: { value: 'ses_abc' } });
    fireEvent.change(screen.getByLabelText('任务意图'), { target: { value: '打开岗位列表并统计条数' } });
    fireEvent.click(screen.getByText('创建任务'));
    await waitFor(() =>
      expect(createSpy).toHaveBeenCalledWith({
        session_id: 'ses_abc',
        intent: '打开岗位列表并统计条数',
        auto_run: false,
      }),
    );
    await waitFor(() => expect(screen.getByText('打开岗位列表并统计条数')).toBeTruthy());
    expect(screen.getByText('运行')).toBeTruthy();
  });

  it('cancel of a running task needs a second confirmation', async () => {
    vi.spyOn(backend.browser, 'audit').mockResolvedValue({ events: [makeEvent()] });
    vi.spyOn(backend.browser, 'getTask').mockResolvedValue({ task: makeTask({ state: 'running', executed_steps: 1 }) });
    const cancelSpy = vi
      .spyOn(backend.browser, 'cancelTask')
      .mockResolvedValue({ task: makeTask({ state: 'cancelled' }) });

    render(<TasksPanel />);
    await waitFor(() => expect(screen.getByText('取消')).toBeTruthy());
    fireEvent.click(screen.getByText('取消'));
    expect(cancelSpy).not.toHaveBeenCalled();
    expect(screen.getByText(/取消运行中的任务/)).toBeTruthy();
    fireEvent.click(screen.getByText('确认取消'));
    await waitFor(() => expect(cancelSpy).toHaveBeenCalledWith('task_1', 'user'));
  });

  it('loads the audit timeline for a task', async () => {
    vi.spyOn(backend.browser, 'audit').mockResolvedValue({ events: [makeEvent()] });
    vi.spyOn(backend.browser, 'getTask').mockResolvedValue({ task: makeTask() });
    vi.spyOn(backend.browser, 'taskAudit').mockResolvedValue({
      events: [
        makeEvent(),
        makeEvent({ event_id: 'ev_2', event_type: 'browser.task.completed', summary: 'all steps done' }),
      ],
    });
    render(<TasksPanel />);
    await waitFor(() => expect(screen.getByText('审计')).toBeTruthy());
    fireEvent.click(screen.getByText('审计'));
    await waitFor(() => expect(screen.getByText(/审计时间线/)).toBeTruthy());
    expect(screen.getByText('browser.task.created')).toBeTruthy();
    expect(screen.getByText('browser.task.completed')).toBeTruthy();
  });
});
