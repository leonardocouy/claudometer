mod app;
mod claude;
mod commands;
mod redact;
mod settings;
mod tray;
mod types;
mod updater;
mod usage_alerts;
mod windows;

pub fn run() {
    app::run();
}
