use crate::redact::redact_session_key;
use crate::types::IpcResult;
use tauri::Runtime;
use tauri_plugin_notification::NotificationExt as _;
use tauri_plugin_updater::UpdaterExt as _;

pub fn check_for_updates_background<R: Runtime>(app: tauri::AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let _ = check_for_updates_startup(app).await;
    });
}

async fn notify<R: Runtime>(app: &tauri::AppHandle<R>, body: &str) {
    let _ = app
        .notification()
        .builder()
        .title("Claudometer")
        .body(body)
        .show();
}

pub async fn check_for_updates_startup<R: Runtime>(app: tauri::AppHandle<R>) -> IpcResult<()> {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => return IpcResult::err("UPDATER", redact_session_key(&e.to_string()).to_string()),
    };

    match updater.check().await {
        Ok(Some(update)) => {
            notify(
                &app,
                &format!(
                    "Update available: v{}. Use “Check for Updates…” to install.",
                    update.version
                ),
            )
            .await;
            IpcResult::ok(())
        }
        Ok(None) => IpcResult::ok(()),
        Err(e) => IpcResult::err("UPDATER", redact_session_key(&e.to_string()).to_string()),
    }
}

pub async fn check_for_updates_now<R: Runtime>(app: tauri::AppHandle<R>) -> IpcResult<()> {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => return IpcResult::err("UPDATER", redact_session_key(&e.to_string()).to_string()),
    };

    match updater.check().await {
        Ok(None) => {
            notify(&app, "You're up to date.").await;
            IpcResult::ok(())
        }
        Ok(Some(update)) => {
            notify(&app, &format!("Downloading update v{}…", update.version)).await;
            match update
                .download_and_install(|_chunk, _total| {}, || {})
                .await
            {
                Ok(()) => {
                    notify(&app, "Update installed. Restarting…").await;
                    IpcResult::ok(())
                }
                Err(e) => IpcResult::err("UPDATER", redact_session_key(&e.to_string()).to_string()),
            }
        }
        Err(e) => IpcResult::err("UPDATER", redact_session_key(&e.to_string()).to_string()),
    }
}
