// Shared UI barrel (Phase 1, batch 1).
//
// These are re-exports of components that already exist and are already used
// through their original modules. No new component names are introduced here:
// UI 2.0 calls `Loading` / `ErrorBox` / `EmptyState` by their real names rather
// than duplicating them under `LoadingState` / `ErrorState`.
export { Badge, StatusBadge, Loading, ErrorBox, EmptyState, Stat, LocalTime } from './ui';
export { ConfirmBar } from './ConfirmBar';
