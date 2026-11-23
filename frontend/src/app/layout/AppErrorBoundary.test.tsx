import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { AppErrorBoundary } from './AppErrorBoundary';

function Boom(): never {
  throw new Error('render exploded');
}

describe('AppErrorBoundary', () => {
  it('renders a retry surface instead of a blank window', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(
      <AppErrorBoundary>
        <Boom />
      </AppErrorBoundary>,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('页面渲染失败');
    expect(screen.getByRole('button', { name: '重试' })).toBeInTheDocument();
    consoleError.mockRestore();
  });
});
