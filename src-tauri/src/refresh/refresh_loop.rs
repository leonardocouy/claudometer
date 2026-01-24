use super::fetch::{bundle, fetch_claude_snapshot, fetch_codex_snapshot};
use super::policy::compute_next_delay_for_latest;
use crate::notifications::maybe_notify_usage_bundle;
use crate::state::{AppState, RefreshRequest};
use crate::types::{IpcErrorCode, IpcResult};
use tauri::{AppHandle, Runtime};
use tokio::sync::mpsc;

async fn refresh_once<R: Runtime>(app: &AppHandle<R>, state: &AppState<R>) -> IpcResult<()> {
    let previous = state.latest_snapshot.lock().await.clone();
    let notify_claude = state.track_claude_enabled();
    let notify_codex = state.track_codex_enabled();

    let debug_bundle = {
        let guard = state.debug_override.lock().await;
        guard
            .active
            .then(|| guard.usage_bundle(state.track_claude_enabled(), state.track_codex_enabled()))
    };
    if let Some(snapshot) = debug_bundle {
        maybe_notify_usage_bundle(
            app,
            state,
            previous.as_ref(),
            &snapshot,
            notify_claude,
            notify_codex,
        )
        .await;
        state.update_snapshot(app, Some(snapshot)).await;
        return IpcResult::ok(());
    }

    let mut keyring_errors = 0_u8;

    let claude = if notify_claude {
        let result = fetch_claude_snapshot(state).await;
        if result.keyring_error {
            keyring_errors += 1;
        }
        Some(result.snapshot)
    } else {
        None
    };

    let codex = if notify_codex {
        let result = fetch_codex_snapshot(state).await;
        if result.keyring_error {
            keyring_errors += 1;
        }
        Some(result.snapshot)
    } else {
        None
    };

    let snapshot = bundle(claude, codex);
    maybe_notify_usage_bundle(
        app,
        state,
        previous.as_ref(),
        &snapshot,
        notify_claude,
        notify_codex,
    )
    .await;
    state.update_snapshot(app, Some(snapshot)).await;

    if keyring_errors > 0 {
        let enabled_providers = notify_claude as u8 + notify_codex as u8;
        if enabled_providers > 0 && keyring_errors >= enabled_providers {
            return IpcResult::err(
                IpcErrorCode::Keyring,
                "OS keychain/secret service is unavailable.",
            );
        }
    }

    IpcResult::ok(())
}

pub fn spawn_refresh_loop<R: Runtime>(
    app: AppHandle<R>,
    state: AppState<R>,
    mut rx: mpsc::UnboundedReceiver<RefreshRequest>,
) {
    tauri::async_runtime::spawn(async move {
        let mut next_delay_ms: Option<u64> = Some(0);

        loop {
            if let Some(delay_ms) = next_delay_ms {
                tokio::select! {
                  req = rx.recv() => {
                    if req.is_none() { break; }
                    let req = req.unwrap();
                    let result = refresh_once(&app, &state).await;
                    let latest = state.latest_snapshot.lock().await.clone();
                    next_delay_ms = compute_next_delay_for_latest(
                        state.track_claude_enabled(),
                        state.track_codex_enabled(),
                        state.refresh_interval_seconds(),
                        latest.as_ref(),
                    );
                    if let Some(tx) = req.respond_to {
                      let _ = tx.send(result);
                    }
                  }
                  _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                    let _ = refresh_once(&app, &state).await;
                    let latest = state.latest_snapshot.lock().await.clone();
                    next_delay_ms = compute_next_delay_for_latest(
                        state.track_claude_enabled(),
                        state.track_codex_enabled(),
                        state.refresh_interval_seconds(),
                        latest.as_ref(),
                    );
                  }
                }
            } else {
                let req = rx.recv().await;
                if req.is_none() {
                    break;
                }
                let req = req.unwrap();
                let result = refresh_once(&app, &state).await;
                let latest = state.latest_snapshot.lock().await.clone();
                next_delay_ms = compute_next_delay_for_latest(
                    state.track_claude_enabled(),
                    state.track_codex_enabled(),
                    state.refresh_interval_seconds(),
                    latest.as_ref(),
                );
                if let Some(tx) = req.respond_to {
                    let _ = tx.send(result);
                }
            }
        }
    });
}
