use crate::db::AppState;
use crate::models::CloudSettings;
use crate::AppResult;
use serde_json::{json, Value};
use tauri::Emitter;

fn cloud_file(state: &AppState) -> std::path::PathBuf {
    state.active_data_dir().join("cloud.json")
}

fn load_cloud(state: &AppState) -> CloudSettings {
    if let Ok(s) = std::fs::read_to_string(cloud_file(state)) {
        if let Ok(c) = serde_json::from_str::<CloudSettings>(&s) {
            return c;
        }
    }
    CloudSettings { url: String::new(), username: String::new(), password: String::new(), enabled: false }
}

#[tauri::command]
pub fn cloud_get(state: tauri::State<AppState>) -> AppResult<CloudSettings> {
    Ok(load_cloud(&state))
}

#[tauri::command]
pub fn cloud_set(state: tauri::State<AppState>, settings: CloudSettings) -> AppResult<CloudSettings> {
    std::fs::write(cloud_file(&state), serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(settings)
}

/// move the whole data dir (db, backups, skills, cloud config) to a new location
#[tauri::command]
pub fn cloud_set_config_dir(state: tauri::State<AppState>, new_dir: String) -> AppResult<String> {
    let target = std::path::PathBuf::from(&new_dir);
    if target.as_os_str().is_empty() {
        return Err("empty path".into());
    }
    if !target.exists() {
        std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    }
    if !target.is_dir() {
        return Err("not a directory".into());
    }
    // checkpoint then copy the database
    {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
    let src = state.active_data_dir().join("ns-switch.db");
    let dst = target.join("ns-switch.db");
    if src.exists() {
        std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    }
    for sub in ["backups", "skills"] {
        std::fs::create_dir_all(target.join(sub)).map_err(|e| e.to_string())?;
    }
    if let Ok(mut guard) = state.data_dir_override.lock() {
        *guard = Some(target.clone());
    }
    // persist the choice so providers stored under the previous dir are reachable
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut settings = crate::db::get_settings_value(&conn);
    if let Some(obj) = settings.as_object_mut() {
        obj.insert("dataDir".to_string(), json!(target.to_string_lossy().to_string()));
    }
    crate::db::set_settings_value(&conn, &settings)?;
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn cloud_test(state: tauri::State<'_, AppState>) -> AppResult<String> {
    let cs = load_cloud(&state);
    if cs.url.trim().is_empty() {
        return Err("no webdav url configured".into());
    }
    let base = cs.url.trim_end_matches('/').to_string();
    let client = make_client(&cs)?;
    let url = format!("{base}/ns-switch/");
    match client.get(&url).send().await {
        Ok(r) if r.status().is_success() || r.status().as_u16() == 404 => Ok(format!("connected ({} {})", url, r.status().as_u16())),
        Ok(r) => Err(crate::err(&format!("webdav responded {}", r.status()))),
        Err(e) => Err(crate::err(&format!("connection failed: {e}"))),
    }
}

#[tauri::command]
pub async fn cloud_push(state: tauri::State<'_, AppState>) -> AppResult<String> {
    let cs = load_cloud(&state);
    if cs.url.trim().is_empty() {
        return Err("no webdav url configured".into());
    }
    {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
    let local = std::fs::read(state.active_data_dir().join("ns-switch.db")).map_err(|e| e.to_string())?;
    let base = cs.url.trim_end_matches('/').to_string();
    let client = make_client(&cs)?;
    let url = format!("{base}/ns-switch/ns-switch.db");
    match client.put(&url).body(local).send().await {
        Ok(r) if r.status().is_success() || r.status().as_u16() == 201 || r.status().as_u16() == 204 => {
            Ok(format!("pushed {} bytes", std::fs::metadata(state.active_data_dir().join("ns-switch.db")).map(|m| m.len()).unwrap_or(0)))
        }
        Ok(r) => Err(crate::err(&format!("push failed: {}", r.status()))),
        Err(e) => Err(crate::err(&format!("push failed: {e}"))),
    }
}

#[tauri::command]
pub async fn cloud_pull(state: tauri::State<'_, AppState>, app: tauri::AppHandle) -> AppResult<String> {
    let cs = load_cloud(&state);
    if cs.url.trim().is_empty() {
        return Err("no webdav url configured".into());
    }
    let base = cs.url.trim_end_matches('/').to_string();
    let client = make_client(&cs)?;
    let url = format!("{base}/ns-switch/ns-switch.db");
    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => return Err(crate::err(&format!("pull failed: {}", r.status()))),
        Err(e) => return Err(crate::err(&format!("pull failed: {e}"))),
    };
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let tmp = state.active_data_dir().join("ns-switch.webdav.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    // swap the database file (applies after restart)
    {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        drop(conn);
    }
    let dst = state.active_data_dir().join("ns-switch.db");
    std::fs::copy(&tmp, &dst).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(state.active_data_dir().join("ns-switch.db-wal"));
    let _ = std::fs::remove_file(state.active_data_dir().join("ns-switch.db-shm"));
    let _ = std::fs::remove_file(&tmp);
    let _ = app.emit("data-pulled", {});
    Ok(format!("pulled {} bytes, restart to apply", bytes.len()))
}

fn make_client(cs: &CloudSettings) -> AppResult<reqwest::Client> {
    use base64::Engine;
    let mut b = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20));
    if !cs.username.is_empty() {
        let token = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", cs.username, cs.password));
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Basic {token}"))
                .map_err(|e| crate::err(&e.to_string()))?,
        );
        b = b.default_headers(headers);
    }
    b.build().map_err(|e| crate::err(&e.to_string()))
}

#[tauri::command]
pub fn cloud_info(state: tauri::State<AppState>) -> AppResult<Value> {
    let cs = load_cloud(&state);
    Ok(json!({
        "url": cs.url,
        "enabled": cs.enabled,
        "dataDir": state.active_data_dir().to_string_lossy().to_string(),
    }))
}