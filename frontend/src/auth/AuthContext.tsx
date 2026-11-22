/**
 * V2.0 阶段 B: authentication state.
 *
 * On mount the provider probes `GET /api/auth/me`:
 *  - 200 → a session exists (multiuser with a valid token, or the V1.0
 *    synthetic local user) → the app renders.
 *  - 401 → multiuser and signed out → the login page renders.
 *
 * The probe carries no refresh attempt (auth paths never refresh), so a
 * missing token surfaces as `unauthenticated`, never a refresh loop.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { authApi } from '../services/endpoints';
import { clearTokens, getRefreshToken, onSessionExpired, setTokens } from './tokenStore';
import type { User } from '../types';

export type AuthStatus = 'checking' | 'authenticated' | 'unauthenticated';

interface AuthContextValue {
  status: AuthStatus;
  user: User | null;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AuthStatus>('checking');
  const [user, setUser] = useState<User | null>(null);

  // F2-1: the API layer drops the session when a silent refresh fails
  // (refresh token revoked/expired). Without this subscription the user saw
  // the authenticated UI while every request kept failing with 401.
  useEffect(
    () =>
      onSessionExpired(() => {
        setUser(null);
        setStatus('unauthenticated');
      }),
    [],
  );

  useEffect(() => {
    let active = true;
    authApi
      .me()
      .then((current) => {
        if (!active) return;
        setUser(current);
        setStatus('authenticated');
      })
      .catch(() => {
        if (!active) return;
        setUser(null);
        setStatus('unauthenticated');
      });
    return () => {
      active = false;
    };
  }, []);

  const login = useCallback(async (email: string, password: string) => {
    const pair = await authApi.login({ email, password });
    setTokens(pair.access_token, pair.refresh_token);
    setUser(pair.user);
    setStatus('authenticated');
  }, []);

  const logout = useCallback(async () => {
    const refreshToken = getRefreshToken();
    if (refreshToken) {
      try {
        await authApi.logout(refreshToken);
      } catch {
        // Best-effort: the local session is cleared regardless.
      }
    }
    clearTokens();
    setUser(null);
    setStatus('unauthenticated');
  }, []);

  const value = useMemo(
    () => ({ status, user, login, logout }),
    [status, user, login, logout],
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
