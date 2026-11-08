//! 统一提示词模型（与具体工具的提示文件名无关）。
//!
//! 一个 [`PromptEntry`] 是某工具的一份提示内容（CLAUDE.md / AGENTS.md /
//! GEMINI.md）。每个工具的读写适配器把它投影到该工具的用户级/项目级
//! 提示文件；写回前先把文件既有内容存回库，不丢失。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 统一的提示词条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptEntry {
    /// 条目名（各工具提示文件内可读的标题；空 = 未命名）。
    pub name: String,
    /// 正文 Markdown。
    pub body: String,
    /// 是否启用（禁用时不写回目标提示文件）。
    pub enabled: bool,
    /// 最后写回时间（Unix 毫秒；0 = 从未写回）。
    pub updated_at: u64,
}

impl Default for PromptEntry {
    fn default() -> Self {
        Self {
            name: String::new(),
            body: String::new(),
            enabled: true,
            updated_at: 0,
        }
    }
}

impl PromptEntry {
    /// 条目名是否合法（作为库键用）。
    pub fn valid_name(&self) -> bool {
        !self.name.trim().is_empty()
    }
}

/// 每工具提示文件路径定位（含 `HOME` 与工具环境变量覆盖）。
#[derive(Debug, Clone)]
pub struct PromptPaths {
    pub home: PathBuf,
}

impl PromptPaths {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        Self { home }
    }

    /// Claude Code 用户级提示文件（`~/.claude/CLAUDE.md`）。
    pub fn claude_user(&self) -> PathBuf {
        let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(".claude"));
        dir.join("CLAUDE.md")
    }

    /// Codex 用户级提示文件（`~/.codex/AGENTS.md`）。
    pub fn codex_user(&self) -> PathBuf {
        let dir = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(".codex"));
        dir.join("AGENTS.md")
    }

    /// Gemini CLI 用户级提示文件（`$GEMINI_CLI_HOME/.gemini/GEMINI.md`）。
    pub fn gemini_user(&self) -> PathBuf {
        let root = std::env::var_os("GEMINI_CLI_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.clone());
        root.join(".gemini").join("GEMINI.md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_name_checks() {
        let mut p = PromptEntry::default();
        assert!(!p.valid_name());
        p.name = "   ".into();
        assert!(!p.valid_name());
        p.name = "项目规范".into();
        assert!(p.valid_name());
    }
}
