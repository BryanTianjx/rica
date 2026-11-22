// The Backend Client (F0-2, per frontend-architecture-r1 §4/F0 and the
// frontend migration plan): the ONE module pages and components import for
// every backend interaction.
//
// Layers, from the outside in:
//   pages/components → services/backend (this file) → services/endpoints
//   (the HTTP implementation — retired INTO this layer, no longer imported
//   anywhere else) → services/api (transport: fetch, 401 single-flight
//   refresh, the unified error envelope).
//
// F1 contract: `BackendClient` is the explicit interface both transports
// implement. The HTTP one is active today (typeof backend below); F2's IPC
// implementations will satisfy the same type per module, so a swap is a
// one-line change in this file and the pages never notice.
//
// UI behavior is frozen: every method here delegates verbatim to the
// endpoint wrappers that the pages called before the abstraction existed —
// same URLs, same params, same DTOs, same errors.
import {
  aiApi,
  authApi,
  browserApi,
  candidatesApi,
  conversationsApi,
  dashboardApi,
  draftsApi,
  draftGroupsApi,
  followUpsApi,
  importsApi,
  messagesApi,
  positionsApi,
  settingsApi,
} from '../endpoints';

// The unified error model: pages catch ApiError and render
// error.message / error.code; the HTTP transport produces it today and the
// IPC adapter will produce the same shape in F2.
export { ApiError } from '../api';

/**
 * The unified Backend Client. One method per contract endpoint, grouped by
 * domain (SPE-BAS-003 §4 DOC-04 / the frontend-architecture §4 F0 mapping).
 *
 * Group keys mirror the endpoint wrapper groups in endpoints.ts; the only
 * rename is the `*Api` suffix → the domain noun, so call sites read
 * `backend.candidates.list()` instead of `candidatesApi.list()`.
 */
export const backend = {
  candidates: candidatesApi,
  positions: positionsApi,
  conversations: conversationsApi,
  messages: messagesApi,
  imports: importsApi,
  followUps: followUpsApi,
  drafts: draftsApi,
  draftGroups: draftGroupsApi,
  ai: aiApi,
  dashboard: dashboardApi,
  auth: authApi,
  settings: settingsApi,
  browser: browserApi,
} as const;

/**
 * The explicit F1 contract: an IPC adapter implements THIS type (derived
 * from the HTTP client, so the two transports can never drift apart
 * silently — a missing IPC method is a compile error in F2).
 */
export type BackendClient = typeof backend;
