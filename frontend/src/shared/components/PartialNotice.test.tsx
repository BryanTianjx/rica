import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { PartialNotice } from './PartialNotice';

describe('PartialNotice', () => {
  it('stays silent when every source succeeded', () => {
    const { container } = render(
      <PartialNotice sources={[{ name: '岗位列表', error: null }]} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('names the failed source and offers retry', () => {
    const retry = vi.fn();
    render(
      <PartialNotice sources={[{ name: '岗位列表', error: 'timeout', retry }]} />,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('部分数据不可用');
    expect(screen.getByText(/岗位列表：timeout/)).toBeInTheDocument();
    screen.getByRole('button', { name: '重试' }).click();
    expect(retry).toHaveBeenCalledOnce();
  });
});
