/**
 * 主题（浅色 / 深色）——令牌全部定义在 index.css 的 :root 与
 * :root[data-theme='light']，这里只负责把选择写到 <html data-theme> 并持久化。
 *
 * 首次访问跟随系统偏好（prefers-color-scheme）；之后以 localStorage 为准。
 * initTheme 必须在 React 渲染前调用，避免首帧闪错主题。
 */

export type Theme = 'light' | 'dark';

const STORAGE_KEY = 'ica-theme';

export function getStoredTheme(): Theme | null {
  try {
    const value = window.localStorage.getItem(STORAGE_KEY);
    return value === 'light' || value === 'dark' ? value : null;
  } catch {
    return null;
  }
}

export function systemTheme(): Theme {
  // jsdom 等测试环境没有 matchMedia：退回深色（应用默认主题）。
  if (typeof window.matchMedia !== 'function') return 'dark';
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

export function initTheme(): Theme {
  const theme = getStoredTheme() ?? systemTheme();
  applyTheme(theme);
  return theme;
}

export function persistTheme(theme: Theme): void {
  applyTheme(theme);
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // 无痕模式等存储不可用：主题仍然生效，只是不跨会话记忆。
  }
}

export function toggleTheme(current: Theme): Theme {
  const next: Theme = current === 'dark' ? 'light' : 'dark';
  persistTheme(next);
  return next;
}
