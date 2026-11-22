/**
 * 会话操作台：新建 / 关闭会话、导航、DOM 快照、截图、点击 / 输入、动作验证。
 *
 * 安全规则（docs/plan/02-frontend.md 硬性要求）：
 * 1. L4（不可逆）动作在后端返回 approval_required 时由界面自己拦住：
 *    展示可读原因与「去审批」入口，而不是裸错误码（AC F6）。
 * 2. 每次动作后可跑 expectations 验证（actions/{id}/verify），验证目标
 *    由操作者给（默认 element_visible + 一个 CSS 选择器）。
 *
 * 截图经 artifacts/{id} 渲染：/api 前缀在桌面宿主里由 ica:// 协议转发到
 * sidecar，dev 由 vite 代理，两端行为一致。
 */

import { useState } from 'react';
import { backend } from '../../services/backend';
import { ApiError } from '../../services/backend';
import { Badge, EmptyState, ErrorBox, Loading } from '../ui';
import { useAsync } from '../../hooks/useAsync';
import { riskTone } from './browserStatus';
import type { BrowserActionResult, BrowserNavigation } from '../../types';

/** 动作目标的最小表达：CSS / 快照 ref / 文本三种常用定位。 */
interface ActionTarget {
  by: 'css' | 'ref' | 'text';
  css?: string;
  ref?: string;
  text?: string;
}

function errText(exc: unknown): string {
  if (exc instanceof ApiError) return `${exc.code}: ${exc.message}`;
  return String(exc);
}

export function SessionPanel() {
  // 面板自持运行时状态：会话创建 / 关闭后 reload，会话存活性才不会拿到旧快照。
  const runtime = useAsync(() => backend.browser.runtimeStatus(), []);
  const runtimeStatus = runtime.data?.runtime ?? null;
  // 会话清单以服务端 status 为准（页面刷新 / 运行时重启后依然可见、可关）；
  // 本地只记录“当前选中”的那一个，未选中时回落到第一个活会话。
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [url, setUrl] = useState('http://127.0.0.1:18799/fixture.html');
  const [clickTarget, setClickTarget] = useState('#btn');
  const [fillTarget, setFillTarget] = useState('#name');
  const [fillValue, setFillValue] = useState('');
  const [verifyTarget, setVerifyTarget] = useState('#out');
  const [navigation, setNavigation] = useState<BrowserNavigation | null>(null);
  const [snapshotText, setSnapshotText] = useState<string | null>(null);
  const [artifactUrl, setArtifactUrl] = useState<string | null>(null);
  const [lastAction, setLastAction] = useState<BrowserActionResult | null>(null);
  const [verifyOutcome, setVerifyOutcome] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [approvalHint, setApprovalHint] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);

  const runtimeReady = runtimeStatus?.state === 'ready';
  const liveSessions = (runtimeStatus?.sessions ?? []).filter(
    (s) => s.state === 'ready' || s.state === 'active',
  );
  const activeSessionId = liveSessions.some((s) => s.session_id === selectedSessionId)
    ? selectedSessionId
    : (liveSessions[0]?.session_id ?? null);
  const atSessionLimit =
    runtimeStatus != null && runtimeStatus.sessions.length >= runtimeStatus.limits.max_sessions;

  const createSession = async () => {
    setBusy(true);
    setError(null);
    try {
      const data = await backend.browser.createSession();
      setSelectedSessionId(data.session.session_id);
      runtime.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  const closeSession = async (targetId: string) => {
    setBusy(true);
    try {
      await backend.browser.closeSession(targetId);
      setSelectedSessionId(null);
      runtime.reload();
      setNavigation(null);
      setSnapshotText(null);
      setArtifactUrl(null);
      setLastAction(null);
      setVerifyOutcome(null);
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  const navigate = async () => {
    if (!activeSessionId) return;
    setLoading(true);
    setError(null);
    try {
      const data = await backend.browser.navigate(activeSessionId, { url: url.trim() });
      setNavigation(data.navigation);
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setLoading(false);
    }
  };

  const takeSnapshot = async () => {
    if (!activeSessionId) return;
    setLoading(true);
    try {
      const data = await backend.browser.snapshot(activeSessionId, {});
      const snap = data.snapshot as { content?: string };
      setSnapshotText(
        typeof snap.content === 'string' ? snap.content : JSON.stringify(data.snapshot, null, 2),
      );
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setLoading(false);
    }
  };

  const takeScreenshot = async () => {
    if (!activeSessionId) return;
    setLoading(true);
    try {
      const data = await backend.browser.screenshot(activeSessionId, {});
      setArtifactUrl(data.artifact.url);
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setLoading(false);
    }
  };

  /** 点击 / 输入；L4 无审批时后端回 approval_required，界面拦截并给「去审批」入口。 */
  const execute = async (kind: 'click' | 'fill', target: ActionTarget) => {
    if (!activeSessionId) return;
    setActionError(null);
    setApprovalHint(null);
    setVerifyOutcome(null);
    setLoading(true);
    try {
      const data = await backend.browser.executeAction(activeSessionId, {
        kind,
        target,
        value: kind === 'fill' && fillValue.length > 0 ? fillValue : undefined,
      });
      setLastAction(data.action);
    } catch (exc) {
      if (exc instanceof ApiError && exc.code === 'approval_required') {
        setActionError(
          '此操作不可逆（风险等级 L4），必须先获得人工审批才能执行。'
          + `动作 ${String(exc.details.action_id ?? '')} 已进入待审批状态（摘要 `
          + `${String(exc.details.action_digest ?? '')}）。`,
        );
        setApprovalHint(String(exc.details.approval_id ?? ''));
      } else {
        setActionError(errText(exc));
      }
    } finally {
      setLoading(false);
    }
  };

  const verifyLastAction = async () => {
    if (!lastAction) return;
    setLoading(true);
    setError(null);
    try {
      const data = await backend.browser.verifyAction(lastAction.action_id, {
        kind: 'element_visible',
        target: { by: 'css', css: verifyTarget.trim() || 'body' },
      });
      setVerifyOutcome(
        data.verification.passed
          ? `验证通过：${verifyTarget} 可见（${data.verification.elapsed_ms ?? 0}ms）。`
          : `验证未通过：${verifyTarget} 不可见。`,
      );
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setLoading(false);
    }
  };

  if (!runtimeReady) {
    return <EmptyState message="运行时未启动：先在「运行时」面板启动浏览器。" />;
  }

  return (
    <div>
      <div className="toolbar">
        <button
          type="button"
          className="primary"
          onClick={() => void createSession()}
          disabled={busy || atSessionLimit}
          title={atSessionLimit ? `已达会话上限（${runtimeStatus?.limits.max_sessions}）` : undefined}
        >
          新建会话
        </button>
        {liveSessions.map((s) => (
          <span
            key={s.session_id}
            className="session-chip"
            data-active={s.session_id === activeSessionId ? 'true' : undefined}
          >
            <button
              type="button"
              className="link-btn mono"
              onClick={() => setSelectedSessionId(s.session_id)}
              title="设为当前操作会话"
            >
              {s.session_id}
            </button>
            <button
              type="button"
              className="link-btn"
              onClick={() => void closeSession(s.session_id)}
              disabled={busy}
              title="关闭该会话"
            >
              关闭
            </button>
          </span>
        ))}
        {atSessionLimit && (
          <span className="muted small">已达会话上限，先关闭现有会话。</span>
        )}
      </div>

      {error && <ErrorBox message={error} />}
      {loading && <Loading label="执行中…" />}

      {activeSessionId && (
        <>
          <div className="toolbar">
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              aria-label="地址栏"
              placeholder="http(s)://…"
            />
            <button type="button" className="primary" onClick={() => void navigate()} disabled={loading}>
              导航
            </button>
            <button type="button" onClick={() => void takeSnapshot()} disabled={loading}>
              DOM 快照
            </button>
            <button type="button" onClick={() => void takeScreenshot()} disabled={loading}>
              截图
            </button>
          </div>

          {navigation && (
            <p className="muted small">
              HTTP {navigation.http_status ?? '—'} · {navigation.final_url ?? navigation.requested_url}
              {navigation.title ? ` · ${navigation.title}` : ''}
              {navigation.blocked_attempts.length > 0 ? ' · 有被拦截的跳转' : ''}
            </p>
          )}

          {artifactUrl && (
            <p>
              <img src={artifactUrl} alt="会话截图" width={480} />
            </p>
          )}

          <div className="toolbar">
            <input
              value={clickTarget}
              onChange={(e) => setClickTarget(e.target.value)}
              aria-label="点击目标"
              placeholder="CSS 选择器（点击）"
            />
            <button
              type="button"
              onClick={() => void execute('click', { by: 'css', css: clickTarget.trim() || 'body' })}
              disabled={loading}
            >
              点击
            </button>
            <input
              value={fillTarget}
              onChange={(e) => setFillTarget(e.target.value)}
              aria-label="输入目标"
              placeholder="CSS 选择器（填写）"
            />
            <input
              value={fillValue}
              onChange={(e) => setFillValue(e.target.value)}
              aria-label="输入内容"
              placeholder="输入内容"
            />
            <button
              type="button"
              onClick={() => void execute('fill', { by: 'css', css: fillTarget.trim() || 'body' })}
              disabled={loading}
            >
              填写
            </button>
            {lastAction && (
              <>
                <Badge tone={riskTone(lastAction.risk_level)}>{lastAction.risk_level}</Badge>
                <button type="button" onClick={() => void verifyLastAction()} disabled={loading}>
                  验证上次动作
                </button>
                <input
                  value={verifyTarget}
                  onChange={(e) => setVerifyTarget(e.target.value)}
                  aria-label="验证目标"
                  placeholder="验证目标（CSS）"
                />
              </>
            )}
          </div>

          {actionError && <ErrorBox message={actionError} />}
          {approvalHint && (
            <div className="notice">
              去审批面板处理该 L4 动作（审批 ID：<span className="mono">{approvalHint}</span>
              ），批准后重新执行同一动作即可。
            </div>
          )}
          {verifyOutcome && <div className="notice">{verifyOutcome}</div>}
          {lastAction && (
            <p className="muted small">
              上次动作 {lastAction.action_id} · {lastAction.kind} · {lastAction.state}
            </p>
          )}

          {snapshotText !== null && (
            <>
              <h4>DOM 快照</h4>
              <pre className="mono small">{snapshotText.slice(0, 4000)}</pre>
            </>
          )}
        </>
      )}
    </div>
  );
}
