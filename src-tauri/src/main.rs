#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(test, allow(dead_code))]

use muse_linux_lib::{agent, daemon, tools};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

const BRIDGE_JS: &str = include_str!("../../ui/native-bridge.js");

/// Injected on every page load: desktop UX fixes for muse.ai.
const DESKTOP_CSS: &str = include_str!("../../ui/desktop.css");
const DESKTOP_JS: &str = include_str!("../../ui/desktop.js");

/// Device connect panel (pair a muse-daemon endpoint from the UI).
const CONNECT_JS: &str = include_str!("../../ui/connect.js");
const SETTINGS_JS: &str = include_str!("../../ui/settings.js");

fn window_state_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
        .join("window-state.txt")
}

fn read_window_state(
    p: &std::path::Path,
) -> Result<Option<(i32, i32, u32, u32, bool)>, Box<dyn std::error::Error>> {
    let Ok(txt) = std::fs::read_to_string(p) else {
        return Ok(None);
    };
    let parts: Vec<&str> = txt.split_whitespace().collect();
    if parts.len() != 5 {
        return Ok(None);
    }
    Ok(Some((
        parts[0].parse()?,
        parts[1].parse()?,
        parts[2].parse()?,
        parts[3].parse()?,
        parts[4] == "max",
    )))
}

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
            .initialization_script(DESKTOP_JS)
            .initialization_script(CONNECT_JS)
            .initialization_script(SETTINGS_JS)
            .initialization_script(&format!(
                "document.addEventListener('DOMContentLoaded',function(){{var s=document.createElement('style');s.id='muse-desktop-css';s.textContent={};document.head.appendChild(s);}});",
                serde_json::to_string(DESKTOP_CSS).expect("css json")
            ))
            .on_navigation(|url| {
                // External links leave the app via xdg-open; only muse.ai stays.
                let host = url.host_str().unwrap_or("");
                if host == "muse.ai" || host == "accounts.muse.ai" || host.is_empty() {
                    true
                } else {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(url.as_str())
                        .spawn();
                    false
                }
            })
            .devtools(true)
            .build()
            .expect("main window");

            let toggle = MenuItem::with_id(app, "toggle", "Show/Hide Muse", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle, &quit])?;
            // real Meta Muse mark (muse.ai official brand asset)
            let icon = app.default_window_icon().cloned().expect("bundle icon");
            let tray = TrayIconBuilder::new()
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
                // left-click on tray icon: restore/focus the window
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;
            let _ = tray;

            // n9: native menubar removed (File/Edit/View clashed with the dark UI).
            // Shortcuts live in ui/desktop.js: Ctrl+L new chat, Ctrl+Shift+D connect,
            // Ctrl+=/-/0 zoom, Ctrl+F find, Ctrl+R reload (browser default).
            let window = app.get_webview_window("main").expect("main window");

            // n6: restore persisted window state
            let state_path = window_state_path(app.handle());
            if let Ok(Some((x, y, wdt, hgt, max))) = read_window_state(&state_path) {
                if max {
                    let _ = window.maximize();
                } else {
                    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                    let _ = window.set_size(tauri::PhysicalSize::new(wdt, hgt));
                }
            }
            // n6: persist on change
            let sp = state_path.clone();
            let last = std::cell::RefCell::new((0i32, 0i32, 0u32, 0u32));
            window.on_window_event(move |event| {
                // Moved gives position-only, Resized size-only; merge them.
                {
                    let mut st = last.borrow_mut();
                    if let WindowEvent::Moved(p) = event {
                        st.0 = p.x;
                        st.1 = p.y;
                    }
                    if let WindowEvent::Resized(sz) = event {
                        st.2 = sz.width;
                        st.3 = sz.height;
                    }
                    if st.2 > 0 {
                        let _ = std::fs::create_dir_all(sp.parent().expect("cfg dir"));
                        let _ = std::fs::write(
                            &sp,
                            format!("{} {} {} {}", st.0, st.1, st.2, st.3),
                        );
                    }
                }
            });

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
            daemon::daemon_probe,
            daemon::daemon_connect,
            daemon::daemon_disconnect,
            daemon::daemon_get_connection,
            daemon::daemon_pair_start,
            daemon::daemon_pair_poll,
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
