//! 每工具 MCP 配置读写适配器。
//!
//! 适配器把统一的 [`McpServer`] 投影到各工具的配置格式并写回：
//! - Claude Code：`.mcp.json` / `~/.claude.json`（`mcpServers` 对象，JSON）。
//! - Codex：`~/.codex/config.toml`（`[mcp_servers.<name>]` 表，TOML）。
//! - Gemini CLI：`~/.gemini/settings.json`（`mcpServers` 对象，JSON）。
//!
//! 所有适配器遵循「最小侵入」：只增删改自己管的 `mcpServers` 键，其余
//! 内容与注释原样保留。

pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod gemini;

// 适配器以模块函数暴露（claude::/codex::/gemini:: 的 read/apply）。


use std::path::PathBuf;

use ns_presets::Tool;

/// 每工具 MCP 配置文件定位（含 `HOME` 与工具环境变量覆盖）。
#[derive(Debug, Clone)]
pub struct McpPaths {
    pub home: PathBuf,
}

impl McpPaths {
    /// 从进程环境解析各工具配置路径。
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        Self { home }
    }

    /// Claude Code 项目共享配置（`<project>/.mcp.json`）。
    pub fn claude_project_mcp(&self, project: &PathBuf) -> PathBuf {
        project.join(".mcp.json")
    }

    /// Claude Code 用户级配置（`~/.claude.json`）。
    pub fn claude_user_json(&self) -> PathBuf {
        let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.clone());
        dir.join(".claude.json")
    }

    /// Codex 用户配置（`~/.codex/config.toml`）。
    pub fn codex_config(&self) -> PathBuf {
        let dir = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(".codex"));
        dir.join("config.toml")
    }

    /// Gemini CLI 用户配置（`~/.gemini/settings.json`）。
    pub fn gemini_settings(&self) -> PathBuf {
        let dir = std::env::var_os("GEMINI_CLI_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(".gemini"));
        dir.join("settings.json")
    }

    /// 某工具的 MCP 配置文件（用户作用域；无项目上下文）。
    pub fn config_for(&self, tool: Tool) -> Option<PathBuf> {
        match tool {
            Tool::Claude | Tool::ClaudeDesktop => Some(self.claude_user_json()),
            Tool::Codex => Some(self.codex_config()),
            Tool::Gemini => Some(self.gemini_settings()),
            _ => None,
        }
    }
}
