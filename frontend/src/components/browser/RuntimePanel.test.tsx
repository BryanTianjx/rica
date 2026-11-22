import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { ApiError } from '../../services/backend';
import { backend } from '../../services/backend';
import { RuntimePanel } from './RuntimePanel';
import type { BrowserRuntimeStatus } from '../../types';

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

describe('RuntimePanel', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders the runtime badge and bridge rows from the real status shape', async () => {
    vi.spyOn(backend.browser, 'runtimeStatus').mockResolvedValue({
      runtime: makeStatus({
        state: 'ready',
        bridge: { pid: 4242, port: 13068, restarts: 1 },
        capabilities: { playwright_available: true, playwright_version: '1.65.2' },
      }),
    });
    render(<RuntimePanel />);
    await waitFor(() => expect(screen.getByText('运行中')).toBeTruthy());
    expect(screen.getByText(/PID 4242/)).toBeTruthy();
    expect(screen.getByText(/playwright 1\.65\.2/)).toBeTruthy();
  });

  it('start and stop buttons call the runtime endpoints', async () => {
    // 首次 stopped；启动后 refresh 拿到 ready，停止按钮才可点
    vi.spyOn(backend.browser, 'runtimeStatus')
      .mockResolvedValueOnce({ runtime: makeStatus() })
      .mockResolvedValue({ runtime: makeStatus({ state: 'ready', bridge: { pid: 9, port: 7, restarts: 0 } }) });
    const startSpy = vi
      .spyOn(backend.browser, 'runtimeStart')
      .mockResolvedValue({ runtime: { state: 'ready', profile_id: 'p1', already_running: false, headed: true } });
    vi.spyOn(backend.browser, 'runtimeStop').mockResolvedValue({
      runtime: { state: 'stopped', closed_sessions: 0, already_stopped: false, forced: true },
    });
    render(<RuntimePanel />);
    await waitFor(() => expect(screen.getByText('启动运行时')).toBeTruthy());
    fireEvent.click(screen.getByText('启动运行时'));
    await waitFor(() => expect(startSpy).toHaveBeenCalled());
    // reload 后状态变 ready，停止按钮从 disabled 变为可点
    await waitFor(() =>
      expect((screen.getByText('停止运行时') as HTMLButtonElement).disabled).toBe(false),
    );
    fireEvent.click(screen.getByText('停止运行时'));
    await waitFor(() =>
      expect(backend.browser.runtimeStop).toHaveBeenCalledWith(true),
    );
  });

  it('self-check reports playwright missing with a readable reason', async () => {
    vi.spyOn(backend.browser, 'runtimeStatus').mockResolvedValue({ runtime: makeStatus() });
    vi.spyOn(backend.browser, 'connectorStatus').mockResolvedValue({
      connector: { running: false, pid: null, frame_port: 0, control_port: 0, uptime_s: null },
      live: {},
      connectors: [],
      sessions: [],
      tabs: [],
      extension_id: 'dagjdegfbllmdckkoopdafogoofjkmba',
    });
    render(<RuntimePanel />);
    await waitFor(() => expect(screen.getByText('一键自检')).toBeTruthy());
    fireEvent.click(screen.getByText('一键自检'));
    await waitFor(() => expect(screen.getByText('Playwright 可导入')).toBeTruthy());
    expect(screen.getAllByText('缺失').length).toBe(2);
    expect(
      screen.getByText(/playwright 未随本次安装/),
    ).toBeTruthy();
    // playwright 缺失时 Chromium 项给出「跳过」原因，而不是笼统的不可用
    expect(screen.getByText(/Playwright 缺失时无法探测 Chromium/)).toBeTruthy();
    // native host 无探针：如实标注「无法验证」并给出验证命令
    expect(screen.getByText('无法验证')).toBeTruthy();
    expect(screen.getByText(/reg query/)).toBeTruthy();
  });

  it('self-check passes all rows when the runtime is ready with a browser', async () => {
    vi.spyOn(backend.browser, 'runtimeStatus').mockResolvedValue({
      runtime: makeStatus({
        state: 'ready',
        capabilities: {
          playwright_available: true,
          playwright_version: '1.65.2',
          browser_version: '159.0.8154.6',
        },
      }),
    });
    vi.spyOn(backend.browser, 'connectorStatus').mockResolvedValue({
      connector: { running: false, pid: null, frame_port: 0, control_port: 0, uptime_s: null },
      live: {},
      connectors: [],
      sessions: [],
      tabs: [],
      extension_id: 'dagjdegfbllmdckkoopdafogoofjkmba',
    });
    render(<RuntimePanel />);
    await waitFor(() => expect(screen.getByText('一键自检')).toBeTruthy());
    fireEvent.click(screen.getByText('一键自检'));
    await waitFor(() => expect(screen.getAllByText('通过').length).toBe(2));
    expect(screen.getByText(/随包 Chromium 正在运行（159\.0\.8154\.6）/)).toBeTruthy();
  });

  it('self-check turns a failed start into a readable chromium reason', async () => {
    vi.spyOn(backend.browser, 'runtimeStatus').mockResolvedValue({
      runtime: makeStatus({
        capabilities: { playwright_available: true, playwright_version: '1.65.2' },
      }),
    });
    vi.spyOn(backend.browser, 'connectorStatus').mockResolvedValue({
      connector: { running: false, pid: null, frame_port: 0, control_port: 0, uptime_s: null },
      live: {},
      connectors: [],
      sessions: [],
      tabs: [],
      extension_id: 'dagjdegfbllmdckkoopdafogoofjkmba',
    });
    vi.spyOn(backend.browser, 'runtimeStart').mockRejectedValue(
      new ApiError(
        503,
        { error: { code: 'browser_start_failed', message: "Executable doesn't exist at C:\\x\\chrome.exe", details: {} } },
        'start failed',
      ),
    );
    render(<RuntimePanel />);
    await waitFor(() => expect(screen.getByText('一键自检')).toBeTruthy());
    fireEvent.click(screen.getByText('一键自检'));
    await waitFor(() =>
      expect(screen.getByText(/Chromium 缺失：可执行文件不存在于 C:\\x\\chrome\.exe/)).toBeTruthy(),
    );
  });
});
