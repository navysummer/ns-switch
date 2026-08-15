use crate::adapters;
use crate::db::AppState;
use crate::models::{Provider, ProviderInput, ToolInfo};
use crate::{state_err, AppResult};
use tauri::Emitter;

pub fn row_to_provider(row: &rusqlite::Row) -> rusqlite::Result<Provider> {
    Ok(Provider {
        id: row.get(0)?,
        tool_id: row.get(1)?,
        name: row.get(2)?,
        base_url: row.get(3)?,
        api_key: row.get(4)?,
        model: row.get(5)?,
        headers: row.get(6)?,
        shared_config: row.get(7)?,
        protocol: row.get(8)?,
        is_preset: row.get::<_, i64>(9)? != 0,
        is_active: row.get::<_, i64>(10)? != 0,
        is_universal: row.get::<_, i64>(11)? != 0,
    })
}

pub fn load_providers(state: &AppState, tool_id: &str) -> AppResult<Vec<Provider>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort
         FROM providers WHERE tool_id = ?1 ORDER BY is_active DESC, sort ASC, id ASC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![tool_id], row_to_provider)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn load_provider_by_id(state: &AppState, id: i64) -> AppResult<Option<Provider>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort
         FROM providers WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_provider)?;
    Ok(rows.next().transpose()?)
}

pub fn set_all_inactive(state: &AppState, tool_id: &str) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "UPDATE providers SET is_active = 0 WHERE tool_id = ?1",
        rusqlite::params![tool_id],
    )?;
    Ok(())
}

pub fn seed_presets(state: &AppState, tool_id: &str) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let key = format!("seeded:{tool_id}");
    let seeded: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", rusqlite::params![&key], |r| r.get(0))
        .ok();
    if seeded.is_some() {
        return Ok(());
    }
let presets = crate::presets::presets_for(tool_id);
    for p in presets {
        conn.execute(
            "INSERT INTO providers
             (tool_id, name, base_url, api_key, model, headers, is_preset, shared_config)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, '')",
            rusqlite::params![p.tool_id, p.name, p.base_url, p.api_key, p.model, p.headers],
        )?;
    }
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value = '1'",
        rusqlite::params![key],
    )?;
    Ok(())
}

pub fn provider_to_input(p: &Provider) -> ProviderInput {
    ProviderInput {
        id: Some(p.id),
        tool_id: p.tool_id.clone(),
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        api_key: p.api_key.clone(),
        model: p.model.clone(),
        headers: p.headers.clone(),
        is_universal: p.is_universal,
        shared_config: p.shared_config.clone(),
        protocol: p.protocol.clone(),
    }
}

/// rebuild the `sort` column from an explicit ordering of provider ids
#[tauri::command]
pub fn reorder_providers(state: tauri::State<AppState>, tool_id: String, ids: Vec<i64>) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let tx = conn.unchecked_transaction()?;
    for (i, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE providers SET sort = ?1 WHERE tool_id = ?2 AND id = ?3",
            rusqlite::params![i as i64, tool_id, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// remove ns-managed entries so the tool falls back to its official login / OAuth
#[tauri::command]
pub fn reset_to_official(state: tauri::State<AppState>, tool_id: String) -> AppResult<()> {
    crate::adapters::reset_config(&tool_id)?;
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "UPDATE providers SET is_active = 0 WHERE tool_id = ?1",
        rusqlite::params![tool_id],
    )?;
    Ok(())
}

/// current raw config JSON of a tool (minus api keys) used to populate shared config
#[tauri::command]
pub fn get_config_snapshot(
    state: tauri::State<AppState>,
    tool_id: String,
) -> AppResult<String> {
    drop(state);
    let path = adapters::config_path(&tool_id).ok_or("unsupported tool")?;
    if !path.exists() {
        return Ok("{}".into());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => serde_json::Value::Object(serde_json::Map::new()),
    };
    if let Some(obj) = v.as_object_mut() {
        obj.remove("env");
        obj.remove("mcpServers");
        obj.remove("model_providers");
    }
    Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?)
}

fn validate_input(input: &ProviderInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return state_err("name is required");
    }
    if !input.headers.trim().is_empty() {
        let _: serde_json::Value = serde_json::from_str(input.headers.trim())
            .map_err(|_| "headers must be valid JSON")?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_tools(state: tauri::State<AppState>) -> AppResult<Vec<ToolInfo>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare("SELECT id, name, visible FROM tools ORDER BY rowid")?;
    let rows = stmt.query_map([], |r| {
        let id: String = r.get(0)?;
        let name: String = r.get(1)?;
        let visible: i64 = r.get(2)?;
        let has_config = adapters::config_exists(&id);
        let config_path = adapters::config_path(&id).map(|p| p.to_string_lossy().to_string());
        Ok(ToolInfo { id, name, visible: visible != 0, has_config, config_path })
    })?;
    let rows: rusqlite::Result<Vec<_>> = rows.collect();
    Ok(rows?)
}

#[tauri::command]
pub fn set_tool_visible(state: tauri::State<AppState>, tool_id: String, visible: bool) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "UPDATE tools SET visible = ?1 WHERE id = ?2",
        rusqlite::params![visible as i64, tool_id],
    )?;
    Ok(())
}

#[tauri::command]
pub fn get_providers(state: tauri::State<AppState>, tool_id: String) -> AppResult<Vec<Provider>> {
    if tool_id != "universal" {
        seed_presets(&state, &tool_id)?;
    }
    load_providers(&state, &tool_id)
}

#[tauri::command]
pub fn get_active_provider(state: tauri::State<AppState>, tool_id: String) -> AppResult<Option<Provider>> {
    get_active_provider_impl(&state, &tool_id)
}

pub fn get_active_provider_impl(state: &AppState, tool_id: &str) -> AppResult<Option<Provider>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort
         FROM providers WHERE tool_id = ?1 AND is_active = 1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![tool_id], row_to_provider)?;
    Ok(rows.next().transpose()?)
}

#[tauri::command]
pub fn create_provider(state: tauri::State<AppState>, input: ProviderInput) -> AppResult<Provider> {
    validate_input(&input)?;
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "INSERT INTO providers (tool_id, name, base_url, api_key, model, headers, is_preset, is_universal, shared_config, protocol)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9)",
        rusqlite::params![
            input.tool_id,
            input.name.trim(),
            input.base_url.trim(),
            input.api_key.trim(),
            input.model.trim(),
            input.headers.trim(),
            input.is_universal as i64,
            input.shared_config.trim(),
            input.protocol.trim()
        ],
    )?;
    let id = conn.last_insert_rowid();
    drop(conn);
    load_provider_by_id(&state, id)?.ok_or_else(|| crate::err("provider not found"))
}

#[tauri::command]
pub fn update_provider(state: tauri::State<AppState>, input: ProviderInput) -> AppResult<Provider> {
    validate_input(&input)?;
    let id = input.id.ok_or_else(|| crate::err("missing id"))?;
    let old: Option<Provider> = {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort
             FROM providers WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![id], row_to_provider)?;
        rows.next().transpose()?
    };
    let Some(old) = old else { return Err("provider not found".into()) };
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "UPDATE providers SET name = ?1, base_url = ?2, api_key = ?3, model = ?4, headers = ?5,
         shared_config = ?6, is_universal = ?7, protocol = ?8 WHERE id = ?9",
        rusqlite::params![
            input.name.trim(),
            input.base_url.trim(),
            input.api_key.trim(),
            input.model.trim(),
            input.headers.trim(),
            input.shared_config.trim(),
            input.is_universal as i64,
            input.protocol.trim(),
            id
        ],
    )?;
    drop(conn);
    if old.is_active {
        let current = load_provider_by_id(&state, id)?.ok_or_else(|| crate::err("provider not found"))?;
        crate::config::write_active(&state, &current).ok();
    }
    load_provider_by_id(&state, id)?.ok_or_else(|| crate::err("provider not found"))
}

#[tauri::command]
pub fn delete_provider(state: tauri::State<AppState>, id: i64) -> AppResult<()> {
    let p = load_provider_by_id(&state, id)?.ok_or_else(|| crate::err("provider not found"))?;
    if p.is_active {
        return Err("cannot delete active provider".into());
    }
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute("DELETE FROM providers WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}

#[tauri::command]
pub fn duplicate_provider(state: tauri::State<AppState>, id: i64) -> AppResult<Provider> {
    let p = load_provider_by_id(&state, id)?.ok_or_else(|| crate::err("provider not found"))?;
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "INSERT INTO providers (tool_id, name, base_url, api_key, model, headers, is_preset, is_active, is_universal, shared_config, protocol)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7, ?8, ?9)",
        rusqlite::params![
            p.tool_id,
            format!("{} (copy)", p.name.trim_end()),
            p.base_url,
            p.api_key,
            p.model,
            p.headers,
            p.is_universal as i64,
            p.shared_config,
            p.protocol
        ],
    )?;
    let new_id = conn.last_insert_rowid();
    drop(conn);
    load_provider_by_id(&state, new_id)?.ok_or_else(|| crate::err("provider not found"))
}

#[tauri::command]
pub fn switch_provider(app: tauri::AppHandle, state: tauri::State<AppState>, tool_id: String, provider_id: i64) -> AppResult<Provider> {
    let p = load_provider_by_id(&state, provider_id)?.ok_or_else(|| crate::err("provider not found"))?;
    set_all_inactive(&state, &tool_id)?;
    {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        conn.execute(
            "UPDATE providers SET is_active = 1 WHERE id = ?1",
            rusqlite::params![provider_id],
        )?;
    }
    crate::config::write_active(&state, &p)?;
    let _ = app.emit(
        "provider-switched",
        serde_json::json!({ "toolId": tool_id, "providerId": provider_id }),
    );
    let _ = crate::commands::settings::user_notify(&app, &state, "NS Switch", &format!("Switched to {}", p.name));
    crate::tray::rebuild_tray(&app);
    Ok(p)
}

#[tauri::command]
pub fn get_presets(state: tauri::State<AppState>, tool_id: String) -> AppResult<Vec<Provider>> {
    seed_presets(&state, &tool_id)?;
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort
         FROM providers WHERE tool_id = ?1 AND is_preset = 1 ORDER BY rowid",
    )?;
    let rows = stmt.query_map(rusqlite::params![tool_id], row_to_provider)?;
    let rows: rusqlite::Result<Vec<_>> = rows.collect();
    Ok(rows?)
}

#[tauri::command]
pub fn export_providers(state: tauri::State<AppState>) -> AppResult<String> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort
         FROM providers WHERE is_preset = 0 ORDER BY tool_id, is_active DESC, id",
    )?;
    let rows = stmt.query_map([], row_to_provider)?;
    let items: Vec<Provider> = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let payload = serde_json::json!({ "version": 1, "app": "ns-switch", "providers": items });
    Ok(serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn import_providers(state: tauri::State<AppState>, data: String) -> AppResult<usize> {
    let parsed: serde_json::Value =
        serde_json::from_str(&data).map_err(|_| "invalid JSON")?;
    let arr = parsed
        .get("providers")
        .and_then(|v| v.as_array())
        .map(Clone::clone)
        .ok_or_else(|| crate::err("invalid format, expected { providers: [...] }"))?;
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut count = 0usize;
    for item in arr {
        let input: ProviderInput = serde_json::from_value(item).map_err(|_| "invalid provider item")?;
        conn.execute(
            "INSERT INTO providers (tool_id, name, base_url, api_key, model, headers, is_preset, is_active, is_universal, shared_config, protocol)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7, ?8, ?9)",
            rusqlite::params![
                input.tool_id,
                input.name,
                input.base_url,
                input.api_key,
                input.model,
                input.headers,
                input.is_universal as i64,
                input.shared_config,
                input.protocol
            ],
        )?;
        count += 1;
    }
    Ok(count)
}

#[tauri::command]
pub fn import_from_cli(state: tauri::State<AppState>, tool_id: String) -> AppResult<Provider> {
    let (base, key, model) = crate::config::read_active(&state, &tool_id);
    if base.is_empty() && key.is_empty() {
        return Err("no existing configuration found".into());
    }
    let input = ProviderInput {
        id: None,
        tool_id: tool_id.clone(),
        name: "Imported".to_string(),
        base_url: base,
        api_key: key,
        model,
        headers: "{}".to_string(),
        is_universal: false,
        shared_config: String::new(),
        protocol: String::new(),
    };
    create_provider(state, input)
}

/// first-run: import an existing CLI config for every tool that has one
#[tauri::command]
pub fn import_all_from_cli(state: tauri::State<AppState>) -> AppResult<Vec<Provider>> {
    let mut out = Vec::new();
    for (tid, _) in crate::db::ALL_TOOLS {
        let tid = tid.to_string();
        let (base, key, model) = crate::config::read_active(&state, &tid);
        if base.is_empty() && key.is_empty() {
            continue;
        }
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        conn.execute(
            "INSERT INTO providers (tool_id, name, base_url, api_key, model, headers, is_preset, is_universal, shared_config, protocol)
             VALUES (?1, 'Imported', ?2, ?3, ?4, '{}', 0, 0, '', '')",
            rusqlite::params![tid, base, key, model],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        if let Some(p) = load_provider_by_id(&state, id)? {
            out.push(p);
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn onboarding_status(state: tauri::State<AppState>) -> AppResult<bool> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let any_provider: i64 = conn
        .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
        .unwrap_or(1);
    let done = crate::db::get_settings_value(&conn)
        .get("onboarding_done")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    Ok(!done && any_provider == 0)
}

#[tauri::command]
pub fn complete_onboarding(state: tauri::State<AppState>) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut v = crate::db::get_settings_value(&conn);
    if let Some(obj) = v.as_object_mut() {
        obj.insert("onboarding_done".into(), serde_json::json!(true));
    }
    crate::db::set_settings_value(&conn, &v)?;
    Ok(())
}