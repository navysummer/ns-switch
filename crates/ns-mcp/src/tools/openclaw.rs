//! OpenClaw MCP 配置适配器（`~/.openclaw/openclaw.json`）。
//!
//! OpenClaw 为 Claude Code 兼容 CLI，MCP 配置采用相同约定：
//! 顶层 `mcpServers` 对象（`~/.claude.json` / `.mcp.json` 风格）。本适配器
//! 复用 [`super::claude`] 的 JSON 投影逻辑，仅路径与命名不同（`~/.openclaw/openclaw.json`）。
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

/// 从磁盘读 OpenClaw MCP 配置。
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
    fn openclaw_reuses_claude_mcpservers_semantics() {
        let s = McpServer {
            name: "git".into(),
            transport: McpTransport::Stdio,
            command: Some("gh".into()),
            args: vec!["mcp".into()],
            ..Default::default()
        };
        let mut doc: Value =
            serde_json::from_str(r#"{ "env": { "K": "V" }, "mcpServers": {} }"#).unwrap();
        assert!(apply_servers(&mut doc, &[], &[s.clone()]));
        let back = read_servers(&doc, None);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "git");
        assert_eq!(back[0].command.as_deref(), Some("gh"));
        assert_eq!(doc.get("env").and_then(|v| v.get("K")).and_then(|v| v.as_str()), Some("V"), "保留其他键");
        // 删除
        assert!(apply_servers(&mut doc, &["git".into()], &[]));
        assert!(read_servers(&doc, None).is_empty());
    }
}
