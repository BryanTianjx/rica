// ARCH-P5 §5.3 — the ONE place where transport selection is decided.
//
// The decision is made at application startup, once, before any page renders.
// Pages and components never import `fetch`, `invoke`, `http.ts` or `ipc.ts`:
// they receive a fully composed `backend` from `./index`. Switching transport
// (or rolling back to the original HTTP calls) is therefore a startup-config
// action, never a UI branch.
//
// Precedence (highest first):
//   1. window.__ICA_BACKEND_CONFIG__.transport  — host-injected runtime config
//   2. localStorage['ica.backend.transport']    — field rollback without rebuild
//   3. VITE_ICA_BACKEND_TRANSPORT               — build-time env
//   4. detected host ('ipc' in a Tauri 2 webview, otherwise 'http')
//
// 'ipc' never moves a domain that is not in the migration manifest; see
// `./migration`. The HTTP server keeps serving every domain regardless.

export type Transport = 'http' | 'ipc';

export interface BackendStartupConfig {
  transport: Transport;
  source: 'runtime-global' | 'local-override' | 'build-env' | 'host-detected';
}

export const TRANSPORT_ENV_KEY = 'VITE_ICA_BACKEND_TRANSPORT';
export const TRANSPORT_STORAGE_KEY = 'ica.backend.transport';
export const TRANSPORT_GLOBAL_KEY = '__ICA_BACKEND_CONFIG__';

declare global {
  interface Window {
    __ICA_BACKEND_CONFIG__?: { transport?: unknown };
  }
}

function isTransport(value: unknown): value is Transport {
  return value === 'http' || value === 'ipc';
}

/** True only inside a Tauri 2 webview (the IPC internals are injected there). */
export function isTauriWindow(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Resolve the startup configuration. No page code calls this directly. */
export function resolveBackendConfig(): BackendStartupConfig {
  const injected = typeof window !== 'undefined' ? window[TRANSPORT_GLOBAL_KEY] : undefined;
  if (injected && isTransport(injected.transport)) {
    return { transport: injected.transport, source: 'runtime-global' };
  }

  if (typeof window !== 'undefined') {
    const stored = window.localStorage?.getItem(TRANSPORT_STORAGE_KEY);
    if (isTransport(stored)) return { transport: stored, source: 'local-override' };
  }

  const fromEnv = import.meta.env[TRANSPORT_ENV_KEY];
  if (isTransport(fromEnv)) return { transport: fromEnv, source: 'build-env' };

  return {
    transport: isTauriWindow() ? 'ipc' : 'http',
    source: 'host-detected',
  };
}
