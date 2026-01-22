use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tauri::Runtime;
use tauri_plugin_store::{JsonValue, Store, StoreBuilder};

const SETTINGS_STORE_FILE: &str = "claudometer-settings.json";

pub const KEY_REFRESH_INTERVAL_SECONDS: &str = "refreshIntervalSeconds";
pub const KEY_SELECTED_ORGANIZATION_ID: &str = "selectedOrganizationId";
pub const KEY_REMEMBER_SESSION_KEY: &str = "rememberSessionKey";
pub const KEY_NOTIFY_ON_USAGE_RESET: &str = "notifyOnUsageReset";
pub const KEY_USAGE_SOURCE: &str = "usageSource";
pub const KEY_PROVIDER: &str = "provider";
pub const KEY_CODEX_USAGE_SOURCE: &str = "codexUsageSource";
pub const KEY_REMEMBER_CODEX_COOKIE: &str = "rememberCodexCookie";
pub const KEY_DUAL_PROVIDER_MODE_ENABLED: &str = "dualProviderModeEnabled";
pub const KEY_TRACK_CLAUDE_ENABLED: &str = "trackClaudeEnabled";
pub const KEY_TRACK_CODEX_ENABLED: &str = "trackCodexEnabled";
pub const KEY_AUTOSTART_ENABLED: &str = "autostartEnabled";
pub const KEY_CHECK_UPDATES_ON_STARTUP: &str = "checkUpdatesOnStartup";
pub const KEY_SESSION_NEAR_LIMIT_NOTIFIED: &str = "sessionNearLimitNotifiedPeriodIdByOrg";
pub const KEY_WEEKLY_NEAR_LIMIT_NOTIFIED: &str = "weeklyNearLimitNotifiedPeriodIdByOrg";
pub const KEY_SESSION_RESET_NOTIFIED: &str = "sessionResetNotifiedPeriodIdByOrg";
pub const KEY_WEEKLY_RESET_NOTIFIED: &str = "weeklyResetNotifiedPeriodIdByOrg";

fn defaults() -> HashMap<String, JsonValue> {
    HashMap::from([
        (KEY_PROVIDER.to_string(), json!("claude")),
        (KEY_DUAL_PROVIDER_MODE_ENABLED.to_string(), json!(false)),
        (KEY_TRACK_CLAUDE_ENABLED.to_string(), json!(true)),
        (KEY_TRACK_CODEX_ENABLED.to_string(), json!(true)),
        (KEY_REFRESH_INTERVAL_SECONDS.to_string(), json!(60)),
        (KEY_SELECTED_ORGANIZATION_ID.to_string(), json!("")),
        (KEY_REMEMBER_SESSION_KEY.to_string(), json!(false)),
        (KEY_CODEX_USAGE_SOURCE.to_string(), json!("auto")),
        (KEY_REMEMBER_CODEX_COOKIE.to_string(), json!(false)),
        (KEY_NOTIFY_ON_USAGE_RESET.to_string(), json!(false)),
        (KEY_AUTOSTART_ENABLED.to_string(), json!(false)),
        (KEY_CHECK_UPDATES_ON_STARTUP.to_string(), json!(true)),
        (KEY_SESSION_NEAR_LIMIT_NOTIFIED.to_string(), json!({})),
        (KEY_WEEKLY_NEAR_LIMIT_NOTIFIED.to_string(), json!({})),
        (KEY_SESSION_RESET_NOTIFIED.to_string(), json!({})),
        (KEY_WEEKLY_RESET_NOTIFIED.to_string(), json!({})),
    ])
}

pub struct SettingsStore<R: Runtime> {
    store: Arc<Store<R>>,
}

impl<R: Runtime> Clone for SettingsStore<R> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<R: Runtime> SettingsStore<R> {
    pub fn new(app: &tauri::AppHandle<R>) -> tauri_plugin_store::Result<Self> {
        let store = StoreBuilder::new(app, SETTINGS_STORE_FILE)
            .defaults(defaults())
            .auto_save(Duration::from_millis(200))
            .build()?;
        Ok(Self { store })
    }

    pub fn get_u64(&self, key: &str, fallback: u64) -> u64 {
        self.store
            .get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(fallback)
    }

    pub fn get_bool(&self, key: &str, fallback: bool) -> bool {
        self.store
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(fallback)
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        let v = self.store.get(key)?;
        let s = v.as_str()?.trim();
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    }

    pub fn get_json(&self, key: &str) -> Option<JsonValue> {
        self.store.get(key)
    }

    pub fn set(&self, key: &str, value: impl Into<JsonValue>) {
        self.store.set(key.to_string(), value.into());
    }

    pub fn remove(&self, key: &str) {
        let _ = self.store.delete(key);
    }
}
