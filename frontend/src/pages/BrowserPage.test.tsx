import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { browserApi } from '../services/endpoints';
import { BrowserPage } from './BrowserPage';
import type { BrowserRuntimeStatus } from '../types';

function makeStatus(overrides: Partial<BrowserRuntimeStatus> = {}): BrowserRuntimeStatus {
  return {
    state: 'stopped',
    headed: true,
    profile_id: null,
    uptime_ms: null,
    cold_start_ms: null,
    bridge: { pid: null, port: null, restarts: 0 },
    last_error: null,
    data_root: 'C:\\data\\browser',
    capabilities: { playwright_available: false },
    sessions: [],
    limits: { max_sessions: 1, snapshot_max_bytes: 524288, screenshot_max_bytes: 8388608 },
    ...overrides,
  };
}

/** F1：/browser 页可达、6 个面板齐全。 */
describe('BrowserPage', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders all six panels', async () => {
    vi.spyOn(browserApi, 'runtimeStatus').mockResolvedValue({ runtime: makeStatus() });
    vi.spyOn(browserApi, 'listPermissions').mockResolvedValue({ permissions: [] });
    vi.spyOn(browserApi, 'audit').mockResolvedValue({ events: [] });
    vi.spyOn(browserApi, 'listExtractions').mockResolvedValue({ extractions: [] });
    vi.spyOn(browserApi, 'connectorStatus').mockResolvedValue({
      connector: { running: false, pid: null, frame_port: 0, control_port: 0, uptime_s: null },
      live: {},
      connectors: [],
      sessions: [],
      tabs: [],
      extension_id: 'dagjdegfbllmdckkoopdafogoofjkmba',
    });
    render(
      <MemoryRouter initialEntries={['/browser']}>
        <BrowserPage />
      </MemoryRouter>,
    );
    expect(screen.getByText('浏览器自动化')).toBeTruthy();
    for (const heading of ['运行时', '会话操作台', '权限', '审批', '任务', '候选人提取']) {
      expect(screen.getByText(heading)).toBeTruthy();
    }
    await waitFor(() => expect(screen.getByText(/运行时未启动/)).toBeTruthy());
  });
});
