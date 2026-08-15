use crate::db::AppState;
use crate::models::{SkillItem, SkillSource};
use crate::{state_err, AppResult};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}
#[cfg(not(target_os = "windows"))]
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

fn row_to_skill(row: &rusqlite::Row) -> rusqlite::Result<SkillItem> {
    Ok(SkillItem {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        source: row.get(3)?,
        status: row.get(4)?,
        install_mode: row.get(5)?,
        installed_at: row.get(6)?,
    })
}

#[tauri::command]
pub fn skills_list(state: tauri::State<AppState>) -> AppResult<Vec<SkillItem>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, name, description, source, status, install_mode, installed_at FROM skills ORDER BY id DESC",
    )?;
    let rows = stmt.query_map([], row_to_skill)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn skills_dir(state: &AppState) -> PathBuf {
    state.active_data_dir().join("skills")
}

/// skills are installed (symlinked or copied) into these application skill folders
fn app_skill_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = dirs::home_dir() {
        out.push(home.join(".claude").join("skills"));
    }
    if let Some(cfg) = xdg_config_home() {
        out.push(cfg.join("opencode").join("skill"));
    }
    out
}

fn xdg_config_home() -> Option<PathBuf> {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(x));
    }
    dirs::config_dir()
}

#[tauri::command]
pub fn skill_install(
    state: tauri::State<AppState>,
    source: String,
    mode: Option<String>,
) -> AppResult<SkillItem> {
    skill_install_impl(&state, source, mode.unwrap_or_else(|| "symlink".to_string()))
}

/// mode: "symlink" (default) or "copy"
pub fn skill_install_impl(state: &AppState, source: String, mode: String) -> AppResult<SkillItem> {
    let source = source.trim().to_string();
    if source.is_empty() {
        return state_err("source is empty");
    }
    let base = skills_dir(state);
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;

    let tmp = base.join(format!(".tmp-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    let result = if source.starts_with("http://") || source.starts_with("https://") {
        clone_git(&source, &tmp)
    } else if source.ends_with(".zip") {
        unzip(&source, &tmp)
    } else if Path::new(&source).is_dir() {
        fs_extra_copy(&source, &tmp)
    } else {
        Err("unsupported source (git url, .zip or directory)".to_string())
    };

    let (name, description) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_dir_all(&tmp);
            return Err(crate::err(&e));
        }
    };

    let final_dir = base.join(&name);
    if final_dir.exists() {
        let _ = fs::remove_dir_all(&final_dir);
    }
    fs::rename(&tmp, &final_dir).map_err(|e| e.to_string())?;

    // install into each app skill folder according to the mode
    for dir in app_skill_dirs() {
        let link = dir.join(&name);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        if link.exists() {
            let _ = fs::remove_dir_all(&link);
            let _ = fs::remove_file(&link);
        }
        if mode == "copy" {
            if let Err(e) = fs_extra_copy(&final_dir.to_string_lossy().to_string(), &link) {
                log::warn!("failed to copy skill {name} into {:?}: {e}", dir);
            }
        } else if let Err(e) = symlink_dir(&final_dir, &link) {
            log::warn!("failed to symlink skill {name} into {:?}: {e}", dir);
        }
    }

    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "INSERT INTO skills (name, description, source, status, install_mode, installed_at)
         VALUES (?1, ?2, ?3, 'installed', ?4, ?5)",
        rusqlite::params![
            name,
            description,
            source,
            mode,
            chrono::Local::now().to_rfc3339()
        ],
    )?;
    let id = conn.last_insert_rowid();
    Ok(SkillItem {
        id,
        name,
        description,
        source: source.clone(),
        status: "installed".into(),
        install_mode: mode,
        installed_at: chrono::Local::now().to_rfc3339(),
    })
}

#[tauri::command]
pub fn skill_uninstall(state: tauri::State<AppState>, id: i64) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let name: Option<String> = conn
        .query_row("SELECT name FROM skills WHERE id = ?1", rusqlite::params![id], |r| r.get(0))
        .ok();
    conn.execute("DELETE FROM skills WHERE id = ?1", rusqlite::params![id])?;
    drop(conn);
    if let Some(name) = name {
        let base = skills_dir(&state);
        let dir = base.join(&name);
        if dir.exists() {
            // keep a copy before removing, like cc-switch's skill backups
            let backups = state.active_data_dir().join("skill-backups");
            if let Ok(()) = fs::create_dir_all(&backups) {
                let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let target = backups.join(format!("{name}-{stamp}"));
                if fs_extra_copy(&dir.to_string_lossy().to_string(), &target).is_ok() {
                    prune_skill_backups(&backups);
                }
            }
            let _ = fs::remove_dir_all(&dir);
        }
        for target in app_skill_dirs() {
            let link = target.join(&name);
            let _ = fs::remove_dir_all(&link);
            let _ = fs::remove_file(&link);
        }
    }
    Ok(())
}

fn prune_skill_backups(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut dirs: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .collect();
    dirs.sort();
    if dirs.len() > 20 {
        for (_, p) in dirs.drain(..dirs.len() - 20) {
            let _ = fs::remove_dir_all(p);
        }
    }
}

// ---- custom repo sources ----

fn row_to_source(row: &rusqlite::Row) -> rusqlite::Result<SkillSource> {
    Ok(SkillSource { id: row.get(0)?, name: row.get(1)?, url: row.get(2)? })
}

#[tauri::command]
pub fn skill_sources_list(state: tauri::State<AppState>) -> AppResult<Vec<SkillSource>> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare("SELECT id, name, url FROM skill_sources ORDER BY id")?;
    let rows = stmt.query_map([], row_to_source)?;
    let out: rusqlite::Result<Vec<SkillSource>> = rows.collect();
    Ok(out?)
}

#[tauri::command]
pub fn skill_source_add(state: tauri::State<AppState>, input: SkillSource) -> AppResult<SkillSource> {
    if input.name.trim().is_empty() || input.url.trim().is_empty() {
        return state_err("name and url are required");
    }
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "INSERT INTO skill_sources (name, url) VALUES (?1, ?2)",
        rusqlite::params![input.name.trim(), input.url.trim()],
    )?;
    let id = conn.last_insert_rowid();
    Ok(SkillSource { id: Some(id), name: input.name.trim().to_string(), url: input.url.trim().to_string() })
}

#[tauri::command]
pub fn skill_source_remove(state: tauri::State<AppState>, id: i64) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute("DELETE FROM skill_sources WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}

fn clone_git(url: &str, dest: &Path) -> Result<(String, String), String> {
    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(dest)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("git clone failed".into());
    }
    discover_skills(dest)
}

fn unzip(path: &str, dest: &Path) -> Result<(String, String), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    archive.extract(dest).map_err(|e| e.to_string())?;
    discover_skills(dest)
}

fn fs_extra_copy(src: &str, dest: &Path) -> Result<(String, String), String> {
    let src_path = Path::new(src);
    let entries = fs::read_dir(src_path).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            if let Err(e) = fs_extra_copy(from.to_str().unwrap_or(""), &to) {
                return Err(e);
            }
        } else if let Err(e) = fs::copy(&from, &to) {
            return Err(e.to_string());
        }
    }
    if src_path.join("SKILL.md").exists() {
        let name = src
            .split('/')
            .filter(|s| !s.is_empty())
            .last()
            .unwrap_or("skill")
            .to_string();
        return Ok((name, parse_skill_md(&src_path.join("SKILL.md"))));
    }
    discover_skills(dest)
}

/// find SKILL.md at root or in immediate subfolders
fn discover_skills(root: &Path) -> Result<(String, String), String> {
    let root_skill = root.join("SKILL.md");
    if root_skill.exists() {
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "skill".to_string());
        return Ok((name, parse_skill_md(&root_skill)));
    }
    let entries = fs::read_dir(root).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("SKILL.md").exists() {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "skill".to_string());
            return Ok((name, parse_skill_md(&p.join("SKILL.md"))));
        }
    }
    Err("no SKILL.md found in source".into())
}

fn parse_skill_md(path: &Path) -> String {
    if let Ok(content) = fs::read_to_string(path) {
        if let Some(d) = extract_frontmatter(&content, "description")
            .or_else(|| extract_frontmatter(&content, "name"))
        {
            return d;
        }
        return content
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .unwrap_or("")
            .chars()
            .take(160)
            .collect();
    }
    String::new()
}

fn extract_frontmatter(content: &str, key: &str) -> Option<String> {
    if !content.starts_with("---") {
        return None;
    }
    let rest = content.trim_start_matches("---");
    let front = rest.split_once("---").map(|(f, _)| f)?;
    for line in front.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix(&format!("{key}:")) {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}