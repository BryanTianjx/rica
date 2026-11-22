/**
 * SEC-03 — the single owner of authentication state in the webview.
 *
 * Startup recovery is centralised here: probe `/auth/me`, and on a 401 use the
 * one deduplicated refresh in `api.ts`, then retry the probe exactly once.
 * A second 401, a rejected refresh or a 5xx refresh ends the session; it never
 * loops. The six-phase reducer itself is `auth/auth-state.ts` (mirroring
 * `ica-application::auth_recovery`), so no page implements its own auth logic.
 *
 * P1-01 made the JWT signing key persistent, so a desktop restart keeps the
 * key and a valid access token verifies again; the recovery chain below still
 * handles expired tokens, key rotation and revoked refresh tokens.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  type ReactNode,
} from 'react';
import { backend } from '../services/backend';
import { ApiError, refreshSession, type RefreshOutcome } from '../services/api';
import { clearTokens, getRefreshToken, onSessionExpired, setTokens } from './tokenStore';
import {
  initialAuthState,
  legacyStatus,
  reduceAuth,
  type AuthPhase,
  type SessionExpiredReason,
} from './auth-state';
import type { User } from '../types';

/** Kept for shells that still branch on the coarse three-way status. */
export type AuthStatus = 'checking' | 'authenticated' | 'unauthenticated';

interface AuthContextValue {
  status: AuthStatus;
  phase: AuthPhase;
  user: User | null;
  reason: SessionExpiredReason | null;
  message: string | null;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

function isUnauthorized(caught: unknown): boolean {
  if (caught instanceof ApiError) return caught.status === 401;
  // Tolerate plain transport errors whose message carries the HTTP code so a
  // mocked/legacy client still drives the same state machine.
  return caught instanceof Error && /\b401\b|unauthorized/i.test(caught.message);
}

function errorMessage(caught: unknown, fallback: string): string {
  return caught instanceof Error && caught.message ? caught.message : fallback;
}

function reasonForRefreshFailure(
  reason: Extract<RefreshOutcome, { ok: false }>['reason'],
): SessionExpiredReason {
  switch (reason) {
    case 'no_refresh_token':
    case 'rejected':
      return 'expired';
    case 'failed':
    default:
      return 'refresh_failed';
  }
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reduceAuth, undefined, initialAuthState);

  // F2-1: the API layer reports a dead session after a failed silent refresh;
  // leave the authenticated UI immediately and drop the unusable tokens.
  useEffect(
    () =>
      onSessionExpired(() => {
        clearTokens();
        dispatch({ type: 'SESSION_EXPIRED', reason: 'refresh_failed' });
      }),
    [],
  );

  useEffect(() => {
    let active = true;
    const probe = () => backend.auth.me();

    async function restore() {
      dispatch({ type: 'RESTORE_START' });
      try {
        const user = await probe();
        if (active) dispatch({ type: 'PROBE_SUCCEEDED', user });
        return;
      } catch (error) {
        if (!active) return;
        if (!isUnauthorized(error)) {
          dispatch({ type: 'PROBE_FAILED', message: errorMessage(error, 'Session check failed') });
          return;
        }
      }

      // /auth/me answered 401: one refresh, then one retry. Entering
      // `Refreshing` here makes duplicate 401s harmless no-ops.
      if (!getRefreshToken()) {
        clearTokens();
        dispatch({ type: 'SESSION_EXPIRED', reason: 'expired' });
        return;
      }
      dispatch({ type: 'PROBE_UNAUTHORIZED' });
      const outcome = await refreshSession();
      if (!active) return;
      if (!outcome.ok) {
        clearTokens();
        dispatch({ type: 'REFRESH_FAILED', reason: reasonForRefreshFailure(outcome.reason) });
        return;
      }
      dispatch({ type: 'REFRESH_SUCCEEDED' });
      try {
        const user = await probe();
        if (active) dispatch({ type: 'PROBE_SUCCEEDED', user });
      } catch (retryError) {
        if (!active) return;
        if (isUnauthorized(retryError)) {
          // The reducer has `retried` armed here, so this ends the session
          // instead of starting another refresh.
          clearTokens();
          dispatch({ type: 'PROBE_UNAUTHORIZED' });
        } else {
          dispatch({
            type: 'PROBE_FAILED',
            message: errorMessage(retryError, 'Session check failed'),
          });
        }
      }
    }

    void restore();
    return () => {
      active = false;
    };
  }, []);

  const login = useCallback(async (email: string, password: string) => {
    dispatch({ type: 'LOGIN_START' });
    try {
      const pair = await backend.auth.login({ email, password });
      setTokens(pair.access_token, pair.refresh_token);
      dispatch({ type: 'LOGIN_SUCCEEDED', user: pair.user });
    } catch (error) {
      dispatch({
        type: 'LOGIN_FAILED',
        message: errorMessage(error, 'Sign in failed'),
      });
      throw error;
    }
  }, []);

  const logout = useCallback(async () => {
    const refreshToken = getRefreshToken();
    if (refreshToken) {
      try {
        await backend.auth.logout(refreshToken);
      } catch {
        // Best-effort: the local session is cleared regardless.
      }
    }
    clearTokens();
    dispatch({ type: 'SIGN_OUT' });
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      status: legacyStatus(state.phase),
      phase: state.phase,
      user: state.user,
      reason: state.reason,
      message: state.message,
      login,
      logout,
    }),
    [state, login, logout],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}
