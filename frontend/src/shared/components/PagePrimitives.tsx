/** Shared page layout primitives: cards, notices, form rows and action bars. */

import type { ReactNode } from 'react';

/**
 * A titled card. Both pages used to repeat the card/card-head/heading trio;
 * the rendered DOM is unchanged, so CSS and role-based tests keep working.
 */
export function Card({
  title,
  aside,
  className,
  heading = 'h2',
  children,
}: {
  title: ReactNode;
  aside?: ReactNode;
  className?: string;
  heading?: 'h2' | 'h3';
  children?: ReactNode;
}) {
  const Heading = heading;
  return (
    <div className={className ? `card ${className}` : 'card'}>
      <div className="card-head">
        <Heading>{title}</Heading>
        {aside}
      </div>
      {children}
    </div>
  );
}

export function Notice({
  children,
  testId,
}: {
  children: ReactNode;
  testId?: string;
}) {
  return (
    <div className="notice" aria-live="polite" data-testid={testId}>
      {children}
    </div>
  );
}

export function FormRow({
  children,
  className,
  two = false,
}: {
  children: ReactNode;
  className?: string;
  two?: boolean;
}) {
  if (two) return <div className="form-row two">{children}</div>;
  return <div className={className ? `form-row ${className}` : 'form-row'}>{children}</div>;
}

export function Actions({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={className ? `actions ${className}` : 'actions'}>{children}</div>;
}
