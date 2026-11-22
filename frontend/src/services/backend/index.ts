// The Backend Client (F0-2): the ONE module pages and components import for
// every backend interaction.
//
// Transport selection: inside a Tauri window the positions branch rides the
// in-process IPC commands (F1-2 vertical slice); everywhere else — and for
// every domain without an IPC adapter yet — it rides HTTP through the
// spawned server. Branch selection is per-domain so F2 switches modules one
// at a time without touching pages.
//
// The UI behavior contract is frozen: both transports emit the same DTOs
// (ica-contracts is the shared shape source) and the same
// {"error": {code, message, details}} envelope, so pages cannot tell the
// transports apart except by being faster.
import { ApiError } from '../api';
import { httpBackend } from './http';
import { ipcPositions } from './ipc';
import type { BackendClient } from './client';

export { ApiError };
export type { BackendClient } from './client';

/// Inside a Tauri 2 webview the IPC internals are injected before any app
/// script runs; outside (plain browser, vitest jsdom) the key is absent.
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const backend: BackendClient = isTauri
  ? { ...httpBackend, positions: ipcPositions }
  : httpBackend;
