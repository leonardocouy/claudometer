use crate::claude::ClaudeWebErrorStatus;
use crate::settings::{
    KEY_AUTOSTART_ENABLED, KEY_CHECK_UPDATES_ON_STARTUP, KEY_CODEX_USAGE_SOURCE,
    KEY_NOTIFY_ON_USAGE_RESET, KEY_REFRESH_INTERVAL_SECONDS, KEY_REMEMBER_SESSION_KEY,
    KEY_SELECTED_ORGANIZATION_ID, KEY_TRACK_CLAUDE_ENABLED, KEY_TRACK_CODEX_ENABLED,
    KEY_USAGE_SOURCE,
};
use crate::state::AppState;
use crate::types::{
    CodexUsageSource, IpcError, IpcErrorCode, IpcResult, SaveSettingsPayload, SettingsState,
    UsageSource,
};
use crate::updater;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_autostart::ManagerExt as _;

type CommandResult<T> = Result<T, IpcError>;

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
        refresh_interval_seconds: state
            .settings
            .get_u64(KEY_REFRESH_INTERVAL_SECONDS, 60)
            .min(u32::MAX as u64) as u32,
        notify_on_usage_reset: state.settings.get_bool(KEY_NOTIFY_ON_USAGE_RESET, false),
        autostart_enabled,
        check_updates_on_startup: state.settings.get_bool(KEY_CHECK_UPDATES_ON_STARTUP, true),
        organizations,
        selected_organization_id: (track_claude_enabled
            && matches!(usage_source, UsageSource::Web))
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
    state.settings.set(KEY_REMEMBER_SESSION_KEY, false);
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
            .update_snapshot(
                &app,
                Some(crate::refresh::bundle(
                    Some(crate::refresh::claude_missing_key_snapshot()),
                    codex,
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
            IpcErrorCode::Validation,
            "Refresh interval must be >= 30 seconds.",
        ));
    }

    let uses_claude = payload.track_claude_enabled;
    let uses_codex = payload.track_codex_enabled;

    if !uses_claude && !uses_codex {
        return Ok(IpcResult::err(
            IpcErrorCode::Validation,
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
            CodexUsageSource::Oauth => "oauth",
            CodexUsageSource::Cli => "cli",
        },
    );

    if uses_claude
        && matches!(payload.usage_source, UsageSource::Web)
        && payload.remember_session_key
        && !state.claude_session_key.is_available()
    {
        return Ok(IpcResult::err(
            IpcErrorCode::Keyring,
            "OS keychain/secret service is unavailable. Disable “Remember session key” to continue.",
        ));
    }

    if payload.autostart_enabled {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }

    state.settings.set(
        KEY_REFRESH_INTERVAL_SECONDS,
        payload.refresh_interval_seconds as u64,
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

    if matches!(payload.usage_source, UsageSource::Cli) {
        state.settings.remove(KEY_SELECTED_ORGANIZATION_ID);
        {
            let mut guard = state.organizations.lock().await;
            guard.clear();
        }
        state.invalidate_orgs_cache().await;
    }

    if !uses_claude {
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
            match state
                .claude
                .fetch_organizations_checked(candidate_key)
                .await
            {
                Ok(orgs) => {
                    if orgs.is_empty() {
                        return Ok(IpcResult::err(
                            IpcErrorCode::Validation,
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
                        if state
                            .claude_session_key
                            .remember(candidate_key)
                            .await
                            .is_err()
                        {
                            return Ok(IpcResult::err(
                                IpcErrorCode::Keyring,
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
                    return Ok(IpcResult::err(IpcErrorCode::Unauthorized, "Unauthorized."));
                }
                Err(ClaudeWebErrorStatus::RateLimited) => {
                    return Ok(IpcResult::err(IpcErrorCode::RateLimited, "Rate limited."));
                }
                Err(ClaudeWebErrorStatus::Error) => {
                    return Ok(IpcResult::err(
                        IpcErrorCode::Network,
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
                state
                    .settings
                    .set(KEY_SELECTED_ORGANIZATION_ID, org_id.to_string());
            }
            if !payload.remember_session_key {
                let _ = state.claude_session_key.delete_persisted().await;
            }
        }
    }

    if payload.check_updates_on_startup {
        updater::check_for_updates_background(app.clone());
    }

    let _ = state.refresh.refresh_now().await;
    Ok(IpcResult::ok(()))
}
