//! 统一 Skills 模型（与具体工具的目录布局无关）。
//!
//! 一个 [`Skill`] 是一份 Markdown 技能文档，含前端 YAML 元数据（name、
//! description、allowed-tools 等）。每个工具的读写适配器把它投影到
//! `~/.<tool>/skills/<name>/SKILL.md`。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 同步策略：如何把 skills 落盘到各工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStrategy {
    /// 自动：优先符号链接，回退复制。
    Auto,
    /// 仅符号链接（共享一份源文件）。
    Symlink,
    /// 仅复制（各工具独立副本）。
    Copy,
}

impl Default for SyncStrategy {
    fn default() -> Self {
        Self::Auto
    }
}

/// 统一的技能描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Skill {
    /// 技能名（目录名 / frontmatter `name`）。
    pub name: String,
    /// 人类可读描述（frontmatter `description`）。
    pub description: String,
    /// 允许使用该技能的工具（frontmatter `allowed-tools`；空 = 不限）。
    pub allowed_tools: Vec<String>,
    /// 技能正文（SKILL.md 去除 frontmatter 后的 Markdown）。
    pub body: String,
    /// 源文件路径（安装来源；None = 原生）。
    pub source_path: Option<PathBuf>,
    /// 源 GitHub 仓库 URL（安装来源；None = 非 GitHub）。
    pub repo_url: Option<String>,
    /// 是否启用（禁用 = 不在目标工具落盘）。
    pub enabled: bool,
}

impl Default for Skill {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            allowed_tools: Vec::new(),
            body: String::new(),
            source_path: None,
            repo_url: None,
            enabled: true,
        }
    }
}

impl Skill {
    /// 技能名是否合法（目录名用）。
    pub fn valid_name(&self) -> bool {
        !self.name.trim().is_empty()
            && self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    }
}

/// 每工具 Skills 目录定位。
#[derive(Debug, Clone)]
pub struct SkillPaths {
    pub home: PathBuf,
    /// 是否使用共享 `~/.agents/skills` 目录（true = 所有工具共用）。
    pub shared: bool,
}

impl SkillPaths {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        Self { home, shared: false }
    }

    /// Claude Code 用户 skills 目录（`~/.claude/skills`）。
    pub fn claude_skills(&self) -> PathBuf {
        let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(".claude"));
        dir.join("skills")
    }

    /// Codex 用户 skills 目录（`~/.agents/skills`，跨工具共享位置）。
    pub fn codex_skills(&self) -> PathBuf {
        self.home.join(".agents").join("skills")
    }

    /// Gemini CLI 用户 skills 目录（`$GEMINI_CLI_HOME/.gemini/skills`）。
    pub fn gemini_skills(&self) -> PathBuf {
        let root = std::env::var_os("GEMINI_CLI_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.clone());
        root.join(".gemini").join("skills")
    }
}
