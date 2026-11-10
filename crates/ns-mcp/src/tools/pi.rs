//! Pi MCP 配置适配器（`~/.pi/agent/mcp.json`）。
//!
//! Pi 的 MCP 约定（用户级 `~/.pi/agent/mcp.json`，项目级 `.pi/mcp.json`）：
//! 顶层 `mcpServers` 对象（Claude Code 风格）。本适配器复用 [`super::claude`]
//! 的 JSON 投影逻辑，仅路径与命名不同（`~/.pi/agent/mcp.json`，`PI_CODING_AGENT_DIR` 覆盖）。
//!
//! 最小侵入：只增删改 `mcpServers` 键，其余内容与格式原样保留。
use std::path::Path;

use serde_json::Value;

use crate::model::McpServer;

pub use super::claude::read_servers;

/// 写 `mcpServers` 对象（同 Claude 约定）。
pub fn apply_servers(
    doc: &mut Value,
    remove_names: &[String],
    upserts: &[McpServer],
) -> bool {
    super::claude::apply_servers(doc, None, remove_names, upserts)
}

/// 校验文件内容是否合法（顶层对象）。
pub fn validate_doc(text: &str) -> Result<Value, String> {
    super::claude::validate_doc(text)
}

/// 从磁盘读 Pi MCP 配置。
pub fn load(path: &Path) -> Result<Vec<McpServer>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("读取 {} 失败: {e}", path.display())),
    };
    let doc = validate_doc(&text)?;
    Ok(read_servers(&doc, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::McpTransport;

    #[test]
    fn pi_reuses_claude_mcpservers_semantics() {
        let s = McpServer {
            name: "chrome-devtools".into(),
            transport: McpTransport::Stdio,
            command: Some("npx".into()),
            args: vec!["-y".into(), "@modelcontextprotocol/server-chrome-devtools".into()],
            ..Default::default()
        };
        let mut doc: Value = serde_json::from_str(r#"{ "version": "1", "mcpServers": {} }"#).unwrap();
        assert!(apply_servers(&mut doc, &[], &[s.clone()]));
        let back = read_servers(&doc, None);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "chrome-devtools");
        assert_eq!(back[0].command.as_deref(), Some("npx"));
        assert_eq!(doc.get("version").and_then(|v| v.as_str()), Some("1"), "保留其他键");
        // 删除
        assert!(apply_servers(&mut doc, &["chrome-devtools".into()], &[]));
        assert!(read_servers(&doc, None).is_empty());
    }
}
