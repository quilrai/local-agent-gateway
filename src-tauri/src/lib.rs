// DLP Demo App - Main Library
//
// A Tauri app that proxies LLM API requests with DLP (Data Loss Prevention) capabilities.
// Currently supports Claude (Anthropic), with plans for OpenAI, Gemini, etc.

mod backends;
mod commands;
mod cursor_hooks;
mod database;
mod dlp;
mod dlp_pattern_config;
mod proxy;
mod requestresponsemetadata;

use database::get_port_from_db;
use dlp_pattern_config::DEFAULT_PORT;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, WindowEvent,
};
use tokio::sync::watch;

#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;

// Helper to show window and set dock visibility on macOS
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(ActivationPolicy::Regular);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// Helper to hide window and hide from dock on macOS
fn hide_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(ActivationPolicy::Accessory);
    }
}

// Global state for reverse proxy control
pub static PROXY_PORT: std::sync::LazyLock<Arc<Mutex<u16>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(DEFAULT_PORT)));
pub static RESTART_SENDER: std::sync::LazyLock<Arc<Mutex<Option<watch::Sender<bool>>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(None)));

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize reverse proxy port from database
    {
        let port = get_port_from_db();
        let mut current_port = PROXY_PORT.lock().unwrap();
        *current_port = port;
    }

    // Spawn reverse proxy server
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(proxy::start_proxy_server());
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Create tray menu items
            let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let hide_item = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            // Build the tray menu
            let menu = Menu::with_items(app, &[&show_item, &hide_item, &quit_item])?;

            // Build the tray icon
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        show_window(app);
                    }
                    "hide" => {
                        hide_window(app);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Prevent the window from closing, hide it instead
                api.prevent_close();
                let app = window.app_handle();
                hide_window(&app);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::get_dashboard_stats,
            commands::get_backends,
            commands::get_models,
            commands::get_message_logs,
            commands::get_port_setting,
            commands::save_port_setting,
            commands::restart_proxy,
            commands::get_dlp_settings,
            commands::add_dlp_pattern,
            commands::update_dlp_pattern,
            commands::toggle_dlp_pattern,
            commands::delete_dlp_pattern,
            commands::get_dlp_detection_stats,
            commands::get_dlp_action_setting,
            commands::save_dlp_action_setting,
            commands::set_shell_env,
            commands::check_shell_env,
            commands::remove_shell_env,
            commands::install_cursor_hooks,
            commands::uninstall_cursor_hooks,
            commands::check_cursor_hooks_installed,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
