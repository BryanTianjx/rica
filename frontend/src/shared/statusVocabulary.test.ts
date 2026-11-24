/** Phase 3: the canonical vocabulary and tone mapping used by all business pages. */
import { describe, expect, it } from 'vitest';
import {
  CANDIDATE_STATUSES,
  FOLLOW_UP_STATUSES,
  FOLLOW_UP_TERMINAL_STATUSES,
  POSITION_STATUSES,
  toneForStatus,
} from './statusVocabulary';

describe('shared status vocabulary', () => {
  it('exposes every candidate/follow-up/position status once', () => {
    expect(CANDIDATE_STATUSES).toHaveLength(9);
    expect(FOLLOW_UP_STATUSES).toHaveLength(12);
    expect(POSITION_STATUSES).toEqual(['Draft', 'Open', 'OnHold', 'Closed']);
    expect(FOLLOW_UP_TERMINAL_STATUSES).toEqual(['Completed', 'Cancelled', 'Expired']);
  });

  it('assigns tones consistently across domains', () => {
    expect(toneForStatus('Active')).toBe('ok');
    expect(toneForStatus('Rejected')).toBe('bad');
    expect(toneForStatus('Pending')).toBe('warn');
    expect(toneForStatus('unknown')).toBe('default');
  });
});
