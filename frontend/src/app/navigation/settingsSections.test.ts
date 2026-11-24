/** Phase 2 WP1: grouped settings navigation model. */
import { describe, expect, it } from 'vitest';
import {
  SETTINGS_GROUPS,
  SECTION_ANCHOR,
  isSettingsSection,
} from './settingsSections';

describe('settingsSection model', () => {
  it('keeps the two UI 2.0 groups and does not add a Hermes shell', () => {
    expect(SETTINGS_GROUPS.map((g) => g.label)).toEqual(['系统配置', '运行管理']);
    const labels = SETTINGS_GROUPS.flatMap((g) => g.items.map((i) => i.label));
    expect(labels).not.toContain('Agent 运行时');
  });

  it('resolves valid sections only', () => {
    expect(isSettingsSection('general')).toBe(true);
    expect(isSettingsSection('routing')).toBe(true);
    expect(isSettingsSection('bogus')).toBe(false);
    expect(SECTION_ANCHOR['ai-connections']).toBe('sec-ai');
  });
});
