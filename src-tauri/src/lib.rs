mod app;
mod claude;
mod codex;
mod commands;
mod notifications;
mod provider_view;
mod redact;
mod refresh;
mod settings;
mod state;
mod tray;
pub mod types;
mod updater;
mod usage_alerts;
mod windows;

pub fn run() {
    app::run();
}
