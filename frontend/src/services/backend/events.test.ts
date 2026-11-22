// F1-3 event channel tests: the local bus stands in for backend Tauri Events
// in a non-Tauri (vitest) host, so the event-driven refresh is testable here.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  POSITION_CHANGED,
  positionChanged,
  subscribePositionChanged,
  resetPositionListeners,
} from './events';

describe('position-change event bus (non-Tauri host)', () => {
  beforeEach(() => resetPositionListeners());

  it('uses the transport-neutral event name shared with ica-contracts', () => {
    expect(POSITION_CHANGED).toBe('ica://position-changed');
  });

  it('delivers an emitted change to every subscriber', () => {
    const a = vi.fn();
    const b = vi.fn();
    subscribePositionChanged(a);
    subscribePositionChanged(b);

    positionChanged({ kind: 'created', id: 'pos-1' });

    expect(a).toHaveBeenCalledWith({ kind: 'created', id: 'pos-1' });
    expect(b).toHaveBeenCalledWith({ kind: 'created', id: 'pos-1' });
  });

  it('an unsubscribed handler stops receiving events', () => {
    const handler = vi.fn();
    const off = subscribePositionChanged(handler);
    positionChanged({ kind: 'updated', id: 'pos-2' });
    expect(handler).toHaveBeenCalledTimes(1);

    off();
    positionChanged({ kind: 'deleted', id: 'pos-2' });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('all change kinds flow through the same channel', () => {
    const seen: string[] = [];
    subscribePositionChanged((p) => seen.push(`${p.kind}:${p.id}`));
    positionChanged({ kind: 'created', id: 'a' });
    positionChanged({ kind: 'updated', id: 'a' });
    positionChanged({ kind: 'deleted', id: 'a' });
    expect(seen).toEqual(['created:a', 'updated:a', 'deleted:a']);
  });
});
