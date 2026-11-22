import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { backend } from '../../services/backend';
import { PermissionsPanel } from './PermissionsPanel';
import type { BrowserPermission } from '../../types';

function makePermission(overrides: Partial<BrowserPermission> = {}): BrowserPermission {
  return {
    permission_id: 'perm_1',
    scope: 'domain',
    domain: '127.0.0.1',
    action_kind: null,
    risk_ceiling: 'L3',
    profile_id: null,
    expires_at: '2026-11-19T15:37:36.312496Z',
    state: 'active',
    ...overrides,
  };
}

describe('PermissionsPanel', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('lists granted domains and grants a new one through the real request shape', async () => {
    const grantSpy = vi
      .spyOn(backend.browser, 'grantPermission')
      .mockResolvedValue({ permission: makePermission({ domain: 'example.com' }) });
    vi.spyOn(backend.browser, 'listPermissions')
      .mockResolvedValueOnce({ permissions: [makePermission()] })
      .mockResolvedValueOnce({
        permissions: [makePermission(), makePermission({ permission_id: 'perm_2', domain: 'example.com' })],
      });
    render(<PermissionsPanel />);
    await waitFor(() => expect(screen.getByText('127.0.0.1')).toBeTruthy());

    fireEvent.change(screen.getByLabelText('域名'), { target: { value: 'example.com' } });
    fireEvent.change(screen.getByLabelText('备注'), { target: { value: 'e2e fixture' } });
    fireEvent.click(screen.getByText('授权'));
    await waitFor(() =>
      expect(grantSpy).toHaveBeenCalledWith({ domain: 'example.com', risk_ceiling: 'L3', note: 'e2e fixture' }),
    );
    await waitFor(() => expect(screen.getByText('example.com')).toBeTruthy());
  });

  it('delete requires a second confirmation (irreversible op)', async () => {
    const revokeSpy = vi
      .spyOn(backend.browser, 'revokePermission')
      .mockResolvedValue(undefined);
    vi.spyOn(backend.browser, 'listPermissions').mockResolvedValue({ permissions: [makePermission()] });
    render(<PermissionsPanel />);
    await waitFor(() => expect(screen.getByText('删除')).toBeTruthy());

    // 第一步：点击删除只出现确认条，不调端点
    fireEvent.click(screen.getByText('删除'));
    expect(revokeSpy).not.toHaveBeenCalled();
    expect(screen.getByText(/撤销 127\.0\.0\.1 的授权/)).toBeTruthy();

    // 第二步：确认后才调用 DELETE
    fireEvent.click(screen.getByText('确认撤销'));
    await waitFor(() => expect(revokeSpy).toHaveBeenCalledWith('perm_1'));
  });
});
