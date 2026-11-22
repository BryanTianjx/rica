import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { LoginPage } from './LoginPage';

const login = vi.fn();
const logout = vi.fn();

vi.mock('../auth/AuthContext', () => ({
  useAuth: () => ({ status: 'unauthenticated', user: null, login, logout }),
}));

describe('LoginPage', () => {
  beforeEach(() => {
    login.mockReset();
    login.mockResolvedValue(undefined);
  });

  it('renders the login form', () => {
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );
    expect(screen.getByLabelText('邮箱')).toBeInTheDocument();
    expect(screen.getByLabelText('密码')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '登录' })).toBeInTheDocument();
  });

  it('submits email + password to login', async () => {
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );
    fireEvent.change(screen.getByLabelText('邮箱'), {
      target: { value: 'a@example.com' },
    });
    fireEvent.change(screen.getByLabelText('密码'), {
      target: { value: 'password123' },
    });
    fireEvent.click(screen.getByRole('button', { name: '登录' }));

    await waitFor(() => expect(login).toHaveBeenCalledWith('a@example.com', 'password123'));
  });

  it('shows the error message on a failed login', async () => {
    login.mockRejectedValue(new Error('Invalid email or password'));
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );
    fireEvent.change(screen.getByLabelText('邮箱'), {
      target: { value: 'a@example.com' },
    });
    fireEvent.change(screen.getByLabelText('密码'), {
      target: { value: 'password123' },
    });
    fireEvent.click(screen.getByRole('button', { name: '登录' }));

    expect(await screen.findByText('Invalid email or password')).toBeInTheDocument();
  });
});
