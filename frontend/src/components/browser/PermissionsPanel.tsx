/**
 * 权限面板：域名白名单增删 + 当前生效策略只读展示。
 *
 * 删除权限不可逆：走 ConfirmBar 二次确认（AC 硬性要求 2）。
 * risk_ceiling 的取值来自 PermissionGrantRequest：L1/L2/L3（L4 是 schema 层
 * 明确禁止的常授权限，不给选项）。
 */

import { useState } from 'react';
import { browserApi } from '../../services/endpoints';
import { Badge, EmptyState, ErrorBox, Loading } from '../ui';
import { ConfirmBar } from '../ConfirmBar';
import { useAsync } from '../../hooks/useAsync';
import { permissionTone, statusLabel } from './browserStatus';
import type { BrowserPermission } from '../../types';

function errText(exc: unknown): string {
  return exc instanceof Error ? exc.message : String(exc);
}

export function PermissionsPanel() {
  const list = useAsync(async () => (await browserApi.listPermissions()).permissions, []);
  const [domain, setDomain] = useState('');
  const [ceiling, setCeiling] = useState<'L1' | 'L2' | 'L3'>('L3');
  const [note, setNote] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState<BrowserPermission | null>(null);

  const permissions = list.data ?? null;

  const grant = async () => {
    const trimmed = domain.trim();
    if (!trimmed) {
      setError('域名不能为空。');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await browserApi.grantPermission({
        domain: trimmed,
        risk_ceiling: ceiling,
        note: note.trim().length > 0 ? note.trim() : undefined,
      });
      setDomain('');
      setNote('');
      setNotice(`已授权 ${trimmed}（上限 ${ceiling}）。`);
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  const revoke = async (permission: BrowserPermission) => {
    setBusy(true);
    try {
      await browserApi.revokePermission(permission.permission_id);
      setNotice(`已撤销 ${permission.domain} 的授权。`);
      setConfirming(null);
      list.reload();
    } catch (exc) {
      setError(errText(exc));
    } finally {
      setBusy(false);
    }
  };

  if (permissions === null && !list.error) {
    return <Loading label="读取权限…" />;
  }

  return (
    <div>
      <div className="toolbar">
        <input
          value={domain}
          onChange={(e) => setDomain(e.target.value)}
          aria-label="域名"
          placeholder="域名，如 127.0.0.1"
        />
        <select
          value={ceiling}
          onChange={(e) => setCeiling(e.target.value as 'L1' | 'L2' | 'L3')}
          aria-label="风险上限"
        >
          <option value="L1">上限 L1</option>
          <option value="L2">上限 L2</option>
          <option value="L3">上限 L3</option>
        </select>
        <input
          value={note}
          onChange={(e) => setNote(e.target.value)}
          aria-label="备注"
          placeholder="备注（可选）"
        />
        <button type="button" className="primary" onClick={() => void grant()} disabled={busy}>
          授权
        </button>
      </div>

      {list.error && <ErrorBox message={`权限读取失败：${list.error}`} />}
      {error && <ErrorBox message={error} />}
      {notice && <div className="notice">{notice}</div>}

      <p className="muted small">
        生效策略（只读）：未授权域名的导航与动作一律拒绝；L4 动作不吃域名授权，
        必须逐次人工审批；授权默认 24 小时过期。
      </p>

      {permissions !== null && permissions.length === 0 ? (
        <EmptyState message="还没有任何域名授权。" />
      ) : (
        <table>
          <thead>
            <tr>
              <th>域名</th>
              <th>范围</th>
              <th>风险上限</th>
              <th>状态</th>
              <th>过期</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {(permissions ?? []).map((p) => (
              <tr key={p.permission_id}>
                <td className="mono">{p.domain}</td>
                <td>{p.action_kind ? p.action_kind : '全站'}</td>
                <td>{p.risk_ceiling}</td>
                <td>
                  <Badge tone={permissionTone(p.state)}>{statusLabel(p.state)}</Badge>
                </td>
                <td>
                  <span className="muted small">{p.expires_at ?? '不过期'}</span>
                </td>
                <td>
                  {confirming?.permission_id === p.permission_id ? (
                    <ConfirmBar
                      message={`撤销 ${p.domain} 的授权？该域名下的自动化会立即被拒绝。`}
                      onConfirm={() => void revoke(p)}
                      onCancel={() => setConfirming(null)}
                      busy={busy}
                      confirmLabel="确认撤销"
                    />
                  ) : (
                    <button type="button" onClick={() => setConfirming(p)} disabled={busy}>
                      删除
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
