/**
 * Phase 6 — Ctrl+K command palette.
 * Navigation-only; it never performs writes. Commands come from the real
 * route list, so it cannot expose a page without a backend-backed feature.
 */
import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { NAV_GROUPS } from '../navigation/navGroups';

const COMMANDS = NAV_GROUPS.flatMap((group) =>
  group.items.map((item) => ({ label: `${group.label} / ${item.label}`, to: item.to })),
);

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [query, setQuery] = useState('');
  const navigate = useNavigate();
  const results = useMemo(
    () =>
      COMMANDS.filter((command) =>
        command.label.toLowerCase().includes(query.trim().toLowerCase()),
      ).slice(0, 8),
    [query],
  );

  useEffect(() => {
    if (open) setQuery('');
  }, [open]);

  if (!open) return null;
  return (
    <div className="command-overlay" role="dialog" aria-modal="true" aria-label="全局命令">
      <div className="command-panel">
        <input
          autoFocus
          aria-label="搜索命令"
          placeholder="输入命令或页面名称…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Escape') onClose();
            if (event.key === 'Enter' && results[0]) {
              navigate(results[0].to);
              onClose();
            }
          }}
        />
        <ul>
          {results.map((command) => (
            <li key={command.to}>
              <button
                type="button"
                onClick={() => {
                  navigate(command.to);
                  onClose();
                }}
              >
                {command.label}
              </button>
            </li>
          ))}
        </ul>
        {results.length === 0 && <p className="muted small">没有匹配的命令</p>}
      </div>
    </div>
  );
}
