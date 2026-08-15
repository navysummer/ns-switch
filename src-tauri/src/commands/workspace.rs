use crate::db::AppState;
use crate::models::WorkspaceFile;
use crate::AppResult;

fn openclaw_root() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".openclaw"))
}

fn safe_join(root: &std::path::Path, rel: &str) -> Result<std::path::PathBuf, String> {
    let p = root.join(rel);
    let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canon = std::fs::canonicalize(&p).unwrap_or(p.clone());
    if canon.starts_with(&canon_root) || rel.trim().is_empty() {
        Ok(p)
    } else {
        Err("path escapes workspace".into())
    }
}

#[tauri::command]
pub fn workspace_files(state: tauri::State<AppState>, dir: Option<String>) -> AppResult<Vec<WorkspaceFile>> {
    drop(state);
    let root = openclaw_root().ok_or("no home dir")?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let target = safe_join(&root, &dir.unwrap_or_default())?;
    if !target.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(&target).map_err(|e| e.to_string())?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let file_type = e.file_type().map_err(|e| e.to_string())?;
        let rel = target
            .strip_prefix(&root)
            .unwrap_or_else(|_| std::path::Path::new(""))
            .join(&name)
            .to_string_lossy()
            .to_string();
        out.push(WorkspaceFile {
            name,
            path: rel,
            is_dir: file_type.is_dir(),
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(out)
}

fn workspace_read_impl(root: &std::path::Path, path: &str) -> Result<String, String> {
    let target = safe_join(root, path)?;
    if !target.is_file() {
        return Err("not a file".into());
    }
    std::fs::read_to_string(&target).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn workspace_read(state: tauri::State<AppState>, path: String) -> AppResult<String> {
    drop(state);
    let root = openclaw_root().ok_or("no home dir")?;
    workspace_read_impl(&root, &path).map_err(|e| crate::err(&e))
}

#[tauri::command]
pub fn workspace_save(state: tauri::State<AppState>, path: String, content: String) -> AppResult<()> {
    drop(state);
    let root = openclaw_root().ok_or("no home dir")?;
    let target = safe_join(&root, &path)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&target, content).map_err(|e| e.to_string())?;
    Ok(())
}