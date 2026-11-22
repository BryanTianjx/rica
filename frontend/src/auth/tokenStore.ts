/**
 * V2.0 阶段 B: token storage.
 *
 * Access + refresh tokens live in localStorage so a page reload keeps the
 * session. The backend shortens the access token's life (default 15 min) and
 * the refresh token (default 7 days) is what the API layer uses to silently
 * re-authenticate on a 401.
 */

const ACCESS_KEY = 'ica.access_token';
const REFRESH_KEY = 'ica.refresh_token';

/** Subscribers notified when the session is irrecoverably gone. */
type SessionExpiredCallback = () => void;

const sessionExpiredSubscribers = new Set<SessionExpiredCallback>();

/**
 * Subscribe to session-expired notifications from the API layer.
 *
 * The API client drops the stored tokens when a silent refresh fails; this
 * lets the auth context react (back to `unauthenticated`) so the UI never
 * keeps rendering the authenticated view once every request would 401 anyway.
 * Returns an unsubscribe function.
 */
export function onSessionExpired(cb: SessionExpiredCallback): () => void {
  sessionExpiredSubscribers.add(cb);
  return () => {
    sessionExpiredSubscribers.delete(cb);
  };
}

/**
 * Fire after the API layer dropped the stored session. Intentionally NOT
 * called from `clearTokens()`: logout is an intentional sign-out and already
 * updates the auth state itself; only the API layer's failed silent refresh
 * escalates to "session expired".
 */
export function notifySessionExpired(): void {
  for (const cb of [...sessionExpiredSubscribers]) {
    try {
      cb();
    } catch {
      // A misbehaving subscriber must not break the API call path.
    }
  }
}

export function getAccessToken(): string | null {
  return localStorage.getItem(ACCESS_KEY);
}

export function getRefreshToken(): string | null {
  return localStorage.getItem(REFRESH_KEY);
}

export function setTokens(accessToken: string, refreshToken: string): void {
  localStorage.setItem(ACCESS_KEY, accessToken);
  localStorage.setItem(REFRESH_KEY, refreshToken);
}

export function clearTokens(): void {
  localStorage.removeItem(ACCESS_KEY);
  localStorage.removeItem(REFRESH_KEY);
}
