//! ns-mcp：MCP 服务器统一管理、跨工具同步。
//!
//! 每个工具用自己的配置格式声明 MCP 服务器（Claude Code `.mcp.json`、
//! Codex `config.toml`、Gemini CLI `settings.json`）。本 crate 提供统一
//! 的 [`McpServer`] 模型与每工具的读写适配器，供 M7 统一 MCP 面板使用。

pub mod manager;
pub mod model;
pub mod tools;
