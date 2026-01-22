use super::{RefreshBus, SecretManager};
use crate::claude::{cli_credentials_available, ClaudeApiClient, ClaudeWebErrorStatus};
use crate::codex::CodexApiClient;
use crate::settings::{
    SettingsStore, KEY_CODEX_USAGE_SOURCE, KEY_REFRESH_INTERVAL_SECONDS, KEY_REMEMBER_SESSION_KEY,
    KEY_SELECTED_ORGANIZATION_ID, KEY_TRACK_CLAUDE_ENABLED, KEY_TRACK_CODEX_ENABLED,
    KEY_USAGE_SOURCE,
};
use crate::tray::TrayUi;
use crate::types::{
    ClaudeModelUsage, ClaudeOrganization, ClaudeUsageSnapshot, CodexUsageSnapshot,
    CodexUsageSource, UsageSnapshotBundle, UsageSource,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, EventTarget, Runtime};
use tokio::sync::Mutex;

pub type OrgsCacheEntry = (Vec<ClaudeOrganization>, Instant);
pub type OrgsCache = Option<OrgsCacheEntry>;

const ORGS_CACHE_TTL_SECONDS: u64 = 300;

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
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

pub struct AppState<R: tauri::Runtime> {
    pub settings: SettingsStore<R>,
    pub claude_session_key: SecretManager,
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

impl<R: tauri::Runtime> Clone for AppState<R> {
    fn clone(&self) -> Self {
        Self {
            settings: self.settings.clone(),
            claude_session_key: self.claude_session_key.clone(),
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

impl<R: tauri::Runtime> AppState<R> {
    pub async fn get_organizations_cached(
        &self,
        session_key: &str,
    ) -> Result<Vec<ClaudeOrganization>, ClaudeWebErrorStatus> {
        {
            let cache = self.orgs_cache.lock().await;
            if let Some((orgs, fetched_at)) = cache.as_ref() {
                if fetched_at.elapsed().as_secs() < ORGS_CACHE_TTL_SECONDS {
                    return Ok(orgs.clone());
                }
            }
        }

        let orgs = self.claude.fetch_organizations_checked(session_key).await?;

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

    pub fn remember_session_key(&self) -> bool {
        self.settings.get_bool(KEY_REMEMBER_SESSION_KEY, false)
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
            Some("cli") => CodexUsageSource::Cli,
            _ => CodexUsageSource::Oauth,
        }
    }

    pub fn refresh_interval_seconds(&self) -> u64 {
        self.settings.get_u64(KEY_REFRESH_INTERVAL_SECONDS, 60)
    }
}

const SNAPSHOT_EVENT: &str = "snapshot:updated";

impl<R: Runtime> AppState<R> {
    pub async fn update_snapshot(&self, app: &AppHandle<R>, snapshot: Option<UsageSnapshotBundle>) {
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
}
