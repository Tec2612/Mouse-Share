mod commands;
mod permissions;
mod state;

use ms_config::ConfigStore;
use state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_state = build_app_state(app.handle())?;
            app.manage(app_state);
            setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_layout,
            commands::save_layout,
            commands::get_config,
            commands::list_paired_devices,
            commands::remove_paired_device,
            commands::get_this_device_fingerprint,
            commands::discover_devices,
            commands::start_pairing,
            commands::confirm_pairing,
            commands::cancel_pairing,
            permissions::check_permissions,
            permissions::request_accessibility,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Mouse Share UI");
}

fn build_app_state(app: &tauri::AppHandle) -> anyhow::Result<AppState> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| anyhow::anyhow!("could not resolve app config dir: {e}"))?;
    std::fs::create_dir_all(&config_dir)?;

    let config_store = ConfigStore::new(config_dir.join("config.toml"));
    let (device_id, device_name) = ms_daemon::identity::load_or_create_device_meta(&config_dir)?;

    let secure_storage: Box<dyn ms_config::SecureStorage> = {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            Box::new(ms_config::KeyringSecureStorage)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Box::new(ms_config::InMemorySecureStorage::default())
        }
    };
    let device_identity = ms_daemon::identity::load_or_generate(secure_storage.as_ref(), device_id, &device_name)?;

    let trust_store_path = ms_daemon::identity::trust_store_path(&config_dir);

    Ok(AppState {
        config_store,
        trust_store_path,
        device_identity,
        device_id,
        device_name,
        pending_pairing: std::sync::Mutex::new(None),
    })
}

/// Tray/menu-bar icon with "Open Dashboard", "Stop Sharing" (the same
/// emergency-release action the configured hotkey triggers — see
/// `docs/security.md#emergency-release` — always reachable here even if a
/// user forgets or hasn't configured the hotkey), and "Quit".
fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Dashboard", true, None::<&str>)?;
    let stop_sharing = MenuItem::with_id(app, "stop_sharing", "Stop Sharing", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Mouse Share", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &stop_sharing, &separator, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .icon(app.default_window_icon().cloned().expect("default window icon must be bundled"))
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "stop_sharing" => {
                // Emits an event the frontend/backend session layer
                // listens for to invoke the same force-release path the
                // emergency hotkey uses (`EdgeStateMachine::force_release`
                // via `ms-core-service`); wiring the live session handle
                // into this handler is the one piece left for when the
                // UI process also owns an active `CoreService` (currently
                // only the headless `ms-daemon` binary does — see
                // `docs/architecture.md`'s status note).
                tracing::info!("Stop Sharing requested from tray menu");
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
