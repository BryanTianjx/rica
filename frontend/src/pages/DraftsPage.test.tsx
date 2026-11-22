import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { DraftsPage } from '../pages/DraftsPage';
import { draftsApi } from '../services/endpoints';
import type { Draft, Page } from '../types';

// ── 测试辅助 ────────────────────────────────────────────────────────────

function pageOf<T>(items: T[]): Page<T> {
  return { items, total: items.length, offset: 0, limit: 50, has_more: false };
}

function makeDraft(overrides: Partial<Draft> = {}): Draft {
  return {
    id: 'draft-1',
    candidate_id: 'cand-1',
    task_id: null,
    message_type: 'greeting',
    tone: 'friendly',
    status: 'generated',
    objective: null,
    message_text: '你好，看到你的简历想和你聊聊。',
    alternatives: [],
    context_references: [],
    uncertainties: [],
    warnings: [],
    validation_status: 'Passed',
    validation_messages: [],
    quality_status: null,
    provider: null,
    model: null,
    prompt_version: null,
    is_edited: false,
    approved_at: null,
    copied_at: null,
    rejected_at: null,
    created_at: '2026-11-02T13:00:00.000000Z',
    updated_at: '2026-11-02T13:00:00.000000Z',
    draft_group_id: null,
    routing_decision_json: null,
    routing_tier: null,
    routing_reason: null,
    cost_cents: null,
    prompt_tokens: null,
    completion_tokens: null,
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

async function renderPage() {
  vi.spyOn(draftsApi, 'list').mockResolvedValue(
    pageOf([
      makeDraft(),
      makeDraft({
        id: 'draft-2',
        message_type: 'follow_up_question',
        message_text: '跟进一下最近的面试安排。',
        status: 'edited',
      }),
    ]),
  );

  render(
    <MemoryRouter>
      <DraftsPage />
    </MemoryRouter>,
  );
  return screen.findByRole('button', { name: /greeting/ });
}

afterEach(() => {
  vi.restoreAllMocks();
});

// ── 测试 ────────────────────────────────────────────────────────────────

describe('DraftsPage — 页面标题', () => {
  it('h1 为「AI 草稿审核」，不再自称 AI 助手（T-01 AC2）', async () => {
    await renderPage();

    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('AI 草稿审核');
  });
});

describe('DraftsPage — 键盘可达性', () => {
  it('草稿行渲染为真实 button，且可通过 Tab 聚焦', async () => {
    const user = userEvent.setup();
    await renderPage();

    const row = screen.getByRole('button', { name: /follow_up_question/ });
    expect(row.tagName).toBe('BUTTON');
    expect(row).toHaveClass('row-link');

    await tabTo(user, row);
    expect(row).toHaveFocus();
  });

  it('聚焦草稿行后按 Enter 选中草稿并回填编辑框', async () => {
    const user = userEvent.setup();
    await renderPage();

    const row = screen.getByRole('button', { name: /follow_up_question/ });
    await tabTo(user, row);
    await user.keyboard('{Enter}');

    expect(screen.getByLabelText('话术内容（可直接编辑）')).toHaveValue(
      '跟进一下最近的面试安排。',
    );
    expect(row).toHaveAttribute('aria-current', 'true');
  });

  it('选中草稿后清空文本保持为空，不会回填原稿（F4-4）', async () => {
    const user = userEvent.setup();
    await renderPage();

    const row = screen.getByRole('button', { name: /greeting/ });
    await tabTo(user, row);
    await user.keyboard('{Enter}');

    const textarea = screen.getByLabelText('话术内容（可直接编辑）');
    await user.clear(textarea);
    expect(textarea).toHaveValue('');

    await user.type(textarea, '改写后的话术');
    expect(textarea).toHaveValue('改写后的话术');
  });
});
