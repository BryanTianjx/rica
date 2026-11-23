import { describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { ToastProvider, ToastViewport, useToast } from './ToastViewport';

function Trigger() {
  const { push } = useToast();
  return <button type="button" onClick={() => push('配置已保存', 'ok')}>notify</button>;
}

describe('ToastViewport', () => {
  it('shows a pushed toast and lets the user dismiss it', async () => {
    vi.useFakeTimers();
    render(
      <ToastProvider>
        <Trigger />
        <ToastViewport />
      </ToastProvider>,
    );
    await act(async () => { screen.getByRole('button', { name: 'notify' }).click(); });
    expect(screen.getByText('配置已保存')).toBeInTheDocument();
    await act(async () => { screen.getByRole('button', { name: '关闭' }).click(); });
    expect(screen.queryByText('配置已保存')).toBeNull();
    vi.useRealTimers();
  });
});
