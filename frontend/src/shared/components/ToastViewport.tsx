/** Phase 1 WP07 — global feedback surface.
 * Minimal by design: pages can push a toast through useToast(); no page is
 * force-migrated in Phase 1, so existing inline notices keep working.
 */
import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';

export interface Toast {
  id: number;
  message: string;
  tone: 'info' | 'ok' | 'bad';
}

interface ToastApi {
  toasts: Toast[];
  push: (message: string, tone?: Toast['tone']) => void;
  dismiss: (id: number) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

let nextId = 1;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const dismiss = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);
  const push = useCallback((message: string, tone: Toast['tone'] = 'info') => {
    const id = nextId++;
    setToasts((current) => [...current, { id, message, tone }]);
    window.setTimeout(() => dismiss(id), 6000);
  }, [dismiss]);
  const value = useMemo<ToastApi>(() => ({ toasts, push, dismiss }), [toasts, push, dismiss]);
  return <ToastContext.Provider value={value}>{children}</ToastContext.Provider>;
}

const NOOP_TOAST: ToastApi = {
  toasts: [],
  push: () => {},
  dismiss: () => {},
};

/**
 * Pages rendered on their own (unit tests, Storybook) have no ToastProvider;
 * degrade to a no-op so they stay renderable. Inside the App shell the real
 * provider supplies the global feedback surface.
 */
export function useToast(): ToastApi {
  return useContext(ToastContext) ?? NOOP_TOAST;
}

export function ToastViewport() {
  const context = useContext(ToastContext);
  if (!context) return null;
  return (
    <div className="toast-viewport" aria-live="polite">
      {context.toasts.map((toast) => (
        <div key={toast.id} className={`toast toast-${toast.tone}`}>
          <span>{toast.message}</span>
          <button type="button" className="link-btn" onClick={() => context.dismiss(toast.id)}>
            关闭
          </button>
        </div>
      ))}
    </div>
  );
}
