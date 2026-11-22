import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { AuthProvider, useAuth } from './AuthContext';
import { authApi } from '../services/endpoints';
import { notifySessionExpired } from './tokenStore';
import type { User } from '../types';

const localUser: User = {
  id: '00000000-0000-0000-0000-000000000001',
  email: '',
  display_name: 'Local User',
  role: 'admin',
  tenant_id: '00000000-0000-0000-0000-000000000000',
  is_active: true,
};

function Probe() {
  const { status, user } = useAuth();
  return (
    <div>
      <span data-testid="status">{status}</span>
      <span data-testid="user">{user?.email ?? 'none'}</span>
    </div>
  );
}

describe('AuthContext', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it('probes /auth/me and authenticates on 200 (local mode)', async () => {
    vi.spyOn(authApi, 'me').mockResolvedValue(localUser);
    render(
      <AuthProvider>
        <Probe />
      </AuthProvider>,
    );
    await waitFor(() => expect(screen.getByTestId('status').textContent).toBe('authenticated'));
    expect(screen.getByTestId('user').textContent).toBe('');
  });

  it('probes /auth/me and marks unauthenticated on 401 (multiuser signed out)', async () => {
    vi.spyOn(authApi, 'me').mockRejectedValue(new Error('Unauthorized'));
    render(
      <AuthProvider>
        <Probe />
      </AuthProvider>,
    );
    await waitFor(() => expect(screen.getByTestId('status').textContent).toBe('unauthenticated'));
    expect(screen.getByTestId('user').textContent).toBe('none');
  });

  it('kicks to unauthenticated when the API layer reports the session expired (F2-1)', async () => {
    vi.spyOn(authApi, 'me').mockResolvedValue(localUser);
    render(
      <AuthProvider>
        <Probe />
      </AuthProvider>,
    );
    await waitFor(() => expect(screen.getByTestId('status').textContent).toBe('authenticated'));

    // The API client drops a dead session (failed silent refresh) and fires
    // the expired notification; the provider must leave the stale
    // authenticated UI instead of letting every request 401 silently.
    notifySessionExpired();
    await waitFor(() => expect(screen.getByTestId('status').textContent).toBe('unauthenticated'));
    expect(screen.getByTestId('user').textContent).toBe('none');
  });

  it('stops reacting to expired notifications after unmount', async () => {
    vi.spyOn(authApi, 'me').mockResolvedValue(localUser);
    const { unmount } = render(
      <AuthProvider>
        <Probe />
      </AuthProvider>,
    );
    await waitFor(() => expect(screen.getByTestId('status').textContent).toBe('authenticated'));
    unmount();
    expect(() => notifySessionExpired()).not.toThrow();
  });
});
