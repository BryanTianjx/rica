/**
 * 浏览器子系统各面板共享的小工具：后端状态词表 → 徽章色调。
 *
 * 只服务 src/components/browser/*（C8 只拦 pages/ 内的状态字面量，这里不受限）。
 * 色调语义沿用 ui.tsx Badge：ok=正常，warn=等待/进行中，bad=失败/高风险，
 * info=只读信息，default=其余。
 */

export type BadgeTone = 'default' | 'ok' | 'warn' | 'bad' | 'info';

const RUNTIME_TONES: Record<string, BadgeTone> = {
  stopped: 'default',
  ready: 'ok',
  failed: 'bad',
};

const TASK_TONES: Record<string, BadgeTone> = {
  pending: 'warn',
  running: 'info',
  awaiting_approval: 'warn',
  completed: 'ok',
  succeeded: 'ok',
  failed: 'bad',
  cancelled: 'bad',
  skipped: 'default',
  denied: 'bad',
};

const APPROVAL_TONES: Record<string, BadgeTone> = {
  pending: 'warn',
  approved: 'ok',
  rejected: 'bad',
  expired: 'default',
};

const EXTRACTION_TONES: Record<string, BadgeTone> = {
  draft: 'warn',
  approved: 'ok',
  rejected: 'bad',
  confirmed: 'ok',
};

const PERMISSION_TONES: Record<string, BadgeTone> = {
  active: 'ok',
  revoked: 'bad',
  expired: 'default',
};

const SELF_CHECK_TONES: Record<string, BadgeTone> = {
  pass: 'ok',
  missing: 'bad',
  unverified: 'warn',
};

function toneOf(map: Record<string, BadgeTone>, value: string | null | undefined): BadgeTone {
  if (value && map[value] !== undefined) return map[value];
  return 'default';
}

export const runtimeTone = (value: string | null | undefined) => toneOf(RUNTIME_TONES, value);
export const taskTone = (value: string | null | undefined) => toneOf(TASK_TONES, value);
export const approvalTone = (value: string | null | undefined) => toneOf(APPROVAL_TONES, value);
export const extractionTone = (value: string | null | undefined) => toneOf(EXTRACTION_TONES, value);
export const permissionTone = (value: string | null | undefined) => toneOf(PERMISSION_TONES, value);
export const selfCheckTone = (value: string | null | undefined) => toneOf(SELF_CHECK_TONES, value);

/** L1..L4 的徽章色调：L4 必须一眼可辨（bad）。 */
export function riskTone(level: string | null | undefined): BadgeTone {
  if (level === 'L4') return 'bad';
  if (level === 'L3' || level === 'L2') return 'warn';
  if (level === 'L1') return 'ok';
  return 'default';
}

/** 状态词的中文展示（后端返回小写英文，界面统一翻译）。 */
const LABELS: Record<string, string> = {
  stopped: '已停止',
  ready: '运行中',
  failed: '故障',
  pending: '待处理',
  running: '执行中',
  awaiting_approval: '待审批',
  completed: '已完成',
  succeeded: '已成功',
  cancelled: '已取消',
  skipped: '已跳过',
  denied: '已拒绝',
  approved: '已批准',
  rejected: '已驳回',
  expired: '已过期',
  draft: '草稿',
  confirmed: '已落档',
  active: '生效中',
  revoked: '已撤销',
  closed: '已关闭',
  open: '打开',
};

export function statusLabel(value: string | null | undefined): string {
  if (!value) return '—';
  return LABELS[value] ?? value;
}
