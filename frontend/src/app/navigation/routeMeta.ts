/** Phase 1 WP06 — breadcrumb metadata driven by the router, not by pages. */
export interface Crumb {
  label: string;
  to?: string;
}

const META: Record<string, Crumb[]> = {
  '/': [{ label: '工作台' }],
  '/index.html': [{ label: '工作台' }],
  '/candidates': [{ label: '工作台', to: '/' }, { label: '候选人' }],
  '/candidates/:id': [
    { label: '工作台', to: '/' },
    { label: '候选人', to: '/candidates' },
    { label: '候选人详情' },
  ],
  '/positions': [{ label: '工作台', to: '/' }, { label: '岗位管理' }],
  '/conversations': [{ label: '工作台', to: '/' }, { label: '沟通记录' }],
  '/follow-ups': [{ label: '工作台', to: '/' }, { label: '跟进任务' }],
  '/ai-workspace': [{ label: '工作台', to: '/' }, { label: 'AI 工作空间' }],
  '/drafts': [{ label: '工作台', to: '/' }, { label: 'AI 草稿审核' }],
  '/browser': [{ label: '工作台', to: '/' }, { label: '浏览器自动化' }],
  '/settings': [{ label: '工作台', to: '/' }, { label: '系统设置' }],
};

/** Resolve the current pathname to breadcrumbs without adding routes. */
export function crumbsFor(pathname: string): Crumb[] {
  if (META[pathname]) return META[pathname];
  if (/^\/candidates\/[^/]+$/.test(pathname)) return META['/candidates/:id'];
  return [{ label: '工作台', to: '/' }, { label: '页面不存在' }];
}
