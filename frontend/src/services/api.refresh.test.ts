/**
 * SEC03-T07/T08 — the real API client's refresh concurrency behaviour.
 * `vi.resetModules()` gives each test its own single-flight coordinator.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

function jsonResponse(status: number, body: unknown): Response {
  const text = JSON.stringify(body);
  return {
    status,
    ok: status >= 200 && status < 300,
    text: async () => text,
    json: async () => body,
  } as unknown as Response;
}

async function loadClient(handler: (url: string, init?: RequestInit) => Promise<Response>) {
  vi.resetModules();
  const fetchMock = vi.fn(handler);
  vi.stubGlobal('fetch', fetchMock);
  const apiModule = await import('./api');
  const store = await import('../auth/tokenStore');
  return { api: apiModule.api, store, fetchMock };
}

function bearer(init?: RequestInit): string | null {
  return (init?.headers as Headers | undefined)?.get?.('Authorization') ?? null;
}

describe('api client — SEC-03 refresh single-flight', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  it('SEC03-T07 concurrent 401s share exactly one rotation and both requests succeed', async () => {
    const gate: { release: (() => void) | null } = { release: null };
    const barrier = new Promise<void>((resolve) => {
      gate.release = resolve;
    });
    const { api, store, fetchMock } = await loadClient(async (url, init) => {
      if (url.includes('/auth/refresh')) {
        await barrier;
        return jsonResponse(200, { access_token: 'new-access', refresh_token: 'new-refresh' });
      }
      if (bearer(init) === 'Bearer new-access') return jsonResponse(200, { items: [] });
      return jsonResponse(401, {
        error: { code: 'unauthorized', message: 'Unauthorized', details: {} },
      });
    });
    store.setTokens('old-access', 'old-refresh');

    const first = api.get<{ items: unknown[] }>('/positions');
    const second = api.get<{ items: unknown[] }>('/shares');
    // Both 401 handlers must be parked on the same in-flight rotation.
    await vi.waitFor(() => expect(fetchMock.mock.calls.filter((c) => c[0].includes('/auth/refresh'))).toHaveLength(1));
    gate.release?.();

    expect(await first).toEqual({ items: [] });
    expect(await second).toEqual({ items: [] });
    expect(fetchMock.mock.calls.filter((c) => c[0].includes('/auth/refresh'))).toHaveLength(1);
    expect(store.getAccessToken()).toBe('new-access');
    expect(store.getRefreshToken()).toBe('new-refresh');
  });

  it('SEC03-T08 a refresh 500 ends the session without a second attempt', async () => {
    const expired: string[] = [];
    const { api, store, fetchMock } = await loadClient(async (url) => {
      if (url.includes('/auth/refresh')) return jsonResponse(500, {});
      return jsonResponse(401, {
        error: { code: 'unauthorized', message: 'Unauthorized', details: {} },
      });
    });
    store.setTokens('old-access', 'old-refresh');
    store.onSessionExpired(() => expired.push('expired'));

    await expect(api.get('/positions')).rejects.toThrow();
    expect(fetchMock.mock.calls.filter((c) => c[0].includes('/auth/refresh'))).toHaveLength(1);
    expect(store.getAccessToken()).toBeNull();
    expect(store.getRefreshToken()).toBeNull();
    expect(expired).toEqual(['expired']);
  });

  it('an /auth/* 401 is never refreshed (no refresh loop)', async () => {
    const { api, store, fetchMock } = await loadClient(async () =>
      jsonResponse(401, { error: { code: 'unauthorized', message: 'Unauthorized', details: {} } }),
    );
    store.setTokens('old-access', 'old-refresh');

    await expect(api.get('/auth/me')).rejects.toThrow();
    expect(fetchMock.mock.calls.filter((c) => c[0].includes('/auth/refresh'))).toHaveLength(0);
    expect(store.getAccessToken()).toBe('old-access');
  });

  it('a missing refresh token fails closed without calling the endpoint', async () => {
    const expired: string[] = [];
    const { api, store, fetchMock } = await loadClient(async () =>
      jsonResponse(401, { error: { code: 'unauthorized', message: 'Unauthorized', details: {} } }),
    );
    store.onSessionExpired(() => expired.push('expired'));

    await expect(api.get('/positions')).rejects.toThrow();
    expect(fetchMock.mock.calls.filter((c) => c[0].includes('/auth/refresh'))).toHaveLength(0);
    expect(expired).toEqual(['expired']);
  });
});
