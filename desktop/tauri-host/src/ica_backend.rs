// The host's IPC surface (F1-2 Positions slice): thin `#[tauri::command]`
// wrappers over `ica_desktop::commands`, which are thin wrappers over the
// application use cases. No contract logic here — the wrappers exist only to
// bridge the Tauri invoke boundary, exactly like the audit's "shell host will
// wrap one layer" ruling (tauri-architecture-audit R0.1).
//
// The commands return `IpcError` (Serialize), so a rejected invoke carries the
// SAME `{"error": {code, message, details}}` envelope the HTTP transport
// emits — the frontend's single error path keeps working unchanged.
//
// Positions only (F1-2 vertical slice): every other domain still rides HTTP
// through the spawned server until its F2 card switches it.

use ica_contracts::dto::PageResponse;
use ica_contracts::dto::PositionRead;
use ica_contracts::events::{PositionChangeKind, PositionChanged, POSITION_CHANGED};
use ica_desktop::commands;
use ica_desktop::error::IpcError;
use ica_desktop::state::DesktopState;
use tauri::{AppHandle, Emitter};

/// Announce a committed positions write to every window (F1-3).
///
/// The event is a notification only: a listener failing to receive it must
/// never turn an already-committed write into an error, so the emit result is
/// deliberately discarded. The payload is the transport-neutral contract type
/// from `ica-contracts`, the same shape the frontend listens for.
fn announce(app: &AppHandle, kind: PositionChangeKind, id: impl Into<String>) {
    let _ = app.emit(POSITION_CHANGED, PositionChanged::new(kind, id));
}

/// The host's in-process backend state: one `DesktopState` over the SAME
/// SQLite database the spawned ica-server serves — so an IPC write and an
/// HTTP read observe each other (F2's persistence-parity acceptance).
/// `DesktopState::from_store` assembles every port view from the one store,
/// mirroring ica-server's assembly one adapter at a time.
pub struct IcaState {
    inner: DesktopState,
}

impl IcaState {
    pub async fn connect(
        database_url: &str,
        config: ica_desktop::DesktopConfig,
    ) -> Result<Self, String> {
        Ok(Self {
            inner: DesktopState::connect(database_url, config).await?,
        })
    }

    pub fn state(&self) -> &DesktopState {
        &self.inner
    }
}

impl From<DesktopState> for IcaState {
    fn from(inner: DesktopState) -> Self {
        Self { inner }
    }
}

// ── Positions (F1-2 vertical slice) ────────────────────────────────────────

#[tauri::command]
pub async fn positions_list(
    state: tauri::State<'_, IcaState>,
    args: ica_desktop::commands::PositionListArgs,
) -> Result<PageResponse<PositionRead>, IpcError> {
    commands::positions_list(state.state(), args).await
}

#[tauri::command]
pub async fn positions_get(
    state: tauri::State<'_, IcaState>,
    id: String,
) -> Result<PositionRead, IpcError> {
    commands::positions_get(state.state(), &id).await
}

#[tauri::command]
pub async fn positions_create(
    state: tauri::State<'_, IcaState>,
    app: AppHandle,
    payload: serde_json::Value,
) -> Result<PositionRead, IpcError> {
    let created = commands::positions_create(state.state(), &payload).await?;
    announce(&app, PositionChangeKind::Created, created.id.clone());
    Ok(created)
}

#[tauri::command]
pub async fn positions_patch(
    state: tauri::State<'_, IcaState>,
    app: AppHandle,
    id: String,
    payload: serde_json::Value,
) -> Result<PositionRead, IpcError> {
    let updated = commands::positions_patch(state.state(), &id, &payload).await?;
    announce(&app, PositionChangeKind::Updated, id.clone());
    Ok(updated)
}

#[tauri::command]
pub async fn positions_delete(
    state: tauri::State<'_, IcaState>,
    app: AppHandle,
    id: String,
) -> Result<(), IpcError> {
    commands::positions_delete(state.state(), &id).await?;
    announce(&app, PositionChangeKind::Deleted, id.clone());
    Ok(())
}
