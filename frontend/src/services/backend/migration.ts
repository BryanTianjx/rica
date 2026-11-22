// ARCH-P5 §5.4 — the module-by-module migration manifest.
//
// Adding an IPC adapter is an explicit, reviewable change to this file plus
// `index.ts`. Domains not listed in MIGRATED_DOMAINS keep their original HTTP
// calls; a global `transport: 'ipc'` startup config does NOT sweep them up.
// This is the "gradual migration, never a one-shot replacement" guarantee.

import type { BackendClient } from './client';
import type { Transport } from './config';

export type BackendDomain = keyof BackendClient;

/** Domains with a registered, contract-parity-tested IPC command branch. */
export const MIGRATED_DOMAINS: readonly BackendDomain[] = ['positions'];

/** Every other domain keeps its original HTTP call path. */
export const UNMIGRATED_DOMAINS: ReadonlyArray<{
  domain: BackendDomain;
  reason: string;
}> = [
  { domain: 'candidates', reason: 'HTTP 已满足；先统一 Use Case，待 host 注册 candidates 命令后迁移' },
  { domain: 'conversations', reason: '涉及状态与授权，按模块边界单独迁移' },
  { domain: 'messages', reason: '与 conversations 同边界，待授权一致性确认' },
  { domain: 'imports', reason: '写路径含事务与审计，沿用 HTTP 直到 IPC parity 覆盖' },
  { domain: 'followUps', reason: '业务状态机正确性优先，统一后再迁移' },
  { domain: 'drafts', reason: 'AI 编排经 Application Service，暂不暴露第二个入口' },
  { domain: 'draftGroups', reason: '与 drafts 共用状态机，同批迁移' },
  { domain: 'ai', reason: '模型配置写端点仍由 Python sidecar 提供，保持 HTTP' },
  { domain: 'dashboard', reason: '只读聚合，无高频 IPC 收益' },
  { domain: 'auth', reason: '会话刷新与 token 轮换是 HTTP 行为，保持原调用方式' },
  { domain: 'settings', reason: 'AI 密钥写入 OS 凭据库，保留 HTTP Server 单一入口' },
  { domain: 'browser', reason: 'Python sidecar 的协议化 HTTP 边界（ARCH-P4），不本地化' },
];

/** Resolve the transport for one domain from the startup config. */
export function transportForDomain(domain: BackendDomain, config: Transport): Transport {
  return config === 'ipc' && MIGRATED_DOMAINS.includes(domain) ? 'ipc' : 'http';
}
