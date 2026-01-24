mod formatters;
mod menu_builder;

use crate::types::UsageSnapshotBundle;
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

impl<R: Runtime> TrayUi<R> {
    pub fn new(app: &AppHandle<R>) -> tauri::Result<Self> {
        let menu = menu_builder::build_menu(app, true, true, None)?;

        let icon = Image::from_bytes(include_bytes!("../../icons/icon.png"))?;

        let tray = TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .menu(&menu)
            .tooltip("Claudometer")
            .title("CL --% · CX --%")
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
        let menu = menu_builder::build_menu(app, track_claude, track_codex, snapshot);
        if let Ok(menu) = menu {
            let _ = self.tray.set_menu(Some(menu));
        }

        let title = formatters::format_tray_title(track_claude, track_codex, snapshot);
        let level = formatters::usage_level(track_claude, track_codex, snapshot);

        #[cfg(target_os = "macos")]
        {
            set_colored_tray_title(&self.tray, &title, level);
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = level;
            let _ = self.tray.set_title(Some(title));
        }
    }
}
