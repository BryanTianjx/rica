/** Phase 1 WP07 — top-level render-error boundary.
 * Keeps a render failure from blanking the whole desktop window; the real
 * error is logged and a retry button remounts the subtree.
 */
import { Component, type ErrorInfo, type ReactNode } from 'react';
import { ErrorBox } from '../../components/ui';

interface Props {
  children: ReactNode;
}
interface State {
  message: string | null;
}

export class AppErrorBoundary extends Component<Props, State> {
  state: State = { message: null };

  static getDerivedStateFromError(error: unknown): State {
    return { message: error instanceof Error ? error.message : '未知渲染错误' };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // eslint-disable-next-line no-console
    console.error('UI render error', error, info.componentStack);
  }

  private retry = () => this.setState({ message: null });

  render() {
    if (this.state.message) {
      return (
        <div className="render-failure">
          <ErrorBox message={`页面渲染失败：${this.state.message}`} />
          <button type="button" className="btn primary" onClick={this.retry}>
            重试
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
