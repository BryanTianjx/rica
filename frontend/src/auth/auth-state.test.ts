/**
 * SEC-03 pure state-machine tests (mirror of ica-application::auth_recovery).
 * Tags map to the remediation plan §5.7 matrix.
 */
import { describe, expect, it } from 'vitest';
import type { User } from '../types';
import {
  AUTH_PHASES,
  initialAuthState,
  legacyStatus,
  reduceAuth,
  type AuthEvent,
  type AuthState,
} from './auth-state';

const user: User = {
  id: '00000000-0000-0000-0000-000000000001',
  email: '',
  display_name: 'Local User',
  role: 'admin',
  tenant_id: '00000000-0000-0000-0000-000000000000',
  is_active: true,
};

function apply(events: AuthEvent[]): AuthState {
  return events.reduce(reduceAuth, initialAuthState());
}

describe('auth-state — the six canonical phases', () => {
  it('declares exactly the six audited phases', () => {
    expect([...AUTH_PHASES]).toEqual([
      'Unauthenticated',
      'Authenticating',
      'Authenticated',
      'Refreshing',
      'SessionExpired',
      'AuthenticationError',
    ]);
  });

  // SEC03-T01
  it('SEC03-T01 login reaches Authenticated and maps to the legacy status', () => {
    const state = apply([
      { type: 'LOGIN_START' },
      { type: 'LOGIN_SUCCEEDED', user },
    ]);
    expect(state.phase).toBe('Authenticated');
    expect(state.user).toBe(user);
    expect(legacyStatus(state.phase)).toBe('authenticated');
  });

  // SEC03-T02
  it('SEC03-T02 an unexpired access token restores without refresh', () => {
    const state = apply([{ type: 'RESTORE_START' }, { type: 'PROBE_SUCCEEDED', user }]);
    expect(state.phase).toBe('Authenticated');
    expect(state.retried).toBe(false);
  });

  // SEC03-T03
  it('SEC03-T03 an expired access token refreshes then retries once', () => {
    const state = apply([
      { type: 'RESTORE_START' },
      { type: 'PROBE_UNAUTHORIZED' },
      { type: 'REFRESH_SUCCEEDED' },
      { type: 'PROBE_SUCCEEDED', user },
    ]);
    expect(state.phase).toBe('Authenticated');
    // The guard is per attempt: a later expiry may refresh again.
    expect(state.retried).toBe(false);
  });

  // SEC03-T04
  it('SEC03-T04 a restart is a fresh reducer over persisted tokens', () => {
    const before = apply([
      { type: 'LOGIN_START' },
      { type: 'LOGIN_SUCCEEDED', user },
    ]);
    const afterRestart = apply([{ type: 'RESTORE_START' }, { type: 'PROBE_SUCCEEDED', user }]);
    expect(afterRestart.phase).toBe(before.phase);
  });

  // SEC03-T05 / T06
  it('SEC03-T05/T06 expired or revoked refresh tokens end the session', () => {
    for (const reason of ['expired', 'revoked'] as const) {
      const viaRefresh = apply([
        { type: 'RESTORE_START' },
        { type: 'PROBE_UNAUTHORIZED' },
        { type: 'REFRESH_FAILED', reason },
      ]);
      expect(viaRefresh.phase).toBe('SessionExpired');
      expect(viaRefresh.reason).toBe(reason);

      const outOfBand = apply([
        { type: 'RESTORE_START' },
        { type: 'PROBE_SUCCEEDED', user },
        { type: 'SESSION_EXPIRED', reason },
      ]);
      expect(outOfBand.phase).toBe('SessionExpired');
    }
  });

  // SEC03-T07
  it('SEC03-T07 duplicate 401s never start a second refresh', () => {
    const refreshing = apply([{ type: 'RESTORE_START' }, { type: 'PROBE_UNAUTHORIZED' }]);
    expect(refreshing.phase).toBe('Refreshing');
    expect(reduceAuth(refreshing, { type: 'PROBE_UNAUTHORIZED' })).toBe(refreshing);

    const retried = reduceAuth(refreshing, { type: 'REFRESH_SUCCEEDED' });
    expect(retried.phase).toBe('Authenticating');
    const ended = reduceAuth(retried, { type: 'PROBE_UNAUTHORIZED' });
    expect(ended.phase).toBe('SessionExpired');
    expect(ended.reason).toBe('invalid_token');
  });

  // SEC03-T08
  it('SEC03-T08 refresh 500 ends the session without another retry', () => {
    const ended = apply([
      { type: 'RESTORE_START' },
      { type: 'PROBE_UNAUTHORIZED' },
      { type: 'REFRESH_FAILED', reason: 'refresh_failed' },
    ]);
    expect(ended.phase).toBe('SessionExpired');
    expect(reduceAuth(ended, { type: 'REFRESH_SUCCEEDED' })).toBe(ended);
  });

  // SEC03-T09
  it('SEC03-T09 sign-out clears user, guard and diagnostics from any phase', () => {
    for (const event of [
      { type: 'RESTORE_START' } as const,
      { type: 'PROBE_UNAUTHORIZED' } as const,
      { type: 'REFRESH_SUCCEEDED' } as const,
      { type: 'PROBE_SUCCEEDED', user } as const,
    ]) {
      const signedOut = reduceAuth(apply([{ type: 'LOGIN_START' }, event]), {
        type: 'SIGN_OUT',
      });
      expect(signedOut).toEqual(initialAuthState());
      expect(legacyStatus(signedOut.phase)).toBe('unauthenticated');
    }
  });

  // SEC03-T11
  it('SEC03-T11 a stale token after the retry is never accepted', () => {
    const state = apply([
      { type: 'RESTORE_START' },
      { type: 'PROBE_UNAUTHORIZED' },
      { type: 'REFRESH_SUCCEEDED' },
      { type: 'PROBE_UNAUTHORIZED' },
    ]);
    expect(state.phase).toBe('SessionExpired');
    expect(state.reason).toBe('invalid_token');
  });

  it('a non-401 probe failure is an error state with a retry path', () => {
    const failed = apply([{ type: 'RESTORE_START' }, { type: 'PROBE_FAILED', message: 'offline' }]);
    expect(failed.phase).toBe('AuthenticationError');
    expect(failed.message).toBe('offline');
    const dismissed = reduceAuth(failed, { type: 'DISMISS_ERROR' });
    expect(dismissed.phase).toBe('Unauthenticated');
    expect(reduceAuth(dismissed, { type: 'LOGIN_START' }).phase).toBe('Authenticating');
  });
});
