import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { AuditPanel } from './AuditPanel';
import { backend } from '../../services/backend';

beforeEach(() => {
  vi.spyOn(backend.browser, 'audit').mockResolvedValue({
    events: [{
      event_id: 'ev-1', event_type: 'browser.action.executed', entity_type: 'browser_action',
      entity_id: 'a-1', summary: 'clicked #apply', metadata: {}, created_at: '2026-11-24T00:00:00Z',
    }],
    total: 1,
  } as never);
});

describe('AuditPanel', () => {
  it('renders real audit events with filters', async () => {
    render(<AuditPanel />);
    expect(await screen.findByText('clicked #apply')).toBeInTheDocument();
    expect(screen.getByLabelText('任务 ID（可选）')).toBeInTheDocument();
    expect(backend.browser.audit).toHaveBeenCalledWith(
      expect.objectContaining({ limit: 100 }),
    );
  });

  it('shows an error when the audit endpoint fails', async () => {
    vi.spyOn(backend.browser, 'audit').mockRejectedValue(new Error('audit down'));
    render(<AuditPanel />);
    expect(await screen.findByText(/审计读取失败/)).toBeInTheDocument();
  });
});
