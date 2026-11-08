//! 工具配置文件编辑器：OpenClaw 工作区指令 / Hermes 记忆。
//!
//! - OpenClaw：`~/.openclaw/AGENTS.md`（工作区指令）、`~/.openclaw/SOUL.md`（persona）。
//! - Hermes：`~/.hermes/memory/MEMORY.md`、`~/.hermes/memory/USER.md`（持久记忆）。
//!
//! 文件不存在时按需创建（保存时）。

use std::path::PathBuf;

/// 一个可编辑的配置文件。
#[derive(Debug, Clone)]
pub struct ConfigFile {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub path: PathBuf,
}

/// home 目录。
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// 可编辑配置文件清单。
pub fn files() -> Vec<ConfigFile> {
    let h = home().unwrap_or_default();
    vec![
        ConfigFile {
            id: "openclaw-agents",
            label: "OpenClaw 工作区指令",
            hint: "AGENTS.md · 工作区规则（OpenClaw）",
            path: h.join(".openclaw").join("AGENTS.md"),
        },
        ConfigFile {
            id: "openclaw-soul",
            label: "OpenClaw Persona",
            hint: "SOUL.md · 人格定义（OpenClaw）",
            path: h.join(".openclaw").join("SOUL.md"),
        },
        ConfigFile {
            id: "hermes-memory",
            label: "Hermes 记忆",
            hint: "MEMORY.md · 持久记忆（Hermes）",
            path: h.join(".hermes").join("memory").join("MEMORY.md"),
        },
        ConfigFile {
            id: "hermes-user",
            label: "Hermes 用户记忆",
            hint: "USER.md · 用户记忆（Hermes）",
            path: h.join(".hermes").join("memory").join("USER.md"),
        },
    ]
}

/// 按 id 找配置文件。
pub fn file_by_id(id: &str) -> Option<ConfigFile> {
    files().into_iter().find(|f| f.id == id)
}

/// 读取文件内容（不存在返回 Ok(None)，未初始化）。
pub fn read_file(id: &str) -> Result<Option<String>, String> {
    let f = file_by_id(id).ok_or("未知配置文件")?;
    if !f.path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(&f.path).map(Some).map_err(|e| e.to_string())
}

/// 写入文件（不存在时创建父目录 + 文件）。
pub fn write_file(id: &str, content: &str) -> Result<(), String> {
    let f = file_by_id(id).ok_or("未知配置文件")?;
    if let Some(parent) = f.path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f.path, content).map_err(|e| e.to_string())
}

/// 文件是否存在。
pub fn exists(id: &str) -> bool {
    file_by_id(id)
        .map(|f| f.path.exists())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_list_includes_both_tools() {
        let fs = files();
        assert!(fs.iter().any(|f| f.id == "openclaw-agents"));
        assert!(fs.iter().any(|f| f.id == "hermes-memory"));
        assert!(fs.iter().any(|f| f.id == "hermes-user"));
    }

    #[test]
    fn read_missing_returns_none() {
        let id = "hermes-memory";
        // 仅当文件不存在时返回 None（避免污染真实文件）。
        if !exists(id) {
            assert_eq!(read_file(id).unwrap(), None);
        }
    }

    #[test]
    fn unknown_id_errors() {
        assert!(file_by_id("nope").is_none());
        assert!(write_file("nope", "x").is_err());
    }
}
