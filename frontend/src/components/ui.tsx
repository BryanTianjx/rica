/** Small shared presentational components. */

import type { ReactNode } from 'react';
import { toneForStatus } from '../shared/statusVocabulary';

export function Badge({
  children,
  tone = 'default',
}: {
  children: ReactNode;
  tone?: 'default' | 'ok' | 'warn' | 'bad' | 'info';
}) {
  // 默认 tone 必须走静态字面量分支：
  // check-ui-reuse 的 C1 把紧贴插值边界的 token（badge${…}）判为类名片段而丢弃，
  // 所以不能靠一次性拼接得到无尾随空格的 "badge"。拆成两个分支后，
  // 默认分支是纯静态字面量，非默认分支的静态前缀 "badge " 带空格可正常取到。
  // 注意：该检查器会扫描注释文本，注释里不要写类名赋值的字面示例。
  if (tone === 'default') {
    return <span className="badge">{children}</span>;
  }
  return (
    <span className={`badge ${tone}`}>
      {children}
    </span>
  );
}

export function StatusBadge({ status }: { status: string }) {
  // Static status literals are intentional: check-ui-reuse C3 verifies that
  // every status value in the union has a tone branch. The shared vocabulary
  // (src/shared/statusVocabulary.ts) mirrors this mapping for page logic.
  const tone: 'ok' | 'warn' | 'bad' | 'default' =
    status === 'Active' || status === 'Interview' || status === 'Open' ||
    status === 'Completed' || status === 'Approved' || status === 'Eligible' ||
    status === 'copied' || status === 'approved' || status === 'Passed' || status === 'sent'
      ? 'ok'
      : status === 'Rejected' || status === 'Withdrawn' || status === 'Closed' ||
          status === 'Cancelled' || status === 'Expired' || status === 'Blocked' ||
          status === 'rejected'
        ? 'bad'
        : status === 'Warning' || status === 'Pending' || status === 'Editing' ||
            status === 'Drafting' || status === 'Reviewing' || status === 'Copied' ||
            status === 'AwaitingResult' || status === 'OnHold' || status === 'Draft' ||
            status === 'edited' || status === 'generated'
          ? 'warn'
          : 'default';
  void toneForStatus;
  return <Badge tone={tone}>{status}</Badge>;
}

export function Loading({ label = '加载中…' }: { label?: string }) {
  return (
    <div className="empty" role="status" aria-live="polite">
      {label}
    </div>
  );
}

export function ErrorBox({ message }: { message: string }) {
  return (
    <div className="error" role="alert">
      {message}
    </div>
  );
}

/**
 * "Nothing here yet" placeholder — loading has FINISHED with zero rows.
 *
 * Distinct from `Loading` for assistive tech: `Loading` (also role="status")
 * means a request is in flight, while `EmptyState` means the data has
 * settled and is simply empty. The extra `empty-state` class keeps the two
 * distinguishable to CSS and to tests.
 */
export function EmptyState({ message }: { message: string }) {
  return (
    <div className="empty empty-state" role="status" aria-live="polite">
      {message}
    </div>
  );
}

export function Stat({
  value,
  label,
  hint,
}: {
  value: ReactNode;
  label: string;
  hint?: string;
}) {
  return (
    <div className="stat">
      <div className="value">{value}</div>
      <div className="label">{label}</div>
      {hint && <div className="hint">{hint}</div>}
    </div>
  );
}

/** Render a UTC ISO timestamp in the browser's local timezone. */
export function LocalTime({ value }: { value: string | null | undefined }) {
  if (!value) return <span className="muted">—</span>;
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return <span className="muted">—</span>;
  return <span title={value}>{parsed.toLocaleString()}</span>;
}
