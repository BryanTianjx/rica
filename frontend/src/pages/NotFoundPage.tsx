/** Phase 1 WP08 — fallback route. Added in addition to the existing 10 routes. */
import { Link } from 'react-router-dom';
import { PageHeader } from '../app/layout/PageHeader';

export function NotFoundPage() {
  return (
    <PageHeader
      title="页面不存在"
      description={<p>该地址没有对应的 ICA 页面，请返回工作台。</p>}
      actions={<Link className="btn primary" to="/">返回工作台</Link>}
    />
  );
}
