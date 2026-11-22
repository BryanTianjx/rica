// ARCH-P5 AC1/AC3/AC4 — frontend-side transport-agnostic evidence.
//
// The SAME business client interface is driven through the HTTP adapter
// (stubbed fetch) and the IPC adapter (stubbed Tauri invoke). Success bodies,
// error code/message/details and the rendered error string must match; only
// the transport-specific HTTP status is allowed to differ (P2 parity form).
// Authorization and transactional audit equivalence are owned by the shared
// Application Core and pinned by P2's adapter_contract_parity.rs — not rebuilt.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import {
  ApiError,
  createBackend,
  describeApiError,
  MIGRATED_DOMAINS,
  UNMIGRATED_DOMAINS,
  resolveBackendConfig,
  activeTransport,
} from './index';
import { httpBackend } from './http';
import type { BackendClient } from './client';

const invokeMock = vi.mocked(invoke);
let fetchMock: ReturnType<typeof vi.fn>;

function jsonResponse(status: number, payload: unknown): Response {
  return {
    status,
    ok: status >= 200 && status < 300,
    text: async () => JSON.stringify(payload),
  } as unknown as Response;
}

type Outcome =
  | { ok: true; value: unknown }
  | { ok: false; error: ApiError };

async function outcome(promise: Promise<unknown>): Promise<Outcome> {
  try {
    return { ok: true, value: await promise };
  } catch (caught) {
    expect(caught).toBeInstanceOf(ApiError);
    return { ok: false, error: caught as ApiError };
  }
}

async function viaHttp<T>(
  call: (client: BackendClient) => Promise<T>,
  status: number,
  payload: unknown,
): Promise<Outcome> {
  fetchMock.mockResolvedValueOnce(jsonResponse(status, payload));
  return outcome(call(createBackend({ transport: 'http', source: 'build-env' })));
}

async function viaIpc<T>(
  call: (client: BackendClient) => Promise<T>,
  payload: unknown,
  reject: boolean,
): Promise<Outcome> {
  if (reject) invokeMock.mockRejectedValueOnce(payload);
  else invokeMock.mockResolvedValueOnce(payload);
  return outcome(call(createBackend({ transport: 'ipc', source: 'build-env' })));
}

function assertSameError(http: Outcome, ipc: Outcome, label: string): void {
  expect(http.ok, `${label}: HTTP must fail`).toBe(false);
  expect(ipc.ok, `${label}: IPC must fail`).toBe(false);
  if (http.ok || ipc.ok) return;
  const shape = (error: ApiError) => ({
    code: error.code,
    message: error.message,
    details: error.details,
    rendered: describeApiError(error, 'fallback'),
  });
  expect(shape(http.error), `${label}: same envelope over both transports`).toEqual(
    shape(ipc.error),
  );
}

const POSITION = {
  id: 'p-1',
  name: 'Backend Engineer',
  location: 'Wuhan',
  status: 'Open',
  archived: false,
  created_at: '2026-11-23T00:00:00Z',
  updated_at: '2026-11-23T00:00:00Z',
};
const PAGE = { items: [POSITION], total: 1, offset: 0, limit: 20 };
const ENVELOPE = (code: string, message: string) => ({ error: { code, message, details: {} } });

beforeEach(() => {
  localStorage.clear();
  delete window.__ICA_BACKEND_CONFIG__;
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  invokeMock.mockReset();
  fetchMock = vi.fn();
  vi.stubGlobal('fetch', fetchMock);
});

describe('startup config decides transport; rollback is config-only', () => {
  it('defaults to HTTP outside a Tauri webview', () => {
    expect(resolveBackendConfig()).toEqual({ transport: 'http', source: 'host-detected' });
  });

  it('honours the build-time env flag', () => {
    vi.stubEnv('VITE_ICA_BACKEND_TRANSPORT', 'ipc');
    expect(resolveBackendConfig()).toEqual({ transport: 'ipc', source: 'build-env' });
  });

  it('localStorage override rolls back to the original HTTP path without rebuild', () => {
    vi.stubEnv('VITE_ICA_BACKEND_TRANSPORT', 'ipc');
    localStorage.setItem('ica.backend.transport', 'http');
    expect(resolveBackendConfig()).toEqual({ transport: 'http', source: 'local-override' });
  });

  it('host-injected runtime config wins and ignores invalid values', () => {
    localStorage.setItem('ica.backend.transport', 'ipc');
    window.__ICA_BACKEND_CONFIG__ = { transport: 'http' };
    expect(resolveBackendConfig()).toEqual({ transport: 'http', source: 'runtime-global' });
    window.__ICA_BACKEND_CONFIG__ = { transport: 'sideways' };
    expect(resolveBackendConfig().transport).toBe('ipc');
  });

  it('moves only migrated domains and keeps every other domain on HTTP', () => {
    const domains = [...MIGRATED_DOMAINS, ...UNMIGRATED_DOMAINS.map((m) => m.domain)].sort();
    expect(domains).toEqual(Object.keys(httpBackend).sort());

    const allHttp = createBackend({ transport: 'http', source: 'build-env' });
    for (const domain of MIGRATED_DOMAINS) expect(allHttp[domain]).toBe(httpBackend[domain]);

    const ipc = createBackend({ transport: 'ipc', source: 'build-env' });
    for (const domain of MIGRATED_DOMAINS) expect(ipc[domain]).not.toBe(httpBackend[domain]);
    for (const entry of UNMIGRATED_DOMAINS) {
      expect(ipc[entry.domain]).toBe(httpBackend[entry.domain]);
      expect(activeTransport(entry.domain, { transport: 'ipc', source: 'build-env' })).toBe('http');
    }
    expect(MIGRATED_DOMAINS).toEqual(['positions']);
  });
});

describe('transport-agnostic parity on the same client interface', () => {
  it('list: identical input → identical Page over HTTP and IPC', async () => {
    const call = (client: BackendClient) => client.positions.list({ limit: 20 });
    const http = await viaHttp(call, 200, PAGE);
    const ipc = await viaIpc(call, PAGE, false);
    expect(http.ok && ipc.ok).toBe(true);
    if (http.ok && ipc.ok) expect(http.value).toEqual(ipc.value);
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/positions?limit=20');
  });

  it('get: identical entity over HTTP and IPC', async () => {
    const call = (client: BackendClient) => client.positions.get('p-1');
    const http = await viaHttp(call, 200, POSITION);
    const ipc = await viaIpc(call, POSITION, false);
    expect(http.ok && ipc.ok).toBe(true);
    if (http.ok && ipc.ok) expect(http.value).toEqual(ipc.value);
  });

  it('not_found: same error code/message/details over both transports', async () => {
    const envelope = ENVELOPE('not_found', 'Position not found');
    const call = (client: BackendClient) => client.positions.get('missing');
    assertSameError(await viaHttp(call, 404, envelope), await viaIpc(call, envelope, true), 'not_found');
  });

  it('validation: same 422 envelope over both transports', async () => {
    const envelope = ENVELOPE('validation_error', 'name is required');
    const call = (client: BackendClient) => client.positions.create({ name: '' });
    assertSameError(
      await viaHttp(call, 422, envelope),
      await viaIpc(call, envelope, true),
      'validation_error',
    );
  });

  it('permission denial: same authorization envelope over both transports', async () => {
    // Authorization itself is executed once in the Application layer (P2);
    // this proves the two adapters surface the identical denial to the UI.
    const envelope = ENVELOPE('forbidden', 'Not permitted');
    const call = (client: BackendClient) => client.positions.remove('p-1');
    assertSameError(
      await viaHttp(call, 403, envelope),
      await viaIpc(call, envelope, true),
      'forbidden',
    );
  });
});
