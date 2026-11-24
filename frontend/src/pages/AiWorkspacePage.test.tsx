import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { AiWorkspacePage } from './AiWorkspacePage';
import { backend } from '../services/backend';

const model = {
  model_id: 'a/b:c', display_name: 'Model A', provider_type: 'openai',
  base_url: 'https://example.com', quality_tier: 'standard', enabled: true,
  is_default: true, source: 'database', api_key_configured: true,
} as never;

beforeEach(() => {
  vi.spyOn(backend.ai, 'listModels').mockResolvedValue({ models: [model], routing: {} } as never);
  vi.spyOn(backend.ai, 'getRouting').mockResolvedValue({ routing: { message_new: 'standard' } });
  vi.spyOn(backend.settings, 'getAiEgress').mockResolvedValue({
    enabled: false, consent_granted: false, provider: 'x', model: 'y', base_url: '',
    masked_fields: ['姓名'], include_candidate_name: false, notice: 'AI 出网默认关闭',
  });
  vi.spyOn(backend.drafts, 'list').mockResolvedValue({ items: [], total: 0, offset: 0, limit: 20, has_more: false } as never);
});

describe('AiWorkspacePage — 真实契约聚合', () => {
  it('shows model, egress boundary and draft-review entry', async () => {
    render(<MemoryRouter><AiWorkspacePage /></MemoryRouter>);
    expect(await screen.findByRole('heading', { name: 'AI 工作空间' })).toBeInTheDocument();
    expect(await screen.findByRole('heading', { name: 'AI 服务状态' })).toBeInTheDocument();
    expect(await screen.findByRole('heading', { name: '出网与隐私边界' })).toBeInTheDocument();
    expect(await screen.findByRole('link', { name: '打开草稿审核' })).toHaveAttribute('href', '/drafts');
    expect(screen.getByText(/Approved 只表示人工批准/)).toBeInTheDocument();
  });

  it('degrades gracefully when AI services fail (business pages stay usable)', async () => {
    vi.spyOn(backend.ai, 'listModels').mockRejectedValue(new Error('AI 服务不可用'));
    render(<MemoryRouter><AiWorkspacePage /></MemoryRouter>);
    expect(await screen.findByRole('heading', { name: 'AI 工作空间' })).toBeInTheDocument();
    expect(await screen.findByText(/部分数据不可用/)).toBeInTheDocument();
  });
});
