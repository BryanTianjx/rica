/** Phase 3: one canonical status vocabulary for the business pages.
 * Replaces arrays re-declared in CandidatesPage / CandidateDetailPage /
 * FollowUpsPage. Types stay in `types.ts`; this module owns the ordered lists
 * used by filters and the tone mapping used by StatusBadge.
 */
import type {
  CandidateStatus,
  DraftStatus,
  FollowUpTaskStatus,
  PositionStatus,
  ResultType,
  SenderType,
  ValidationStatus,
} from '../types';

export const CANDIDATE_STATUSES: CandidateStatus[] = [
  'New', 'Contacted', 'ReadNoReply', 'Replied', 'Active',
  'Interview', 'Rejected', 'Withdrawn', 'Closed',
];

export const POSITION_STATUSES: PositionStatus[] = ['Draft', 'Open', 'OnHold', 'Closed'];

export const FOLLOW_UP_STATUSES: FollowUpTaskStatus[] = [
  'Pending', 'Eligible', 'Drafting', 'Reviewing', 'Rejected', 'Approved',
  'Copied', 'AwaitingResult', 'Completed', 'Cancelled', 'Expired', 'Blocked',
];

export const FOLLOW_UP_TERMINAL_STATUSES: FollowUpTaskStatus[] = [
  'Completed', 'Cancelled', 'Expired',
];

export const FOLLOW_UP_NEXT: Partial<Record<FollowUpTaskStatus, FollowUpTaskStatus>> = {
  Pending: 'Eligible',
};

export const RESULT_OPTIONS: { value: ResultType; label: string }[] = [
  { value: 'sent', label: '已发送' },
  { value: 'not_sent', label: '未发送' },
  { value: 'candidate_replied', label: '候选人已回复' },
  { value: 'candidate_no_reply', label: '候选人未回复' },
  { value: 'candidate_declined', label: '候选人拒绝' },
  { value: 'interview', label: '进入面试' },
  { value: 'other', label: '其他' },
];

export const SENDER_OPTIONS: SenderType[] = ['Candidate', 'Recruiter', 'System'];
export const DRAFT_STATUSES: DraftStatus[] = ['generated', 'edited', 'approved', 'rejected', 'copied'];
export const VALIDATION_STATUSES: ValidationStatus[] = ['Passed', 'Warning', 'Blocked'];

/** All values that carry a semantic tone. Keeps StatusBadge's mapping single-source. */
export const TONED_STATUSES = new Set<string>([
  ...CANDIDATE_STATUSES, ...POSITION_STATUSES, ...FOLLOW_UP_STATUSES,
  ...DRAFT_STATUSES, ...VALIDATION_STATUSES,
]);

export type StatusTone = 'ok' | 'warn' | 'bad' | 'default';

/** C8: page code imports these instead of re-typing the DRAFT_STATUSES values. */
export const DRAFT_STATUS = {
  Generated: 'generated',
  Edited: 'edited',
  Approved: 'approved',
  Rejected: 'rejected',
  Copied: 'copied',
} as const;

/** Follow-up state machine tables shared by the page (C8) and the vocabulary. */
export const FOLLOW_UP_ADVANCE_TARGETS: Partial<Record<FollowUpTaskStatus, FollowUpTaskStatus>> = {
  Pending: 'Eligible',
  Eligible: 'Drafting',
  Drafting: 'Reviewing',
};

export const FOLLOW_UP_REJECT_TARGETS: Partial<Record<FollowUpTaskStatus, FollowUpTaskStatus>> = {
  Reviewing: 'Rejected',
};

export const FOLLOW_UP_RESULT_COMPLETABLE_STATUSES: FollowUpTaskStatus[] = [
  'Pending', 'Eligible', 'Drafting', 'Reviewing', 'Approved', 'Copied', 'AwaitingResult',
];

export const FOLLOW_UP_CANCELLABLE_STATUSES: FollowUpTaskStatus[] = [
  'Pending', 'Eligible', 'Drafting', 'Reviewing', 'Rejected', 'Approved', 'Copied',
  'AwaitingResult', 'Blocked',
];

export function toneForStatus(status: string): StatusTone {
  const ok = new Set([
    'Active', 'Interview', 'Open', 'Completed', 'Approved', 'Sent',
    'approved', 'copied', 'Passed', 'Eligible', 'sent', 'interview',
  ]);
  const bad = new Set([
    'Rejected', 'Withdrawn', 'Closed', 'Cancelled', 'Expired', 'Blocked',
    'rejected', 'Dropped', 'candidate_declined', 'on_hold',
  ]);
  const warn = new Set([
    'Warning', 'Pending', 'Editing', 'edited', 'generated', 'Drafting',
    'Reviewing', 'Copied', 'AwaitingResult', 'OnHold', 'Draft',
  ]);
  if (ok.has(status)) return 'ok';
  if (bad.has(status)) return 'bad';
  if (warn.has(status)) return 'warn';
  return 'default';
}
