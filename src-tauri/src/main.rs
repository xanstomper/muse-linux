#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(test, allow(dead_code))]

use muse_linux_lib::{agent, daemon, tools};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};

const BRIDGE_JS: &str = include_str!("../../ui/native-bridge.js");

fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Main window is defined in code (not config) so we can attach
            // the native-bridge init script + devtools.
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External("https://muse.ai/".parse().expect("url")),
            )
            .title("Muse")
            .inner_size(1200.0, 860.0)
            .min_inner_size(480.0, 600.0)
            .initialization_script(BRIDGE_JS)
            .devtools(true)
            .build()
            .expect("main window");

            let toggle = MenuItem::with_id(app, "toggle", "Show/Hide Muse", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle, &quit])?;
            // real Meta Muse mark (muse.ai official brand asset)
            let icon = app.default_window_icon().cloned().expect("bundle icon");
            TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .tooltip("Muse")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => {
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tools::sys_exec,
            tools::fs_list,
            tools::fs_read,
            tools::fs_write,
            tools::fs_search,
            tools::procs,
            tools::sys_kill,
            tools::open_path,
            tools::notify,
            tools::push_to_muse,
            tools::set_yolo,
            tools::audit_tail,
            agent::term_exec,
            agent::term_reset,
            agent::git_status,
            agent::apply_patch,
            daemon::daemon_call,
            daemon::daemon_status,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Muse Linux");
}

fn main() {
    // Alder Lake-N iGPU: WebKit's DMA-buf renderer idles at ~46% CPU and
    // janks scrolling. Measured 2026-10-25: compositing-off = smoothest
    // scroll (311 jiffies vs 365) with 2% idle. Must be set pre-launch.
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    run();
}
