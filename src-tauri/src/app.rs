use crate::claude::ClaudeApiClient;
use crate::commands::{self, AppState, RefreshBus, SessionKeyManager};
use crate::settings::SettingsStore;
use crate::tray::{self, TrayUi};
use tauri::Manager;
use tokio::sync::mpsc;

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
      commands::settings_refresh_now,
      commands::open_settings,
      commands::check_for_updates,
    ])
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
      let settings = SettingsStore::new(&app_handle)
        .map_err(|e| {
          let err: Box<dyn std::error::Error> = Box::new(e);
          tauri::Error::Setup(err.into())
        })?;

      let tray = TrayUi::new(&app_handle)?;

      let claude = ClaudeApiClient::new()
        .map_err(|e| {
          let err: Box<dyn std::error::Error> = Box::new(e);
          tauri::Error::Setup(err.into())
        })?;

      let (tx, rx) = mpsc::unbounded_channel();
      let refresh = RefreshBus::new(tx);

      let state = AppState {
        settings: settings.clone(),
        session_key: SessionKeyManager::new(),
        claude: std::sync::Arc::new(claude),
        organizations: std::sync::Arc::new(tokio::sync::Mutex::new(vec![])),
        latest_snapshot: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        tray: tray.clone(),
        refresh: refresh.clone(),
      };

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
