//! Codex MCP 配置适配器（`~/.codex/config.toml`）。
//!
//! 每个服务器是 `[mcp_servers.<name>]` 表；`enabled = false` 表示禁用但保留配置。

use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::model::{McpServer, McpTransport};

// (无未用辅助函数；读取直接经 `Item::as_table`/`as_str`。)


/// 从 `config.toml` 读服务器。
pub fn read_servers(doc: &DocumentMut) -> Vec<McpServer> {
    let Some(t) = doc.get("mcp_servers").and_then(Item::as_table) else {
        return Vec::new();
    };
    t.iter()
        .filter_map(|(name, item)| {
            let table = item.as_table()?;
            let command = table.get("command").and_then(Item::as_str).map(str::to_string);
            let args = table
                .get("args")
                .and_then(Item::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let env = table
                .get("env")
                .and_then(Item::as_table)
                .map(|t| {
                    t.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.to_string(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            let url = table
                .get("url")
                .and_then(Item::as_str)
                .map(str::to_string);
            let enabled = table
                .get("enabled")
                .and_then(Item::as_bool)
                .unwrap_or(true);
            // Codex：command+args → stdio；url → streamable HTTP。
            let transport = if url.is_some() {
                McpTransport::Http
            } else {
                McpTransport::Stdio
            };
            Some(McpServer {
                name: name.to_string(),
                transport,
                command,
                args,
                env,
                url,
                headers: std::collections::BTreeMap::new(),
                enabled,
            })
        })
        .collect()
}

// (item_from 未用；写入直接经 `to_table` 构造 toml_edit::Table。)


/// 把 [`McpServer`] 投影为 Codex 的 `[mcp_servers.<name>]` 表。
pub fn to_table(server: &McpServer) -> Table {
    let mut t = Table::new();
    match server.transport {
        McpTransport::Stdio => {
            if let Some(cmd) = &server.command {
                t.insert("command", Item::Value(Value::from(cmd.clone())));
            }
            if !server.args.is_empty() {
                let mut arr = toml_edit::Array::new();
                for a in &server.args {
                    arr.push(Value::from(a.clone()));
                }
                t.insert("args", Item::Value(Value::Array(arr)));
            }
            if !server.env.is_empty() {
                let mut env = Table::new();
                for (k, v) in &server.env {
                    env.insert(k, Item::Value(Value::from(v.clone())));
                }
                t.insert("env", Item::Table(env));
            }
        }
        McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
            if let Some(url) = &server.url {
                t.insert("url", Item::Value(Value::from(url.clone())));
            }
        }
    }
    if !server.enabled {
        t.insert("enabled", Item::Value(Value::from(false)));
    }
    t
}

/// 写 `mcp_servers` 表；返回是否有变化。
pub fn apply_servers(
    doc: &mut DocumentMut,
    remove_names: &[String],
    upserts: &[McpServer],
) -> bool {
    if !doc.get("mcp_servers").is_some_and(|i| i.is_table()) {
        doc.insert("mcp_servers", Item::Table(Table::new()));
    }
    let mcp = doc.get_mut("mcp_servers").and_then(Item::as_table_mut).expect("mcp_servers table");
    let mut changed = false;
    for name in remove_names {
        if mcp.remove(name).is_some() {
            changed = true;
        }
    }
    for s in upserts {
        if s.enabled {
            let table = to_table(s);
            let new_item = Item::Table(table);
            let same = mcp
                .get(&s.name)
                .map(|i| i.to_string())
                .is_some_and(|cur| cur == new_item.to_string());
            if !same {
                mcp.insert(&s.name, new_item);
                changed = true;
            }
        } else if mcp.get(&s.name).is_some() {
            // 禁用但保留配置：写 enabled=false。
            let mut table = to_table(s);
            table.insert("enabled", Item::Value(Value::from(false)));
            mcp.insert(&s.name, Item::Table(table));
            changed = true;
        }
    }
    changed
}

/// 从磁盘读 Codex MCP 配置。
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

    #[test]
    fn roundtrip_and_disable() {
        let s = McpServer {
            name: "bookmarks".into(),
            transport: McpTransport::Stdio,
            command: Some("uv".into()),
            args: vec!["--directory".into(), "/path".into(), "run".into(), "b.py".into()],
            env: [("K".into(), "v".into())].into(),
            enabled: true,
            ..Default::default()
        };
        let mut doc = "keep = true\n".parse::<DocumentMut>().unwrap();
        assert!(apply_servers(&mut doc, &[], &[s.clone()]));
        let back = read_servers(&doc);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "bookmarks");
        assert_eq!(back[0].command.as_deref(), Some("uv"));
        assert_eq!(back[0].args.len(), 4);
        // 禁用
        let mut off = s.clone();
        off.enabled = false;
        assert!(apply_servers(&mut doc, &[], &[off]));
        let back = read_servers(&doc);
        assert_eq!(back.len(), 1);
        assert!(!back[0].enabled);
        // 保留 keep
        assert!(doc.get("keep").is_some());
    }

    #[test]
    fn remote_http() {
        let s = McpServer {
            name: "example".into(),
            transport: McpTransport::Http,
            url: Some("https://mcp.example.com/mcp".into()),
            enabled: true,
            ..Default::default()
        };
        let mut doc = DocumentMut::new();
        assert!(apply_servers(&mut doc, &[], &[s]));
        let back = read_servers(&doc);
        assert_eq!(back[0].transport, McpTransport::Http);
        assert_eq!(back[0].url.as_deref(), Some("https://mcp.example.com/mcp"));
    }
}
