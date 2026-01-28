use super::formatters::{format_datetime_full, format_percent, format_reset_at_short};
use crate::types::{ClaudeUsageSnapshot, CodexUsageSnapshot, UsageSnapshotBundle, UsageStatus};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

use super::{
    ITEM_CHECK_UPDATES, ITEM_DEBUG_BUMP_RESETS_AT, ITEM_DEBUG_CLEAR_SIMULATION,
    ITEM_DEBUG_SET_BELOW_LIMIT, ITEM_DEBUG_SET_NEAR_LIMIT, ITEM_OPEN_SETTINGS, ITEM_QUIT,
    ITEM_REFRESH_NOW,
};

fn debug_menu_enabled() -> bool {
    matches!(
        std::env::var("CLAUDOMETER_DEBUG").as_deref(),
        Ok("1") | Ok("true") | Ok("yes") | Ok("on")
    )
}

pub(super) fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    track_claude: bool,
    track_codex: bool,
    snapshot: Option<&UsageSnapshotBundle>,
) -> tauri::Result<Menu<R>> {
    fn status_label_claude(status: UsageStatus) -> &'static str {
        match status {
            UsageStatus::Ok => "ok",
            UsageStatus::Unauthorized => "unauthorized",
            UsageStatus::RateLimited => "rate limited",
            UsageStatus::Error => "error",
            UsageStatus::MissingKey => "needs session key",
        }
    }

    fn status_label_codex(status: UsageStatus) -> &'static str {
        match status {
            UsageStatus::Ok => "ok",
            UsageStatus::Unauthorized => "unauthorized",
            UsageStatus::RateLimited => "rate limited",
            UsageStatus::Error => "error",
            UsageStatus::MissingKey => "missing credentials",
        }
    }

    let header_text = if track_claude && track_codex {
        "Claudometer - Usage".to_string()
    } else if track_claude {
        let status = snapshot
            .and_then(|s| s.claude.as_ref())
            .map(|c| status_label_claude(c.status()));
        match status {
            None => "Claudometer - Claude Usage (no data)".to_string(),
            Some("ok") => "Claudometer - Claude Usage".to_string(),
            Some(other) => format!("Claudometer - Claude Usage ({other})"),
        }
    } else if track_codex {
        let status = snapshot
            .and_then(|s| s.codex.as_ref())
            .map(|c| status_label_codex(c.status()));
        match status {
            None => "Claudometer - Codex Usage (no data)".to_string(),
            Some("ok") => "Claudometer - Codex Usage".to_string(),
            Some(other) => format!("Claudometer - Codex Usage ({other})"),
        }
    } else {
        "Claudometer - Usage (disabled)".to_string()
    };

    let header = MenuItem::with_id(app, "header", header_text, false, None::<&str>)?;

    let refresh_now = MenuItem::with_id(app, ITEM_REFRESH_NOW, "Refresh now", true, None::<&str>)?;
    let open_settings = MenuItem::with_id(
        app,
        ITEM_OPEN_SETTINGS,
        "Open Settings…",
        true,
        None::<&str>,
    )?;
    let check_updates = MenuItem::with_id(
        app,
        ITEM_CHECK_UPDATES,
        "Check for Updates…",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, ITEM_QUIT, "Quit", true, None::<&str>)?;

    let sep = PredefinedMenuItem::separator(app)?;
    let sep_between_sections = if track_claude && track_codex {
        Some(PredefinedMenuItem::separator(app)?)
    } else {
        None
    };
    let sep_before_actions = PredefinedMenuItem::separator(app)?;
    let sep_before_quit = PredefinedMenuItem::separator(app)?;

    let mut refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = vec![&header, &sep];

    let build_claude_items =
        |snap: Option<&ClaudeUsageSnapshot>| -> tauri::Result<Vec<MenuItem<R>>> {
            let status = snap.map(|s| s.status());
            let label = match status {
                None => "Claude (no data)".to_string(),
                Some(UsageStatus::Ok) => "Claude".to_string(),
                Some(st) => format!("Claude ({})", status_label_claude(st)),
            };
            let mut items: Vec<MenuItem<R>> = vec![MenuItem::with_id(
                app,
                "claude_section_header",
                label,
                false,
                None::<&str>,
            )?];

            match snap {
                Some(ClaudeUsageSnapshot::Ok {
                    session_percent,
                    session_resets_at,
                    weekly_percent,
                    weekly_resets_at,
                    models,
                    last_updated_at,
                    ..
                }) => {
                    let session_time = session_resets_at
                        .as_deref()
                        .and_then(format_reset_at_short)
                        .filter(|t| !t.is_empty())
                        .map(|t| format!(" (resets {t})"))
                        .unwrap_or_default();
                    let weekly_time = weekly_resets_at
                        .as_deref()
                        .and_then(format_reset_at_short)
                        .filter(|t| !t.is_empty())
                        .map(|t| format!(" (resets {t})"))
                        .unwrap_or_default();

                    items.push(MenuItem::with_id(
                        app,
                        "claude_session",
                        format!(
                            "Session: {}{session_time}",
                            format_percent(Some(*session_percent))
                        ),
                        false,
                        None::<&str>,
                    )?);
                    items.push(MenuItem::with_id(
                        app,
                        "claude_weekly",
                        format!(
                            "Weekly: {}{weekly_time}",
                            format_percent(Some(*weekly_percent))
                        ),
                        false,
                        None::<&str>,
                    )?);

                    if models.is_empty() {
                        items.push(MenuItem::with_id(
                            app,
                            "claude_model_none",
                            "Models (weekly): (none)",
                            false,
                            None::<&str>,
                        )?);
                    } else {
                        for (idx, m) in models.iter().enumerate() {
                            let model_time = m
                                .resets_at
                                .as_deref()
                                .and_then(format_reset_at_short)
                                .filter(|t| !t.is_empty())
                                .map(|t| format!(" (resets {t})"))
                                .unwrap_or_default();
                            items.push(MenuItem::with_id(
                                app,
                                format!("claude_model_{idx}"),
                                format!(
                                    "{} (weekly): {}{model_time}",
                                    m.name,
                                    format_percent(Some(m.percent))
                                ),
                                false,
                                None::<&str>,
                            )?);
                        }
                    }

                    items.push(MenuItem::with_id(
                        app,
                        "claude_last_updated",
                        format!("Last updated: {}", format_datetime_full(last_updated_at)),
                        false,
                        None::<&str>,
                    )?);
                }
                Some(other) => {
                    let error_message = match other {
                        ClaudeUsageSnapshot::Unauthorized { error_message, .. }
                        | ClaudeUsageSnapshot::RateLimited { error_message, .. }
                        | ClaudeUsageSnapshot::Error { error_message, .. }
                        | ClaudeUsageSnapshot::MissingKey { error_message, .. } => {
                            error_message.clone().unwrap_or_default()
                        }
                        _ => String::new(),
                    };

                    items.push(MenuItem::with_id(
                        app,
                        "claude_session",
                        "Session: --%",
                        false,
                        None::<&str>,
                    )?);
                    items.push(MenuItem::with_id(
                        app,
                        "claude_weekly",
                        "Weekly: --%",
                        false,
                        None::<&str>,
                    )?);
                    items.push(MenuItem::with_id(
                        app,
                        "claude_model_placeholder",
                        "Models (weekly): --%",
                        false,
                        None::<&str>,
                    )?);
                    if !error_message.trim().is_empty() {
                        items.push(MenuItem::with_id(
                            app,
                            "claude_error",
                            error_message,
                            false,
                            None::<&str>,
                        )?);
                    }
                    items.push(MenuItem::with_id(
                        app,
                        "claude_last_updated",
                        format!(
                            "Last updated: {}",
                            format_datetime_full(other.last_updated_at())
                        ),
                        false,
                        None::<&str>,
                    )?);
                }
                None => {
                    items.push(MenuItem::with_id(
                        app,
                        "claude_session",
                        "Session: --%",
                        false,
                        None::<&str>,
                    )?);
                    items.push(MenuItem::with_id(
                        app,
                        "claude_weekly",
                        "Weekly: --%",
                        false,
                        None::<&str>,
                    )?);
                    items.push(MenuItem::with_id(
                        app,
                        "claude_model_placeholder",
                        "Models (weekly): --%",
                        false,
                        None::<&str>,
                    )?);
                    items.push(MenuItem::with_id(
                        app,
                        "claude_last_updated",
                        "Last updated: --",
                        false,
                        None::<&str>,
                    )?);
                }
            }

            Ok(items)
        };

    let build_codex_items = |snap: Option<&CodexUsageSnapshot>| -> tauri::Result<Vec<MenuItem<R>>> {
        let status = snap.map(|s| s.status());
        let label = match status {
            None => "Codex (no data)".to_string(),
            Some(UsageStatus::Ok) => "Codex".to_string(),
            Some(st) => format!("Codex ({})", status_label_codex(st)),
        };
        let mut items: Vec<MenuItem<R>> = vec![MenuItem::with_id(
            app,
            "codex_section_header",
            label,
            false,
            None::<&str>,
        )?];

        match snap {
            Some(CodexUsageSnapshot::Ok {
                session_percent,
                session_resets_at,
                weekly_percent,
                weekly_resets_at,
                last_updated_at,
            }) => {
                let session_time = session_resets_at
                    .as_deref()
                    .and_then(format_reset_at_short)
                    .filter(|t| !t.is_empty())
                    .map(|t| format!(" (resets {t})"))
                    .unwrap_or_default();
                let weekly_time = weekly_resets_at
                    .as_deref()
                    .and_then(format_reset_at_short)
                    .filter(|t| !t.is_empty())
                    .map(|t| format!(" (resets {t})"))
                    .unwrap_or_default();

                items.push(MenuItem::with_id(
                    app,
                    "codex_session",
                    format!(
                        "Session: {}{session_time}",
                        format_percent(Some(*session_percent))
                    ),
                    false,
                    None::<&str>,
                )?);
                items.push(MenuItem::with_id(
                    app,
                    "codex_weekly",
                    format!(
                        "Weekly: {}{weekly_time}",
                        format_percent(Some(*weekly_percent))
                    ),
                    false,
                    None::<&str>,
                )?);
                items.push(MenuItem::with_id(
                    app,
                    "codex_last_updated",
                    format!("Last updated: {}", format_datetime_full(last_updated_at)),
                    false,
                    None::<&str>,
                )?);
            }
            Some(other) => {
                let error_message = match other {
                    CodexUsageSnapshot::Unauthorized { error_message, .. }
                    | CodexUsageSnapshot::RateLimited { error_message, .. }
                    | CodexUsageSnapshot::Error { error_message, .. }
                    | CodexUsageSnapshot::MissingKey { error_message, .. } => {
                        error_message.clone().unwrap_or_default()
                    }
                    _ => String::new(),
                };
                items.push(MenuItem::with_id(
                    app,
                    "codex_session",
                    "Session: --%",
                    false,
                    None::<&str>,
                )?);
                items.push(MenuItem::with_id(
                    app,
                    "codex_weekly",
                    "Weekly: --%",
                    false,
                    None::<&str>,
                )?);
                items.push(MenuItem::with_id(
                    app,
                    "codex_error",
                    error_message.clone(),
                    false,
                    None::<&str>,
                )?);
                if error_message.trim().is_empty() {
                    items.pop();
                }
                items.push(MenuItem::with_id(
                    app,
                    "codex_last_updated",
                    format!(
                        "Last updated: {}",
                        format_datetime_full(other.last_updated_at())
                    ),
                    false,
                    None::<&str>,
                )?);
            }
            None => {
                items.push(MenuItem::with_id(
                    app,
                    "codex_session",
                    "Session: --%",
                    false,
                    None::<&str>,
                )?);
                items.push(MenuItem::with_id(
                    app,
                    "codex_weekly",
                    "Weekly: --%",
                    false,
                    None::<&str>,
                )?);
                items.push(MenuItem::with_id(
                    app,
                    "codex_last_updated",
                    "Last updated: --",
                    false,
                    None::<&str>,
                )?);
            }
        }

        Ok(items)
    };

    let claude_items = if track_claude {
        let claude = snapshot.and_then(|s| s.claude.as_ref());
        Some(build_claude_items(claude)?)
    } else {
        None
    };

    if let Some(claude_items) = &claude_items {
        for item in claude_items {
            refs.push(item);
        }
    }

    if let Some(sep_between_sections) = &sep_between_sections {
        refs.push(sep_between_sections);
    }

    let codex_items = if track_codex {
        let codex = snapshot.and_then(|s| s.codex.as_ref());
        Some(build_codex_items(codex)?)
    } else {
        None
    };

    if let Some(codex_items) = &codex_items {
        for item in codex_items {
            refs.push(item);
        }
    }
    refs.push(&sep_before_actions);
    refs.push(&refresh_now);
    refs.push(&open_settings);
    refs.push(&check_updates);

    let sep_debug = PredefinedMenuItem::separator(app)?;
    let debug_set_below = MenuItem::with_id(
        app,
        ITEM_DEBUG_SET_BELOW_LIMIT,
        "Debug: Simulate below limit",
        true,
        None::<&str>,
    )?;
    let debug_set_near = MenuItem::with_id(
        app,
        ITEM_DEBUG_SET_NEAR_LIMIT,
        "Debug: Simulate near limit (>= 90%)",
        true,
        None::<&str>,
    )?;
    let debug_bump_resets = MenuItem::with_id(
        app,
        ITEM_DEBUG_BUMP_RESETS_AT,
        "Debug: Bump resets_at (simulate reset)",
        true,
        None::<&str>,
    )?;
    let debug_clear = MenuItem::with_id(
        app,
        ITEM_DEBUG_CLEAR_SIMULATION,
        "Debug: Clear simulation",
        true,
        None::<&str>,
    )?;

    if debug_menu_enabled() {
        refs.push(&sep_debug);
        refs.push(&debug_set_below);
        refs.push(&debug_set_near);
        refs.push(&debug_bump_resets);
        refs.push(&debug_clear);
    } else {
        let _ = sep_debug;
        let _ = debug_set_below;
        let _ = debug_set_near;
        let _ = debug_bump_resets;
        let _ = debug_clear;
    }

    refs.push(&sep_before_quit);
    refs.push(&quit);

    Menu::with_items(app, refs.as_slice())
}
