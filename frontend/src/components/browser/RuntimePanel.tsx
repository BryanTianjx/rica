/**
 * 运行时面板：状态徽章、启动 / 停止、一键自检三项。
 *
 * 自检三项的数据来源与边界（无专用后端探针端点，不新增端点）：
 * 1. Playwright 可导入 —— GET /runtime/status 的 capabilities.playwright_available
 *    （sidecar 侧 find_spec 探针，缺装时为 false）；
 * 2. Chromium 可执行文件 —— 状态 ready 时以 capabilities.browser_version 作为
 *    「存在且可用」的实证；停止状态下通过一次用户点击的 runtime/start 探测，
 *    失败时把后端给出的可读原因（如 Executable doesn't exist at …）原样展示；
 * 3. Native Messaging host —— 后端没有任何暴露注册表状态的端点：已有
 *    connector 会话（Chrome/Edge 真把 host 拉起过）即视为通过；否则如实显示
 *    「系统内无法验证」并给出验证方法，不假装通过也不谎报缺失。
 */

import { useState } from 'react';
import { browserApi } from '../../services/endpoints';
import { ApiError } from '../../services/api';
import { Badge, ErrorBox, Loading } from '../ui';
import { useAsync } from '../../hooks/useAsync';
import { runtimeTone, selfCheckTone, statusLabel } from './browserStatus';
import type { BrowserSelfCheckItem } from '../../types';

/** 把一次启动失败翻译成自检第二项的可读原因。 */
function chromiumReasonFromError(exc: ApiError): string {
  const raw = exc.message || '';
  const match = raw.match(/Executable doesn't exist at (.+)$/);
  if (match) {
    return `Chromium 缺失：可执行文件不存在于 ${match[1]}（运行时未随包或路径漂移）`;
  }
  return `无法确认 Chromium：启动失败（${exc.code}）${raw ? '：' + raw : ''}`;
}

export function RuntimePanel() {
  const runtime = useAsync(() => browserApi.runtimeStatus(), []);
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [checks, setChecks] = useState<BrowserSelfCheckItem[] | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const status = runtime.data?.runtime ?? null;

  const start = async () => {
    setBusy(true);
    setActionError(null);
    try {
      await browserApi.runtimeStart();
      runtime.reload();
    } catch (exc) {
      setActionError(exc instanceof ApiError ? `${exc.code}: ${exc.message}` : String(exc));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    setActionError(null);
    try {
      await browserApi.runtimeStop(true);
      runtime.reload();
    } catch (exc) {
      setActionError(exc instanceof ApiError ? `${exc.code}: ${exc.message}` : String(exc));
    } finally {
      setBusy(false);
    }
  };

  /** 一键自检：三项逐个给出「通过 / 缺失+原因 / 无法验证」。 */
  const runSelfCheck = async () => {
    setChecking(true);
    setActionError(null);
    const items: BrowserSelfCheckItem[] = [];
    try {
      const data = await browserApi.runtimeStatus();
      const rt = data.runtime;
      const caps = rt.capabilities;
      // ① Playwright 可导入（sidecar 探针）
      items.push(
        caps.playwright_available
          ? {
              key: 'playwright',
              label: 'Playwright 可导入',
              verdict: 'pass',
              detail: `playwright ${caps.playwright_version ?? '（版本未知）'} 已随包安装`,
            }
          : {
              key: 'playwright',
              label: 'Playwright 可导入',
              verdict: 'missing',
              detail:
                'playwright 未随本次安装（sidecar 无法导入可选依赖）——重装随包发布版或检查 site-packages',
            },
      );
      // ② Chromium 可执行文件：ready 即实证；否则用一次启动探测
      if (rt.state === 'ready' && caps.browser_version) {
        items.push({
          key: 'chromium',
          label: 'Chromium 可执行文件',
          verdict: 'pass',
          detail: `随包 Chromium 正在运行（${caps.browser_version}）`,
        });
      } else if (!caps.playwright_available) {
        items.push({
          key: 'chromium',
          label: 'Chromium 可执行文件',
          verdict: 'missing',
          detail: '跳过：Playwright 缺失时无法探测 Chromium',
        });
      } else {
        try {
          const started = await browserApi.runtimeStart();
          runtime.reload();
          items.push({
            key: 'chromium',
            label: 'Chromium 可执行文件',
            verdict: 'pass',
            detail: `启动成功（${started.runtime.browser_version ?? '版本未知'}）`,
          });
        } catch (exc) {
          items.push({
            key: 'chromium',
            label: 'Chromium 可执行文件',
            verdict: 'missing',
            detail:
              exc instanceof ApiError
                ? chromiumReasonFromError(exc)
                : `无法启动浏览器运行时：${String(exc)}`,
          });
        }
      }
      // ③ Native Messaging host：有活动 connector 会话即实证；否则无法验证
      const connector = await browserApi.connectorStatus();
      const liveKeys = Object.keys(connector.live ?? {}).length;
      if (connector.connector.running && liveKeys > 0) {
        items.push({
          key: 'native_host',
          label: 'Native Messaging host 已注册',
          verdict: 'pass',
          detail: 'Chrome/Edge 已通过 com.ica.connector 拉起过本机 host（有活动连接会话）',
        });
      } else {
        items.push({
          key: 'native_host',
          label: 'Native Messaging host 已注册',
          verdict: 'unverified',
          detail:
            '系统内没有探针端点可读注册表：注册由安装器写入 '
            + 'HKCU\\Software\\Google\\Chrome|Microsoft\\Edge\\NativeMessagingHosts\\com.ica.connector。'
            + '验证方法：重启 Chrome/Edge 并在扩展里连接 ICA，或运行 '
            + 'reg query "HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\com.ica.connector"',
        });
      }
      setChecks(items);
    } catch (exc) {
      setActionError(exc instanceof ApiError ? `${exc.code}: ${exc.message}` : String(exc));
    } finally {
      setChecking(false);
    }
  };

  if (runtime.error && !status) {
    return <ErrorBox message={`运行时状态读取失败：${runtime.error}`} />;
  }
  if (!status) {
    return <Loading label="读取浏览器运行时状态…" />;
  }

  const caps = status.capabilities;

  return (
    <div>
      <div className="toolbar">
        <span>
          <Badge tone={runtimeTone(status.state)}>{statusLabel(status.state)}</Badge>
          {caps.playwright_available ? (
            <span className="muted small">
              {' '}playwright {caps.playwright_version ?? '?'}
              {caps.browser_version ? ` · Chromium ${caps.browser_version}` : ''}
            </span>
          ) : (
            <span className="muted small"> playwright 不可用</span>
          )}
        </span>
        <span className="actions">
          <button
            type="button"
            className="primary"
            onClick={() => void start()}
            disabled={busy || status.state === 'ready'}
          >
            启动运行时
          </button>
          <button
            type="button"
            onClick={() => void stop()}
            disabled={busy || status.state === 'stopped'}
          >
            停止运行时
          </button>
          <button type="button" onClick={() => void runSelfCheck()} disabled={checking}>
            {checking ? '自检中…' : '一键自检'}
          </button>
        </span>
      </div>

      {actionError && <ErrorBox message={actionError} />}
      {runtime.error && <ErrorBox message={`运行时状态刷新失败：${runtime.error}`} />}
      {status.last_error && <ErrorBox message={`运行时上次错误：${status.last_error}`} />}

      <table>
        <tbody>
          <tr>
            <td>Bridge 进程</td>
            <td>
              {status.bridge.pid
                ? `PID ${status.bridge.pid} @ 127.0.0.1:${status.bridge.port}（重启 ${status.bridge.restarts} 次）`
                : '未运行'}
            </td>
          </tr>
          <tr>
            <td>Profile</td>
            <td>{status.profile_id ?? '未创建'}</td>
          </tr>
          <tr>
            <td>数据目录</td>
            <td>{status.data_root}</td>
          </tr>
          <tr>
            <td>会话</td>
            <td>
              {status.sessions.length === 0
                ? '无'
                : status.sessions.map((s) => (
                    <span key={s.session_id} className="mono small">
                      {s.session_id}{' '}
                    </span>
                  ))}
            </td>
          </tr>
          <tr>
            <td>上限</td>
            <td>
              最多 {status.limits.max_sessions} 个会话 · 快照 ≤{' '}
              {Math.round(status.limits.snapshot_max_bytes / 1024)} KiB · 截图 ≤{' '}
              {Math.round(status.limits.screenshot_max_bytes / 1024 / 1024)} MiB
            </td>
          </tr>
        </tbody>
      </table>

      <h3>自检三项</h3>
      {checks === null ? (
        <p className="muted small">点击「一键自检」检查 Playwright / Chromium / Native host。</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>检查项</th>
              <th>结论</th>
              <th>说明</th>
            </tr>
          </thead>
          <tbody>
            {checks.map((item) => (
              <tr key={item.key}>
                <td>{item.label}</td>
                <td>
                  <Badge tone={selfCheckTone(item.verdict)}>
                    {item.verdict === 'pass'
                      ? '通过'
                      : item.verdict === 'missing'
                        ? '缺失'
                        : '无法验证'}
                  </Badge>
                </td>
                <td>{item.detail}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
