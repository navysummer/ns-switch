//! 统一 MCP 服务器模型（与具体工具的配置格式无关）。
//!
//! 一个 [`McpServer`] 可声明为 stdio 本地进程或远程 HTTP/SSE/WS 端点。
//! 每个工具的读写适配器把它投影到该工具的配置格式。

use serde::{Deserialize, Serialize};

/// MCP 服务器传输方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// 本地子进程（`command` + `args` + `env`）。
    Stdio,
    /// 远程流式 HTTP 端点（`url`）。
    Http,
    /// 远程 SSE 端点（`url`）。
    Sse,
    /// 远程 WebSocket 端点（`url`）。
    Ws,
}

impl Default for McpTransport {
    fn default() -> Self {
        Self::Stdio
    }
}

/// 统一的 MCP 服务器描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpServer {
    /// 服务器名（各工具均要求：字母/数字/连字符/下划线）。
    pub name: String,
    pub transport: McpTransport,
    /// stdio：启动命令（如 `npx`）。
    pub command: Option<String>,
    /// stdio：命令参数。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// stdio：注入环境变量。
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, String>,
    /// 远程：端点 URL。
    pub url: Option<String>,
    /// 远程：请求头（如 `Authorization`）。
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub headers: std::collections::BTreeMap<String, String>,
    /// 是否启用（禁用时不写入目标配置）。
    pub enabled: bool,
}

impl Default for McpServer {
    fn default() -> Self {
        Self {
            name: String::new(),
            transport: McpTransport::Stdio,
            command: None,
            args: Vec::new(),
            env: std::collections::BTreeMap::new(),
            url: None,
            headers: std::collections::BTreeMap::new(),
            enabled: true,
        }
    }
}

impl McpServer {
    /// 校验服务器是否合法（可写为目标配置）。
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("服务器名不能为空".into());
        }
        let ok_name = self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !ok_name {
            return Err(format!("服务器名含非法字符: {}", self.name));
        }
        match self.transport {
            McpTransport::Stdio => {
                if self.command.as_deref().unwrap_or("").trim().is_empty() {
                    return Err("stdio 服务器缺少 command".into());
                }
            }
            McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
                if self.url.as_deref().unwrap_or("").trim().is_empty() {
                    return Err("远程服务器缺少 url".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_stdio_and_remote() {
        let mut s = McpServer {
            name: "docs".into(),
            transport: McpTransport::Stdio,
            command: Some("npx".into()),
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.command = None;
        assert!(s.validate().is_err());
        s = McpServer {
            name: "api".into(),
            transport: McpTransport::Http,
            url: Some("https://x/mcp".into()),
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.url = None;
        assert!(s.validate().is_err());
        assert!(McpServer {
            name: "bad name".into(),
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
