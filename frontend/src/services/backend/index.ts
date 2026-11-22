// ARCH-P5 AC1/AC4 — the single `backend` facade every page consumes.
//
// Transport selection happens at startup via `resolveBackendConfig()` plus the
// migration manifest; this module is the only place that wires a transport to
// a domain. Page components never decide, never import `fetch` / `invoke`.
//
// The facade also re-exports `ApiError`, `describeApiError` and session refresh
// so UI code no longer reaches past the client into the HTTP implementation
// (`services/api.ts`). That file stays the HTTP transport's internals.

import { ApiError, describeApiError, refreshSession } from '../api';
import type { RefreshOutcome } from '../api';
import type { BackendClient } from './client';
import { resolveBackendConfig, type BackendStartupConfig, type Transport } from './config';
import { transportForDomain, type BackendDomain } from './migration';
import { httpBackend } from './http';
import { ipcPositions } from './ipc';

export { ApiError, describeApiError, refreshSession, resolveBackendConfig };
export { MIGRATED_DOMAINS, UNMIGRATED_DOMAINS } from './migration';
export type { RefreshOutcome };
export type { BackendClient } from './client';
export type { BackendStartupConfig, Transport } from './config';
export type { BackendDomain } from './migration';

/**
 * Compose a client from the startup config. Every domain starts on its
 * original HTTP path; only domains present in MIGRATED_DOMAINS can switch to
 * their IPC adapter. Calling this with `transport: 'http'` is the rollback
 * path — behaviour returns to the pre-migration call graph with no page edit.
 */
export function createBackend(config: BackendStartupConfig = resolveBackendConfig()): BackendClient {
  const migrated = transportForDomain('positions', config.transport) === 'ipc';
  // `httpBackend` is `as const`; compose a new object instead of mutating it.
  // Unmigrated domains are copied verbatim from the original HTTP client.
  return migrated ? { ...httpBackend, positions: ipcPositions } : { ...httpBackend };
}

/** Diagnostics only: which transport a domain actually rides. */
export function activeTransport(domain: BackendDomain, config: BackendStartupConfig): Transport {
  return transportForDomain(domain, config.transport);
}

/** Resolved once at startup, before the first React render. */
export const backendConfig = resolveBackendConfig();
export const backend: BackendClient = createBackend(backendConfig);
