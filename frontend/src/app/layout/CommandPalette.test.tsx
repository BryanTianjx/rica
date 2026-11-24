import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { CommandPalette } from './CommandPalette';

describe('CommandPalette', () => {
  it('lists real navigation commands and filters by query', () => {
    render(<MemoryRouter><CommandPalette open onClose={() => {}} /></MemoryRouter>);
    expect(screen.getByRole('button', { name: /工作台/ })).toBeInTheDocument();
    fireEvent.change(screen.getByRole('textbox', { name: '搜索命令' }), {
      target: { value: '候选人' },
    });
    expect(screen.getByRole('button', { name: /候选人/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /工作台/ })).toBeNull();
  });

  it('calls onClose on Escape', () => {
    const onClose = vi.fn();
    render(<MemoryRouter><CommandPalette open onClose={onClose} /></MemoryRouter>);
    fireEvent.keyDown(screen.getByRole('textbox', { name: '搜索命令' }), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});
