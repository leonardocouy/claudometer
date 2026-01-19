use crate::types::{ClaudeUsageSnapshot, UsageStatus};
use chrono::format::Locale;
use chrono::{DateTime, FixedOffset, Local};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{image::Image, AppHandle, Runtime};

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
fn format_tray_title(snapshot: Option<&ClaudeUsageSnapshot>) -> String {
    match snapshot {
        Some(ClaudeUsageSnapshot::Ok { session_percent, .. }) => {
            let percent = session_percent.round() as i64;
            format!("{}%", percent)
        }
        _ => "--%".to_string(),
    }
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
        let menu = build_menu(app, None)?;

        let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;

        let tray = TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .menu(&menu)
            .tooltip("Claudometer")
            .title("--%")  // Initial placeholder until first data fetch
            .build(app)?;

        Ok(Self { tray })
    }

    pub fn update_snapshot(&self, snapshot: Option<&ClaudeUsageSnapshot>) {
        let app = self.tray.app_handle();
        let menu = build_menu(app, snapshot);
        if let Ok(menu) = menu {
            let _ = self.tray.set_menu(Some(menu));
        }

        // Update tray title with usage percentage
        let title = format_tray_title(snapshot);
        let _ = self.tray.set_title(Some(title));
    }
}

fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    snapshot: Option<&ClaudeUsageSnapshot>,
) -> tauri::Result<Menu<R>> {
    let header_text = match snapshot {
        None => "Claudometer - Claude Usage (no data)".to_string(),
        Some(s) => match s.status() {
            UsageStatus::MissingKey => "Claudometer - Claude Usage (needs session key)".to_string(),
            UsageStatus::Unauthorized => "Claudometer - Claude Usage (unauthorized)".to_string(),
            UsageStatus::RateLimited => "Claudometer - Claude Usage (rate limited)".to_string(),
            UsageStatus::Error => "Claudometer - Claude Usage (error)".to_string(),
            UsageStatus::Ok => "Claudometer - Claude Usage".to_string(),
        },
    };

    let header = MenuItem::with_id(app, "header", header_text, false, None::<&str>)?;
    let last_updated =
        MenuItem::with_id(app, "last_updated", "Last updated: --", false, None::<&str>)?;

    let (session, weekly, model_items, error_text, last_updated_text) = match snapshot {
        None => (
            "Session: --%".to_string(),
            "Weekly: --%".to_string(),
            vec![MenuItem::with_id(
                app,
                "model_placeholder",
                "Models (weekly): --%",
                false,
                None::<&str>,
            )?],
            "".to_string(),
            "Last updated: --".to_string(),
        ),
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

            let model_items = if models.is_empty() {
                vec![MenuItem::with_id(
                    app,
                    "model_none",
                    "Models (weekly): (none)",
                    false,
                    None::<&str>,
                )?]
            } else {
                let mut items = Vec::with_capacity(models.len());
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
                        format!("model_{idx}"),
                        format!(
                            "{} (weekly): {}{model_time}",
                            m.name,
                            format_percent(Some(m.percent))
                        ),
                        false,
                        None::<&str>,
                    )?);
                }
                items
            };

            (
                format!(
                    "Session: {}{session_time}",
                    format_percent(Some(*session_percent))
                ),
                format!(
                    "Weekly: {}{weekly_time}",
                    format_percent(Some(*weekly_percent))
                ),
                model_items,
                "".to_string(),
                format!("Last updated: {}", format_datetime_full(last_updated_at)),
            )
        }
        Some(s) => {
            let error_message = match s {
                ClaudeUsageSnapshot::Unauthorized { error_message, .. }
                | ClaudeUsageSnapshot::RateLimited { error_message, .. }
                | ClaudeUsageSnapshot::Error { error_message, .. }
                | ClaudeUsageSnapshot::MissingKey { error_message, .. } => {
                    error_message.clone().unwrap_or_default()
                }
                _ => String::new(),
            };
            (
                "Session: --%".to_string(),
                "Weekly: --%".to_string(),
                vec![MenuItem::with_id(
                    app,
                    "model_placeholder",
                    "Models (weekly): --%",
                    false,
                    None::<&str>,
                )?],
                error_message,
                format!(
                    "Last updated: {}",
                    format_datetime_full(s.last_updated_at())
                ),
            )
        }
    };

    let session = MenuItem::with_id(app, "session", session, false, None::<&str>)?;
    let weekly = MenuItem::with_id(app, "weekly", weekly, false, None::<&str>)?;
    let _ = last_updated.set_text(last_updated_text);

    let error_line = if error_text.trim().is_empty() {
        None
    } else {
        let item = MenuItem::with_id(app, "error_line", "", false, None::<&str>)?;
        let _ = item.set_text(error_text);
        Some(item)
    };

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
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let sep4 = PredefinedMenuItem::separator(app)?;

    let mut refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = vec![&header, &sep, &session, &weekly];
    for item in &model_items {
        refs.push(item);
    }
    if let Some(error_line) = &error_line {
        refs.push(error_line);
    }
    refs.push(&sep2);
    refs.push(&last_updated);
    refs.push(&sep3);
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
    }

    refs.push(&sep4);
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

    fn make_ok_snapshot(session_percent: f64) -> ClaudeUsageSnapshot {
        ClaudeUsageSnapshot::Ok {
            organization_id: "org-123".to_string(),
            session_percent,
            session_resets_at: Some("2026-01-07T05:00:00Z".to_string()),
            weekly_percent: 30.0,
            weekly_resets_at: Some("2026-01-13T00:00:00Z".to_string()),
            models: vec![],
            last_updated_at: "2026-01-06T22:59:31Z".to_string(),
        }
    }

    #[test]
    fn format_tray_title_shows_percentage() {
        let snapshot = make_ok_snapshot(25.0);
        let title = format_tray_title(Some(&snapshot));
        assert_eq!(title, "25%");
    }

    #[test]
    fn format_tray_title_rounds_49_point_9_to_50() {
        let snapshot = make_ok_snapshot(49.9);
        let title = format_tray_title(Some(&snapshot));
        assert_eq!(title, "50%");
    }

    #[test]
    fn format_tray_title_shows_100_percent() {
        let snapshot = make_ok_snapshot(100.0);
        let title = format_tray_title(Some(&snapshot));
        assert_eq!(title, "100%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_none() {
        let title = format_tray_title(None);
        assert_eq!(title, "--%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_unauthorized() {
        let snapshot = ClaudeUsageSnapshot::Unauthorized {
            organization_id: None,
            error_message: Some("Invalid session".to_string()),
            last_updated_at: "2026-01-06T22:59:31Z".to_string(),
        };
        let title = format_tray_title(Some(&snapshot));
        assert_eq!(title, "--%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_missing_key() {
        let snapshot = ClaudeUsageSnapshot::MissingKey {
            organization_id: None,
            error_message: None,
            last_updated_at: "2026-01-06T22:59:31Z".to_string(),
        };
        let title = format_tray_title(Some(&snapshot));
        assert_eq!(title, "--%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_rate_limited() {
        let snapshot = ClaudeUsageSnapshot::RateLimited {
            organization_id: None,
            error_message: Some("Too many requests".to_string()),
            last_updated_at: "2026-01-06T22:59:31Z".to_string(),
        };
        let title = format_tray_title(Some(&snapshot));
        assert_eq!(title, "--%");
    }

    #[test]
    fn format_tray_title_shows_placeholder_for_error() {
        let snapshot = ClaudeUsageSnapshot::Error {
            organization_id: None,
            error_message: Some("Network error".to_string()),
            last_updated_at: "2026-01-06T22:59:31Z".to_string(),
        };
        let title = format_tray_title(Some(&snapshot));
        assert_eq!(title, "--%");
    }

    #[test]
    fn format_tray_title_rounds_percentage_correctly() {
        let snapshot = make_ok_snapshot(75.7);
        let title = format_tray_title(Some(&snapshot));
        assert!(title.contains("76%"), "75.7 should round to 76");

        let snapshot = make_ok_snapshot(75.4);
        let title = format_tray_title(Some(&snapshot));
        assert!(title.contains("75%"), "75.4 should round to 75");
    }
}
