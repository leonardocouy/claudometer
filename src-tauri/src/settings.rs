use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tauri::Runtime;
use tauri_plugin_store::{JsonValue, Store, StoreBuilder};

const SETTINGS_STORE_FILE: &str = "claudometer-settings.json";

pub const KEY_REFRESH_INTERVAL_SECONDS: &str = "refreshIntervalSeconds";
pub const KEY_SELECTED_ORGANIZATION_ID: &str = "selectedOrganizationId";
pub const KEY_REMEMBER_SESSION_KEY: &str = "rememberSessionKey";
pub const KEY_NOTIFY_ON_USAGE_RESET: &str = "notifyOnUsageReset";
pub const KEY_AUTOSTART_ENABLED: &str = "autostartEnabled";
pub const KEY_CHECK_UPDATES_ON_STARTUP: &str = "checkUpdatesOnStartup";
pub const KEY_SESSION_NEAR_LIMIT_NOTIFIED: &str = "sessionNearLimitNotifiedPeriodIdByOrg";
pub const KEY_WEEKLY_NEAR_LIMIT_NOTIFIED: &str = "weeklyNearLimitNotifiedPeriodIdByOrg";
pub const KEY_SESSION_RESET_NOTIFIED: &str = "sessionResetNotifiedPeriodIdByOrg";
pub const KEY_WEEKLY_RESET_NOTIFIED: &str = "weeklyResetNotifiedPeriodIdByOrg";

fn defaults() -> HashMap<String, JsonValue> {
  HashMap::from([
    (KEY_REFRESH_INTERVAL_SECONDS.to_string(), json!(60)),
    (KEY_SELECTED_ORGANIZATION_ID.to_string(), json!("")),
    (KEY_REMEMBER_SESSION_KEY.to_string(), json!(false)),
    (KEY_NOTIFY_ON_USAGE_RESET.to_string(), json!(true)),
    (KEY_AUTOSTART_ENABLED.to_string(), json!(false)),
    (KEY_CHECK_UPDATES_ON_STARTUP.to_string(), json!(true)),
    (KEY_SESSION_NEAR_LIMIT_NOTIFIED.to_string(), json!({})),
    (KEY_WEEKLY_NEAR_LIMIT_NOTIFIED.to_string(), json!({})),
    (KEY_SESSION_RESET_NOTIFIED.to_string(), json!({})),
    (KEY_WEEKLY_RESET_NOTIFIED.to_string(), json!({})),
  ])
}

#[derive(Clone)]
pub struct SettingsStore<R: Runtime> {
  store: Arc<Store<R>>,
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
    self
      .store
      .get(key)
      .and_then(|v| v.as_u64())
      .unwrap_or(fallback)
  }

  pub fn get_bool(&self, key: &str, fallback: bool) -> bool {
    self
      .store
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

  pub fn set(&self, key: &str, value: impl Into<JsonValue>) {
    self.store.set(key.to_string(), value.into());
  }

  pub fn remove(&self, key: &str) {
    let _ = self.store.delete(key.to_string());
  }

  pub fn get_string_map(&self, key: &str) -> HashMap<String, String> {
    let Some(value) = self.store.get(key) else {
      return HashMap::new();
    };
    let Some(obj) = value.as_object() else {
      return HashMap::new();
    };
    obj.iter()
      .filter_map(|(k, v)| {
        let k = k.trim();
        let v = v.as_str()?.trim();
        if k.is_empty() || v.is_empty() {
          return None;
        }
        Some((k.to_string(), v.to_string()))
      })
      .collect()
  }

  pub fn set_string_map_entry(&self, key: &str, map_key: &str, map_value: &str) {
    let mk = map_key.trim();
    let mv = map_value.trim();
    if mk.is_empty() || mv.is_empty() {
      return;
    }
    let mut map = self.get_string_map(key);
    map.insert(mk.to_string(), mv.to_string());
    self.set(key, json!(map));
  }

  pub fn clear_string_map_entry(&self, key: &str, map_key: &str) {
    let mk = map_key.trim();
    if mk.is_empty() {
      return;
    }
    let mut map = self.get_string_map(key);
    map.remove(mk);
    self.set(key, json!(map));
  }
}

