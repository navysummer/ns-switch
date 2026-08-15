use crate::db::AppState;
use crate::models::{PromptInput, PromptItem};
use crate::{state_err, AppResult};
use std::fs;
use std::path::PathBuf;

fn row_to_prompt(row: &rusqlite::Row) -> rusqlite::Result<PromptItem> {
    Ok(PromptItem {
        id: row.get(0)?,
        name: row.get(1)?,
        content: row.get(2)?,
        file_type: row.get(3)?,
        is_active: row.get::<_, i64>(4)? != 0,
    })
}

#[tauri::command]
pub fn prompts_list(state: tauri::State<AppState>) -> AppResult<Vec<PromptItem>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare("SELECT id, name, content, file_type, is_active FROM prompts ORDER BY id DESC")?;
    let rows = stmt.query_map([], row_to_prompt)?;
    let out: rusqlite::Result<Vec<PromptItem>> = rows.collect();
    Ok(out?)
}

#[tauri::command]
pub fn prompt_save(state: tauri::State<AppState>, input: PromptInput) -> AppResult<PromptItem> {
    if input.name.trim().is_empty() {
        return state_err("name is required");
    }
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    match input.id {
        Some(id) => {
            conn.execute(
                "UPDATE prompts SET name = ?1, content = ?2, file_type = ?3 WHERE id = ?4",
                rusqlite::params![input.name.trim(), input.content, input.file_type, id],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO prompts (name, content, file_type) VALUES (?1, ?2, ?3)",
                rusqlite::params![input.name.trim(), input.content, input.file_type],
            )?;
        }
    }
    let id = input.id.unwrap_or_else(|| conn.last_insert_rowid());
    drop(conn);
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare("SELECT id, name, content, file_type, is_active FROM prompts WHERE id = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_prompt)?;
    rows.next().transpose()?.ok_or_else(|| crate::err("prompt not found"))
}

#[tauri::command]
pub fn prompt_delete(state: tauri::State<AppState>, id: i64) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute("DELETE FROM prompts WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}

#[tauri::command]
pub fn prompt_activate(state: tauri::State<AppState>, id: i64) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let ft: Option<String> = conn
        .query_row("SELECT file_type FROM prompts WHERE id = ?1", rusqlite::params![id], |r| r.get(0))
        .ok();
    let Some(ft) = ft else { return state_err("prompt not found") };
    let content: String = conn
        .query_row("SELECT content FROM prompts WHERE id = ?1", rusqlite::params![id], |r| r.get(0))
        .map_err(|_| "prompt not found")?;
    conn.execute("UPDATE prompts SET is_active = 0 WHERE file_type = ?1", rusqlite::params![ft])?;
    conn.execute("UPDATE prompts SET is_active = 1 WHERE id = ?1", rusqlite::params![id])?;
    drop(conn);

    let target = prompt_file(&ft).ok_or_else(|| crate::err("unsupported file type"))?;
    let section = manage_section(&ft, &content);
    write_target(&target, &ft, &section)?;
    Ok(())
}

/// deactivate a prompt and remove its managed section from the target file
#[tauri::command]
pub fn prompt_deactivate(state: tauri::State<AppState>, id: i64) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let ft: Option<String> = conn
        .query_row("SELECT file_type FROM prompts WHERE id = ?1", rusqlite::params![id], |r| r.get(0))
        .ok();
    let Some(ft) = ft else { return state_err("prompt not found") };
    conn.execute("UPDATE prompts SET is_active = 0 WHERE id = ?1", rusqlite::params![id])?;
    drop(conn);
    if let Some(target) = prompt_file(&ft) {
        if target.exists() {
            if let Ok(existing) = fs::read_to_string(&target) {
                if let Some(cleaned) = remove_managed_section(&existing, &ft) {
                    let _ = fs::write(&target, cleaned);
                }
            }
        }
    }
    Ok(())
}

fn manage_section(ft: &str, content: &str) -> String {
    format!(
        "\n<!-- ns-switch:managed:{ft}:START -->\n{}\n<!-- ns-switch:managed:{ft}:END -->\n",
        content.trim()
    )
}

fn remove_managed_section(existing: &str, ft: &str) -> Option<String> {
    let start_marker = format!("<!-- ns-switch:managed:{ft}:START -->");
    let end_marker = format!("<!-- ns-switch:managed:{ft}:END -->");
    if existing.contains(&start_marker) && existing.contains(&end_marker) {
        let start = existing.find(&start_marker)?;
        let end = existing.find(&end_marker)? + end_marker.len();
        Some(format!("{}{}", &existing[..start], &existing[end..]))
    } else {
        None
    }
}

fn write_target(target: &std::path::Path, ft: &str, section: &str) -> AppResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let existing = fs::read_to_string(target).unwrap_or_default();
    let body = match remove_managed_section(&existing, ft) {
        Some(cleaned) => format!("{cleaned}{section}"),
        None => format!("{existing}{section}"),
    };
    fs::write(target, body).map_err(|e| e.to_string())?;
    Ok(())
}

fn prompt_file(file_type: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match file_type {
        "CLAUDE_MD" => Some(home.join(".claude").join("CLAUDE.md")),
        "AGENTS_MD" => Some(xdg_config_home().join("opencode").join("AGENTS.md")),
        "GEMINI_MD" => Some(home.join(".gemini").join("GEMINI.md")),
        "OPENCODE_MD" => Some(xdg_config_home().join("opencode").join("AGENTS.md")),
        _ => None,
    }
}

fn xdg_config_home() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(x);
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_section_roundtrip() {
        let sec = manage_section("CLAUDE_MD", "be nice");
        assert!(sec.contains("CLAUDE_MD:START"));
        // adding to a file then removing leaves nothing but the original content
        let file = "existing\n";
        let updated = format!("{file}{sec}");
        let cleaned = remove_managed_section(&updated, "CLAUDE_MD").unwrap();
        assert_eq!(cleaned.trim(), file.trim());
        // ids that don't match are untouched
        assert!(remove_managed_section(&updated, "AGENTS_MD").is_none());
    }
}