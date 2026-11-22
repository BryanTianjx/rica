/**
 * Inline confirmation bar for destructive actions (delete, restore, …).
 *
 * Replaces the three page-local `.confirm-bar` blocks with one primitive so
 * the markup and a11y story stay identical everywhere. The root element
 * MUST keep the literal `confirm-bar` class: page tests locate it via
 * `classList.contains('confirm-bar')` and the CSS rule keys on it.
 *
 * Behavior contract per call site:
 * - `busy` disables the confirm button and swaps its label to `busyLabel`.
 * - `cancelDisabled` is separate because the delete bars keep 取消 enabled
 *   during the request while the restore bar disables it.
 */
import type { ReactNode } from 'react';

export function ConfirmBar({
  message,
  onConfirm,
  onCancel,
  busy = false,
  cancelDisabled = false,
  confirmLabel = '确认',
  busyLabel = '处理中…',
}: {
  message: ReactNode;
  onConfirm: () => void;
  onCancel: () => void;
  busy?: boolean;
  cancelDisabled?: boolean;
  confirmLabel?: string;
  busyLabel?: string;
}) {
  return (
    <div className="confirm-bar">
      <span>{message}</span>
      <div className="actions">
        <button className="danger" onClick={onConfirm} disabled={busy}>
          {busy ? busyLabel : confirmLabel}
        </button>
        <button onClick={onCancel} disabled={cancelDisabled}>
          取消
        </button>
      </div>
    </div>
  );
}
