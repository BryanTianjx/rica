/** Phase 2 WP1 — settings centre navigation model.
 * Grouped like the UI 2.0 spec §7.1. `section` query selects one section;
 * no query keeps the historical all-sections view (test compatibility).
 */
export type SettingsSection =
  | 'general'
  | 'ai-connections'
  | 'models'
  | 'routing'
  | 'privacy'
  | 'data-backup'
  | 'audit';

export interface SettingsNavGroup {
  label: string;
  items: Array<{ key: SettingsSection; label: string; href: string }>;
}

export const SETTINGS_GROUPS: SettingsNavGroup[] = [
  {
    label: '系统配置',
    items: [
      { key: 'general', label: '通用设置', href: '/settings?section=general' },
      { key: 'ai-connections', label: 'AI 服务连接', href: '/settings?section=ai-connections' },
      { key: 'models', label: '模型管理', href: '/settings?section=models' },
      { key: 'routing', label: '场景路由', href: '/settings?section=routing' },
      { key: 'privacy', label: '隐私与安全', href: '/settings?section=privacy' },
      { key: 'data-backup', label: '数据与备份', href: '/settings?section=data-backup' },
    ],
  },
  {
    label: '运行管理',
    items: [
      { key: 'audit', label: '审计日志', href: '/settings?section=audit' },
      { key: 'general', label: '浏览器管理', href: '/browser' },
    ],
  },
];

/** Which page section a nav key maps to today (some share one rendered block). */
export const SECTION_ANCHOR: Record<SettingsSection, string> = {
  general: 'sec-general',
  'ai-connections': 'sec-ai',
  models: 'sec-ai',
  routing: 'sec-routing',
  privacy: 'sec-privacy',
  'data-backup': 'sec-backup',
  audit: 'sec-audit',
};

export function isSettingsSection(value: string | null): value is SettingsSection {
  return value !== null && value in SECTION_ANCHOR;
}
