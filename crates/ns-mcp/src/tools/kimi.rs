//! Kimi Code MCP 配置适配器（`~/.kimi-code/config.toml`）。
//!
//! Kimi Code 与 Codex 同为 OpenAI 系 CLI，MCP 配置采用相同约定：
//! `[mcp_servers.<name>]` 表 + `enabled = false` 停用保留。本适配器复用
//! [`super::codex`] 的 TOML 投影逻辑，仅路径与命名不同（`~/.kimi-code/config.toml`）。
//!
//! 最小侵入：只增删改 `mcp_servers` 表，其余内容与注释原样保留。
use std::path::Path;

use toml_edit::DocumentMut;

use crate::model::McpServer;

pub use super::codex::read_servers;

/// 写 `mcp_servers` 表（同 Codex 约定）。
pub fn apply_servers(
    doc: &mut DocumentMut,
    remove_names: &[String],
    upserts: &[McpServer],
) -> bool {
    super::codex::apply_servers(doc, remove_names, upserts)
}

/// 从磁盘读 Kimi Code MCP 配置。
pub fn load(path: &Path) -> Result<Vec<McpServer>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("读取 {} 失败: {e}", path.display())),
    };
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("解析 {} 失败: {e}", path.display()))?;
    Ok(read_servers(&doc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::McpTransport;

    #[test]
    fn kimi_reuses_codex_table_semantics() {
        let s = McpServer {
            name: "git".into(),
            transport: McpTransport::Stdio,
            command: Some("gh".into()),
            args: vec!["mcp".into()],
            ..Default::default()
        };
        let mut doc = "keep = true\n".parse::<DocumentMut>().unwrap();
        assert!(apply_servers(&mut doc, &[], &[s.clone()]));
        let back = read_servers(&doc);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "git");
        assert_eq!(back[0].command.as_deref(), Some("gh"));
        assert!(doc.get("keep").is_some(), "保留其他键");
        // 停用保留
        let mut off = s.clone();
        off.enabled = false;
        assert!(apply_servers(&mut doc, &[], &[off]));
        let back = read_servers(&doc);
        assert!(!back[0].enabled);
    }
}
