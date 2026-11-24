/** Phase 1 WP06 + follow-up: one shared header primitive.
 *
 * App Shell renders it with `breadcrumb` only (no title). Pages render it with
 * `title`/`description`/`actions` (no breadcrumb), so the <h1> is produced by
 * one component instead of hand-written in every page. A page rendered on its
 * own in a test still exposes its heading, because PageHeader renders it in
 * place of the old `.page-head` block.
 */
import { Link } from 'react-router-dom';
import type { ReactNode } from 'react';
import type { Crumb } from '../navigation/routeMeta';

export function Breadcrumb({ items }: { items: Crumb[] }) {
  return (
    <nav className="breadcrumb" aria-label="面包屑">
      {items.map((item, index) => (
        <span key={`${item.label}-${index}`}>
          {index > 0 && <span className="breadcrumb-sep" aria-hidden="true">/</span>}
          {item.to ? <Link to={item.to}>{item.label}</Link> : <span aria-current="page">{item.label}</span>}
        </span>
      ))}
    </nav>
  );
}

export function PageHeader({
  breadcrumb,
  title,
  description,
  actions,
}: {
  breadcrumb?: Crumb[];
  title?: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="page-header">
      {breadcrumb && <Breadcrumb items={breadcrumb} />}
      {title !== undefined ? (
        <div className="page-head">
          <div>
            <h1>{title}</h1>
            {description}
          </div>
          {actions && <div className="actions">{actions}</div>}
        </div>
      ) : (
        actions && <div className="page-header-actions">{actions}</div>
      )}
    </header>
  );
}
