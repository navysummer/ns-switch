use crate::db::AppState;
use crate::models::{McpServer, McpServerInput};
use crate::{state_err, AppResult};

pub fn row_to_server(row: &rusqlite::Row) -> rusqlite::Result<McpServer> {
    let args_str: String = row.get(3)?;
    let env_str: String = row.get(4)?;
    let tools_str: String = row.get(8)?;
    Ok(McpServer {
        id: row.get(0)?,
        name: row.get(1)?,
        command: row.get(2)?,
        args: serde_json::from_str(&args_str).unwrap_or_default(),
        env: serde_json::from_str(&env_str).unwrap_or_default(),
        cwd: row.get(5)?,
        transport: row.get(6)?,
        is_active: row.get::<_, i64>(7)? != 0,
        tools: serde_json::from_str(&tools_str).unwrap_or_default(),
    })
}

#[tauri::command]
pub fn mcp_list(state: tauri::State<AppState>) -> AppResult<Vec<McpServer>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, name, command, args, env, cwd, transport, is_active, tools
         FROM mcp_servers ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_to_server)?;
    let rows: rusqlite::Result<Vec<_>> = rows.collect();
    Ok(rows?)
}

pub fn mcp_servers_for_tool(state: &AppState, tool_id: &str) -> Vec<McpServer> {
    let Ok(conn) = state.db.lock() else { return vec![] };
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, name, command, args, env, cwd, transport, is_active, tools
         FROM mcp_servers ORDER BY id",
    ) else {
        return vec![];
    };
    let rows: Vec<McpServer> = stmt
        .query_map([], row_to_server)
        .and_then(|r| r.collect())
        .unwrap_or_default();
    rows.into_iter().filter(|s| s.is_active && s.tools.iter().any(|t| t == tool_id)).collect()
}

#[tauri::command]
pub fn mcp_save(state: tauri::State<AppState>, input: McpServerInput) -> AppResult<McpServer> {
    if input.name.trim().is_empty() {
        return state_err("name is required");
    }
    if input.command.trim().is_empty() {
        return state_err("command is required");
    }
    let args = serde_json::to_string(&input.args).map_err(|e| e.to_string())?;
    let env = serde_json::to_string(&input.env).map_err(|e| e.to_string())?;
    let tools = serde_json::to_string(&input.tools).map_err(|e| e.to_string())?;
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let id = match input.id {
        Some(id) => {
            conn.execute(
                "UPDATE mcp_servers SET name = ?1, command = ?2, args = ?3, env = ?4,
                 cwd = ?5, transport = ?6, is_active = ?7, tools = ?8 WHERE id = ?9",
                rusqlite::params![
                    input.name.trim(),
                    input.command.trim(),
                    args,
                    env,
                    input.cwd.trim(),
                    input.transport.trim(),
                    input.is_active as i64,
                    tools,
                    id
                ],
            )?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO mcp_servers (name, command, args, env, cwd, transport, is_active, tools)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    input.name.trim(),
                    input.command.trim(),
                    args,
                    env,
                    input.cwd.trim(),
                    input.transport.trim(),
                    input.is_active as i64,
                    tools
                ],
            )?;
            conn.last_insert_rowid()
        }
    };
    drop(conn);
    let server = load_by_id(&state, id)?.ok_or_else(|| crate::err("mcp server not found"))?;
    for tid in &server.tools {
        crate::config::sync_mcp_for_tool(&state, tid);
    }
    Ok(server)
}

#[tauri::command]
pub fn mcp_delete(state: tauri::State<AppState>, id: i64) -> AppResult<()> {
    let server = load_by_id(&state, id)?.ok_or_else(|| crate::err("mcp server not found"))?;
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute("DELETE FROM mcp_servers WHERE id = ?1", rusqlite::params![id])?;
    drop(conn);
    for tid in &server.tools {
        crate::config::sync_mcp_for_tool(&state, tid);
    }
    Ok(())
}

#[tauri::command]
pub fn mcp_sync(state: tauri::State<AppState>) -> AppResult<std::collections::HashMap<String, String>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, name, command, args, env, cwd, transport, is_active, tools
         FROM mcp_servers ORDER BY id",
    )?;
    let servers: Vec<McpServer> = stmt
        .query_map([], row_to_server)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    drop(conn);

    // bidirectional sync: import servers that already exist in tool configs
    // (so edits made directly in the CLI appear in this panel)
    for tid in crate::adapters::all_tool_ids() {
        let existing = crate::adapters::read_mcp_from_config(&tid);
        for (name, spec) in existing {
            if servers.iter().any(|s| s.name == name) {
                continue;
            }
            if let Ok(c) = state.db.lock() {
                let (command, args, env, cwd, transport, active) = spec;
                let _ = c.execute(
                    "INSERT INTO mcp_servers (name, command, args, env, cwd, transport, is_active, tools)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        name,
                        command,
                        serde_json::to_string(&args).unwrap_or("[]".into()),
                        serde_json::to_string(&env).unwrap_or("{}".into()),
                        cwd,
                        transport,
                        active as i64,
                        serde_json::to_string(&vec![tid.clone()]).unwrap_or("[]".into())
                    ],
                );
            }
        }
    }

    let mut out = std::collections::HashMap::new();
    for tid in crate::adapters::all_tool_ids() {
        let applicable: Vec<McpServer> = servers
            .iter()
            .filter(|s| s.is_active && s.tools.iter().any(|t| *t == tid))
            .cloned()
            .collect();
        match crate::adapters::write_mcp_to_tool(&tid, &applicable) {
            Ok(_) => {
                out.insert(tid, "ok".to_string());
            }
            Err(e) => {
                out.insert(tid, format!("err:{e}"));
            }
        }
    }
    Ok(out)
}

fn load_by_id(state: &AppState, id: i64) -> AppResult<Option<McpServer>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, name, command, args, env, cwd, transport, is_active, tools
         FROM mcp_servers WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_server)?;
    Ok(rows.next().transpose()?)
}