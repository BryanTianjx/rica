/**
 * API client for the ICA backend.
 *
 * The backend serves one JSON error envelope for every failure:
 *   { error: { code, message, details } }
 * `request()` unwraps that into a typed `ApiError` so callers never have to
 * dig through response shapes.
 *
 * V2.0 阶段 B: every request carries the stored bearer token; a 401 on a
 * business endpoint triggers one silent refresh (rotate the refresh token)
 * and retries the original request. Auth endpoints (/auth/*) never refresh,
 * so a failed login or a missing token surfaces as a clean 401 instead of a
 * refresh loop.
 */

import { clearTokens, getAccessToken, getRefreshToken, notifySessionExpired, setTokens } from '../auth/tokenStore';

const BASE_URL = import.meta.env.VITE_API_BASE_URL ?? '/api';

export interface ApiErrorBody {
  error: {
    code: string;
    message: string;
    details: Record<string, unknown>;
  };
}

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: Record<string, unknown>;

  constructor(status: number, body: ApiErrorBody | null, fallback: string) {
    super(body?.error?.message ?? fallback);
    this.name = 'ApiError';
    this.status = status;
    this.code = body?.error?.code ?? 'unknown_error';
    this.details = body?.error?.details ?? {};
  }
}

/**
 * The one error-rendering path (F0-4): `code: message` for an ApiError, the
 * plain text (or `fallback`) otherwise. Pages use this instead of reading
 * `.message` directly so the error code is never silently dropped.
 */
export function describeApiError(caught: unknown, fallback: string): string {
  if (caught instanceof ApiError) return `${caught.code}: ${caught.message}`;
  return caught instanceof Error && caught.message ? caught.message : fallback;
}

interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  body?: unknown;
  signal?: AbortSignal;
}

/**
 * SEC-03 single-flight refresh: every concurrent 401 in the webview shares
 * one rotation, so a rotated refresh token is presented exactly once.
 * `rotate()` talks to the endpoint with raw `fetch` — it never goes through
 * `request()` — so a refresh response can never recursively trigger a refresh.
 */
export type RefreshOutcome =
  | { ok: true; accessToken: string }
  | { ok: false; reason: 'no_refresh_token' | 'rejected' | 'failed' };

let refreshInFlight: Promise<RefreshOutcome> | null = null;

async function rotate(): Promise<RefreshOutcome> {
  const refreshToken = getRefreshToken();
  if (!refreshToken) return { ok: false, reason: 'no_refresh_token' };
  try {
    const response = await fetch(`${BASE_URL}/auth/refresh`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    if (!response.ok) return { ok: false, reason: 'rejected' };
    const data = (await response.json()) as { access_token: string; refresh_token: string };
    setTokens(data.access_token, data.refresh_token);
    return { ok: true, accessToken: data.access_token };
  } catch {
    return { ok: false, reason: 'failed' };
  }
}

export function refreshSession(): Promise<RefreshOutcome> {
  if (!refreshInFlight) {
    refreshInFlight = rotate().finally(() => {
      refreshInFlight = null;
    });
  }
  return refreshInFlight;
}

function isAuthPath(path: string): boolean {
  return path.startsWith('/auth/');
}

async function request<T>(
  path: string,
  options: RequestOptions = {},
  retried = false,
): Promise<T> {
  const { method = 'GET', body, signal } = options;

  const headers = new Headers();
  if (body !== undefined) {
    headers.set('Content-Type', 'application/json');
  }
  const accessToken = getAccessToken();
  if (accessToken) {
    headers.set('Authorization', `Bearer ${accessToken}`);
  }

  const response = await fetch(`${BASE_URL}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    signal,
  });

  if (response.status === 401 && !retried && !isAuthPath(path)) {
    // Access token expired (or missing): rotate once (deduplicated), then replay.
    const outcome = await refreshSession();
    if (outcome.ok) {
      return request<T>(path, options, true);
    }
    // Refresh failed: the session is gone — drop the tokens and tell the
    // auth layer so the UI leaves the authenticated state (F2-1: previously
    // the tokens were cleared in secret and every later request just 401'd).
    notifySessionExpired();
    clearTokens();
  }

  if (response.status === 204) {
    return undefined as T;
  }

  const text = await response.text();
  let parsed: unknown = null;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = null;
    }
  }

  if (!response.ok) {
    throw new ApiError(
      response.status,
      parsed as ApiErrorBody | null,
      `Request to ${path} failed with HTTP ${response.status}`,
    );
  }

  return parsed as T;
}

/** Build a query string, skipping empty and null values. */
export function query(params: Record<string, string | number | boolean | undefined | null>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === '') continue;
    search.set(key, String(value));
  }
  const encoded = search.toString();
  return encoded ? `?${encoded}` : '';
}

export const api = {
  get: <T>(path: string, signal?: AbortSignal) => request<T>(path, { signal }),
  post: <T>(path: string, body?: unknown) => request<T>(path, { method: 'POST', body }),
  put: <T>(path: string, body?: unknown) => request<T>(path, { method: 'PUT', body }),
  patch: <T>(path: string, body?: unknown) => request<T>(path, { method: 'PATCH', body }),
  delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
};
