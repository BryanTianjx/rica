/**
 * SEC-03 — the one authentication state machine (remediation plan §5.2).
 *
 * Six phases, one pure reducer. The canonical Rust definition lives in
 * `crates/ica-application/src/auth_recovery.rs` and is re-exported by
 * `ica-contracts`; this module is its webview mirror (a source assertion in
 * `ica-contracts/src/auth_session.rs` pins the two name lists together).
 *
 * Nothing else in the UI may implement "what happens on a 401": pages consume
 * `useAuth()` and the provider owns every transition, including the single
 * `refresh → retry` recovery cycle. Keeping the reducer pure makes the full
 * desktop lifecycle testable without a Tauri runtime.
 */

import type { User } from '../types';

export const AUTH_PHASES = [
  'Unauthenticated',
  'Authenticating',
  'Authenticated',
  'Refreshing',
  'SessionExpired',
  'AuthenticationError',
] as const;

export type AuthPhase = (typeof AUTH_PHASES)[number];

export type SessionExpiredReason =
  | 'refresh_failed'
  | 'refresh_rejected'
  | 'expired'
  | 'revoked'
  | 'invalid_token';

export interface AuthState {
  phase: AuthPhase;
  user: User | null;
  /** Set once a refresh+retry cycle has been consumed for this attempt. */
  retried: boolean;
  reason: SessionExpiredReason | null;
  message: string | null;
}

export type AuthEvent =
  | { type: 'RESTORE_START' }
  | { type: 'LOGIN_START' }
  | { type: 'PROBE_SUCCEEDED'; user: User }
  | { type: 'PROBE_UNAUTHORIZED' }
  | { type: 'PROBE_FAILED'; message: string }
  | { type: 'REFRESH_SUCCEEDED' }
  | { type: 'REFRESH_FAILED'; reason: SessionExpiredReason }
  | { type: 'SESSION_EXPIRED'; reason: SessionExpiredReason }
  | { type: 'LOGIN_SUCCEEDED'; user: User }
  | { type: 'LOGIN_FAILED'; message: string }
  | { type: 'SIGN_OUT' }
  | { type: 'DISMISS_ERROR' };

export function initialAuthState(): AuthState {
  return {
    phase: 'Unauthenticated',
    user: null,
    retried: false,
    reason: null,
    message: null,
  };
}

function fresh(phase: AuthPhase): AuthState {
  return { ...initialAuthState(), phase };
}

function ended(reason: SessionExpiredReason): AuthState {
  return { ...initialAuthState(), phase: 'SessionExpired', reason };
}

function errored(message: string): AuthState {
  return { ...initialAuthState(), phase: 'AuthenticationError', message };
}

/**
 * The single transition function. Illegal/duplicate events are no-ops — that
 * is the state-level half of the refresh single-flight guarantee: a second
 * 401 while `Refreshing`, or after the one allowed retry, can never schedule
 * another refresh.
 */
export function reduceAuth(state: AuthState, event: AuthEvent): AuthState {
  switch (event.type) {
    case 'SIGN_OUT':
      return fresh('Unauthenticated');

    case 'RESTORE_START':
    case 'LOGIN_START':
      if (
        state.phase === 'Unauthenticated' ||
        state.phase === 'SessionExpired' ||
        state.phase === 'AuthenticationError'
      ) {
        return fresh('Authenticating');
      }
      return state;

    case 'PROBE_SUCCEEDED':
    case 'LOGIN_SUCCEEDED':
      if (state.phase === 'Authenticating') {
        return { ...fresh('Authenticated'), user: event.user };
      }
      return state;

    case 'PROBE_UNAUTHORIZED':
      if (state.phase !== 'Authenticating') return state;
      if (state.retried) return ended('invalid_token');
      return { ...state, phase: 'Refreshing' };

    case 'PROBE_FAILED':
      if (state.phase === 'Authenticating' || state.phase === 'Refreshing') {
        return errored(event.message);
      }
      return state;

    case 'REFRESH_SUCCEEDED':
      if (state.phase === 'Refreshing') {
        return { ...fresh('Authenticating'), retried: true, user: state.user };
      }
      return state;

    case 'REFRESH_FAILED':
      if (state.phase === 'Refreshing') return ended(event.reason);
      return state;

    case 'SESSION_EXPIRED':
      if (
        state.phase === 'Authenticated' ||
        state.phase === 'Authenticating' ||
        state.phase === 'Refreshing' ||
        state.phase === 'SessionExpired'
      ) {
        return ended(event.reason);
      }
      return state;

    case 'LOGIN_FAILED':
      if (state.phase === 'Authenticating') return errored(event.message);
      return state;

    case 'DISMISS_ERROR':
      if (state.phase === 'AuthenticationError') return fresh('Unauthenticated');
      return state;

    default:
      return state;
  }
}

/**
 * Backward-compatible view for shells that only know "checking / in / out".
 * New code should branch on `phase` instead.
 */
export function legacyStatus(
  phase: AuthPhase,
): 'checking' | 'authenticated' | 'unauthenticated' {
  if (phase === 'Authenticating' || phase === 'Refreshing') return 'checking';
  if (phase === 'Authenticated') return 'authenticated';
  return 'unauthenticated';
}
