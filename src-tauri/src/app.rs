use crate::claude::ClaudeApiClient;
use crate::codex::CodexApiClient;
use crate::commands::{self, AppState, RefreshBus, SecretManager};
use crate::settings::SettingsStore;
use crate::tray::{self, TrayUi};
use std::collections::HashMap;
use tauri::Manager;
use tokio::sync::mpsc;

fn format_rfc3339(dt: time::OffsetDateTime) -> String {
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn debug_resets_at(base: time::OffsetDateTime) -> (String, String) {
    let session = base + time::Duration::hours(5);
    let weekly = base + time::Duration::days(7);
    (format_rfc3339(session), format_rfc3339(weekly))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::settings_get_state,
            commands::settings_save,
            commands::settings_forget_key,
            commands::settings_forget_claude_key,
            commands::settings_refresh_now,
            commands::open_settings,
            commands::check_for_updates,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Prevent closing the settings window from exiting the app
                if window.label() == crate::windows::SETTINGS_WINDOW_LABEL {
                    window.hide().unwrap_or_default();
                    api.prevent_close();
                }
            }
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                tray::ITEM_OPEN_SETTINGS => {
                    let _ = crate::windows::open_settings_window(app);
                }
                tray::ITEM_REFRESH_NOW => {
                    let refresh = app.state::<AppState<tauri::Wry>>().refresh.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = refresh.refresh_now().await;
                    });
                }
                tray::ITEM_CHECK_UPDATES => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::updater::check_for_updates_now(app).await;
                    });
                }
                tray::ITEM_DEBUG_SET_BELOW_LIMIT => {
                    let state = app.state::<AppState<tauri::Wry>>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        {
                            let mut guard = state.debug_override.lock().await;
                            guard.active = true;
                            guard.session_percent = 50.0;
                            guard.weekly_percent = 50.0;
                        }
                        let _ = state.refresh.refresh_now().await;
                    });
                }
                tray::ITEM_DEBUG_SET_NEAR_LIMIT => {
                    let state = app.state::<AppState<tauri::Wry>>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        // Ensure the near-limit notification triggers by simulating a
                        // transition from < 90% to >= 90%.
                        {
                            let mut guard = state.debug_override.lock().await;
                            guard.active = true;
                            guard.session_percent = 50.0;
                            guard.weekly_percent = 50.0;
                        }
                        let _ = state.refresh.refresh_now().await;

                        let (session_resets_at, weekly_resets_at) =
                            debug_resets_at(time::OffsetDateTime::now_utc());
                        {
                            let mut guard = state.debug_override.lock().await;
                            guard.active = true;
                            guard.session_percent = 95.0;
                            guard.weekly_percent = 95.0;
                            guard.session_resets_at = session_resets_at;
                            guard.weekly_resets_at = weekly_resets_at;
                        }
                        let _ = state.refresh.refresh_now().await;
                    });
                }
                tray::ITEM_DEBUG_BUMP_RESETS_AT => {
                    let state = app.state::<AppState<tauri::Wry>>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        // Ensure reset notifications trigger by recording a baseline and then
                        // changing the period identifiers.
                        let base = time::OffsetDateTime::now_utc();
                        let (session_resets_at_1, weekly_resets_at_1) = debug_resets_at(base);
                        let (session_resets_at_2, weekly_resets_at_2) =
                            debug_resets_at(base + time::Duration::seconds(1));
                        {
                            let mut guard = state.debug_override.lock().await;
                            guard.active = true;
                            guard.session_resets_at = session_resets_at_1;
                            guard.weekly_resets_at = weekly_resets_at_1;
                        }
                        let _ = state.refresh.refresh_now().await;

                        {
                            let mut guard = state.debug_override.lock().await;
                            guard.active = true;
                            guard.session_resets_at = session_resets_at_2;
                            guard.weekly_resets_at = weekly_resets_at_2;
                        }
                        let _ = state.refresh.refresh_now().await;
                    });
                }
                tray::ITEM_DEBUG_CLEAR_SIMULATION => {
                    let state = app.state::<AppState<tauri::Wry>>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        {
                            let mut guard = state.debug_override.lock().await;
                            guard.active = false;
                        }
                        let _ = state.refresh.refresh_now().await;
                    });
                }
                tray::ITEM_QUIT => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let app_handle = app.handle().clone();
            let settings = SettingsStore::new(&app_handle).map_err(|e| {
                let err: Box<dyn std::error::Error> = Box::new(e);
                tauri::Error::Setup(err.into())
            })?;

            let tray = TrayUi::new(&app_handle)?;

            let claude = ClaudeApiClient::new().map_err(|e| {
                let err: Box<dyn std::error::Error> = Box::new(e);
                tauri::Error::Setup(err.into())
            })?;

            let codex = CodexApiClient::new().map_err(|e| {
                let err: Box<dyn std::error::Error> = Box::new(e);
                tauri::Error::Setup(err.into())
            })?;

            let (tx, rx) = mpsc::unbounded_channel();
            let refresh = RefreshBus::new(tx);

            let state = AppState {
                settings: settings.clone(),
                claude_session_key: SecretManager::new(commands::KEYRING_USER_CLAUDE_SESSION_KEY),
                claude: std::sync::Arc::new(claude),
                codex: std::sync::Arc::new(codex),
                organizations: std::sync::Arc::new(tokio::sync::Mutex::new(vec![])),
                orgs_cache: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
                latest_snapshot: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
                reset_baseline_by_org: std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                debug_override: std::sync::Arc::new(tokio::sync::Mutex::new(
                    crate::commands::DebugOverride::default(),
                )),
                tray: tray.clone(),
                refresh: refresh.clone(),
            };

            state.tray.update_snapshot(
                state.track_claude_enabled(),
                state.track_codex_enabled(),
                None,
            );
            commands::spawn_refresh_loop(app_handle.clone(), state.clone(), rx);

            if settings.get_bool(crate::settings::KEY_CHECK_UPDATES_ON_STARTUP, true) {
                crate::updater::check_for_updates_background(app_handle.clone());
            }

            app.manage(state);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
