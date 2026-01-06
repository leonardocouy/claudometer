use crate::types::{ClaudeUsageSnapshot, UsageStatus};
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
  value.map(|v| format!("{}%", v.round() as i64)).unwrap_or_else(|| "--%".to_string())
}

fn format_time_short(iso: &str) -> Option<String> {
  let Ok(dt) = time::OffsetDateTime::parse(iso, &time::format_description::well_known::Rfc3339) else {
    return None;
  };
  Some(
    dt.time()
      .format(&time::format_description::parse("[hour]:[minute]").unwrap())
      .unwrap_or_else(|_| "".to_string()),
  )
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
      .build(app)?;

    Ok(Self { tray })
  }

  pub fn update_snapshot(&self, snapshot: Option<&ClaudeUsageSnapshot>) {
    let app = self.tray.app_handle();
    let menu = build_menu(app, snapshot);
    if let Ok(menu) = menu {
      let _ = self.tray.set_menu(Some(menu));
    }
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
  let error_line = MenuItem::with_id(app, "error_line", "", false, None::<&str>)?;
  let last_updated = MenuItem::with_id(app, "last_updated", "Last updated: --", false, None::<&str>)?;

  let (session, weekly, model_items, error_text, last_updated_text) = match snapshot {
    None => (
      "Session: --%".to_string(),
      "Weekly: --%".to_string(),
      vec![MenuItem::with_id(app, "model_placeholder", "Models (weekly): --%", false, None::<&str>)?],
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
        vec![MenuItem::with_id(app, "model_none", "Models (weekly): (none)", false, None::<&str>)?]
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
            format!("{} (weekly): {}{model_time}", m.name, format_percent(Some(m.percent))),
            false,
            None::<&str>,
          )?);
        }
        items
      };

      (
        format!("Session: {}{session_time}", format_percent(Some(*session_percent))),
        format!("Weekly: {}{weekly_time}", format_percent(Some(*weekly_percent))),
        model_items,
        "".to_string(),
        format!("Last updated: {last_updated_at}"),
      )
    }
    Some(s) => {
      let error_message = match s {
        ClaudeUsageSnapshot::Unauthorized { error_message, .. }
        | ClaudeUsageSnapshot::RateLimited { error_message, .. }
        | ClaudeUsageSnapshot::Error { error_message, .. }
        | ClaudeUsageSnapshot::MissingKey { error_message, .. } => error_message.clone().unwrap_or_default(),
        _ => String::new(),
      };
      (
        "Session: --%".to_string(),
        "Weekly: --%".to_string(),
        vec![MenuItem::with_id(app, "model_placeholder", "Models (weekly): --%", false, None::<&str>)?],
        error_message,
        format!("Last updated: {}", s.last_updated_at()),
      )
    }
  };

  let session = MenuItem::with_id(app, "session", session, false, None::<&str>)?;
  let weekly = MenuItem::with_id(app, "weekly", weekly, false, None::<&str>)?;
  let _ = error_line.set_text(error_text);
  let _ = last_updated.set_text(last_updated_text);

  let refresh_now = MenuItem::with_id(app, ITEM_REFRESH_NOW, "Refresh now", true, None::<&str>)?;
  let open_settings = MenuItem::with_id(app, ITEM_OPEN_SETTINGS, "Open Settings…", true, None::<&str>)?;
  let check_updates = MenuItem::with_id(app, ITEM_CHECK_UPDATES, "Check for Updates…", true, None::<&str>)?;
  let quit = MenuItem::with_id(app, ITEM_QUIT, "Quit", true, None::<&str>)?;

  let sep = PredefinedMenuItem::separator(app)?;
  let sep2 = PredefinedMenuItem::separator(app)?;
  let sep3 = PredefinedMenuItem::separator(app)?;
  let sep4 = PredefinedMenuItem::separator(app)?;

  let mut refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = Vec::new();
  refs.push(&header);
  refs.push(&sep);
  refs.push(&session);
  refs.push(&weekly);
  for item in &model_items {
    refs.push(item);
  }
  refs.push(&error_line);
  refs.push(&sep2);
  refs.push(&last_updated);
  refs.push(&sep3);
  refs.push(&refresh_now);
  refs.push(&open_settings);
  refs.push(&check_updates);

  let sep_debug = PredefinedMenuItem::separator(app)?;
  let debug_set_below =
    MenuItem::with_id(app, ITEM_DEBUG_SET_BELOW_LIMIT, "Debug: Simulate below limit", true, None::<&str>)?;
  let debug_set_near =
    MenuItem::with_id(app, ITEM_DEBUG_SET_NEAR_LIMIT, "Debug: Simulate near limit (>= 90%)", true, None::<&str>)?;
  let debug_bump_resets =
    MenuItem::with_id(app, ITEM_DEBUG_BUMP_RESETS_AT, "Debug: Bump resets_at (simulate reset)", true, None::<&str>)?;
  let debug_clear =
    MenuItem::with_id(app, ITEM_DEBUG_CLEAR_SIMULATION, "Debug: Clear simulation", true, None::<&str>)?;

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
