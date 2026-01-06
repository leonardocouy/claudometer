use crate::claude::{
  read_cli_oauth_access_token, CliCredentialsError, ClaudeApiClient, ClaudeWebErrorStatus,
};
use crate::redact::redact_session_key;
use crate::settings::{
  SettingsStore, KEY_AUTOSTART_ENABLED, KEY_CHECK_UPDATES_ON_STARTUP, KEY_NOTIFY_ON_USAGE_RESET,
  KEY_REFRESH_INTERVAL_SECONDS, KEY_REMEMBER_SESSION_KEY, KEY_SELECTED_ORGANIZATION_ID,
  KEY_SESSION_NEAR_LIMIT_NOTIFIED, KEY_SESSION_RESET_NOTIFIED, KEY_USAGE_SOURCE,
  KEY_WEEKLY_NEAR_LIMIT_NOTIFIED, KEY_WEEKLY_RESET_NOTIFIED,
};
use crate::tray::TrayUi;
use crate::types::{
  ClaudeModelUsage, ClaudeOrganization, ClaudeUsageSnapshot, IpcError, IpcResult, SaveSettingsPayload,
  SettingsState, UsageSource, UsageStatus,
};
use crate::updater;
use crate::usage_alerts::{decide_near_limit_alerts, decide_usage_resets, DecideNearLimitAlertsParams, DecideUsageResetsParams};
use crate::windows::open_settings_window;
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, EventTarget, Runtime, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::NotificationExt as _;
use tokio::sync::{mpsc, oneshot, Mutex};

const SNAPSHOT_EVENT: &str = "snapshot:updated";

const KEYRING_SERVICE: &str = "com.softaworks.claudometer";
const KEYRING_USER: &str = "claude_session_key";

type CommandResult<T> = Result<T, IpcError>;

#[derive(Clone)]
pub struct SessionKeyManager {
  in_memory: Arc<Mutex<Option<String>>>,
}

impl SessionKeyManager {
  pub fn new() -> Self {
    Self {
      in_memory: Arc::new(Mutex::new(None)),
    }
  }

  fn entry(&self) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
  }

  pub fn is_available(&self) -> bool {
    let Ok(entry) = self.entry() else {
      return false;
    };

    match entry.get_password() {
      Ok(_) => true,
      Err(keyring::Error::NoEntry) => true,
      Err(keyring::Error::BadEncoding(_)) => true,
      Err(keyring::Error::Ambiguous(_)) => true,
      Err(keyring::Error::NoStorageAccess(_)) => false,
      Err(keyring::Error::PlatformFailure(_)) => false,
      Err(_) => false,
    }
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

    let entry = self.entry().map_err(|_| ())?;

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
      Err(keyring::Error::NoEntry) => Ok(None),
      Err(keyring::Error::NoStorageAccess(_)) => Err(()),
      Err(keyring::Error::PlatformFailure(_)) => Err(()),
      Err(_) => Ok(None),
    }
  }

  pub async fn remember(&self, session_key: &str) -> Result<(), ()> {
    let entry = self.entry().map_err(|_| ())?;
    entry.set_password(session_key).map_err(|_| ())?;
    Ok(())
  }

  pub async fn delete_persisted(&self) -> Result<(), ()> {
    if let Ok(entry) = self.entry() {
      let _ = entry.delete_credential();
    };
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

  pub async fn refresh_now(&self) -> IpcResult<()> {
    let (tx, rx) = oneshot::channel();
    if self.tx.send(RefreshRequest { respond_to: Some(tx) }).is_err() {
      return IpcResult::err("UNKNOWN", "Refresh loop is not available.");
    }
    rx.await.unwrap_or_else(|_| IpcResult::err("UNKNOWN", "Refresh loop failed."))
  }
}

pub struct AppState<R: Runtime> {
  pub settings: SettingsStore<R>,
  pub session_key: SessionKeyManager,
  pub claude: Arc<ClaudeApiClient>,
  pub organizations: Arc<Mutex<Vec<ClaudeOrganization>>>,
  pub latest_snapshot: Arc<Mutex<Option<ClaudeUsageSnapshot>>>,
  pub reset_baseline_by_org: Arc<Mutex<HashMap<String, UsageResetBaseline>>>,
  pub debug_override: Arc<Mutex<DebugOverride>>,
  pub tray: TrayUi<R>,
  pub refresh: RefreshBus,
}

impl<R: Runtime> Clone for AppState<R> {
  fn clone(&self) -> Self {
    Self {
      settings: self.settings.clone(),
      session_key: self.session_key.clone(),
      claude: self.claude.clone(),
      organizations: self.organizations.clone(),
      latest_snapshot: self.latest_snapshot.clone(),
      reset_baseline_by_org: self.reset_baseline_by_org.clone(),
      debug_override: self.debug_override.clone(),
      tray: self.tray.clone(),
      refresh: self.refresh.clone(),
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct UsageResetBaseline {
  pub session_period_id: Option<String>,
  pub weekly_period_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DebugOverride {
  pub active: bool,
  pub organization_id: String,
  pub session_percent: f64,
  pub weekly_percent: f64,
  pub session_resets_at: String,
  pub weekly_resets_at: String,
}

impl Default for DebugOverride {
  fn default() -> Self {
    Self {
      active: false,
      organization_id: "debug".to_string(),
      session_percent: 50.0,
      weekly_percent: 50.0,
      session_resets_at: "2099-01-01T05:00:00.000Z".to_string(),
      weekly_resets_at: "2099-01-08T00:00:00.000Z".to_string(),
    }
  }
}

impl DebugOverride {
  pub fn snapshot(&self) -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::Ok {
      organization_id: self.organization_id.clone(),
      session_percent: self.session_percent,
      session_resets_at: Some(self.session_resets_at.clone()),
      weekly_percent: self.weekly_percent,
      weekly_resets_at: Some(self.weekly_resets_at.clone()),
      models: vec![
        ClaudeModelUsage {
          name: "Sonnet".to_string(),
          percent: self.weekly_percent,
          resets_at: Some(self.weekly_resets_at.clone()),
        },
        ClaudeModelUsage {
          name: "Opus".to_string(),
          percent: (self.weekly_percent * 0.7).clamp(0.0, 100.0),
          resets_at: Some(self.weekly_resets_at.clone()),
        },
      ],
      last_updated_at: now_iso(),
    }
  }
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

  pub fn usage_source(&self) -> UsageSource {
    match self.settings.get_string(KEY_USAGE_SOURCE).as_deref() {
      Some("cli") => UsageSource::Cli,
      _ => UsageSource::Web,
    }
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

fn read_period_id_map<R: Runtime>(settings: &SettingsStore<R>, key: &str) -> JsonMap<String, JsonValue> {
  match settings.get_json(key) {
    Some(JsonValue::Object(map)) => map,
    _ => JsonMap::new(),
  }
}

fn map_get_org_period_id(map: &JsonMap<String, JsonValue>, org_id: &str) -> Option<String> {
  map.get(org_id).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn map_set_org_period_id(map: &mut JsonMap<String, JsonValue>, org_id: &str, period_id: &str) {
  map.insert(org_id.to_string(), JsonValue::String(period_id.to_string()));
}

async fn notify<R: Runtime>(app: &AppHandle<R>, body: &str) {
  let _ = app
    .notification()
    .builder()
    .title("Claudometer")
    .body(body)
    .show();
}

async fn maybe_notify_usage<R: Runtime>(
  app: &AppHandle<R>,
  state: &AppState<R>,
  previous: Option<&ClaudeUsageSnapshot>,
  current: &ClaudeUsageSnapshot,
) {
  let ClaudeUsageSnapshot::Ok {
    organization_id,
    session_percent,
    session_resets_at,
    weekly_percent,
    weekly_resets_at,
    ..
  } = current
  else {
    return;
  };

  let (prev_session, prev_weekly) = match previous {
    Some(ClaudeUsageSnapshot::Ok {
      organization_id: prev_org,
      session_percent: ps,
      weekly_percent: pw,
      ..
    }) if prev_org == organization_id => (Some(*ps), Some(*pw)),
    _ => (None, None),
  };

  // Near-limit (>= 90%) once per period per org.
  let session_map = read_period_id_map(&state.settings, KEY_SESSION_NEAR_LIMIT_NOTIFIED);
  let weekly_map = read_period_id_map(&state.settings, KEY_WEEKLY_NEAR_LIMIT_NOTIFIED);
  let last_session_notified = map_get_org_period_id(&session_map, organization_id);
  let last_weekly_notified = map_get_org_period_id(&weekly_map, organization_id);

  let decision = decide_near_limit_alerts(DecideNearLimitAlertsParams {
    current_session_percent: *session_percent,
    current_weekly_percent: *weekly_percent,
    current_session_resets_at: session_resets_at.as_deref(),
    current_weekly_resets_at: weekly_resets_at.as_deref(),
    previous_session_percent: prev_session,
    previous_weekly_percent: prev_weekly,
    last_notified_session_period_id: last_session_notified.as_deref(),
    last_notified_weekly_period_id: last_weekly_notified.as_deref(),
  });

  if let Some(session_period_id) = decision.session_period_id.as_deref() {
    notify(app, "Session usage is near the limit (>= 90%).").await;
    let mut map = session_map;
    map_set_org_period_id(&mut map, organization_id, session_period_id);
    state
      .settings
      .set(KEY_SESSION_NEAR_LIMIT_NOTIFIED, JsonValue::Object(map));
  }

  if let Some(weekly_period_id) = decision.weekly_period_id.as_deref() {
    notify(app, "Weekly usage is near the limit (>= 90%).").await;
    let mut map = weekly_map;
    map_set_org_period_id(&mut map, organization_id, weekly_period_id);
    state
      .settings
      .set(KEY_WEEKLY_NEAR_LIMIT_NOTIFIED, JsonValue::Object(map));
  }

  // Reset notifications (gated; no first-baseline notification).
  let notify_on_usage_reset = state.settings.get_bool(KEY_NOTIFY_ON_USAGE_RESET, true);

  let (last_seen_session, last_seen_weekly) = {
    let guard = state.reset_baseline_by_org.lock().await;
    let baseline = guard.get(organization_id);
    (
      baseline.and_then(|b| b.session_period_id.clone()),
      baseline.and_then(|b| b.weekly_period_id.clone()),
    )
  };

  let session_reset_map = read_period_id_map(&state.settings, KEY_SESSION_RESET_NOTIFIED);
  let weekly_reset_map = read_period_id_map(&state.settings, KEY_WEEKLY_RESET_NOTIFIED);
  let last_session_reset_notified = map_get_org_period_id(&session_reset_map, organization_id);
  let last_weekly_reset_notified = map_get_org_period_id(&weekly_reset_map, organization_id);

  let reset_decision = decide_usage_resets(DecideUsageResetsParams {
    current_session_resets_at: session_resets_at.as_deref(),
    current_weekly_resets_at: weekly_resets_at.as_deref(),
    last_seen_session_period_id: last_seen_session.as_deref(),
    last_seen_weekly_period_id: last_seen_weekly.as_deref(),
    last_notified_session_reset_period_id: last_session_reset_notified.as_deref(),
    last_notified_weekly_reset_period_id: last_weekly_reset_notified.as_deref(),
  });

  if notify_on_usage_reset {
    if let Some(session_period_id) = reset_decision.session_reset_period_id.as_deref() {
      notify(app, "Session usage window has reset.").await;
      let mut map = session_reset_map;
      map_set_org_period_id(&mut map, organization_id, session_period_id);
      state
        .settings
        .set(KEY_SESSION_RESET_NOTIFIED, JsonValue::Object(map));
    }

    if let Some(weekly_period_id) = reset_decision.weekly_reset_period_id.as_deref() {
      notify(app, "Weekly usage window has reset.").await;
      let mut map = weekly_reset_map;
      map_set_org_period_id(&mut map, organization_id, weekly_period_id);
      state
        .settings
        .set(KEY_WEEKLY_RESET_NOTIFIED, JsonValue::Object(map));
    }
  }

  // Always update baseline after processing, so first observation never notifies.
  {
    let mut guard = state.reset_baseline_by_org.lock().await;
    let entry = guard.entry(organization_id.clone()).or_default();
    if let Some(s) = session_resets_at.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
      entry.session_period_id = Some(s.to_string());
    }
    if let Some(s) = weekly_resets_at.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
      entry.weekly_period_id = Some(s.to_string());
    }
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
  let previous = state.latest_snapshot.lock().await.clone();

  let debug_snapshot = {
    let guard = state.debug_override.lock().await;
    guard.active.then(|| guard.snapshot())
  };
  if let Some(snapshot) = debug_snapshot {
    maybe_notify_usage(app, state, previous.as_ref(), &snapshot).await;
    state.update_snapshot(app, Some(snapshot)).await;
    return IpcResult::ok(());
  }

  match state.usage_source() {
    UsageSource::Web => {
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
      maybe_notify_usage(app, state, previous.as_ref(), &snapshot).await;
      state.update_snapshot(app, Some(snapshot)).await;
      IpcResult::ok(())
    }
    UsageSource::Cli => {
      let access_token = match read_cli_oauth_access_token() {
        Ok(t) => t,
        Err(CliCredentialsError::HomeMissing) => {
          state
            .update_snapshot(app, Some(error_snapshot("HOME is not set; cannot locate CLI credentials.")))
            .await;
          return IpcResult::ok(());
        }
        Err(CliCredentialsError::MissingFile | CliCredentialsError::MissingAccessToken) => {
          state
            .update_snapshot(
              app,
              Some(unauthorized_snapshot(
                "Claude CLI credentials not found. Run `claude login` and try again.",
              )),
            )
            .await;
          return IpcResult::ok(());
        }
        Err(CliCredentialsError::InvalidJson) => {
          state
            .update_snapshot(
              app,
              Some(unauthorized_snapshot(
                "Claude CLI credentials are invalid. Re-authenticate (run `claude login`).",
              )),
            )
            .await;
          return IpcResult::ok(());
        }
      };

      let snapshot = state.claude.fetch_oauth_usage_snapshot(&access_token).await;
      maybe_notify_usage(app, state, previous.as_ref(), &snapshot).await;
      state.update_snapshot(app, Some(snapshot)).await;
      IpcResult::ok(())
    }
  }
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
) -> CommandResult<SettingsState> {
  let latest_snapshot = state.latest_snapshot.lock().await.clone();
  let usage_source = state.usage_source();
  let organizations = if matches!(usage_source, UsageSource::Web) {
    state.organizations.lock().await.clone()
  } else {
    vec![]
  };

  let autostart_enabled = app
    .autolaunch()
    .is_enabled()
    .unwrap_or(state.settings.get_bool(KEY_AUTOSTART_ENABLED, false));

  Ok(SettingsState {
    usage_source,
    remember_session_key: state.settings.get_bool(KEY_REMEMBER_SESSION_KEY, false),
    refresh_interval_seconds: state.settings.get_u64(KEY_REFRESH_INTERVAL_SECONDS, 60),
    notify_on_usage_reset: state.settings.get_bool(KEY_NOTIFY_ON_USAGE_RESET, true),
    autostart_enabled,
    check_updates_on_startup: state.settings.get_bool(KEY_CHECK_UPDATES_ON_STARTUP, true),
    organizations,
    selected_organization_id: matches!(usage_source, UsageSource::Web)
      .then(|| state.selected_org_id())
      .flatten(),
    latest_snapshot,
    keyring_available: state.session_key.is_available(),
  })
}

#[tauri::command]
pub async fn settings_refresh_now<R: Runtime>(
  _app: AppHandle<R>,
  state: State<'_, AppState<R>>,
) -> CommandResult<IpcResult<()>> {
  Ok(state.refresh.refresh_now().await)
}

#[tauri::command]
pub async fn settings_forget_key<R: Runtime>(
  app: AppHandle<R>,
  state: State<'_, AppState<R>>,
) -> CommandResult<IpcResult<()>> {
  let usage_source = state.usage_source();
  let _ = state.session_key.forget_all().await;
  state.settings.set(KEY_REMEMBER_SESSION_KEY, false);
  state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
  {
    let mut guard = state.organizations.lock().await;
    guard.clear();
  }
  if matches!(usage_source, UsageSource::Web) {
    state.update_snapshot(&app, Some(missing_key_snapshot())).await;
  } else {
    let _ = state.refresh.refresh_now().await;
  }
  Ok(IpcResult::ok(()))
}

#[tauri::command]
pub async fn settings_save<R: Runtime>(
  app: AppHandle<R>,
  state: State<'_, AppState<R>>,
  payload: SaveSettingsPayload,
) -> CommandResult<IpcResult<()>> {
  let usage_source = payload.usage_source;

  if payload.refresh_interval_seconds < 10 {
    return Ok(IpcResult::err(
      "VALIDATION",
      "Refresh interval must be >= 10 seconds.",
    ));
  }

  state.settings.set(
    KEY_USAGE_SOURCE,
    match usage_source {
      UsageSource::Web => "web",
      UsageSource::Cli => "cli",
    },
  );

  if matches!(usage_source, UsageSource::Web)
    && payload.remember_session_key
    && !state.session_key.is_available()
  {
    return Ok(IpcResult::err(
      "KEYRING",
      "OS keychain/secret service is unavailable. Disable “Remember session key” to continue.",
    ));
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

  if matches!(usage_source, UsageSource::Cli) {
    state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
    {
      let mut guard = state.organizations.lock().await;
      guard.clear();
    }

    if payload.check_updates_on_startup {
      updater::check_for_updates_background(app.clone());
    }

    let _ = state.refresh.refresh_now().await;
    return Ok(IpcResult::ok(()));
  }

  let candidate_key = payload
    .session_key
    .as_deref()
    .map(|s| s.trim())
    .filter(|s| !s.is_empty());

  if let Some(candidate_key) = candidate_key {
    match state.claude.fetch_organizations_checked(candidate_key).await {
      Ok(orgs) => {
        if orgs.is_empty() {
          return Ok(IpcResult::err(
            "VALIDATION",
            "No organizations found for this account.",
          ));
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
            return Ok(IpcResult::err(
              "KEYRING",
              "Failed to store session key in OS keychain/secret service.",
            ));
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
      Err(ClaudeWebErrorStatus::Unauthorized) => {
        return Ok(IpcResult::err("UNAUTHORIZED", "Unauthorized."));
      }
      Err(ClaudeWebErrorStatus::RateLimited) => {
        return Ok(IpcResult::err("RATE_LIMITED", "Rate limited."));
      }
      Err(ClaudeWebErrorStatus::Error) => {
        return Ok(IpcResult::err(
          "NETWORK",
          "Failed to validate session key.",
        ));
      }
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
  Ok(IpcResult::ok(()))
}

#[tauri::command]
pub async fn open_settings<R: Runtime>(app: AppHandle<R>) -> CommandResult<IpcResult<()>> {
  Ok(match open_settings_window(&app) {
    Ok(()) => IpcResult::ok(()),
    Err(e) => IpcResult::err("UNKNOWN", redact_session_key(&e.to_string()).to_string()),
  })
}

#[tauri::command]
pub async fn check_for_updates<R: Runtime>(app: AppHandle<R>) -> CommandResult<IpcResult<()>> {
  Ok(updater::check_for_updates_now(app).await)
}
