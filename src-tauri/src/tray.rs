use crate::types::{ClaudeUsageSnapshot, CodexUsageSnapshot, UsageSnapshotBundle, UsageStatus};
use chrono::format::Locale;
use chrono::{DateTime, FixedOffset, Local};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{image::Image, AppHandle, Runtime};

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSColor, NSForegroundColorAttributeName};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSAttributedString, NSDictionary, NSString};

pub const TRAY_ID: &str = "main";

pub const ITEM_REFRESH_NOW: &str = "refresh_now";
pub const ITEM_OPEN_SETTINGS: &str = "open_settings";
pub const ITEM_CHECK_UPDATES: &str = "check_updates";
pub const ITEM_QUIT: &str = "quit";

pub const ITEM_DEBUG_SET_BELOW_LIMIT: &str = "debug_set_below_limit";
pub const ITEM_DEBUG_SET_NEAR_LIMIT: &str = "debug_set_near_limit";
pub const ITEM_DEBUG_BUMP_RESETS_AT: &str = "debug_bump_resets_at";
pub const ITEM_DEBUG_CLEAR_SIMULATION: &str = "debug_clear_simulation";

pub struct TrayUi<R: Runtime> {
    tray: TrayIcon<R>,
}

impl<R: Runtime> Clone for TrayUi<R> {
    fn clone(&self) -> Self {
        Self {
            tray: self.tray.clone(),
        }
    }
}

fn format_percent(value: Option<f64>) -> String {
    value
        .map(|v| format!("{}%", v.round() as i64))
        .unwrap_or_else(|| "--%".to_string())
}

/// Generate the tray title text based on usage snapshot.
/// Returns percentage for Ok state, "--%" for error states.
fn format_tray_title(
    track_claude: bool,
    track_codex: bool,
    snapshot: Option<&UsageSnapshotBundle>,
) -> String {
    fn session_percent(s: &ClaudeUsageSnapshot) -> Option<f64> {
        match s {
            ClaudeUsageSnapshot::Ok {
                session_percent, ..
            } => Some(*session_percent),
            _ => None,
        }
    }
    fn session_percent_codex(s: &CodexUsageSnapshot) -> Option<f64> {
        match s {
            CodexUsageSnapshot::Ok {
                session_percent, ..
            } => Some(*session_percent),
            _ => None,
        }
    }

    if track_claude && track_codex {
        let claude = snapshot
            .and_then(|s| s.claude.as_ref())
            .and_then(session_percent)
            .map(|v| format!("{}%", v.round() as i64))
            .unwrap_or_else(|| "--%".to_string());
        let codex = snapshot
            .and_then(|s| s.codex.as_ref())
            .and_then(session_percent_codex)
            .map(|v| format!("{}%", v.round() as i64))
            .unwrap_or_else(|| "--%".to_string());
        return format!("CL {claude} · CX {codex}");
    }

    if track_claude {
        let percent = snapshot
            .and_then(|s| s.claude.as_ref())
            .and_then(session_percent);
        let value = percent
            .map(|v| format!("{}%", v.round() as i64))
            .unwrap_or_else(|| "--%".to_string());
        return format!("CL {value}");
    }
    if track_codex {
        let percent = snapshot
            .and_then(|s| s.codex.as_ref())
            .and_then(session_percent_codex);
        let value = percent
            .map(|v| format!("{}%", v.round() as i64))
            .unwrap_or_else(|| "--%".to_string());
        return format!("CX {value}");
    }

    "--%".to_string()
}

/// Determine usage level from session percentage.
/// Returns: 0 = low (green), 1 = medium (orange), 2 = high (red), -1 = unknown (gray)
fn usage_level(
    track_claude: bool,
    track_codex: bool,
    snapshot: Option<&UsageSnapshotBundle>,
) -> i8 {
    let claude = if track_claude {
        snapshot.and_then(|s| match s.claude.as_ref() {
            Some(ClaudeUsageSnapshot::Ok {
                session_percent, ..
            }) => Some(*session_percent),
            _ => None,
        })
    } else {
        None
    };
    let codex = if track_codex {
        snapshot.and_then(|s| match s.codex.as_ref() {
            Some(CodexUsageSnapshot::Ok {
                session_percent, ..
            }) => Some(*session_percent),
            _ => None,
        })
    } else {
        None
    };

    let session_percent = match (claude, codex) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    let Some(session_percent) = session_percent else {
        return -1;
    };

    if session_percent < 50.0 {
        0 // green
    } else if session_percent <= 70.0 {
        1 // orange
    } else {
        2 // red
    }
}

/// Set colored attributed title on macOS tray button.
#[cfg(target_os = "macos")]
fn set_colored_tray_title<R: Runtime>(tray: &TrayIcon<R>, title: &str, level: i8) {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2_foundation::NSCopying;

    let title_owned = title.to_string();

    let _ = tray.with_inner_tray_icon(move |inner| {
        let Some(ns_status_item) = inner.ns_status_item() else {
            return;
        };

        // Safety: We're on the main thread (Tauri ensures this for tray operations)
        let mtm = unsafe { MainThreadMarker::new_unchecked() };

        let Some(button) = ns_status_item.button(mtm) else {
            return;
        };

        // Create color based on level
        let color: Retained<NSColor> = match level {
            0 => NSColor::colorWithSRGBRed_green_blue_alpha(0.298, 0.686, 0.314, 1.0), // green #4CAF50
            1 => NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 0.596, 0.0, 1.0), // orange #FF9800
            2 => NSColor::colorWithSRGBRed_green_blue_alpha(0.957, 0.263, 0.212, 1.0), // red #F44336
            _ => NSColor::colorWithSRGBRed_green_blue_alpha(0.5, 0.5, 0.5, 1.0),       // gray
        };

        // Create attributed string with foreground color
        let ns_string = NSString::from_str(&title_owned);
        let key = unsafe { NSForegroundColorAttributeName };

        // Create dictionary - cast types for compatibility
        let color_ref: &NSColor = &color;
        let key_ref: &NSString = &key;
        // Safety: NSColor is an Objective-C object, so it can be treated as AnyObject for
        // NSDictionary storage. The resulting reference is only used within this closure.
        let color_obj: &AnyObject =
            unsafe { std::mem::transmute::<&NSColor, &AnyObject>(color_ref) };
        // Safety: NSDictionary keys must conform to NSCopying. NSString does, and Tauri/objc2
        // expects keys as `ProtocolObject<dyn NSCopying>`.
        let key_copy: &ProtocolObject<dyn NSCopying> =
            unsafe { std::mem::transmute::<&NSString, &ProtocolObject<dyn NSCopying>>(key_ref) };
        let attrs: Retained<NSDictionary<NSString, AnyObject>> = unsafe {
            // Safety: objc2 returns a dictionary typed as `NSDictionary<AnyObject, AnyObject>`.
            // We control both key/value types (NSString/AnyObject) and immediately pass it to
            // NSAttributedString creation.
            std::mem::transmute(
                NSDictionary::<AnyObject, AnyObject>::dictionaryWithObject_forKey(
                    color_obj, key_copy,
                ),
            )
        };
        let attributed_string = unsafe {
            NSAttributedString::initWithString_attributes(mtm.alloc(), &ns_string, Some(&attrs))
        };

        // Set the attributed title on the button
        button.setAttributedTitle(&attributed_string);
    });
}

fn system_locale_tag() -> Option<String> {
    for key in ["LC_TIME", "LC_ALL", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn normalize_locale_tag(tag: &str) -> String {
    let tag = tag.trim();
    let tag = tag.split('.').next().unwrap_or(tag);
    let tag = tag.replace('-', "_");

    if tag.eq_ignore_ascii_case("c") {
        return "POSIX".to_string();
    }

    let (main, modifier) = match tag.split_once('@') {
        Some((main, modifier)) => (main, Some(modifier)),
        None => (tag.as_str(), None),
    };

    let mut parts = main.split('_');
    let language = parts.next().unwrap_or(main).to_ascii_lowercase();
    let territory = parts.next().map(|t| t.to_ascii_uppercase());

    let mut normalized = match territory {
        Some(territory) => format!("{language}_{territory}"),
        None => language,
    };
    if let Some(modifier) = modifier {
        normalized.push('@');
        normalized.push_str(modifier);
    }

    normalized
}

fn system_locale() -> Locale {
    let tag = system_locale_tag().unwrap_or_else(|| "en_US".to_string());
    let tag = normalize_locale_tag(&tag);
    tag.parse().unwrap_or(Locale::en_US)
}

fn strip_seconds_from_time_string(input: &str) -> String {
    let mut out = Vec::new();
    for token in input.split_whitespace() {
        let mut replaced = token.to_string();
        if token.matches(':').count() >= 2 {
            if let Some((before_last, last)) = token.rsplit_once(':') {
                if last.len() == 2 && last.chars().all(|c| c.is_ascii_digit()) {
                    replaced = before_last.to_string();
                }
            }
        }
        out.push(replaced);
    }
    out.join(" ")
}

fn format_time_short(iso: &str) -> Option<String> {
    // Locale + TZ rules:
    // - Time zone conversion uses `chrono::Local` (the OS-configured local time zone, incl. DST).
    // - Locale detection uses `LC_TIME` → `LC_ALL` → `LANG`.
    // - Formatting uses `chrono` `unstable-locales` (pure-rust-locales) for locale patterns.
    let dt: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(iso).ok()?;
    let locale = system_locale();
    let formatted = dt
        .with_timezone(&Local)
        .format_localized("%X", locale)
        .to_string();
    Some(strip_seconds_from_time_string(&formatted))
}

fn format_datetime_full(iso: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(iso) else {
        return iso.to_string();
    };
    let local = dt.with_timezone(&Local);
    let locale = system_locale();
    let date = local.format_localized("%x", locale).to_string();
    let time = local.format_localized("%X", locale).to_string();
    format!("{date}, {time}")
}

fn debug_menu_enabled() -> bool {
    matches!(
        std::env::var("CLAUDOMETER_DEBUG").as_deref(),
        Ok("1") | Ok("true") | Ok("yes") | Ok("on")
    )
}

impl<R: Runtime> TrayUi<R> {
    pub fn new(app: &AppHandle<R>) -> tauri::Result<Self> {
        let menu = build_menu(app, true, true, None)?;

        let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;

        let tray = TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .menu(&menu)
            .tooltip("Claudometer")
            .title("CL --% · CX --%") // Initial placeholder until first data fetch
            .build(app)?;

        Ok(Self { tray })
    }

    pub fn update_snapshot(
        &self,
        track_claude: bool,
        track_codex: bool,
        snapshot: Option<&UsageSnapshotBundle>,
    ) {
        let app = self.tray.app_handle();
        let menu = build_menu(app, track_claude, track_codex, snapshot);
        if let Ok(menu) = menu {
            let _ = self.tray.set_menu(Some(menu));
        }

        // Update tray title with usage percentage
        let title = format_tray_title(track_claude, track_codex, snapshot);
        let level = usage_level(track_claude, track_codex, snapshot);

        #[cfg(target_os = "macos")]
        {
            set_colored_tray_title(&self.tray, &title, level);
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = level; // suppress unused warning on non-macOS
            let _ = self.tray.set_title(Some(title));
        }
    }
}

fn build_menu<R: Runtime>(
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
                        .and_then(format_time_short)
                        .filter(|t| !t.is_empty())
                        .map(|t| format!(" (resets {t})"))
                        .unwrap_or_default();
                    let weekly_time = weekly_resets_at
                        .as_deref()
                        .and_then(format_time_short)
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
                                .and_then(format_time_short)
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
                    .and_then(format_time_short)
                    .filter(|t| !t.is_empty())
                    .map(|t| format!(" (resets {t})"))
                    .unwrap_or_default();
                let weekly_time = weekly_resets_at
                    .as_deref()
                    .and_then(format_time_short)
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
                    "codex_model_na",
                    "Models (weekly): (n/a)",
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
                    "codex_model_placeholder",
                    "Models (weekly): --%",
                    false,
                    None::<&str>,
                )?);
                if !error_message.trim().is_empty() {
                    items.push(MenuItem::with_id(
                        app,
                        "codex_error",
                        error_message,
                        false,
                        None::<&str>,
                    )?);
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
                    "codex_model_placeholder",
                    "Models (weekly): --%",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rfc3339_utc() -> &'static str {
        "2026-01-06T22:59:31Z"
    }

    #[test]
    fn format_time_short_strips_seconds_for_common_time_patterns() {
        assert_eq!(strip_seconds_from_time_string("06:54:32 AM"), "06:54 AM");
        assert_eq!(strip_seconds_from_time_string("18:54:32"), "18:54");
        assert_eq!(strip_seconds_from_time_string("06:54 AM"), "06:54 AM");
    }

    #[test]
    fn format_time_short_and_datetime_full_do_not_crash() {
        assert!(format_time_short(sample_rfc3339_utc()).is_some());
        let formatted = format_datetime_full(sample_rfc3339_utc());
        assert!(formatted.contains(", "));
    }

    #[test]
    fn format_datetime_full_falls_back_to_raw_input_on_parse_error() {
        let input = "not-a-datetime";
        assert_eq!(format_datetime_full(input), input);
    }

    #[test]
    fn normalize_locale_tag_handles_common_env_values() {
        assert_eq!(normalize_locale_tag("pt_BR.UTF-8"), "pt_BR");
        assert_eq!(normalize_locale_tag("de-de.UTF-8"), "de_DE");
        assert_eq!(normalize_locale_tag("C.UTF-8"), "POSIX");
    }

    fn make_claude_ok_bundle(session_percent: f64) -> UsageSnapshotBundle {
        UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::Ok {
                organization_id: "org-123".to_string(),
                session_percent,
                session_resets_at: Some("2026-01-07T05:00:00Z".to_string()),
                weekly_percent: 30.0,
                weekly_resets_at: Some("2026-01-13T00:00:00Z".to_string()),
                models: vec![],
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        }
    }

    fn make_codex_ok_bundle(session_percent: f64) -> UsageSnapshotBundle {
        UsageSnapshotBundle {
            claude: None,
            codex: Some(CodexUsageSnapshot::Ok {
                session_percent,
                session_resets_at: Some("2026-01-07T05:00:00Z".to_string()),
                weekly_percent: 30.0,
                weekly_resets_at: Some("2026-01-13T00:00:00Z".to_string()),
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
        }
    }

    #[test]
    fn format_tray_title_shows_percentage() {
        let snapshot = make_claude_ok_bundle(25.0);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL 25%");
    }

    #[test]
    fn format_tray_title_rounds_49_point_9_to_50() {
        let snapshot = make_claude_ok_bundle(49.9);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL 50%");
    }

    #[test]
    fn format_tray_title_shows_100_percent() {
        let snapshot = make_claude_ok_bundle(100.0);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL 100%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_none() {
        let title = format_tray_title(true, false, None);
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_unauthorized() {
        let snapshot = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::Unauthorized {
                organization_id: None,
                error_message: Some("Invalid session".to_string()),
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_missing_key() {
        let snapshot = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::MissingKey {
                organization_id: None,
                error_message: None,
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_rate_limited() {
        let snapshot = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::RateLimited {
                organization_id: None,
                error_message: Some("Too many requests".to_string()),
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_error() {
        let snapshot = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::Error {
                organization_id: None,
                error_message: Some("Network error".to_string()),
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        let title = format_tray_title(true, false, Some(&snapshot));
        assert_eq!(title, "CL --%");
    }

    #[test]
    fn format_tray_title_rounds_percentage_correctly() {
        let snapshot = make_claude_ok_bundle(75.7);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert!(title.contains("76%"), "75.7 should round to 76");

        let snapshot = make_claude_ok_bundle(75.4);
        let title = format_tray_title(true, false, Some(&snapshot));
        assert!(title.contains("75%"), "75.4 should round to 75");
    }

    #[test]
    fn usage_level_returns_green_below_50() {
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(0.0))),
            0
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(25.0))),
            0
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(49.9))),
            0
        );
    }

    #[test]
    fn usage_level_returns_orange_between_50_and_70() {
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(50.0))),
            1
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(65.0))),
            1
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(70.0))),
            1
        );
    }

    #[test]
    fn usage_level_returns_red_above_70() {
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(70.1))),
            2
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(90.0))),
            2
        );
        assert_eq!(
            usage_level(true, false, Some(&make_claude_ok_bundle(100.0))),
            2
        );
    }

    #[test]
    fn usage_level_returns_unknown_for_error_states() {
        assert_eq!(usage_level(true, false, None), -1);
        let error = UsageSnapshotBundle {
            claude: Some(ClaudeUsageSnapshot::Error {
                organization_id: None,
                error_message: None,
                last_updated_at: "2026-01-06T22:59:31Z".to_string(),
            }),
            codex: None,
        };
        assert_eq!(usage_level(true, false, Some(&error)), -1);
    }

    #[test]
    fn format_tray_title_in_dual_mode_shows_both_providers() {
        let snapshot = UsageSnapshotBundle {
            claude: make_claude_ok_bundle(25.0).claude,
            codex: make_codex_ok_bundle(10.0).codex,
        };
        let title = format_tray_title(true, true, Some(&snapshot));
        assert_eq!(title, "CL 25% · CX 10%");
    }

    #[test]
    fn usage_level_in_dual_mode_uses_max_severity() {
        let snapshot = UsageSnapshotBundle {
            claude: make_claude_ok_bundle(10.0).claude,
            codex: make_codex_ok_bundle(95.0).codex,
        };
        assert_eq!(usage_level(true, true, Some(&snapshot)), 2);
    }
}
