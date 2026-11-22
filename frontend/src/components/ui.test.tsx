import { fireEvent, render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ConfirmBar } from './ConfirmBar';
import {
  Badge,
  EmptyState,
  ErrorBox,
  Loading,
  StatusBadge,
  Stat,
  LocalTime,
} from '../components/ui';

describe('Badge', () => {
  it('renders children with tone class', () => {
    const { container } = render(
      <Badge tone="ok">OK</Badge>,
    );
    const span = container.querySelector('span');
    expect(span).toBeInTheDocument();
    expect(span).toHaveClass('badge', 'ok');
    expect(span).toHaveTextContent('OK');
  });

  it('defaults to default tone', () => {
    const { container } = render(<Badge>Default</Badge>);
    const span = container.querySelector('span');
    expect(span).toHaveClass('badge');
    expect(span).not.toHaveClass('ok');
  });
});

describe('StatusBadge', () => {
  it('renders Active as ok tone', () => {
    const { container } = render(<StatusBadge status="Active" />);
    const span = container.querySelector('span');
    expect(span).toHaveClass('badge', 'ok');
  });

  it('renders Blocked as bad tone', () => {
    const { container } = render(<StatusBadge status="Blocked" />);
    const span = container.querySelector('span');
    expect(span).toHaveClass('badge', 'bad');
  });

  it('renders Completed as ok tone', () => {
    const { container } = render(<StatusBadge status="Completed" />);
    const span = container.querySelector('span');
    expect(span).toHaveClass('badge', 'ok');
  });

  it('renders unknown status as default', () => {
    const { container } = render(<StatusBadge status="Unknown" />);
    const span = container.querySelector('span');
    expect(span).toHaveClass('badge');
    expect(span).not.toHaveClass('ok', 'bad', 'warn');
  });
});

describe('Stat', () => {
  it('renders value, label, and optional hint', () => {
    const { container } = render(
      <Stat value={42} label="候选人总数" hint="当前统计" />,
    );
    const value = container.querySelector('.value');
    const label = container.querySelector('.label');
    const hint = container.querySelector('.hint');
    expect(value).toHaveTextContent('42');
    expect(label).toHaveTextContent('候选人总数');
    expect(hint).toHaveTextContent('当前统计');
  });

  it('omits hint when not provided', () => {
    const { container } = render(<Stat value={1} label="仅标签" />);
    expect(container.querySelector('.hint')).not.toBeInTheDocument();
  });
});

describe('LocalTime', () => {
  it('renders locale-formatted timestamp', () => {
    const { container } = render(<LocalTime value="2026-11-02T13:35:16.738715Z" />);
    const span = container.querySelector('span');
    expect(span).toBeInTheDocument();
    // The Z suffix is used as the title attribute for unambiguous UTC display.
    expect(span).toHaveAttribute('title', '2026-11-02T13:35:16.738715Z');
  });

  it('renders em dash when value is null', () => {
    const { container } = render(<LocalTime value={null} />);
    const span = container.querySelector('span');
    expect(span).toHaveTextContent('—');
  });

  it('renders em dash when value is undefined', () => {
    const { container } = render(<LocalTime value={undefined} />);
    const span = container.querySelector('span');
    expect(span).toHaveTextContent('—');
  });

  it('renders em dash for invalid date', () => {
    const { container } = render(<LocalTime value="not-a-date" />);
    const span = container.querySelector('span');
    expect(span).toHaveTextContent('—');
  });
});

// ── a11y 契约（G2-03 批 1，t_a64c4d96 第 8 项）────────────────────────────

describe('Loading — a11y 契约', () => {
  it('暴露 role="status" 与 aria-live="polite"', () => {
    const { getByRole } = render(<Loading />);
    const node = getByRole('status');
    expect(node).toHaveAttribute('aria-live', 'polite');
    expect(node).toHaveTextContent('加载中…');
  });
});

describe('ErrorBox — a11y 契约', () => {
  it('以 role="alert" 渲染错误文本', () => {
    const { getByRole } = render(<ErrorBox message="保存失败" />);
    expect(getByRole('alert')).toHaveTextContent('保存失败');
  });
});

describe('EmptyState — a11y 契约', () => {
  it('对辅助技术可区分：role + aria-live + 可区分标识类', () => {
    const { getByRole } = render(<EmptyState message="还没有会话" />);
    const node = getByRole('status');
    expect(node).toHaveAttribute('aria-live', 'polite');
    // 与 Loading（同为 role="status"）的可区分标识：专属 marker 类。
    expect(node).toHaveClass('empty-state');
  });

  it('语义是“加载完成但无数据”，与 Loading 共享 .empty 观感', () => {
    const { container } = render(<EmptyState message="暂无数据" />);
    const node = container.firstElementChild as HTMLElement;
    expect(node).toHaveClass('empty', 'empty-state');
  });
});

describe('Badge — 默认 tone 的精确 class 契约', () => {
  it('默认 tone 输出的 className 精确为 "badge"，无尾随空格', () => {
    const { getByText } = render(<Badge>Default</Badge>);
    // 按 role/text 定位（span 无隐式 role，退化为 text 匹配）
    const span = getByText('Default');
    expect(span.className).toBe('badge');
  });

  it('非默认 tone 输出 "badge <tone>"，同样无尾随空格', () => {
    const { getByText } = render(<Badge tone="warn">接近上限</Badge>);
    expect(getByText('接近上限').className).toBe('badge warn');
  });
});

describe('Stat — value 接受 ReactNode', () => {
  it('value 传 JSX 片段能渲染（SettingsPage 预算格的使用方式）', () => {
    const { container } = render(
      <Stat value={<><span>128</span> 分</>} label="今日已用" />,
    );
    const value = container.querySelector('.value');
    expect(value).toHaveTextContent('128 分');
  });
});

describe('ConfirmBar — 行为契约', () => {
  it('点击主操作触发 onConfirm', () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    const { getByRole } = render(
      <ConfirmBar message="确定删除？" onConfirm={onConfirm} onCancel={onCancel} />,
    );
    fireEvent.click(getByRole('button', { name: '确认' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('点击取消触发 onCancel', () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    const { getByRole } = render(
      <ConfirmBar message="确定删除？" onConfirm={onConfirm} onCancel={onCancel} />,
    );
    fireEvent.click(getByRole('button', { name: '取消' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it('busy 时主操作禁用并切换为 busyLabel，根元素保留 confirm-bar 类', () => {
    const onConfirm = vi.fn();
    const { getByRole, container } = render(
      <ConfirmBar
        message="确定删除？"
        onConfirm={onConfirm}
        onCancel={() => {}}
        busy
        confirmLabel="确认删除"
        busyLabel="删除中…"
      />,
    );
    const confirm = getByRole('button', { name: '删除中…' }) as HTMLButtonElement;
    expect(confirm).toBeDisabled();
    expect(getByRole('button', { name: '取消' })).toBeEnabled();
    expect(container.firstElementChild).toHaveClass('confirm-bar');
    // 破坏性主操作按 button.danger 解析（C1 compound_mismatch 的根修）
    expect(confirm.className).toBe('danger');
  });

  it('cancelDisabled 时取消按钮禁用（SettingsPage 恢复条的站点行为）', () => {
    const { getByRole } = render(
      <ConfirmBar
        message="确定恢复？"
        onConfirm={() => {}}
        onCancel={() => {}}
        busy
        cancelDisabled
      />,
    );
    expect(getByRole('button', { name: '取消' })).toBeDisabled();
  });
});
