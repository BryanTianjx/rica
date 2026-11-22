import { NavLink, Navigate, Route, Routes } from 'react-router-dom';
import { useState } from 'react';
import { useAuth } from './auth/AuthContext';
import { initTheme, toggleTheme, type Theme } from './theme';
import { Loading } from './components/ui';
import { LoginPage } from './pages/LoginPage';
import { DashboardPage } from './pages/DashboardPage';
import { CandidatesPage } from './pages/CandidatesPage';
import { CandidateDetailPage } from './pages/CandidateDetailPage';
import { PositionsPage } from './pages/PositionsPage';
import { ConversationsPage } from './pages/ConversationsPage';
import { FollowUpsPage } from './pages/FollowUpsPage';
import { DraftsPage } from './pages/DraftsPage';
import { SettingsPage } from './pages/SettingsPage';
import { BrowserPage } from './pages/BrowserPage';

const NAV = [
  { to: '/', label: '工作台', end: true },
  { to: '/candidates', label: '候选人' },
  { to: '/positions', label: '岗位管理' },
  { to: '/conversations', label: '沟通记录' },
  { to: '/follow-ups', label: '跟进任务' },
  // `/drafts` 是草稿审核页，不是 AI 对话页；标签必须与 /assistant 区分（ICA-UX-009 §1 D-01）。
  { to: '/drafts', label: 'AI 草稿审核' },
  { to: '/browser', label: '浏览器自动化' },
];

/** 系统设置不占一级导航：以齿轮按钮固定在侧栏底部（1.7px 单线图标）。 */
function SettingsIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.44.76a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.44.76a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.44-.76a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.44-.76a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

/** 主题切换按钮：显示将切换到的模式（深色时显示「浅色模式」）。 */
function ThemeToggle({ theme, onToggle }: { theme: Theme; onToggle: () => void }) {
  const goingLight = theme === 'dark';
  return (
    <button
      type="button"
      className="theme-toggle"
      onClick={onToggle}
      title={goingLight ? '切换到浅色模式' : '切换到深色模式'}
    >
      <svg
        viewBox="0 0 24 24"
        width="16"
        height="16"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        {goingLight ? (
          <>
            <circle cx="12" cy="12" r="4" />
            <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
          </>
        ) : (
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        )}
      </svg>
      {goingLight ? '浅色模式' : '深色模式'}
    </button>
  );
}

function Shell() {
  const { status, user, logout } = useAuth();
  // initTheme 已在 main.tsx（首帧前）把主题写到 <html data-theme>，这里只是镜像。
  const [theme, setTheme] = useState<Theme>(() => initTheme());

  if (status === 'checking') {
    return (
      <div className="login-wrap">
        <Loading label="检查会话…" />
      </div>
    );
  }

  if (status === 'unauthenticated') {
    return <LoginPage />;
  }

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="brand">
          <span className="brand-text">
            ICA Personal
            <small>本地招聘沟通助理</small>
          </span>
        </div>
        {NAV.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            className={({ isActive }) => (isActive ? 'nav-link active' : 'nav-link')}
          >
            {item.label}
          </NavLink>
        ))}
        <NavLink
          to="/settings"
          className={({ isActive }) => (isActive ? 'nav-link nav-settings active' : 'nav-link nav-settings')}
        >
          <SettingsIcon />
          系统设置
        </NavLink>
        <ThemeToggle theme={theme} onToggle={() => setTheme(toggleTheme(theme))} />
        {user && !user.email && (
          <div className="sidebar-user">
            <span className="muted small">本地模式</span>
          </div>
        )}
        {user && user.email && (
          <div className="sidebar-user">
            <span className="small">{user.display_name}</span>
            <button type="button" className="link-btn" onClick={() => void logout()}>
              退出登录
            </button>
          </div>
        )}
      </nav>

      <main className="main">
        <Routes>
          <Route path="/" element={<DashboardPage />} />
          {/* 桌面宿主旧版本直接打开 /index.html：重定向到工作台，避免主区域空白 */}
          <Route path="/index.html" element={<Navigate to="/" replace />} />
          <Route path="/candidates" element={<CandidatesPage />} />
          <Route path="/candidates/:id" element={<CandidateDetailPage />} />
          <Route path="/positions" element={<PositionsPage />} />
          <Route path="/conversations" element={<ConversationsPage />} />
          <Route path="/follow-ups" element={<FollowUpsPage />} />
          <Route path="/drafts" element={<DraftsPage />} />
          <Route path="/browser" element={<BrowserPage />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Routes>
      </main>
    </div>
  );
}

export default function App() {
  return <Shell />;
}
