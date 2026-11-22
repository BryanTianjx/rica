// The F1/F2 transport contract: both the HTTP implementation (http.ts) and
// the per-module IPC adapters (ipc.ts, F2 slices) must satisfy THIS type.
// Derived from the HTTP client so the two transports can never drift apart
// silently — a missing IPC method is a compile error, not a runtime gap.
import type { HttpBackend } from './http';

export type BackendClient = HttpBackend;
export type PositionsBackend = BackendClient["positions"];
