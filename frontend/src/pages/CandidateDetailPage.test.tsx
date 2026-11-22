import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { CandidateDetailPage } from '../pages/CandidateDetailPage';
import { aiApi, candidatesApi, draftsApi, followUpsApi, settingsApi } from '../services/endpoints';
import type { Candidate, ContextPreview, Draft, FollowUpDecision } from '../types';

// ── 测试辅助 ────────────────────────────────────────────────────────────

function makeCandidate(overrides: Partial<Candidate> = {}): Candidate {
  return {
    id: 'cand-1',
    name: '张三',
    source: 'manual',
    position_id: 'pos-1',
    experience_summary: null,
    skill_summary: null,
    expected_salary: '25-35k',
    expected_location: '上海',
    job_intention: 'Python 后端',
    notes: '原有备注',
    status: 'New',
    archived: false,
    created_at: '2026-11-02T13:00:00.000000Z',
    updated_at: '2026-11-02T13:00:00.000000Z',
    position_name: '前端工程师',
    message_count: 3,
    active_follow_up_count: 1,
    ...overrides,
  };
}

function makeContext(): ContextPreview {
  return {
    candidate_id: 'cand-1',
    candidate_summary: {},
    position_summary: {},
    message_count: 0,
    recent_messages: [],
    notes: [],
    unknowns: [],
    conflicts: [],
    redaction_applied: [],
  };
}

function makeDecision(): FollowUpDecision {
  return {
    outcome: 'eligible',
    eligible: true,
    reasons: [],
    block_reasons: [],
    recommended_at: null,
    suggested_objective: null,
    risks: [],
  };
}

/** 渲染详情页路由，并 mock 挂载时的全部只读接口。 */
function renderDetail(candidate: Candidate = makeCandidate()) {
  vi.spyOn(candidatesApi, 'get').mockResolvedValue(candidate);
  vi.spyOn(candidatesApi, 'timeline').mockResolvedValue([]);
  vi.spyOn(candidatesApi, 'context').mockResolvedValue(makeContext());
  vi.spyOn(followUpsApi, 'evaluate').mockResolvedValue(makeDecision());
  vi.spyOn(settingsApi, 'getAiEgress').mockResolvedValue({
    enabled: true,
    consent_granted: true,
    provider: 'openai_compatible',
    model: 'glm-4-flash',
    base_url: 'https://example.invalid/v4',
    masked_fields: ['phone_number'],
    include_candidate_name: false,
    notice: '出网告知',
  });
  vi.spyOn(aiApi, 'listModels').mockResolvedValue({ models: [], routing: {} });
  render(
    <MemoryRouter initialEntries={['/candidates/cand-1']}>
      <Routes>
        <Route path="/candidates/:id" element={<CandidateDetailPage />} />
      </Routes>
    </MemoryRouter>,
  );
  return screen.findByRole('heading', { name: /张三/ });
}

// ── 测试 ────────────────────────────────────────────────────────────────

describe('CandidateDetailPage — 档案展示', () => {
  it('渲染档案字段值，空字段显示未知占位', async () => {
    await renderDetail();
    expect(await screen.findByText('25-35k')).toBeInTheDocument();
    expect(screen.getByText('上海')).toBeInTheDocument();
    expect(screen.getByText('Python 后端')).toBeInTheDocument();
    // experience_summary 为 null → 显示「未知」
    expect(screen.getAllByText('未知').length).toBeGreaterThanOrEqual(2);
  });
});

describe('CandidateDetailPage — 档案编辑与清空语义', () => {
  it('清空期望薪资与备注后保存，PATCH 显式提交 null（清空真正生效）', async () => {
    const mockUpdate = vi
      .spyOn(candidatesApi, 'update')
      .mockResolvedValue(makeCandidate({ expected_salary: null, notes: null }));
    await renderDetail();
    await screen.findByText('25-35k');
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));

    // 两个待清空字段：期望薪资、备注
    fireEvent.change(screen.getByDisplayValue('25-35k'), { target: { value: '' } });
    fireEvent.change(screen.getByDisplayValue('原有备注'), { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        'cand-1',
        expect.objectContaining({
          expected_salary: null,
          notes: null,
          // 未清空的字段原样保留
          expected_location: '上海',
          job_intention: 'Python 后端',
        }),
      );
    });
    // null 必须真的出现在载荷里（JSON.stringify 会丢掉 undefined，
    // 而后端 exclude_unset 把「键缺失」当作「未提供」→ 旧值保留）
    const payload = mockUpdate.mock.calls[0][1];
    expect(payload).toHaveProperty('expected_salary', null);
    expect(payload).toHaveProperty('notes', null);
  });

  it('修改字段后保存提交新值（非空路径不受影响）', async () => {
    const mockUpdate = vi
      .spyOn(candidatesApi, 'update')
      .mockResolvedValue(makeCandidate({ expected_salary: '30-40k' }));
    await renderDetail();
    await screen.findByText('25-35k');
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));

    fireEvent.change(screen.getByDisplayValue('25-35k'), { target: { value: '30-40k' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        'cand-1',
        expect.objectContaining({
          expected_salary: '30-40k',
          expected_location: '上海',
        }),
      );
    });
  });

  it('取消编辑不触发保存', async () => {
    const mockUpdate = vi.spyOn(candidatesApi, 'update').mockResolvedValue(makeCandidate());
    await renderDetail();
    await screen.findByText('25-35k');
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(mockUpdate).not.toHaveBeenCalled();
  });
});

// ── A2: AI 生成「消息长度」参数（PRD §5.9） ─────────────────────────────

function makeDraft(): Draft {
  return {
    id: 'draft-1',
    candidate_id: 'cand-1',
    task_id: null,
    message_type: 'reply',
    tone: 'friendly',
    status: 'generated',
    objective: null,
    message_text: '你好，方便聊聊吗？',
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
    created_at: '2026-11-16T00:00:00.000000Z',
    updated_at: '2026-11-16T00:00:00.000000Z',
    draft_group_id: null,
    routing_decision_json: null,
    routing_tier: null,
    routing_reason: null,
    cost_cents: null,
    prompt_tokens: null,
    completion_tokens: null,
  };
}

describe('CandidateDetailPage — 消息长度参数', () => {
  it('填写合法长度后生成，载荷携带 max_length', async () => {
    const mockGenerate = vi
      .spyOn(draftsApi, 'generate')
      .mockResolvedValue(makeDraft());
    await renderDetail();
    await screen.findByText('25-35k');

    fireEvent.change(screen.getByLabelText(/消息长度/), { target: { value: '300' } });
    fireEvent.click(screen.getByRole('button', { name: '生成草稿' }));

    await waitFor(() => {
      expect(mockGenerate).toHaveBeenCalledWith(
        expect.objectContaining({ max_length: 300 }),
      );
    });
    expect(screen.getByText('你好，方便聊聊吗？')).toBeInTheDocument();
  });

  it('非法长度（30）被客户端拦截：显示错误且不发起请求', async () => {
    const mockGenerate = vi.spyOn(draftsApi, 'generate').mockResolvedValue(makeDraft());
    await renderDetail();
    await screen.findByText('25-35k');

    fireEvent.change(screen.getByLabelText(/消息长度/), { target: { value: '30' } });
    fireEvent.click(screen.getByRole('button', { name: '生成草稿' }));

    expect(await screen.findByText(/消息长度需为 50–2000 之间的整数/)).toBeInTheDocument();
    expect(mockGenerate).not.toHaveBeenCalled();
  });

  it('留空时不发送 max_length 键（后端按无限制处理）', async () => {
    const mockGenerate = vi.spyOn(draftsApi, 'generate').mockResolvedValue(makeDraft());
    await renderDetail();
    await screen.findByText('25-35k');

    fireEvent.click(screen.getByRole('button', { name: '生成草稿' }));

    await waitFor(() => {
      expect(mockGenerate).toHaveBeenCalled();
    });
    expect(mockGenerate.mock.calls[0][0]).not.toHaveProperty('max_length');
  });
});
