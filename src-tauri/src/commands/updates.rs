use crate::redact::redact_secrets;
use crate::types::{IpcError, IpcErrorCode, IpcResult};
use crate::updater;
use crate::windows::open_settings_window;
use tauri::{AppHandle, Runtime};

type CommandResult<T> = Result<T, IpcError>;

#[tauri::command]
pub async fn open_settings<R: Runtime>(app: AppHandle<R>) -> CommandResult<IpcResult<()>> {
    Ok(match open_settings_window(&app) {
        Ok(()) => IpcResult::ok(()),
        Err(e) => IpcResult::err(
            IpcErrorCode::Unknown,
            redact_secrets(&e.to_string()).to_string(),
        ),
    })
}

#[tauri::command]
pub async fn check_for_updates<R: Runtime>(app: AppHandle<R>) -> CommandResult<IpcResult<()>> {
    Ok(updater::check_for_updates_now(app).await)
}
