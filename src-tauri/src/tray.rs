use crate::types::{ClaudeUsageSnapshot, UsageStatus};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{image::Image, AppHandle, Runtime};

pub const TRAY_ID: &str = "main";

pub const ITEM_REFRESH_NOW: &str = "refresh_now";
pub const ITEM_OPEN_SETTINGS: &str = "open_settings";
pub const ITEM_CHECK_UPDATES: &str = "check_updates";
pub const ITEM_QUIT: &str = "quit";

pub struct TrayUi<R: Runtime> {
  header: MenuItem<R>,
  session: MenuItem<R>,
  weekly: MenuItem<R>,
  model_weekly: MenuItem<R>,
  error_line: MenuItem<R>,
  last_updated: MenuItem<R>,
}

impl<R: Runtime> Clone for TrayUi<R> {
  fn clone(&self) -> Self {
    Self {
      header: self.header.clone(),
      session: self.session.clone(),
      weekly: self.weekly.clone(),
      model_weekly: self.model_weekly.clone(),
      error_line: self.error_line.clone(),
      last_updated: self.last_updated.clone(),
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

impl<R: Runtime> TrayUi<R> {
  pub fn new(app: &AppHandle<R>) -> tauri::Result<Self> {
    let header = MenuItem::with_id(app, "header", "Claudometer - Claude Usage", false, None::<&str>)?;
    let session = MenuItem::with_id(app, "session", "Session: --%", false, None::<&str>)?;
    let weekly = MenuItem::with_id(app, "weekly", "Weekly: --%", false, None::<&str>)?;
    let model_weekly =
      MenuItem::with_id(app, "model_weekly", "Model (weekly): --%", false, None::<&str>)?;
    let error_line = MenuItem::with_id(app, "error_line", "", false, None::<&str>)?;
    let last_updated = MenuItem::with_id(app, "last_updated", "Last updated: --", false, None::<&str>)?;

    let refresh_now = MenuItem::with_id(app, ITEM_REFRESH_NOW, "Refresh now", true, None::<&str>)?;
    let open_settings =
      MenuItem::with_id(app, ITEM_OPEN_SETTINGS, "Open Settings…", true, None::<&str>)?;
    let check_updates =
      MenuItem::with_id(app, ITEM_CHECK_UPDATES, "Check for Updates…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ITEM_QUIT, "Quit", true, None::<&str>)?;

    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let sep4 = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
      app,
      &[
        &header,
        &sep,
        &session,
        &weekly,
        &model_weekly,
        &error_line,
        &sep2,
        &last_updated,
        &sep3,
        &refresh_now,
        &open_settings,
        &check_updates,
        &sep4,
        &quit,
      ],
    )?;

    let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;

    TrayIconBuilder::with_id(TRAY_ID)
      .icon(icon)
      .menu(&menu)
      .tooltip("Claudometer")
      .build(app)?;

    Ok(Self {
      header,
      session,
      weekly,
      model_weekly,
      error_line,
      last_updated,
    })
  }

  pub fn update_snapshot(&self, snapshot: Option<&ClaudeUsageSnapshot>) {
    let (header, session, weekly, model_weekly, error_line, last_updated) = match snapshot {
      None => (
        "Claudometer - Claude Usage (no data)".to_string(),
        "Session: --%".to_string(),
        "Weekly: --%".to_string(),
        "Model (weekly): --%".to_string(),
        "".to_string(),
        "Last updated: --".to_string(),
      ),
      Some(s) => match s {
        ClaudeUsageSnapshot::Ok {
          session_percent,
          session_resets_at,
          weekly_percent,
          weekly_resets_at,
          model_weekly_percent,
          model_weekly_name,
          model_weekly_resets_at,
          last_updated_at,
          ..
        } => {
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
          let model_time = model_weekly_resets_at
            .as_deref()
            .and_then(format_time_short)
            .filter(|t| !t.is_empty())
            .map(|t| format!(" (resets {t})"))
            .unwrap_or_default();

          (
            "Claudometer - Claude Usage".to_string(),
            format!("Session: {}{session_time}", format_percent(Some(*session_percent))),
            format!("Weekly: {}{weekly_time}", format_percent(Some(*weekly_percent))),
            format!(
              "{} (weekly): {}{model_time}",
              model_weekly_name.clone().unwrap_or_else(|| "Model".to_string()),
              format_percent(Some(*model_weekly_percent))
            ),
            "".to_string(),
            format!("Last updated: {last_updated_at}"),
          )
        }
        _ => {
          let status = s.status();
          let header = match status {
            UsageStatus::MissingKey => "Claudometer - Claude Usage (needs session key)",
            UsageStatus::Unauthorized => "Claudometer - Claude Usage (unauthorized)",
            UsageStatus::RateLimited => "Claudometer - Claude Usage (rate limited)",
            UsageStatus::Error => "Claudometer - Claude Usage (error)",
            UsageStatus::Ok => "Claudometer - Claude Usage",
          }
          .to_string();

          let error_message = match s {
            ClaudeUsageSnapshot::Unauthorized { error_message, .. }
            | ClaudeUsageSnapshot::RateLimited { error_message, .. }
            | ClaudeUsageSnapshot::Error { error_message, .. }
            | ClaudeUsageSnapshot::MissingKey { error_message, .. } => error_message.clone().unwrap_or_default(),
            _ => String::new(),
          };

          (
            header,
            "Session: --%".to_string(),
            "Weekly: --%".to_string(),
            "Model (weekly): --%".to_string(),
            error_message,
            format!("Last updated: {}", s.last_updated_at()),
          )
        }
      },
    };

    let _ = self.header.set_text(header);
    let _ = self.session.set_text(session);
    let _ = self.weekly.set_text(weekly);
    let _ = self.model_weekly.set_text(model_weekly);
    let _ = self.error_line.set_text(error_line);
    let _ = self.last_updated.set_text(last_updated);
  }

}
