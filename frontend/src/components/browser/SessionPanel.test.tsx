import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { ApiError } from '../../services/api';
import { browserApi } from '../../services/endpoints';
import { SessionPanel } from './SessionPanel';
import type { BrowserRuntimeStatus } from '../../types';

function makeStatus(overrides: Partial<BrowserRuntimeStatus> = {}): BrowserRuntimeStatus {
  return {
    state: 'ready',
    headed: true,
    profile_id: 'p1',
    uptime_ms: 1000,
    cold_start_ms: 1000,
    bridge: { pid: 4242, port: 13068, restarts: 0 },
    last_error: null,
    data_root: 'C:\\data\\browser',
    capabilities: { playwright_available: true, playwright_version: '1.65.2' },
    sessions: [],
    limits: { max_sessions: 1, snapshot_max_bytes: 524288, screenshot_max_bytes: 8388608 },
    ...overrides,
  };
}

/** 断言 F6：L4 无审批时界面必须自己拦住，给出可读原因 + 「去审批」入口。 */
describe('SessionPanel — L4 approval interception (F6)', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('shows a readable block and the approval entry when the backend refuses an L4 action', async () => {
    // mount: 无会话；createSession 后 reload：会话已在 runtime status 里
    vi.spyOn(browserApi, 'runtimeStatus')
      .mockResolvedValueOnce({ runtime: makeStatus() })
      .mockResolvedValue({
        runtime: makeStatus({ sessions: [{ session_id: 'ses_l4', state: 'ready' }] }),
      });
    vi.spyOn(browserApi, 'createSession').mockResolvedValue({
      session: { session_id: 'ses_l4', state: 'ready', profile_id: 'p1', created_at: null },
    });
    vi.spyOn(browserApi, 'executeAction').mockRejectedValue(
      new ApiError(
        409,
        {
          error: {
            code: 'approval_required',
            message: 'This action cannot be undone and needs a human approval',
            details: {
              action_id: 'act_1',
              approval_id: 'apr_1',
              risk_level: 'L4',
              action_digest: 'digest-abc',
            },
          },
        },
        'refused',
      ),
    );
    render(<SessionPanel />);
    fireEvent.click(await waitFor(() => screen.getByText('新建会话')));
    await waitFor(() => expect(screen.getByText('ses_l4')).toBeTruthy());
    fireEvent.click(screen.getByText('点击'));
    await waitFor(() => expect(screen.getByText(/必须先获得人工审批/)).toBeTruthy());
    expect(screen.getByText(/去审批面板处理该 L4 动作/)).toBeTruthy();
    expect(screen.getByText('apr_1')).toBeTruthy();
    // 拦截必须在界面上有明确文案，而不是裸 409 错误码
    expect(screen.queryByText(/approval_required/)).toBeNull();
  });
});

describe('SessionPanel — session ops', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('create -> navigate -> snapshot -> screenshot -> verify uses the session endpoints', async () => {
    vi.spyOn(browserApi, 'runtimeStatus')
      .mockResolvedValueOnce({ runtime: makeStatus() })
      .mockResolvedValue({
        runtime: makeStatus({ sessions: [{ session_id: 'ses_abc', state: 'ready' }] }),
      });
    const createdSpy = vi
      .spyOn(browserApi, 'createSession')
      .mockResolvedValue({ session: { session_id: 'ses_abc', state: 'ready', profile_id: 'p1', created_at: null } });
    vi.spyOn(browserApi, 'navigate').mockResolvedValue({
      navigation: {
        session_id: 'ses_abc',
        requested_url: 'http://127.0.0.1:18799/fixture.html',
        final_url: 'http://127.0.0.1:18799/fixture.html',
        http_status: 200,
        title: 'ICA Fixture',
        duration_ms: 120,
        action_id: 'act_2',
        permission_id: 'perm_1',
        blocked_attempts: [],
        new_tabs: [],
      },
    });
    vi.spyOn(browserApi, 'snapshot').mockResolvedValue({
      snapshot: { snapshot_id: 'act_3', content: '<button>点我</button>' },
    });
    vi.spyOn(browserApi, 'screenshot').mockResolvedValue({
      artifact: {
        artifact_id: 'act_4',
        sha256: 'ab12',
        bytes: 1234,
        width: 1280,
        height: 860,
        format: 'png',
        url: '/api/browser/v1/artifacts/act_4',
        path: 'C:\\art\\act_4.png',
        created_at: null,
      },
    });
    const verifySpy = vi.spyOn(browserApi, 'verifyAction').mockResolvedValue({
      verification: { passed: true, kind: 'element_visible', attempts: 1, elapsed_ms: 40 },
    });
    vi.spyOn(browserApi, 'executeAction').mockResolvedValueOnce({
      action: {
        action_id: 'act_5',
        state: 'succeeded',
        kind: 'click',
        risk_level: 'L2',
        target_domain: '127.0.0.1',
        duration_ms: 90,
        observed: {},
        verification: null,
        permission_id: 'perm_1',
        approval_id: null,
        blocked_attempts: [],
        new_tabs: [],
      },
    });

    render(<SessionPanel />);
    fireEvent.click(await waitFor(() => screen.getByText('新建会话')));
    await waitFor(() => expect(createdSpy).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('ses_abc')).toBeTruthy());

    fireEvent.click(screen.getByText('导航'));
    await waitFor(() => expect(screen.getByText(/HTTP 200/)).toBeTruthy());

    fireEvent.click(screen.getByText('DOM 快照'));
    await waitFor(() => expect(screen.getByText(/<button>点我<\/button>/)).toBeTruthy());

    fireEvent.click(screen.getByText('截图'));
    await waitFor(() => {
      const img = screen.getByAltText('会话截图') as HTMLImageElement;
      expect(img.getAttribute('src')).toBe('/api/browser/v1/artifacts/act_4');
    });

    fireEvent.click(screen.getByText('点击'));
    await waitFor(() => expect(screen.getByText(/上次动作 act_5/)).toBeTruthy());

    fireEvent.click(screen.getByText('验证上次动作'));
    await waitFor(() =>
      expect(verifySpy).toHaveBeenCalledWith('act_5', {
        kind: 'element_visible',
        target: { by: 'css', css: '#out' },
      }),
    );
    expect(screen.getByText(/验证通过/)).toBeTruthy();
  });

  it('shows the blocked state when the runtime is not ready', async () => {
    vi.spyOn(browserApi, 'runtimeStatus').mockResolvedValue({
      runtime: makeStatus({ state: 'stopped' }),
    });
    render(<SessionPanel />);
    await waitFor(() => expect(screen.getByText(/运行时未启动/)).toBeTruthy());
  });
});
