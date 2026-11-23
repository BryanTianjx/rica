/** Phase 1 WP06 — Shell-owned page header and breadcrumb.
 * Page components still own their <h1> in Phase 1; this header only provides
 * breadcrumb + optional actions so existing page tests remain valid.
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
  actions,
}: {
  breadcrumb: Crumb[];
  actions?: ReactNode;
}) {
  return (
    <header className="page-header">
      <Breadcrumb items={breadcrumb} />
      {actions && <div className="page-header-actions">{actions}</div>}
    </header>
  );
}
