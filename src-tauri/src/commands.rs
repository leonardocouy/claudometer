use crate::claude::{ClaudeApiClient, ClaudeWebErrorStatus};
use crate::redact::redact_session_key;
use crate::settings::{
  SettingsStore, KEY_AUTOSTART_ENABLED, KEY_CHECK_UPDATES_ON_STARTUP, KEY_NOTIFY_ON_USAGE_RESET,
  KEY_REFRESH_INTERVAL_SECONDS, KEY_REMEMBER_SESSION_KEY, KEY_SELECTED_ORGANIZATION_ID,
};
use crate::tray::TrayUi;
use crate::types::{
  ClaudeOrganization, ClaudeUsageSnapshot, IpcResult, SaveSettingsPayload, SettingsState,
  UsageStatus,
};
use crate::updater;
use crate::windows::open_settings_window;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, EventTarget, Runtime, State};
use tauri_plugin_autostart::ManagerExt as _;
use tokio::sync::{mpsc, oneshot, Mutex};

const SNAPSHOT_EVENT: &str = "snapshot:updated";

const KEYRING_SERVICE: &str = "com.softaworks.claudometer";
const KEYRING_USER: &str = "claude_session_key";

#[derive(Clone)]
pub struct SessionKeyManager {
  entry: Option<keyring::Entry>,
  in_memory: Arc<Mutex<Option<String>>>,
}

impl SessionKeyManager {
  pub fn new() -> Self {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok();
    Self {
      entry,
      in_memory: Arc::new(Mutex::new(None)),
    }
  }

  pub fn is_available(&self) -> bool {
    self.entry.is_some()
  }

  pub async fn set_in_memory(&self, value: Option<String>) {
    let mut guard = self.in_memory.lock().await;
    *guard = value.and_then(|v| {
      let trimmed = v.trim().to_string();
      if trimmed.is_empty() {
        None
      } else {
        Some(trimmed)
      }
    });
  }

  pub async fn get_current(&self, remember: bool) -> Result<Option<String>, ()> {
    if let Some(value) = self.in_memory.lock().await.clone() {
      return Ok(Some(value));
    }

    if !remember {
      return Ok(None);
    }

    let Some(entry) = &self.entry else {
      return Err(());
    };

    match entry.get_password() {
      Ok(pwd) => {
        let trimmed = pwd.trim().to_string();
        if trimmed.is_empty() {
          Ok(None)
        } else {
          self.set_in_memory(Some(trimmed.clone())).await;
          Ok(Some(trimmed))
        }
      }
      Err(_) => Ok(None),
    }
  }

  pub async fn remember(&self, session_key: &str) -> Result<(), ()> {
    let Some(entry) = &self.entry else {
      return Err(());
    };
    entry.set_password(session_key).map_err(|_| ())?;
    Ok(())
  }

  pub async fn delete_persisted(&self) -> Result<(), ()> {
    if let Some(entry) = &self.entry {
      let _ = entry.delete_password();
    }
    Ok(())
  }

  pub async fn forget_all(&self) -> Result<(), ()> {
    let _ = self.delete_persisted().await;
    self.set_in_memory(None).await;
    Ok(())
  }
}

#[derive(Clone)]
pub struct RefreshBus {
  tx: mpsc::UnboundedSender<RefreshRequest>,
}

pub(crate) struct RefreshRequest {
  respond_to: Option<oneshot::Sender<IpcResult<()>>>,
}

impl RefreshBus {
  pub(crate) fn new(tx: mpsc::UnboundedSender<RefreshRequest>) -> Self {
    Self { tx }
  }

  pub fn trigger(&self) {
    let _ = self.tx.send(RefreshRequest { respond_to: None });
  }

  pub async fn refresh_now(&self) -> IpcResult<()> {
    let (tx, rx) = oneshot::channel();
    if self.tx.send(RefreshRequest { respond_to: Some(tx) }).is_err() {
      return IpcResult::err("UNKNOWN", "Refresh loop is not available.");
    }
    rx.await.unwrap_or_else(|_| IpcResult::err("UNKNOWN", "Refresh loop failed."))
  }
}

#[derive(Clone)]
pub struct AppState<R: Runtime> {
  pub settings: SettingsStore<R>,
  pub session_key: SessionKeyManager,
  pub claude: Arc<ClaudeApiClient>,
  pub organizations: Arc<Mutex<Vec<ClaudeOrganization>>>,
  pub latest_snapshot: Arc<Mutex<Option<ClaudeUsageSnapshot>>>,
  pub tray: TrayUi<R>,
  pub refresh: RefreshBus,
}

impl<R: Runtime> AppState<R> {
  pub async fn update_snapshot(&self, app: &AppHandle<R>, snapshot: Option<ClaudeUsageSnapshot>) {
    {
      let mut guard = self.latest_snapshot.lock().await;
      *guard = snapshot.clone();
    }

    self.tray.update_snapshot(snapshot.as_ref());
    let _ = app.emit_to(EventTarget::any(), SNAPSHOT_EVENT, snapshot);
  }

  pub fn remember_session_key(&self) -> bool {
    self.settings.get_bool(KEY_REMEMBER_SESSION_KEY, false)
  }

  pub fn selected_org_id(&self) -> Option<String> {
    self.settings.get_string(KEY_SELECTED_ORGANIZATION_ID)
  }
}

fn now_iso() -> String {
  time::OffsetDateTime::now_utc()
    .format(&time::format_description::well_known::Rfc3339)
    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn missing_key_snapshot() -> ClaudeUsageSnapshot {
  ClaudeUsageSnapshot::MissingKey {
    organization_id: None,
    last_updated_at: now_iso(),
    error_message: Some("Session key is not configured.".to_string()),
  }
}

fn unauthorized_snapshot(message: &str) -> ClaudeUsageSnapshot {
  ClaudeUsageSnapshot::Unauthorized {
    organization_id: None,
    last_updated_at: now_iso(),
    error_message: Some(message.to_string()),
  }
}

fn rate_limited_snapshot(message: &str) -> ClaudeUsageSnapshot {
  ClaudeUsageSnapshot::RateLimited {
    organization_id: None,
    last_updated_at: now_iso(),
    error_message: Some(message.to_string()),
  }
}

fn error_snapshot(message: &str) -> ClaudeUsageSnapshot {
  ClaudeUsageSnapshot::Error {
    organization_id: None,
    last_updated_at: now_iso(),
    error_message: Some(message.to_string()),
  }
}

fn compute_next_delay_ms<R: Runtime>(state: &AppState<R>, snapshot: &ClaudeUsageSnapshot) -> u64 {
  let base_seconds = state.settings.get_u64(KEY_REFRESH_INTERVAL_SECONDS, 60).max(10);
  let base_ms = base_seconds * 1000;

  let (base, ratio) = if snapshot.status() == UsageStatus::RateLimited {
    (5 * 60 * 1000, 0.2)
  } else {
    (base_ms, 0.1)
  };

  let nanos = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
  let frac = ((nanos % 1000) as f64) / 1000.0;
  let delta = (frac * 2.0 - 1.0) * (base as f64 * ratio);
  ((base as f64 + delta).max(1000.0)) as u64
}

async fn resolve_organization_id<R: Runtime>(
  state: &AppState<R>,
  session_key: &str,
) -> Result<Option<String>, ClaudeWebErrorStatus> {
  let orgs = state.claude.fetch_organizations_checked(session_key).await?;

  {
    let mut guard = state.organizations.lock().await;
    *guard = orgs.clone();
  }

  let stored = state.selected_org_id();
  if let Some(stored) = stored {
    if orgs.iter().any(|o| o.id == stored) {
      return Ok(Some(stored));
    }
  }

  let first = orgs.first().map(|o| o.id.clone());
  if let Some(id) = &first {
    state.settings.set(KEY_SELECTED_ORGANIZATION_ID, id.clone());
  }
  Ok(first)
}

async fn refresh_once<R: Runtime>(app: &AppHandle<R>, state: &AppState<R>) -> IpcResult<()> {
  let remember = state.remember_session_key();

  let session_key = match state.session_key.get_current(remember).await {
    Ok(Some(k)) => k,
    Ok(None) => {
      state.update_snapshot(app, Some(missing_key_snapshot())).await;
      return IpcResult::ok(());
    }
    Err(()) => {
      state.update_snapshot(app, Some(missing_key_snapshot())).await;
      return IpcResult::err("KEYRING", "OS keychain/secret service is unavailable.");
    }
  };

  let org_id = match resolve_organization_id(state, &session_key).await {
    Ok(Some(id)) => id,
    Ok(None) => {
      state
        .update_snapshot(app, Some(error_snapshot("No organizations found.")))
        .await;
      return IpcResult::ok(());
    }
    Err(ClaudeWebErrorStatus::Unauthorized) => {
      state
        .update_snapshot(app, Some(unauthorized_snapshot("Unauthorized.")))
        .await;
      return IpcResult::ok(());
    }
    Err(ClaudeWebErrorStatus::RateLimited) => {
      state
        .update_snapshot(app, Some(rate_limited_snapshot("Rate limited.")))
        .await;
      return IpcResult::ok(());
    }
    Err(ClaudeWebErrorStatus::Error) => {
      state
        .update_snapshot(app, Some(error_snapshot("Failed to fetch organizations.")))
        .await;
      return IpcResult::ok(());
    }
  };

  let snapshot = state.claude.fetch_usage_snapshot(&session_key, &org_id).await;
  state.update_snapshot(app, Some(snapshot)).await;
  IpcResult::ok(())
}

pub(crate) fn spawn_refresh_loop<R: Runtime>(
  app: AppHandle<R>,
  state: AppState<R>,
  mut rx: mpsc::UnboundedReceiver<RefreshRequest>,
) {
  tauri::async_runtime::spawn(async move {
    // Trigger an initial refresh attempt.
    let mut next_delay_ms: Option<u64> = Some(0);

    loop {
      if let Some(delay_ms) = next_delay_ms {
        tokio::select! {
          req = rx.recv() => {
            if req.is_none() { break; }
            let req = req.unwrap();
            let result = refresh_once(&app, &state).await;
            let latest = state.latest_snapshot.lock().await.clone();
            if let Some(snapshot) = latest {
              let paused = matches!(snapshot.status(), UsageStatus::MissingKey | UsageStatus::Unauthorized);
              next_delay_ms = if paused { None } else { Some(compute_next_delay_ms(&state, &snapshot)) };
            } else {
              next_delay_ms = Some(60_000);
            }
            if let Some(tx) = req.respond_to {
              let _ = tx.send(result);
            }
          }
          _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
            let _ = refresh_once(&app, &state).await;
            let latest = state.latest_snapshot.lock().await.clone();
            if let Some(snapshot) = latest {
              let paused = matches!(snapshot.status(), UsageStatus::MissingKey | UsageStatus::Unauthorized);
              next_delay_ms = if paused { None } else { Some(compute_next_delay_ms(&state, &snapshot)) };
            } else {
              next_delay_ms = Some(60_000);
            }
          }
        }
      } else {
        let req = rx.recv().await;
        if req.is_none() { break; }
        let req = req.unwrap();
        let result = refresh_once(&app, &state).await;
        let latest = state.latest_snapshot.lock().await.clone();
        if let Some(snapshot) = latest {
          let paused = matches!(snapshot.status(), UsageStatus::MissingKey | UsageStatus::Unauthorized);
          next_delay_ms = if paused { None } else { Some(compute_next_delay_ms(&state, &snapshot)) };
        } else {
          next_delay_ms = Some(60_000);
        }
        if let Some(tx) = req.respond_to {
          let _ = tx.send(result);
        }
      }
    }
  });
}

#[tauri::command]
pub async fn settings_get_state<R: Runtime>(
  app: AppHandle<R>,
  state: State<'_, AppState<R>>,
) -> SettingsState {
  let organizations = state.organizations.lock().await.clone();
  let latest_snapshot = state.latest_snapshot.lock().await.clone();

  let autostart_enabled = app
    .autolaunch()
    .is_enabled()
    .unwrap_or(state.settings.get_bool(KEY_AUTOSTART_ENABLED, false));

  SettingsState {
    remember_session_key: state.settings.get_bool(KEY_REMEMBER_SESSION_KEY, false),
    refresh_interval_seconds: state.settings.get_u64(KEY_REFRESH_INTERVAL_SECONDS, 60),
    notify_on_usage_reset: state.settings.get_bool(KEY_NOTIFY_ON_USAGE_RESET, true),
    autostart_enabled,
    check_updates_on_startup: state.settings.get_bool(KEY_CHECK_UPDATES_ON_STARTUP, true),
    organizations,
    selected_organization_id: state.selected_org_id(),
    latest_snapshot,
    keyring_available: state.session_key.is_available(),
  }
}

#[tauri::command]
pub async fn settings_refresh_now<R: Runtime>(
  _app: AppHandle<R>,
  state: State<'_, AppState<R>>,
) -> IpcResult<()> {
  state.refresh.refresh_now().await
}

#[tauri::command]
pub async fn settings_forget_key<R: Runtime>(
  app: AppHandle<R>,
  state: State<'_, AppState<R>>,
) -> IpcResult<()> {
  let _ = state.session_key.forget_all().await;
  state.settings.set(KEY_REMEMBER_SESSION_KEY, false);
  state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
  {
    let mut guard = state.organizations.lock().await;
    guard.clear();
  }
  state.update_snapshot(&app, Some(missing_key_snapshot())).await;
  IpcResult::ok(())
}

#[tauri::command]
pub async fn settings_save<R: Runtime>(
  app: AppHandle<R>,
  state: State<'_, AppState<R>>,
  payload: SaveSettingsPayload,
) -> IpcResult<()> {
  if payload.refresh_interval_seconds < 10 {
    return IpcResult::err("VALIDATION", "Refresh interval must be >= 10 seconds.");
  }

  if payload.remember_session_key && !state.session_key.is_available() {
    return IpcResult::err(
      "KEYRING",
      "OS keychain/secret service is unavailable. Disable “Remember session key” to continue.",
    );
  }

  // Autostart
  if payload.autostart_enabled {
    let _ = app.autolaunch().enable();
  } else {
    let _ = app.autolaunch().disable();
  }

  state
    .settings
    .set(KEY_REFRESH_INTERVAL_SECONDS, payload.refresh_interval_seconds);
  state.settings.set(KEY_NOTIFY_ON_USAGE_RESET, payload.notify_on_usage_reset);
  state
    .settings
    .set(KEY_AUTOSTART_ENABLED, payload.autostart_enabled);
  state
    .settings
    .set(KEY_CHECK_UPDATES_ON_STARTUP, payload.check_updates_on_startup);

  let candidate_key = payload
    .session_key
    .as_deref()
    .map(|s| s.trim())
    .filter(|s| !s.is_empty());

  if let Some(candidate_key) = candidate_key {
    match state.claude.fetch_organizations_checked(candidate_key).await {
      Ok(orgs) => {
        if orgs.is_empty() {
          return IpcResult::err("VALIDATION", "No organizations found for this account.");
        }

        {
          let mut guard = state.organizations.lock().await;
          *guard = orgs.clone();
        }

        let desired = payload
          .selected_organization_id
          .as_deref()
          .map(|s| s.trim())
          .filter(|s| !s.is_empty())
          .map(|s| s.to_string())
          .or_else(|| state.selected_org_id());

        let resolved = desired
          .clone()
          .filter(|id| orgs.iter().any(|o| o.id == *id))
          .or_else(|| orgs.first().map(|o| o.id.clone()));

        if let Some(org_id) = resolved.clone() {
          state.settings.set(KEY_SELECTED_ORGANIZATION_ID, org_id);
        } else {
          state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
        }

        if payload.remember_session_key {
          if state.session_key.remember(candidate_key).await.is_err() {
            return IpcResult::err(
              "KEYRING",
              "Failed to store session key in OS keychain/secret service.",
            );
          }
          state
            .session_key
            .set_in_memory(Some(candidate_key.to_string()))
            .await;
        } else {
          state
            .session_key
            .set_in_memory(Some(candidate_key.to_string()))
            .await;
          let _ = state.session_key.delete_persisted().await;
        }

        state
          .settings
          .set(KEY_REMEMBER_SESSION_KEY, payload.remember_session_key);
      }
      Err(ClaudeWebErrorStatus::Unauthorized) => return IpcResult::err("UNAUTHORIZED", "Unauthorized."),
      Err(ClaudeWebErrorStatus::RateLimited) => return IpcResult::err("RATE_LIMITED", "Rate limited."),
      Err(ClaudeWebErrorStatus::Error) => return IpcResult::err("NETWORK", "Failed to validate session key."),
    }
  } else {
    if let Some(org_id) = payload
      .selected_organization_id
      .as_deref()
      .map(|s| s.trim())
      .filter(|s| !s.is_empty())
    {
      state.settings.set(KEY_SELECTED_ORGANIZATION_ID, org_id.to_string());
    }
    state
      .settings
      .set(KEY_REMEMBER_SESSION_KEY, payload.remember_session_key);
    if !payload.remember_session_key {
      let _ = state.session_key.delete_persisted().await;
    }
  }

  if payload.check_updates_on_startup {
    updater::check_for_updates_background(app.clone());
  }

  let _ = state.refresh.refresh_now().await;
  IpcResult::ok(())
}

#[tauri::command]
pub async fn open_settings<R: Runtime>(app: AppHandle<R>) -> IpcResult<()> {
  match open_settings_window(&app) {
    Ok(()) => IpcResult::ok(()),
    Err(e) => IpcResult::err("UNKNOWN", redact_session_key(&e.to_string()).to_string()),
  }
}

#[tauri::command]
pub async fn check_for_updates<R: Runtime>(app: AppHandle<R>) -> IpcResult<()> {
  updater::check_for_updates_now(app).await
}
