/**
 * V2.0 阶段 B: login page.
 *
 * Shown only when the backend is in multiuser mode and no valid session
 * exists. In local mode the `/auth/me` probe returns the synthetic local
 * user, so this page never renders.
 */

import { useState, type FormEvent } from 'react';
import { useAuth } from '../auth/AuthContext';
import { ErrorBox } from '../components/ui';

export function LoginPage() {
  const { login } = useAuth();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await login(email, password);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : '登录失败，请重试');
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="login-wrap">
      <div className="login-card">
        <div className="brand">
          <span className="brand-text">
            ICA Personal
            <small>本地招聘沟通助理</small>
          </span>
        </div>
        <form className="login-form" onSubmit={handleSubmit}>
          {error && <ErrorBox message={error} />}
          <div className="form-row two">
            <label htmlFor="login-email">邮箱</label>
            <input
              id="login-email"
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              autoComplete="username"
              required
            />
          </div>
          <div className="form-row two">
            <label htmlFor="login-password">密码</label>
            <input
              id="login-password"
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              autoComplete="current-password"
              required
            />
          </div>
          <div className="actions">
            <button type="submit" disabled={submitting}>
              {submitting ? '登录中…' : '登录'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
