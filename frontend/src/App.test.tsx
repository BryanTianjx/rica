import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import App from './App';
import { dashboardApi } from './services/endpoints';

// 桌面宿主旧版直接打开 /index.html（G4-09b 黑屏根因）：必须重定向到工作台
vi.mock('./auth/AuthContext', async (importOriginal) => ({
  ...(await importOriginal()),
  useAuth: () => ({ user: null, logout: vi.fn() }),
}));

describe('App — 启动路由', () => {
  it('路径 / 渲染工作台', async () => {
    vi.spyOn(dashboardApi, 'get').mockResolvedValue({
      candidate_total: 0,
      candidate_status_counts: {},
      active_follow_up_count: 0,
      pending_follow_ups: [],
      awaiting_result_count: 0,
      pending_draft_count: 0,
      recent_messages: [],
      position_total: 0,
      open_position_count: 0,
    });
    render(
      <MemoryRouter initialEntries={['/']}>
        <App />
      </MemoryRouter>,
    );
    expect(await screen.findByRole('heading', { name: '工作台' })).toBeInTheDocument();
  });

  it('/index.html 重定向到工作台（桌面宿主旧版启动路径兜底）', async () => {
    render(
      <MemoryRouter initialEntries={['/index.html']}>
        <App />
      </MemoryRouter>,
    );
    // 重定向后落在 / → Dashboard；不再出现无路由匹配的空白主区域
    expect(await screen.findByRole('heading', { name: '工作台' })).toBeInTheDocument();
    expect(dashboardApi.get).toHaveBeenCalled();
  });
});
