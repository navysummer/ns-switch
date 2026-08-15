use crate::db::{self, AppState};
use crate::models::AppSettings;
use crate::AppResult;
use serde_json::Value;

#[tauri::command]
pub fn get_settings(state: tauri::State<AppState>) -> AppResult<AppSettings> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let v = db::get_settings_value(&conn);
    let dir = state.active_data_dir();
    Ok(settings_from_json(&v, &dir))
}

#[tauri::command]
pub fn set_settings(state: tauri::State<AppState>, settings: Value) -> AppResult<AppSettings> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let current = db::get_settings_value(&conn);
    let mut merged = current.as_object().cloned().unwrap_or_default();
    if let Some(obj) = settings.as_object() {
        for (k, v) in obj {
            merged.insert(k.clone(), v.clone());
        }
    }
    let new_value = Value::Object(merged);
    db::set_settings_value(&conn, &new_value)?;
    let dir = state.active_data_dir();
    let settings = settings_from_json(&new_value, &dir);
    Ok(settings)
}

fn settings_from_json(v: &Value, data_dir: &std::path::Path) -> AppSettings {
    use std::collections::HashMap;
    let theme = v.get("theme").and_then(|x| x.as_str()).unwrap_or("system").to_string();
    let language = v.get("language").and_then(|x| x.as_str()).unwrap_or("zh").to_string();
    let mut proxy_ports: HashMap<String, u16> = HashMap::new();
    if let Some(p) = v.get("proxyPorts").and_then(|x| x.as_object()) {
        for (k, val) in p {
            if let Some(n) = val.as_u64() {
                proxy_ports.insert(k.clone(), n.min(65535) as u16);
            }
        }
    }
    let mut proxy_enabled: HashMap<String, bool> = HashMap::new();
    if let Some(e) = v.get("proxyEnabled").and_then(|x| x.as_object()) {
        for (k, val) in e {
            if let Some(b) = val.as_bool() {
                proxy_enabled.insert(k.clone(), b);
            }
        }
    }
    let notifications = v.get("notifications").and_then(|x| x.as_bool()).unwrap_or(true);
    AppSettings {
        theme,
        language,
        data_dir: data_dir.to_string_lossy().to_string(),
        proxy_ports,
        proxy_enabled,
        notifications,
    }
}

#[tauri::command]
pub fn notify(app: tauri::AppHandle, state: tauri::State<AppState>, title: String, body: String) -> AppResult<()> {
    user_notify(&app, &state, &title, &body)
}

pub fn user_notify(app: &tauri::AppHandle, state: &AppState, title: &str, body: &str) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let enabled = crate::db::get_settings_value(&conn)
        .get("notifications")
        .and_then(|x| x.as_bool())
        .unwrap_or(true);
    drop(conn);
    if !enabled {
        return Ok(());
    }
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
    Ok(())
}

#[tauri::command]
pub fn backup_now(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> AppResult<crate::models::BackupInfo> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let name = format!("ns-switch-{stamp}.db");
    let dest = state.active_data_dir().join("backups").join(&name);
    let src = state.active_data_dir().join("ns-switch.db");
    {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
    if src.exists() {
        std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    }
    crate::db::rotate_backups(&state);
    let _ = user_notify(&app, &state, "NS Switch", &format!("Backup created: {name}"));
    Ok(crate::models::BackupInfo {
        path: dest.to_string_lossy().to_string(),
        name,
        created: chrono::Local::now().to_rfc3339(),
        size: std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
    })
}

#[tauri::command]
pub fn list_backups(state: tauri::State<AppState>) -> AppResult<Vec<crate::models::BackupInfo>> {
    let dir = state.active_data_dir().join("backups");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "db").unwrap_or(false) {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let metadata = std::fs::metadata(&p).ok();
                out.push(crate::models::BackupInfo {
                    path: p.to_string_lossy().to_string(),
                    name: name.clone(),
                    created: metadata
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .map(|t| {
                            let dt: chrono::DateTime<chrono::Local> = t.into();
                            dt.to_rfc3339()
                        })
                        .unwrap_or_default(),
                    size: metadata.map(|m| m.len()).unwrap_or(0),
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[tauri::command]
pub fn restore_backup(state: tauri::State<AppState>, path: String) -> AppResult<()> {
    let src = std::path::PathBuf::from(&path);
    if !src.exists() {
        return Err("backup file not found".into());
    }
    // fresh connection to copy the file while db is closed
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    drop(conn);
    let dst = state.active_data_dir().join("ns-switch.db");
    std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(state.active_data_dir().join("ns-switch.db-wal"));
    let _ = std::fs::remove_file(state.active_data_dir().join("ns-switch.db-shm"));
    Ok(())
}

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// check for a new release from a configurable JSON endpoint:
/// { "version": "x.y.z", "url": "..." }
#[tauri::command]
pub async fn check_updates(state: tauri::State<'_, AppState>) -> AppResult<crate::models::UpdateResult> {
    let url: Option<String> = {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        let v = db::get_settings_value(&conn);
        v.get("updateUrl").and_then(|x| x.as_str()).map(ToString::to_string)
    };
    let Some(mut url) = url.filter(|u| !u.trim().is_empty()) else {
        return Ok(crate::models::UpdateResult {
            current: APP_VERSION.to_string(),
            latest: None,
            has_update: false,
            url: None,
        });
    };
    if !url.trim_start().starts_with("http") {
        url = format!("https://{url}");
    }
    let resp = reqwest::get(&url).await.map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let latest = v
        .get("version")
        .or_else(|| v.get("tag_name"))
        .and_then(|x| x.as_str())
        .map(ToString::to_string);
    let dl = v.get("url").and_then(|x| x.as_str()).map(ToString::to_string);
    let has_update = latest
        .as_ref()
        .map(|l| version_newer(l, APP_VERSION))
        .unwrap_or(false);
    Ok(crate::models::UpdateResult { current: APP_VERSION.to_string(), latest, has_update, url: dl })
}

fn version_newer(latest: &str, current: &str) -> bool {
    let cut = |s: &str| {
        s.trim_start_matches(|c: char| !c.is_ascii_digit())
            .split('.')
            .filter_map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok())
            .collect::<Vec<u32>>()
    };
    let a = cut(latest);
    let b = cut(current);
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}