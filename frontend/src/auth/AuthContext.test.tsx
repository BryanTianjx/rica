/**
 * SEC-03 AuthContext + desktop lifecycle tests.
 *
 * The "restart" cases unmount and remount the provider over the same
 * localStorage (the webview reload) and a simulated backend whose token pair
 * survives the process boundary — the desktop login → close → restart →
 * recovery → expiry → refresh failure → logout sequence from plan §5.8.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { AuthProvider, useAuth } from './AuthContext';
import { ApiError, refreshSession } from '../services/api';
import { backend } from '../services/backend';
import { getAccessToken, getRefreshToken, notifySessionExpired, setTokens } from './tokenStore';
import type { User } from '../types';

vi.mock('../services/api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/api')>();
  return { ...actual, refreshSession: vi.fn() };
});

const refreshMock = vi.mocked(refreshSession);

const localUser: User = {
  id: '00000000-0000-0000-0000-000000000001',
  email: '',
  display_name: 'Local User',
  role: 'admin',
  tenant_id: '00000000-0000-0000-0000-000000000000',
  is_active: true,
};

function unauthorized(): ApiError {
  return new ApiError(
    401,
    { error: { code: 'unauthorized', message: 'Unauthorized', details: {} } },
    'Unauthorized',
  );
}

interface Pair {
  access_token: string;
  refresh_token: string;
  user: User;
  token_type: string;
  expires_in: number;
}

/** A tiny in-memory auth service: tokens live in localStorage, so a provider
 * remount sees them exactly as a restarted Tauri webview would. */
function fakeServer(initialKey = 'key-A') {
  let key = initialKey;
  let sequence = 0;
  let invalidAccess = false;
  let rejectRefresh = false;
  let staleAfterRefresh = false;
  const revoked = new Set<string>();

  function issue(): Pair {
    sequence += 1;
    return {
      access_token: `${key}:access-${sequence}`,
      refresh_token: `${key}:refresh-${sequence}`,
      user: localUser,
      token_type: 'bearer',
      expires_in: 900,
    };
  }

  return {
    async login(): Promise<Pair> {
      return issue();
    },
    async me(): Promise<User> {
      const access = getAccessToken();
      if (
        invalidAccess ||
        staleAfterRefresh ||
        !access ||
        !access.startsWith(`${key}:`) ||
        revoked.has(access)
      ) {
        throw unauthorized();
      }
      return localUser;
    },
    async refresh(): Promise<Pair> {
      if (rejectRefresh) throw new Error('refresh rejected');
      const current = getRefreshToken();
      if (!current || !current.startsWith(`${key}:`) || revoked.has(current)) {
        throw new Error('refresh rejected');
      }
      revoked.add(current);
      invalidAccess = false;
      return issue();
    },
    async logout(token: string): Promise<void> {
      revoked.add(token);
    },
    rotateKey(next: string) {
      key = next;
    },
    expireAccess() {
      invalidAccess = true;
    },
    keepStaleAfterRefresh() {
      staleAfterRefresh = true;
    },
    rejectRefresh() {
      rejectRefresh = true;
    },
  };
}

let server = fakeServer();

function wireServer() {
  vi.spyOn(backend.auth, 'me').mockImplementation(() => server.me());
  vi.spyOn(backend.auth, 'login').mockImplementation(() => server.login());
  vi.spyOn(backend.auth, 'logout').mockImplementation((token) => server.logout(token));
  refreshMock.mockReset();
  refreshMock.mockImplementation(async () => {
    try {
      const pair = await server.refresh();
      setTokens(pair.access_token, pair.refresh_token);
      return { ok: true as const, accessToken: pair.access_token };
    } catch {
      return { ok: false as const, reason: 'rejected' as const };
    }
  });
}

function Probe() {
  const { status, phase, user, reason } = useAuth();
  return (
    <div>
      <span data-testid="status">{status}</span>
      <span data-testid="phase">{phase}</span>
      <span data-testid="reason">{reason ?? 'none'}</span>
      <span data-testid="user">{user?.email ?? 'none'}</span>
    </div>
  );
}

function Controls() {
  const { login, logout } = useAuth();
  return (
    <div>
      <button onClick={() => void login('user@example.com', 'password').catch(() => {})}>
        login
      </button>
      <button onClick={() => void logout()}>logout</button>
    </div>
  );
}

function renderApp() {
  return render(
    <AuthProvider>
      <Probe />
      <Controls />
    </AuthProvider>,
  );
}

async function expectPhase(phase: string) {
  await waitFor(() => expect(screen.getByTestId('phase').textContent).toBe(phase));
}

describe('AuthContext', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
    server = fakeServer();
    wireServer();
  });

  it('probes /auth/me and authenticates on 200 (local mode)', async () => {
    vi.spyOn(backend.auth, 'me').mockResolvedValue(localUser);
    renderApp();
    await expectPhase('Authenticated');
    expect(screen.getByTestId('status').textContent).toBe('authenticated');
    expect(screen.getByTestId('user').textContent).toBe('');
  });

  it('a 401 without a refresh token ends as unauthenticated (no refresh loop)', async () => {
    renderApp();
    await expectPhase('SessionExpired');
    expect(screen.getByTestId('status').textContent).toBe('unauthenticated');
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it('kicks to unauthenticated when the API layer reports the session expired (F2-1)', async () => {
    vi.spyOn(backend.auth, 'me').mockResolvedValue(localUser);
    renderApp();
    await expectPhase('Authenticated');
    notifySessionExpired();
    await expectPhase('SessionExpired');
    expect(screen.getByTestId('status').textContent).toBe('unauthenticated');
    expect(getAccessToken()).toBeNull();
  });

  it('stops reacting to expired notifications after unmount', async () => {
    vi.spyOn(backend.auth, 'me').mockResolvedValue(localUser);
    const { unmount } = renderApp();
    await expectPhase('Authenticated');
    unmount();
    expect(() => notifySessionExpired()).not.toThrow();
  });

  it('SEC03-T01 login moves to Authenticated and stores the pair', async () => {
    renderApp();
    await expectPhase('SessionExpired');
    fireEvent.click(screen.getByText('login'));
    await expectPhase('Authenticated');
    expect(getAccessToken()).toBe('key-A:access-1');
    expect(getRefreshToken()).toBe('key-A:refresh-1');
  });

  it('SEC03-T04 restart (unmount/remount) restores the session from persisted tokens', async () => {
    const first = renderApp();
    await expectPhase('SessionExpired');
    fireEvent.click(screen.getByText('login'));
    await expectPhase('Authenticated');
    first.unmount();

    // WebView reload: localStorage survives, new provider, same server key.
    renderApp();
    await expectPhase('Authenticated');
    expect(vi.mocked(backend.auth.me)).toHaveBeenCalledTimes(2);
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it('SEC03-T03 access-token 401 triggers refresh then a successful retry', async () => {
    const first = renderApp();
    await expectPhase('SessionExpired');
    fireEvent.click(screen.getByText('login'));
    await expectPhase('Authenticated');
    first.unmount();
    server.expireAccess();

    // Simulate a desktop restart with the same persisted JWT key: the probe
    // 401s, the refresh token is still live, so one rotation restores it.
    const view = renderApp();
    await expectPhase('Authenticated');
    expect(refreshMock).toHaveBeenCalledTimes(1);
    expect(getAccessToken()).not.toBe('key-A:access-1');
    view.unmount();
  });

  it('SEC03-T05/T06 a rejected refresh clears the session', async () => {
    const first = renderApp();
    await expectPhase('SessionExpired');
    fireEvent.click(screen.getByText('login'));
    await expectPhase('Authenticated');
    first.unmount();
    server.expireAccess();
    server.rejectRefresh();

    renderApp();
    await expectPhase('SessionExpired');
    expect(screen.getByTestId('reason').textContent).toBe('expired');
    expect(getAccessToken()).toBeNull();
    expect(getRefreshToken()).toBeNull();
  });

  it('SEC03-T07 a second 401 after the retry ends the session without re-refreshing', async () => {
    const first = renderApp();
    await expectPhase('SessionExpired');
    fireEvent.click(screen.getByText('login'));
    await expectPhase('Authenticated');
    first.unmount();
    server.expireAccess();
    server.keepStaleAfterRefresh();

    renderApp();
    await expectPhase('SessionExpired');
    expect(screen.getByTestId('reason').textContent).toBe('invalid_token');
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it('SEC03-T10 repeated restarts keep the session state consistent', async () => {
    const first = renderApp();
    await expectPhase('SessionExpired');
    fireEvent.click(screen.getByText('login'));
    await expectPhase('Authenticated');
    first.unmount();
    for (let restart = 0; restart < 3; restart += 1) {
      const view = renderApp();
      await expectPhase('Authenticated');
      view.unmount();
    }
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it('SEC03-T11 a changed signing key cannot recover with the old pair', async () => {
    const first = renderApp();
    await expectPhase('SessionExpired');
    fireEvent.click(screen.getByText('login'));
    await expectPhase('Authenticated');
    first.unmount();

    // P1-01 persists the key, so this only happens on an explicit rotate() or
    // a corrupted slot: both tokens were signed under the previous key.
    server.rotateKey('key-B');
    const view = renderApp();
    await expectPhase('SessionExpired');
    expect(screen.getByTestId('status').textContent).toBe('unauthenticated');
    view.unmount();
  });

  it('SEC03-T09 intentional logout revokes the token and clears local state', async () => {
    renderApp();
    await expectPhase('SessionExpired');
    fireEvent.click(screen.getByText('login'));
    await expectPhase('Authenticated');
    fireEvent.click(screen.getByText('logout'));
    await expectPhase('Unauthenticated');
    expect(getAccessToken()).toBeNull();
    expect(getRefreshToken()).toBeNull();
  });
});
