/**
 * Phase 1 WP05 — grouped primary navigation.
 * The route set is unchanged: these are the same 7 links plus the fixed
 * settings link, only reorganised into the three UI 2.0 work areas.
 */
export interface NavItem {
  to: string;
  label: string;
  end?: boolean;
}

export interface NavGroup {
  label: string;
  items: NavItem[];
}

/** AI 助手 / 安全审计 deliberately absent: no real backend yet (see D01/D02). */
export const NAV_GROUPS: NavGroup[] = [
  {
    label: '工作空间',
    items: [
      { to: '/', label: '工作台', end: true },
      { to: '/candidates', label: '候选人' },
      { to: '/positions', label: '岗位管理' },
      { to: '/conversations', label: '沟通记录' },
      { to: '/follow-ups', label: '跟进任务' },
    ],
  },
  {
    label: 'AI 工作空间',
    items: [
      // `/drafts` 是草稿审核页，不是 AI 对话页（ICA-UX-009 §1 D-01）。
      { to: '/drafts', label: 'AI 草稿审核' },
      { to: '/browser', label: '浏览器自动化' },
    ],
  },
  {
    label: '系统管理',
    items: [{ to: '/settings', label: '系统设置' }],
  },
];

export const SETTINGS_ITEM = NAV_GROUPS[2].items[0];
