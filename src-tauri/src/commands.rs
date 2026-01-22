use crate::claude::{
    cli_credentials_available, read_cli_oauth_access_token, ClaudeApiClient, ClaudeWebErrorStatus,
    CliCredentialsError,
};
use crate::codex::{read_codex_oauth_credentials, CodexApiClient};
use crate::redact::redact_secrets;
use crate::settings::{
    SettingsStore, KEY_AUTOSTART_ENABLED, KEY_CHECK_UPDATES_ON_STARTUP, KEY_CODEX_USAGE_SOURCE,
    KEY_NOTIFY_ON_USAGE_RESET, KEY_REFRESH_INTERVAL_SECONDS, KEY_REMEMBER_CODEX_COOKIE,
    KEY_REMEMBER_SESSION_KEY, KEY_SELECTED_ORGANIZATION_ID, KEY_SESSION_NEAR_LIMIT_NOTIFIED,
    KEY_SESSION_RESET_NOTIFIED, KEY_TRACK_CLAUDE_ENABLED, KEY_TRACK_CODEX_ENABLED, KEY_USAGE_SOURCE,
    KEY_WEEKLY_NEAR_LIMIT_NOTIFIED, KEY_WEEKLY_RESET_NOTIFIED,
};
use crate::tray::TrayUi;
use crate::types::{
    ClaudeModelUsage, ClaudeOrganization, ClaudeUsageSnapshot, CodexUsageSnapshot, CodexUsageSource,
    IpcError, IpcResult, SaveSettingsPayload, SettingsState, UsageSnapshotBundle, UsageSource,
    UsageStatus,
};
use crate::updater;
use crate::usage_alerts::{
    decide_near_limit_alerts, decide_usage_resets, DecideNearLimitAlertsParams,
    DecideUsageResetsParams,
};
use crate::windows::open_settings_window;
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, EventTarget, Runtime, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::NotificationExt as _;
use tokio::sync::{mpsc, oneshot, Mutex};

const SNAPSHOT_EVENT: &str = "snapshot:updated";
const ORGS_CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

const KEYRING_SERVICE: &str = "com.softaworks.claudometer";
pub const KEYRING_USER_CLAUDE_SESSION_KEY: &str = "claude_session_key";
pub const KEYRING_USER_CODEX_COOKIE: &str = "codex_cookie";

type CommandResult<T> = Result<T, IpcError>;
type OrgsCacheEntry = (Vec<ClaudeOrganization>, Instant);
type OrgsCache = Option<OrgsCacheEntry>;

#[derive(Clone)]
pub struct SecretManager {
    user: &'static str,
    in_memory: Arc<Mutex<Option<String>>>,
}

impl SecretManager {
    pub fn new(user: &'static str) -> Self {
        Self {
            user,
            in_memory: Arc::new(Mutex::new(None)),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, keyring::Error> {
        keyring::Entry::new(KEYRING_SERVICE, self.user)
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
        if self
            .tx
            .send(RefreshRequest {
                respond_to: Some(tx),
            })
            .is_err()
        {
            return IpcResult::err("UNKNOWN", "Refresh loop is not available.");
        }
        rx.await
            .unwrap_or_else(|_| IpcResult::err("UNKNOWN", "Refresh loop failed."))
    }
}

pub struct AppState<R: Runtime> {
    pub settings: SettingsStore<R>,
    pub claude_session_key: SecretManager,
    pub codex_cookie: SecretManager,
    pub claude: Arc<ClaudeApiClient>,
    pub codex: Arc<CodexApiClient>,
    pub organizations: Arc<Mutex<Vec<ClaudeOrganization>>>,
    pub orgs_cache: Arc<Mutex<OrgsCache>>,
    pub latest_snapshot: Arc<Mutex<Option<UsageSnapshotBundle>>>,
    pub reset_baseline_by_org: Arc<Mutex<HashMap<String, UsageResetBaseline>>>,
    pub debug_override: Arc<Mutex<DebugOverride>>,
    pub tray: TrayUi<R>,
    pub refresh: RefreshBus,
}

impl<R: Runtime> Clone for AppState<R> {
    fn clone(&self) -> Self {
        Self {
            settings: self.settings.clone(),
            claude_session_key: self.claude_session_key.clone(),
            codex_cookie: self.codex_cookie.clone(),
            claude: self.claude.clone(),
            codex: self.codex.clone(),
            organizations: self.organizations.clone(),
            orgs_cache: self.orgs_cache.clone(),
            latest_snapshot: self.latest_snapshot.clone(),
            reset_baseline_by_org: self.reset_baseline_by_org.clone(),
            debug_override: self.debug_override.clone(),
            tray: self.tray.clone(),
            refresh: self.refresh.clone(),
        }
    }
}

impl<R: Runtime> AppState<R> {
    pub async fn get_organizations_cached(
        &self,
        session_key: &str,
    ) -> Result<Vec<ClaudeOrganization>, ClaudeWebErrorStatus> {
        // Check cache first
        {
            let cache = self.orgs_cache.lock().await;
            if let Some((orgs, fetched_at)) = cache.as_ref() {
                if fetched_at.elapsed() < ORGS_CACHE_TTL {
                    return Ok(orgs.clone());
                }
            }
        }

        // Fetch fresh organizations
        let orgs = self.claude.fetch_organizations_checked(session_key).await?;

        // Update cache
        {
            let mut cache = self.orgs_cache.lock().await;
            *cache = Some((orgs.clone(), Instant::now()));
        }

        Ok(orgs)
    }

    pub async fn invalidate_orgs_cache(&self) {
        let mut cache = self.orgs_cache.lock().await;
        *cache = None;
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
    fn claude_snapshot(&self) -> ClaudeUsageSnapshot {
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

    fn codex_snapshot(&self) -> CodexUsageSnapshot {
        CodexUsageSnapshot::Ok {
            session_percent: self.session_percent,
            session_resets_at: Some(self.session_resets_at.clone()),
            weekly_percent: self.weekly_percent,
            weekly_resets_at: Some(self.weekly_resets_at.clone()),
            last_updated_at: now_iso(),
        }
    }

    pub fn usage_bundle(&self, track_claude: bool, track_codex: bool) -> UsageSnapshotBundle {
        UsageSnapshotBundle {
            claude: track_claude.then(|| self.claude_snapshot()),
            codex: track_codex.then(|| self.codex_snapshot()),
        }
    }
}

impl<R: Runtime> AppState<R> {
    pub async fn update_snapshot(
        &self,
        app: &AppHandle<R>,
        snapshot: Option<UsageSnapshotBundle>,
    ) {
        {
            let mut guard = self.latest_snapshot.lock().await;
            *guard = snapshot.clone();
        }

        self.tray.update_snapshot(
            self.track_claude_enabled(),
            self.track_codex_enabled(),
            snapshot.as_ref(),
        );
        let _ = app.emit_to(EventTarget::any(), SNAPSHOT_EVENT, snapshot);
    }

    pub fn remember_session_key(&self) -> bool {
        self.settings.get_bool(KEY_REMEMBER_SESSION_KEY, false)
    }

    pub fn remember_codex_cookie(&self) -> bool {
        self.settings.get_bool(KEY_REMEMBER_CODEX_COOKIE, false)
    }

    pub fn track_claude_enabled(&self) -> bool {
        self.settings.get_bool(KEY_TRACK_CLAUDE_ENABLED, true)
    }

    pub fn track_codex_enabled(&self) -> bool {
        self.settings.get_bool(KEY_TRACK_CODEX_ENABLED, true)
    }

    pub fn selected_org_id(&self) -> Option<String> {
        self.settings.get_string(KEY_SELECTED_ORGANIZATION_ID)
    }

    pub fn usage_source(&self) -> UsageSource {
        if let Some(value) = self.settings.get_string(KEY_USAGE_SOURCE) {
            return if value == "cli" {
                UsageSource::Cli
            } else {
                UsageSource::Web
            };
        }

        if cli_credentials_available() {
            UsageSource::Cli
        } else {
            UsageSource::Web
        }
    }

    pub fn codex_usage_source(&self) -> CodexUsageSource {
        match self.settings.get_string(KEY_CODEX_USAGE_SOURCE).as_deref() {
            Some("oauth") => CodexUsageSource::Oauth,
            Some("web") => CodexUsageSource::Web,
            Some("cli") => CodexUsageSource::Cli,
            _ => CodexUsageSource::Auto,
        }
    }
}

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn bundle(
    claude: Option<ClaudeUsageSnapshot>,
    codex: Option<CodexUsageSnapshot>,
) -> UsageSnapshotBundle {
    UsageSnapshotBundle { claude, codex }
}

fn claude_missing_key_snapshot() -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::MissingKey {
        organization_id: None,
        last_updated_at: now_iso(),
        error_message: Some("Session key is not configured.".to_string()),
    }
}

fn claude_unauthorized_snapshot(message: &str) -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::Unauthorized {
        organization_id: None,
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn claude_rate_limited_snapshot(message: &str) -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::RateLimited {
        organization_id: None,
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn claude_error_snapshot(message: &str) -> ClaudeUsageSnapshot {
    ClaudeUsageSnapshot::Error {
        organization_id: None,
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn codex_missing_cookie_snapshot(message: &str) -> CodexUsageSnapshot {
    CodexUsageSnapshot::MissingKey {
        last_updated_at: now_iso(),
        error_message: Some(message.to_string()),
    }
}

fn read_period_id_map<R: Runtime>(
    settings: &SettingsStore<R>,
    key: &str,
) -> JsonMap<String, JsonValue> {
    match settings.get_json(key) {
        Some(JsonValue::Object(map)) => map,
        _ => JsonMap::new(),
    }
}

fn map_get_org_period_id(map: &JsonMap<String, JsonValue>, org_id: &str) -> Option<String> {
    map.get(org_id)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn map_set_org_period_id(map: &mut JsonMap<String, JsonValue>, org_id: &str, period_id: &str) {
    map.insert(org_id.to_string(), JsonValue::String(period_id.to_string()));
}

async fn notify_near_limit<R: Runtime>(app: &AppHandle<R>, body: &str) {
    let notification = app.notification().builder().title("Claudometer").body(body);

    #[cfg(target_os = "macos")]
    let notification = notification.sound("Ping");

    #[cfg(target_os = "linux")]
    let notification = notification.sound("bell");

    let _ = notification.show();
}

async fn notify_usage_reset<R: Runtime>(app: &AppHandle<R>, body: &str) {
    let notification = app.notification().builder().title("Claudometer").body(body);

    #[cfg(target_os = "macos")]
    let notification = notification.sound("Ping");

    #[cfg(target_os = "linux")]
    let notification = notification.sound("complete");

    let _ = notification.show();
}

struct OkView<'a> {
    provider_label: &'static str,
    scope_id: &'a str,
    session_percent: f64,
    weekly_percent: f64,
    session_resets_at: Option<&'a str>,
    weekly_resets_at: Option<&'a str>,
}

fn view_claude(snapshot: &ClaudeUsageSnapshot) -> Option<OkView<'_>> {
    match snapshot {
        ClaudeUsageSnapshot::Ok {
            organization_id,
            session_percent,
            session_resets_at,
            weekly_percent,
            weekly_resets_at,
            ..
        } => Some(OkView {
            provider_label: "Claude",
            scope_id: organization_id,
            session_percent: *session_percent,
            weekly_percent: *weekly_percent,
            session_resets_at: session_resets_at.as_deref(),
            weekly_resets_at: weekly_resets_at.as_deref(),
        }),
        _ => None,
    }
}

fn view_codex(snapshot: &CodexUsageSnapshot) -> Option<OkView<'_>> {
    match snapshot {
        CodexUsageSnapshot::Ok {
            session_percent,
            session_resets_at,
            weekly_percent,
            weekly_resets_at,
            ..
        } => Some(OkView {
            provider_label: "Codex",
            scope_id: "codex",
            session_percent: *session_percent,
            weekly_percent: *weekly_percent,
            session_resets_at: session_resets_at.as_deref(),
            weekly_resets_at: weekly_resets_at.as_deref(),
        }),
        _ => None,
    }
}

async fn maybe_notify_ok_view<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState<R>,
    cur: OkView<'_>,
    prev_session: Option<f64>,
    prev_weekly: Option<f64>,
) {
    // Near-limit (>= 90%) once per period per org.
    let session_map = read_period_id_map(&state.settings, KEY_SESSION_NEAR_LIMIT_NOTIFIED);
    let weekly_map = read_period_id_map(&state.settings, KEY_WEEKLY_NEAR_LIMIT_NOTIFIED);
    let last_session_notified = map_get_org_period_id(&session_map, cur.scope_id);
    let last_weekly_notified = map_get_org_period_id(&weekly_map, cur.scope_id);

    let decision = decide_near_limit_alerts(DecideNearLimitAlertsParams {
        current_session_percent: cur.session_percent,
        current_weekly_percent: cur.weekly_percent,
        current_session_resets_at: cur.session_resets_at,
        current_weekly_resets_at: cur.weekly_resets_at,
        previous_session_percent: prev_session,
        previous_weekly_percent: prev_weekly,
        last_notified_session_period_id: last_session_notified.as_deref(),
        last_notified_weekly_period_id: last_weekly_notified.as_deref(),
    });

    if let Some(session_period_id) = decision.session_period_id.as_deref() {
        notify_near_limit(
            app,
            &format!(
                "{} session usage is near the limit (>= 90%).",
                cur.provider_label
            ),
        )
        .await;
        let mut map = session_map;
        map_set_org_period_id(&mut map, cur.scope_id, session_period_id);
        state
            .settings
            .set(KEY_SESSION_NEAR_LIMIT_NOTIFIED, JsonValue::Object(map));
    }

    if let Some(weekly_period_id) = decision.weekly_period_id.as_deref() {
        notify_near_limit(
            app,
            &format!(
                "{} weekly usage is near the limit (>= 90%).",
                cur.provider_label
            ),
        )
        .await;
        let mut map = weekly_map;
        map_set_org_period_id(&mut map, cur.scope_id, weekly_period_id);
        state
            .settings
            .set(KEY_WEEKLY_NEAR_LIMIT_NOTIFIED, JsonValue::Object(map));
    }

    // Reset notifications (gated; no first-baseline notification).
    let notify_on_usage_reset = state.settings.get_bool(KEY_NOTIFY_ON_USAGE_RESET, false);

    let (last_seen_session, last_seen_weekly) = {
        let guard = state.reset_baseline_by_org.lock().await;
        let baseline = guard.get(cur.scope_id);
        (
            baseline.and_then(|b| b.session_period_id.clone()),
            baseline.and_then(|b| b.weekly_period_id.clone()),
        )
    };

    let session_reset_map = read_period_id_map(&state.settings, KEY_SESSION_RESET_NOTIFIED);
    let weekly_reset_map = read_period_id_map(&state.settings, KEY_WEEKLY_RESET_NOTIFIED);
    let last_session_reset_notified = map_get_org_period_id(&session_reset_map, cur.scope_id);
    let last_weekly_reset_notified = map_get_org_period_id(&weekly_reset_map, cur.scope_id);

    let reset_decision = decide_usage_resets(DecideUsageResetsParams {
        current_session_resets_at: cur.session_resets_at,
        current_weekly_resets_at: cur.weekly_resets_at,
        last_seen_session_period_id: last_seen_session.as_deref(),
        last_seen_weekly_period_id: last_seen_weekly.as_deref(),
        last_notified_session_reset_period_id: last_session_reset_notified.as_deref(),
        last_notified_weekly_reset_period_id: last_weekly_reset_notified.as_deref(),
    });

    if notify_on_usage_reset {
        if let Some(session_period_id) = reset_decision.session_reset_period_id.as_deref() {
            notify_usage_reset(
                app,
                &format!("{} session usage window has reset.", cur.provider_label),
            )
            .await;
            let mut map = session_reset_map;
            map_set_org_period_id(&mut map, cur.scope_id, session_period_id);
            state
                .settings
                .set(KEY_SESSION_RESET_NOTIFIED, JsonValue::Object(map));
        }

        if let Some(weekly_period_id) = reset_decision.weekly_reset_period_id.as_deref() {
            notify_usage_reset(
                app,
                &format!("{} weekly usage window has reset.", cur.provider_label),
            )
            .await;
            let mut map = weekly_reset_map;
            map_set_org_period_id(&mut map, cur.scope_id, weekly_period_id);
            state
                .settings
                .set(KEY_WEEKLY_RESET_NOTIFIED, JsonValue::Object(map));
        }
    }

    // Always update baseline after processing, so first observation never notifies.
    {
        let mut guard = state.reset_baseline_by_org.lock().await;
        let entry = guard.entry(cur.scope_id.to_string()).or_default();
        if let Some(s) = cur
            .session_resets_at
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            entry.session_period_id = Some(s.to_string());
        }
        if let Some(s) = cur
            .weekly_resets_at
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            entry.weekly_period_id = Some(s.to_string());
        }
    }
}

async fn maybe_notify_usage_bundle<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState<R>,
    previous: Option<&UsageSnapshotBundle>,
    current: &UsageSnapshotBundle,
    notify_claude: bool,
    notify_codex: bool,
) {
    if notify_claude {
        if let Some(cur) = current.claude.as_ref().and_then(view_claude) {
            let (prev_session, prev_weekly) = match previous.and_then(|p| p.claude.as_ref()) {
                Some(ClaudeUsageSnapshot::Ok {
                    organization_id,
                    session_percent,
                    weekly_percent,
                    ..
                }) if organization_id == cur.scope_id => (Some(*session_percent), Some(*weekly_percent)),
                _ => (None, None),
            };
            maybe_notify_ok_view(app, state, cur, prev_session, prev_weekly).await;
        }
    }

    if notify_codex {
        if let Some(cur) = current.codex.as_ref().and_then(view_codex) {
            let (prev_session, prev_weekly) = match previous.and_then(|p| p.codex.as_ref()) {
                Some(CodexUsageSnapshot::Ok {
                    session_percent,
                    weekly_percent,
                    ..
                }) => (Some(*session_percent), Some(*weekly_percent)),
                _ => (None, None),
            };
            maybe_notify_ok_view(app, state, cur, prev_session, prev_weekly).await;
        }
    }
}

fn compute_next_delay_ms<R: Runtime>(state: &AppState<R>, snapshot: &UsageSnapshotBundle) -> u64 {
    let base_seconds = state
        .settings
        .get_u64(KEY_REFRESH_INTERVAL_SECONDS, 60)
        .max(30);
    let base_ms = base_seconds * 1000;

    let any_rate_limited = snapshot
        .claude
        .as_ref()
        .is_some_and(|s| s.status() == UsageStatus::RateLimited)
        || snapshot
            .codex
            .as_ref()
            .is_some_and(|s| s.status() == UsageStatus::RateLimited);

    let (base, ratio) = if any_rate_limited {
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
    let orgs = state.get_organizations_cached(session_key).await?;

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

struct FetchSnapshot<T> {
    snapshot: T,
    keyring_error: bool,
}

async fn fetch_claude_snapshot<R: Runtime>(state: &AppState<R>) -> FetchSnapshot<ClaudeUsageSnapshot> {
    match state.usage_source() {
        UsageSource::Web => {
            let remember = state.remember_session_key();
            let session_key = match state.claude_session_key.get_current(remember).await {
                Ok(Some(k)) => k,
                Ok(None) => {
                    return FetchSnapshot {
                        snapshot: claude_missing_key_snapshot(),
                        keyring_error: false,
                    };
                }
                Err(()) => {
                    return FetchSnapshot {
                        snapshot: ClaudeUsageSnapshot::MissingKey {
                            organization_id: None,
                            last_updated_at: now_iso(),
                            error_message: Some(
                                "OS keychain/secret service is unavailable.".to_string(),
                            ),
                        },
                        keyring_error: true,
                    };
                }
            };

            let org_id = match resolve_organization_id(state, &session_key).await {
                Ok(Some(id)) => id,
                Ok(None) => {
                    return FetchSnapshot {
                        snapshot: claude_error_snapshot("No organizations found."),
                        keyring_error: false,
                    };
                }
                Err(ClaudeWebErrorStatus::Unauthorized) => {
                    return FetchSnapshot {
                        snapshot: claude_unauthorized_snapshot("Unauthorized."),
                        keyring_error: false,
                    };
                }
                Err(ClaudeWebErrorStatus::RateLimited) => {
                    return FetchSnapshot {
                        snapshot: claude_rate_limited_snapshot("Rate limited."),
                        keyring_error: false,
                    };
                }
                Err(ClaudeWebErrorStatus::Error) => {
                    return FetchSnapshot {
                        snapshot: claude_error_snapshot("Failed to fetch organizations."),
                        keyring_error: false,
                    };
                }
            };

            FetchSnapshot {
                snapshot: state.claude.fetch_usage_snapshot(&session_key, &org_id).await,
                keyring_error: false,
            }
        }
        UsageSource::Cli => {
            let access_token = match read_cli_oauth_access_token() {
                Ok(t) => t,
                Err(CliCredentialsError::HomeMissing) => {
                    return FetchSnapshot {
                        snapshot: claude_error_snapshot(
                            "HOME is not set; cannot locate CLI credentials.",
                        ),
                        keyring_error: false,
                    };
                }
                Err(CliCredentialsError::MissingFile | CliCredentialsError::MissingAccessToken) => {
                    return FetchSnapshot {
                        snapshot: claude_unauthorized_snapshot(
                            "Claude CLI credentials not found. Run `claude login` and try again.",
                        ),
                        keyring_error: false,
                    };
                }
                Err(CliCredentialsError::InvalidJson) => {
                    return FetchSnapshot {
                        snapshot: claude_unauthorized_snapshot(
                            "Claude CLI credentials are invalid. Re-authenticate (run `claude login`).",
                        ),
                        keyring_error: false,
                    };
                }
            };

            FetchSnapshot {
                snapshot: state.claude.fetch_oauth_usage_snapshot(&access_token).await,
                keyring_error: false,
            }
        }
    }
}

async fn fetch_codex_snapshot<R: Runtime>(state: &AppState<R>) -> FetchSnapshot<CodexUsageSnapshot> {
    let try_cli = state.codex.fetch_cli_usage_snapshot("codex");
    let try_web_cookie = async {
        let remember = state.remember_codex_cookie();
        match state.codex_cookie.get_current(remember).await {
            Ok(Some(v)) => Ok(Some(v)),
            Ok(None) => Ok(None),
            Err(()) => Err(()),
        }
    };

    match state.codex_usage_source() {
        CodexUsageSource::Oauth => match read_codex_oauth_credentials() {
            Ok(creds) => FetchSnapshot {
                snapshot: state
                    .codex
                    .fetch_oauth_usage_snapshot(&creds.access_token, creds.account_id.as_deref())
                    .await,
                keyring_error: false,
            },
            Err(_) => FetchSnapshot {
                snapshot: CodexUsageSnapshot::Unauthorized {
                    last_updated_at: now_iso(),
                    error_message: Some(
                        "Codex credentials not found. Run `codex` to log in.".to_string(),
                    ),
                },
                keyring_error: false,
            },
        },
        CodexUsageSource::Web => {
            let remember = state.remember_codex_cookie();
            let cookie = match state.codex_cookie.get_current(remember).await {
                Ok(Some(v)) => v,
                Ok(None) => {
                    return FetchSnapshot {
                        snapshot: codex_missing_cookie_snapshot("Codex cookie is not configured."),
                        keyring_error: false,
                    };
                }
                Err(()) => {
                    return FetchSnapshot {
                        snapshot: codex_missing_cookie_snapshot(
                            "OS keychain/secret service is unavailable.",
                        ),
                        keyring_error: true,
                    };
                }
            };

            FetchSnapshot {
                snapshot: state.codex.fetch_web_cookie_usage_snapshot(&cookie).await,
                keyring_error: false,
            }
        }
        CodexUsageSource::Cli => FetchSnapshot {
            snapshot: try_cli.await,
            keyring_error: false,
        },
        CodexUsageSource::Auto => {
            // OAuth → Web cookie → CLI
            if let Ok(creds) = read_codex_oauth_credentials() {
                let snap = state
                    .codex
                    .fetch_oauth_usage_snapshot(&creds.access_token, creds.account_id.as_deref())
                    .await;
                if snap.status() == UsageStatus::Ok {
                    return FetchSnapshot {
                        snapshot: snap,
                        keyring_error: false,
                    };
                }
            }

            match try_web_cookie.await {
                Ok(Some(cookie)) => {
                    let snap = state.codex.fetch_web_cookie_usage_snapshot(&cookie).await;
                    if snap.status() == UsageStatus::Ok {
                        return FetchSnapshot {
                            snapshot: snap,
                            keyring_error: false,
                        };
                    }
                }
                Ok(None) => {}
                Err(()) => {
                    // Keyring unavailable; still allow CLI fallback.
                    let cli = try_cli.await;
                    return FetchSnapshot {
                        snapshot: cli,
                        keyring_error: true,
                    };
                }
            }

            FetchSnapshot {
                snapshot: try_cli.await,
                keyring_error: false,
            }
        }
    }
}

async fn refresh_once<R: Runtime>(app: &AppHandle<R>, state: &AppState<R>) -> IpcResult<()> {
    let previous = state.latest_snapshot.lock().await.clone();
    let notify_claude = state.track_claude_enabled();
    let notify_codex = state.track_codex_enabled();

    let debug_bundle = {
        let guard = state.debug_override.lock().await;
        guard.active
            .then(|| guard.usage_bundle(state.track_claude_enabled(), state.track_codex_enabled()))
    };
    if let Some(snapshot) = debug_bundle {
        maybe_notify_usage_bundle(app, state, previous.as_ref(), &snapshot, notify_claude, notify_codex)
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
    maybe_notify_usage_bundle(app, state, previous.as_ref(), &snapshot, notify_claude, notify_codex).await;
    state.update_snapshot(app, Some(snapshot)).await;

    if keyring_errors > 0 {
        // Only return an error if every enabled provider needs the keyring.
        let enabled_providers = notify_claude as u8 + notify_codex as u8;
        if enabled_providers > 0 && keyring_errors >= enabled_providers {
            return IpcResult::err("KEYRING", "OS keychain/secret service is unavailable.");
        }
    }

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
                      let claude_paused = state.track_claude_enabled() && snapshot.claude.as_ref().is_some_and(|s| matches!(s.status(), UsageStatus::MissingKey | UsageStatus::Unauthorized));
                      let codex_paused = state.track_codex_enabled() && snapshot.codex.as_ref().is_some_and(|s| matches!(s.status(), UsageStatus::MissingKey | UsageStatus::Unauthorized));
                      let paused = if state.track_claude_enabled() && state.track_codex_enabled() {
                        claude_paused && codex_paused
                      } else {
                        claude_paused || codex_paused
                      };
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
                      let claude_paused = state.track_claude_enabled() && snapshot.claude.as_ref().is_some_and(|s| matches!(s.status(), UsageStatus::MissingKey | UsageStatus::Unauthorized));
                      let codex_paused = state.track_codex_enabled() && snapshot.codex.as_ref().is_some_and(|s| matches!(s.status(), UsageStatus::MissingKey | UsageStatus::Unauthorized));
                      let paused = if state.track_claude_enabled() && state.track_codex_enabled() {
                        claude_paused && codex_paused
                      } else {
                        claude_paused || codex_paused
                      };
                      next_delay_ms = if paused { None } else { Some(compute_next_delay_ms(&state, &snapshot)) };
                    } else {
                      next_delay_ms = Some(60_000);
                    }
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
                if let Some(snapshot) = latest {
                    let claude_paused = state.track_claude_enabled()
                        && snapshot.claude.as_ref().is_some_and(|s| {
                            matches!(
                                s.status(),
                                UsageStatus::MissingKey | UsageStatus::Unauthorized
                            )
                        });
                    let codex_paused = state.track_codex_enabled()
                        && snapshot.codex.as_ref().is_some_and(|s| {
                            matches!(
                                s.status(),
                                UsageStatus::MissingKey | UsageStatus::Unauthorized
                            )
                        });
                    let paused =
                        if state.track_claude_enabled() && state.track_codex_enabled() {
                            claude_paused && codex_paused
                        } else {
                            claude_paused || codex_paused
                        };
                    next_delay_ms = if paused {
                        None
                    } else {
                        Some(compute_next_delay_ms(&state, &snapshot))
                    };
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
    let track_claude_enabled = state.track_claude_enabled();
    let track_codex_enabled = state.track_codex_enabled();
    let organizations = if track_claude_enabled && matches!(usage_source, UsageSource::Web) {
        state.organizations.lock().await.clone()
    } else {
        vec![]
    };

    let autostart_enabled = app
        .autolaunch()
        .is_enabled()
        .unwrap_or(state.settings.get_bool(KEY_AUTOSTART_ENABLED, false));

    Ok(SettingsState {
        track_claude_enabled,
        track_codex_enabled,
        usage_source,
        remember_session_key: state.settings.get_bool(KEY_REMEMBER_SESSION_KEY, false),
        codex_usage_source: state.codex_usage_source(),
        remember_codex_cookie: state.settings.get_bool(KEY_REMEMBER_CODEX_COOKIE, false),
        refresh_interval_seconds: state.settings.get_u64(KEY_REFRESH_INTERVAL_SECONDS, 60),
        notify_on_usage_reset: state.settings.get_bool(KEY_NOTIFY_ON_USAGE_RESET, false),
        autostart_enabled,
        check_updates_on_startup: state.settings.get_bool(KEY_CHECK_UPDATES_ON_STARTUP, true),
        organizations,
        selected_organization_id: (track_claude_enabled && matches!(usage_source, UsageSource::Web))
            .then(|| state.selected_org_id())
            .flatten(),
        latest_snapshot,
        keyring_available: state.claude_session_key.is_available(),
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
    _app: AppHandle<R>,
    state: State<'_, AppState<R>>,
) -> CommandResult<IpcResult<()>> {
    let _ = state.claude_session_key.forget_all().await;
    let _ = state.codex_cookie.forget_all().await;
    state.settings.set(KEY_REMEMBER_SESSION_KEY, false);
    state.settings.set(KEY_REMEMBER_CODEX_COOKIE, false);
    state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
    {
        let mut guard = state.organizations.lock().await;
        guard.clear();
    }
    state.invalidate_orgs_cache().await;
    let _ = state.refresh.refresh_now().await;
    Ok(IpcResult::ok(()))
}

#[tauri::command]
pub async fn settings_forget_claude_key<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState<R>>,
) -> CommandResult<IpcResult<()>> {
    let _ = state.claude_session_key.forget_all().await;
    state.settings.set(KEY_REMEMBER_SESSION_KEY, false);
    state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
    {
        let mut guard = state.organizations.lock().await;
        guard.clear();
    }
    state.invalidate_orgs_cache().await;

    if state.track_claude_enabled() && matches!(state.usage_source(), UsageSource::Web) {
        let previous = state.latest_snapshot.lock().await.clone();
        let codex = previous.and_then(|b| b.codex);
        state
            .update_snapshot(&app, Some(bundle(Some(claude_missing_key_snapshot()), codex)))
            .await;
    }

    Ok(IpcResult::ok(()))
}

#[tauri::command]
pub async fn settings_forget_codex_cookie<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState<R>>,
) -> CommandResult<IpcResult<()>> {
    let _ = state.codex_cookie.forget_all().await;
    state.settings.set(KEY_REMEMBER_CODEX_COOKIE, false);

    if state.track_codex_enabled() && matches!(state.codex_usage_source(), CodexUsageSource::Web) {
        let previous = state.latest_snapshot.lock().await.clone();
        let claude = previous.and_then(|b| b.claude);
        state
            .update_snapshot(
                &app,
                Some(bundle(
                    claude,
                    Some(codex_missing_cookie_snapshot("Codex cookie is not configured.")),
                )),
            )
            .await;
    }

    Ok(IpcResult::ok(()))
}

#[tauri::command]
pub async fn settings_save<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState<R>>,
    payload: SaveSettingsPayload,
) -> CommandResult<IpcResult<()>> {
    if payload.refresh_interval_seconds < 30 {
        return Ok(IpcResult::err(
            "VALIDATION",
            "Refresh interval must be >= 30 seconds.",
        ));
    }

    let uses_claude = payload.track_claude_enabled;
    let uses_codex = payload.track_codex_enabled;

    if !uses_claude && !uses_codex {
        return Ok(IpcResult::err(
            "VALIDATION",
            "Enable at least one provider (Claude or Codex).",
        ));
    }

    state
        .settings
        .set(KEY_TRACK_CLAUDE_ENABLED, payload.track_claude_enabled);
    state
        .settings
        .set(KEY_TRACK_CODEX_ENABLED, payload.track_codex_enabled);

    state.settings.set(
        KEY_USAGE_SOURCE,
        match payload.usage_source {
            UsageSource::Web => "web",
            UsageSource::Cli => "cli",
        },
    );

    state.settings.set(
        KEY_CODEX_USAGE_SOURCE,
        match payload.codex_usage_source {
            CodexUsageSource::Auto => "auto",
            CodexUsageSource::Oauth => "oauth",
            CodexUsageSource::Web => "web",
            CodexUsageSource::Cli => "cli",
        },
    );

    if uses_claude
        && matches!(payload.usage_source, UsageSource::Web)
        && payload.remember_session_key
        && !state.claude_session_key.is_available()
    {
        return Ok(IpcResult::err(
            "KEYRING",
            "OS keychain/secret service is unavailable. Disable “Remember session key” to continue.",
        ));
    }

    if uses_codex
        && matches!(payload.codex_usage_source, CodexUsageSource::Web)
        && payload.remember_codex_cookie
        && !state.claude_session_key.is_available()
    {
        return Ok(IpcResult::err(
            "KEYRING",
            "OS keychain/secret service is unavailable. Disable “Remember” to continue.",
        ));
    }

    // Autostart
    if payload.autostart_enabled {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }

    state.settings.set(
        KEY_REFRESH_INTERVAL_SECONDS,
        payload.refresh_interval_seconds,
    );
    state
        .settings
        .set(KEY_NOTIFY_ON_USAGE_RESET, payload.notify_on_usage_reset);
    state
        .settings
        .set(KEY_AUTOSTART_ENABLED, payload.autostart_enabled);
    state.settings.set(
        KEY_CHECK_UPDATES_ON_STARTUP,
        payload.check_updates_on_startup,
    );

    state
        .settings
        .set(KEY_REMEMBER_SESSION_KEY, payload.remember_session_key);
    state
        .settings
        .set(KEY_REMEMBER_CODEX_COOKIE, payload.remember_codex_cookie);

    // Claude: organization selection is only meaningful for Web mode.
    if matches!(payload.usage_source, UsageSource::Cli) {
        state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
        {
            let mut guard = state.organizations.lock().await;
            guard.clear();
        }
        state.invalidate_orgs_cache().await;
    }

    if !uses_claude {
        // Preserve existing behavior when switching to Codex-only: clear Claude org selection.
        state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
        {
            let mut guard = state.organizations.lock().await;
            guard.clear();
        }
        state.invalidate_orgs_cache().await;
    }

    if uses_claude && matches!(payload.usage_source, UsageSource::Web) {
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
                    state.invalidate_orgs_cache().await;

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
                        if state.claude_session_key.remember(candidate_key).await.is_err() {
                            return Ok(IpcResult::err(
                                "KEYRING",
                                "Failed to store session key in OS keychain/secret service.",
                            ));
                        }
                        state
                            .claude_session_key
                            .set_in_memory(Some(candidate_key.to_string()))
                            .await;
                    } else {
                        state
                            .claude_session_key
                            .set_in_memory(Some(candidate_key.to_string()))
                            .await;
                        let _ = state.claude_session_key.delete_persisted().await;
                    }
                }
                Err(ClaudeWebErrorStatus::Unauthorized) => {
                    return Ok(IpcResult::err("UNAUTHORIZED", "Unauthorized."));
                }
                Err(ClaudeWebErrorStatus::RateLimited) => {
                    return Ok(IpcResult::err("RATE_LIMITED", "Rate limited."));
                }
                Err(ClaudeWebErrorStatus::Error) => {
                    return Ok(IpcResult::err("NETWORK", "Failed to validate session key."));
                }
            }
        } else {
            if let Some(org_id) = payload
                .selected_organization_id
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                state
                    .settings
                    .set(KEY_SELECTED_ORGANIZATION_ID, org_id.to_string());
            }
            if !payload.remember_session_key {
                let _ = state.claude_session_key.delete_persisted().await;
            }
        }
    }

    if uses_codex {
        let candidate_cookie = payload
            .codex_cookie
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        if let Some(candidate_cookie) = candidate_cookie {
            if payload.remember_codex_cookie {
                if state.codex_cookie.remember(candidate_cookie).await.is_err() {
                    return Ok(IpcResult::err(
                        "KEYRING",
                        "Failed to store cookie in OS keychain/secret service.",
                    ));
                }
                state
                    .codex_cookie
                    .set_in_memory(Some(candidate_cookie.to_string()))
                    .await;
            } else {
                state
                    .codex_cookie
                    .set_in_memory(Some(candidate_cookie.to_string()))
                    .await;
                let _ = state.codex_cookie.delete_persisted().await;
            }
        } else if !payload.remember_codex_cookie {
            let _ = state.codex_cookie.delete_persisted().await;
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
        Err(e) => IpcResult::err("UNKNOWN", redact_secrets(&e.to_string()).to_string()),
    })
}

#[tauri::command]
pub async fn check_for_updates<R: Runtime>(app: AppHandle<R>) -> CommandResult<IpcResult<()>> {
    Ok(updater::check_for_updates_now(app).await)
}
