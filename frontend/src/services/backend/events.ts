// The frontend half of the F1-3 event channel.
//
// The backend emits `ica://position-changed` after a positions write (Tauri
// Events). The view subscribes once and refreshes on the event, instead of
// manually reloading after each write. The event name and payload mirror the
// transport-neutral contract in `ica-contracts/src/events.rs` — one shape,
// no second contract invented on the client.
//
// Two execution paths, one handler:
//   * Tauri window: `listen()` receives the backend event; writes do NOT
//     emit locally (the backend does, so every window stays in sync).
//   * Browser / vitest (no `__TAURI_INTERNALS__`): a local in-process bus
//     stands in for the backend so the same event-driven flow is testable
//     without a desktop host. Writes emit on this bus.

import { listen } from '@tauri-apps/api/event';

/** Mirrors ica-contracts::events::POSITION_CHANGED. */
export const POSITION_CHANGED = 'ica://position-changed';

export interface PositionChangedPayload {
  kind: 'created' | 'updated' | 'deleted';
  id: string;
}

type Handler = (payload: PositionChangedPayload) => void;

const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

// Local bus for non-Tauri hosts (browser / vitest). Module-scoped so a
// subscription set in a page sees an emit from the same page's write call.
const localHandlers = new Set<Handler>();

/**
 * Subscribe to position-change events. Returns an unsubscribe function.
 * In Tauri, the listener is async-registered; the returned function is still
 * synchronous and safe to call from a React cleanup.
 */
export function subscribePositionChanged(handler: Handler): () => void {
  if (isTauri) {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<PositionChangedPayload>(POSITION_CHANGED, (event) => {
      if (!cancelled) handler(event.payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }
  localHandlers.add(handler);
  return () => {
    localHandlers.delete(handler);
  };
}

/**
 * Announce a completed positions write. In a Tauri window the backend has
 * already emitted; in the browser/vitest we emit on the local bus so the
 * event-driven flow is identical and testable.
 */
export function positionChanged(payload: PositionChangedPayload): void {
  if (isTauri) return;
  for (const handler of [...localHandlers]) handler(payload);
}

/** Test-only: drop all local subscriptions (vitest isolation). */
export function resetPositionListeners(): void {
  localHandlers.clear();
}
