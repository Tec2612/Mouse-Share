// Prevents an additional console window on Windows in release builds;
// no effect elsewhere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tracing_subscriber::fmt::init();
    mouse_share_ui_lib::run();
}
