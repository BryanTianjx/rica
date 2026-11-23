/** Phase 1 WP08 — fallback route. Added in addition to the existing 10 routes. */
import { Link } from 'react-router-dom';

export function NotFoundPage() {
  return (
    <div className="page-head">
      <div>
        <h1>页面不存在</h1>
        <p>该地址没有对应的 ICA 页面，请返回工作台。</p>
      </div>
      <div className="actions">
        <Link className="btn primary" to="/">返回工作台</Link>
      </div>
    </div>
  );
}
