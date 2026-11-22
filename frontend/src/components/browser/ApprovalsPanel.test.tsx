import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { browserApi } from '../../services/endpoints';
import { ApprovalsPanel } from './ApprovalsPanel';
import type { BrowserApproval, BrowserAuditEvent } from '../../types';

function makeApproval(overrides: Partial<BrowserApproval> = {}): BrowserApproval {
  return {
    approval_id: 'apr_1',
    state: 'pending',
    request_type: 'action',
    risk_level: 'L4',
    summary: 'click needs a human approval (L4)',
    action_digest: 'digest-abc',
    action_id: 'act_1',
    task_id: null,
    expires_at: '2026-11-18T16:00:00Z',
    ...overrides,
  };
}

function makeEvent(overrides: Partial<BrowserAuditEvent> = {}): BrowserAuditEvent {
  return {
    event_id: 'ev_1',
    event_type: 'browser.approval.requested',
    entity_type: 'browser_approval',
    entity_id: 'apr_1',
    summary: 'click needs a human approval (L4)',
    metadata: {},
    created_at: '2026-11-18T15:40:00Z',
    ...overrides,
  };
}

describe('ApprovalsPanel', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('derives pending approvals from audit events and can approve / reject', async () => {
    vi.spyOn(browserApi, 'audit').mockResolvedValue({
      events: [makeEvent(), makeEvent({ event_id: 'ev_2', event_type: 'browser.runtime.started', entity_type: 'browser_profile' })],
    });
    const getSpy = vi
      .spyOn(browserApi, 'getApproval')
      .mockResolvedValue({ approval: makeApproval() });
    const decideSpy = vi
      .spyOn(browserApi, 'decideApproval')
      .mockResolvedValue({ approval: makeApproval({ state: 'approved' }) });

    render(<ApprovalsPanel />);
    await waitFor(() => expect(getSpy).toHaveBeenCalledWith('apr_1'));
    await waitFor(() => expect(screen.getByText('批准')).toBeTruthy());

    fireEvent.click(screen.getByText('批准'));
    await waitFor(() =>
      expect(decideSpy).toHaveBeenCalledWith('apr_1', 'approve', undefined),
    );
    expect(screen.getByText(/已批准/)).toBeTruthy();
  });

  it('keeps decided rows read-only', async () => {
    vi.spyOn(browserApi, 'audit').mockResolvedValue({
      events: [
        makeEvent(),
        makeEvent({ event_id: 'ev_3', event_type: 'browser.approval.decided' }),
      ],
    });
    vi.spyOn(browserApi, 'getApproval').mockResolvedValue({
      approval: makeApproval({ state: 'approved' }),
    });
    render(<ApprovalsPanel />);
    await waitFor(() => expect(screen.getByText(/已决（/)).toBeTruthy());
    expect(screen.queryByText('批准')).toBeNull();
  });
});
