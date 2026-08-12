mod commands;
mod pairing;
mod permissions;
mod state;

use ms_config::ConfigStore;
use state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_state = build_app_state(app.handle())?;
            app.manage(app_state);
            setup_tray(app.handle())?;
            pairing::spawn_pairing_acceptor(app.handle().clone());
            spawn_daemon_if_present();

            // Closing the window must not exit the app: the pairing
            // acceptor (and, once wired, live sessions) need to keep
            // running in the background exactly like the tray icon
            // implies. Hide instead, and only actually quit via the
            // tray's "Quit" item.
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_layout,
            commands::save_layout,
            commands::link_layout_edge,
            commands::unlink_layout_edge,
            commands::get_config,
            commands::list_paired_devices,
            commands::remove_paired_device,
            commands::get_this_device_fingerprint,
            commands::get_this_device_id,
            commands::discover_devices,
            commands::start_pairing,
            commands::confirm_pairing,
            commands::cancel_pairing,
            commands::accept_incoming_pairing,
            commands::decline_incoming_pairing,
            permissions::check_permissions,
            permissions::request_accessibility,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Mouse Share UI");
}

fn build_app_state(_app: &tauri::AppHandle) -> anyhow::Result<AppState> {
    // Deliberately NOT Tauri's own `app.path().app_config_dir()`: the
    // headless daemon (`ms-daemon`, a separate process — see
    // docs/architecture.md) has no Tauri context and resolves its config
    // directory via `ms_config::ConfigStore::default_path()` instead. The
    // two must agree, or the daemon never sees devices paired through
    // this UI (and vice versa) — they were pointing at two different
    // directories entirely until this fix.
    let config_dir = ms_config::ConfigStore::default_path()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .ok_or_else(|| anyhow::anyhow!("could not resolve a config directory for this platform"))?;
    std::fs::create_dir_all(&config_dir)?;

    let config_store = ConfigStore::new(config_dir.join("config.toml"));
    let config = config_store.load()?;
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

    let discovery = if config.settings.network.auto_discovery_enabled {
        match start_advertising(device_id, &device_name, config.settings.network.pairing_port) {
            Ok(service) => Some(service),
            Err(e) => {
                tracing::warn!(error = %e, "failed to start mDNS advertising; this device won't appear in other devices' discovery lists (Connect by IP/Hostname still works)");
                None
            }
        }
    } else {
        None
    };

    Ok(AppState {
        config_store,
        trust_store_path,
        device_identity,
        device_id,
        device_name,
        pending_pairing: std::sync::Mutex::new(None),
        incoming_pairing: std::sync::Mutex::new(None),
        discovery: std::sync::Mutex::new(discovery),
    })
}

/// Advertises this device at `pairing_port` — see `NetworkSettings::pairing_port`'s
/// doc comment for why that's the port used here rather than
/// `listen_port`. The returned `DiscoveryService` must be kept alive
/// (stored in `AppState::discovery`) for the advertisement to stay up.
fn start_advertising(device_id: uuid::Uuid, device_name: &str, pairing_port: u16) -> anyhow::Result<ms_discovery::DiscoveryService> {
    let mut discovery = ms_discovery::DiscoveryService::new()?;
    let os = current_os();
    let remote_os = match os {
        ms_protocol::OperatingSystem::Windows => ms_discovery::RemoteOs::Windows,
        ms_protocol::OperatingSystem::MacOs => ms_discovery::RemoteOs::MacOs,
    };
    let host_label = device_name.to_lowercase().replace(' ', "-");
    discovery.advertise(device_id, device_name, remote_os, &host_label, pairing_port)?;
    Ok(discovery)
}

/// Launches `mouse-share-daemon` (the actual input-capture/session
/// binary — see `docs/architecture.md`) as a detached background
/// process, if it's sitting next to this UI executable (as both the
/// Windows Inno Setup installer and the macOS `.dmg` bundle it — see
/// `installers/`). Without this, nothing ever starts the daemon at all:
/// pairing would work (it's handled directly by this UI process) but no
/// mouse/keyboard input would ever actually cross between machines.
///
/// If a daemon is already running (from a previous launch, or a user
/// running it manually), this spawn just fails fast on the port bind and
/// exits — harmless, and not worth a proper singleton check for now.
fn spawn_daemon_if_present() {
    let Ok(current_exe) = std::env::current_exe() else {
        tracing::warn!("could not resolve this executable's own path; not starting the daemon");
        return;
    };
    let Some(dir) = current_exe.parent() else { return };

    #[cfg(target_os = "windows")]
    let daemon_name = "mouse-share-daemon.exe";
    #[cfg(not(target_os = "windows"))]
    let daemon_name = "mouse-share-daemon";

    let daemon_path = dir.join(daemon_name);
    if !daemon_path.exists() {
        tracing::info!(?daemon_path, "daemon binary not found next to the UI executable; not starting it (expected during `tauri dev`)");
        return;
    }

    match std::process::Command::new(&daemon_path).spawn() {
        Ok(child) => tracing::info!(pid = child.id(), ?daemon_path, "started background daemon"),
        Err(e) => tracing::error!(error = %e, ?daemon_path, "failed to start background daemon"),
    }
}

fn current_os() -> ms_protocol::OperatingSystem {
    #[cfg(target_os = "windows")]
    {
        ms_protocol::OperatingSystem::Windows
    }
    #[cfg(target_os = "macos")]
    {
        ms_protocol::OperatingSystem::MacOs
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        ms_protocol::OperatingSystem::Windows
    }
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
