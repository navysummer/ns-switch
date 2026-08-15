use crate::db::{AppState, ALL_TOOLS};
use crate::models::Provider;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

fn load_all_providers(state: &AppState) -> Vec<(String, String, Vec<Provider>)> {
    let mut out = Vec::new();
    // only include tools that are visible in the app
    let visible: Vec<String> = {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return out,
        };
        let mut stmt = match conn.prepare("SELECT id FROM tools WHERE visible = 1") {
            Ok(s) => s,
            Err(_) => return out,
        };
        stmt.query_map([], |r| r.get::<_, String>(0))
            .ok()
            .map(|rows| rows.flatten().collect::<Vec<String>>())
            .unwrap_or_default()
    };
    for (tid, tname) in ALL_TOOLS {
        if !visible.iter().any(|v| v == tid) {
            continue;
        }
        let providers = crate::commands::providers::load_providers(&state, tid).unwrap_or_default();
        if providers.is_empty() {
            continue;
        }
        out.push((tid.to_string(), tname.to_string(), providers));
    }
    // universal providers (apply to several tools) get their own submenu
    if let Ok(universal) = crate::commands::providers::load_providers(&state, "universal") {
        if !universal.is_empty() {
            out.push(("universal".to_string(), "Universal".to_string(), universal));
        }
    }
    out
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let open = MenuItem::with_id(app, "open", "Open NS Switch", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(app, &[&open, &sep])?;

    for (tid, tname, providers) in load_all_providers(&state) {
        if providers.is_empty() {
            continue;
        }
        let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();
        for p in &providers {
            let item = CheckMenuItem::with_id(
                app,
                format!("switch:{tid}:{}", p.id),
                p.name.as_str(),
                true,
                p.is_active,
                None::<&str>,
            )?;
            items.push(Box::new(item));
        }
        let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
            items.iter().map(|b| b.as_ref()).collect();
        let sub = Submenu::with_items(app, tname, true, &refs)?;
        menu.append(&sub)?;
    }

    menu.append(&sep)?;
    menu.append(&quit)?;
    Ok(menu)
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
    TrayIconBuilder::with_id("ns-switch")
        .icon(icon)
        .menu(&menu)
        .tooltip("NS Switch")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id.as_ref());
        })
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
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "open" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "quit" => {
            app.exit(0);
        }
        _ => {
            if let Some(rest) = id.strip_prefix("switch:") {
                let mut parts = rest.splitn(3, ':');
                if let (Some(tool), Some(pid)) = (parts.next(), parts.next().and_then(|x| x.parse::<i64>().ok())) {
                    let state = app.state::<AppState>();
                    if let Ok(_p) = crate::commands::providers::switch_provider(app.clone(), state, tool.to_string(), pid) {
                        let _ = rebuild(app);
                    }
                }
            }
        }
    }
}

pub fn rebuild(app: &AppHandle) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("ns-switch") {
        let menu = build_menu(app)?;
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

pub fn rebuild_tray(app: &AppHandle) {
    let _ = rebuild(app);
}