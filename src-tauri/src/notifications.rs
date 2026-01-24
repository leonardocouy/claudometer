use crate::provider_view::{view_claude, view_codex, ProviderOkView};
use crate::settings::{
    KEY_NOTIFY_ON_USAGE_RESET, KEY_SESSION_NEAR_LIMIT_NOTIFIED, KEY_SESSION_RESET_NOTIFIED,
    KEY_WEEKLY_NEAR_LIMIT_NOTIFIED, KEY_WEEKLY_RESET_NOTIFIED,
};
use crate::state::AppState;
use crate::types::{ClaudeUsageSnapshot, CodexUsageSnapshot, UsageSnapshotBundle};
use crate::usage_alerts::{
    decide_near_limit_alerts, decide_usage_resets, DecideNearLimitAlertsParams,
    DecideUsageResetsParams,
};
use serde_json::{Map as JsonMap, Value as JsonValue};
use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt as _;

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

fn read_period_id_map<R: Runtime>(state: &AppState<R>, key: &str) -> JsonMap<String, JsonValue> {
    match state.settings.get_json(key) {
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

async fn maybe_notify_ok_view<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState<R>,
    cur: ProviderOkView<'_>,
    prev_session: Option<f64>,
    prev_weekly: Option<f64>,
) {
    let session_map = read_period_id_map(state, KEY_SESSION_NEAR_LIMIT_NOTIFIED);
    let weekly_map = read_period_id_map(state, KEY_WEEKLY_NEAR_LIMIT_NOTIFIED);
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

    let notify_on_usage_reset = state.settings.get_bool(KEY_NOTIFY_ON_USAGE_RESET, false);

    let (last_seen_session, last_seen_weekly) = {
        let guard = state.reset_baseline_by_org.lock().await;
        let baseline = guard.get(cur.scope_id);
        (
            baseline.and_then(|b| b.session_period_id.clone()),
            baseline.and_then(|b| b.weekly_period_id.clone()),
        )
    };

    let session_reset_map = read_period_id_map(state, KEY_SESSION_RESET_NOTIFIED);
    let weekly_reset_map = read_period_id_map(state, KEY_WEEKLY_RESET_NOTIFIED);
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

pub async fn maybe_notify_usage_bundle<R: Runtime>(
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
                }) if organization_id == cur.scope_id => {
                    (Some(*session_percent), Some(*weekly_percent))
                }
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
