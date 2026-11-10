//! MiniMax Code MCP 配置适配器（`~/.minimax/mcp.json`）。
//!
//! MiniMax Code 的 MCP 约定（`<data-dir>/mcp.json`，默认 `~/.minimax/mcp.json`）：
//! 顶层 `mcpServers` 对象，条目用显式 `type`（`stdio`/`http`/`streamable-http`/`sse`）
//! + `command`（字符串）/`args`（数组）/`url`/`headers`/`env`，`enabled` 持久化。
//!
//! 最小侵入：只增删改 `mcpServers` 键，其余内容与格式原样保留。
use std::path::Path;

use serde_json::{Map, Value};

use crate::model::{McpServer, McpTransport};

/// 读 `mcpServers` 对象为 [`McpServer`] 列表。
pub fn read_servers(doc: &Value) -> Vec<McpServer> {
    let Some(obj) = doc.get("mcpServers").and_then(Value::as_object) else {
        return Vec::new();
    };
    obj.iter().map(|(name, v)| from_json(name, v)).collect()
}

/// 把 JSON 值投影为 [`McpServer`]。
fn from_json(name: &str, v: &Value) -> McpServer {
    let get = |k: &str| v.get(k).cloned();
    let str_of = |k: &str| get(k).and_then(|x| x.as_str().map(str::to_string));
    let arr_of = |k: &str| {
        get(k)
            .and_then(|x| x.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect::<Vec<_>>()
    };
    let map_of = |k: &str| {
        get(k)
            .and_then(|x| x.as_object().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let transport = match str_of("type").as_deref() {
        Some("http") | Some("streamable-http") => McpTransport::Http,
        Some("sse") => McpTransport::Sse,
        Some("ws") => McpTransport::Ws,
        _ => McpTransport::Stdio,
    };
    McpServer {
        name: name.to_string(),
        transport,
        command: str_of("command"),
        args: arr_of("args"),
        env: map_of("env"),
        url: str_of("url"),
        headers: map_of("headers"),
        enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(true),
    }
}

/// 把 [`McpServer`] 投影为 MiniMax 的 `mcpServers` 条目。
pub fn to_json(server: &McpServer) -> Value {
    let mut m = Map::new();
    match server.transport {
        McpTransport::Stdio => {
            m.insert("type".into(), Value::String("stdio".into()));
            if let Some(cmd) = &server.command {
                m.insert("command".into(), Value::String(cmd.clone()));
            }
            if !server.args.is_empty() {
                m.insert(
                    "args".into(),
                    Value::Array(server.args.iter().map(|a| Value::String(a.clone())).collect()),
                );
            }
            if !server.env.is_empty() {
                m.insert(
                    "env".into(),
                    Value::Object(
                        server
                            .env
                            .iter()
                            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                            .collect(),
                    ),
                );
            }
        }
        McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
            let ty = match server.transport {
                McpTransport::Http => "streamable-http",
                McpTransport::Sse => "sse",
                _ => "ws",
            };
            m.insert("type".into(), Value::String(ty.into()));
            if let Some(url) = &server.url {
                m.insert("url".into(), Value::String(url.clone()));
            }
            if !server.headers.is_empty() {
                m.insert(
                    "headers".into(),
                    Value::Object(
                        server
                            .headers
                            .iter()
                            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                            .collect(),
                    ),
                );
            }
        }
    }
    // 停用保留条目（enabled=false 持久化）。
    if !server.enabled {
        m.insert("enabled".into(), Value::Bool(false));
    }
    Value::Object(m)
}

/// 写 `mcpServers` 到 JSON 文档（原地增删改；保留其他键）。
pub fn apply_servers(
    doc: &mut Value,
    remove_names: &[String],
    upserts: &[McpServer],
) -> bool {
    let root = doc.as_object_mut().expect("root object");
    if !root.get("mcpServers").is_some_and(|v| v.is_object()) {
        root.insert("mcpServers".into(), Value::Object(Map::new()));
    }
    let servers = root
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .expect("mcpServers object");
    let mut changed = false;
    for name in remove_names {
        if servers.remove(name).is_some() {
            changed = true;
        }
    }
    for s in upserts {
        let v = to_json(s);
        if servers.get(&s.name) != Some(&v) {
            servers.insert(s.name.clone(), v);
            changed = true;
        }
    }
    changed
}

/// 校验文件内容是否合法（顶层对象）。
pub fn validate_doc(text: &str) -> Result<Value, String> {
    let v: Value = serde_json_lenient::from_str(text).map_err(|e| e.to_string())?;
    if !v.is_object() {
        return Err("MCP 配置顶层应为对象".into());
    }
    Ok(v)
}

/// 从磁盘读 MiniMax Code MCP 配置。
pub fn load(path: &Path) -> Result<Vec<McpServer>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("读取 {} 失败: {e}", path.display())),
    };
    let doc = validate_doc(&text)?;
    Ok(read_servers(&doc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::McpTransport;

    #[test]
    fn minimax_roundtrip_stdio_and_remote() {
        let s = McpServer {
            name: "playwright".into(),
            transport: McpTransport::Stdio,
            command: Some("npx".into()),
            args: vec!["-y".into(), "@playwright/mcp".into()],
            ..Default::default()
        };
        let mut doc: Value = serde_json::from_str(r#"{ "other": 1, "mcpServers": {} }"#).unwrap();
        assert!(apply_servers(&mut doc, &[], &[s.clone()]));
        let back = read_servers(&doc);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "playwright");
        assert_eq!(back[0].command.as_deref(), Some("npx"));
        assert_eq!(back[0].args.len(), 2);
        assert!(doc.get("other").is_some(), "保留其他键");
        // 停用保留 enabled=false
        let mut off = s.clone();
        off.enabled = false;
        assert!(apply_servers(&mut doc, &[], &[off]));
        let back = read_servers(&doc);
        assert!(!back[0].enabled);
        // 远程
        let r = McpServer {
            name: "search".into(),
            transport: McpTransport::Http,
            url: Some("https://example.com/mcp".into()),
            ..Default::default()
        };
        assert!(apply_servers(&mut doc, &[], &[r.clone()]));
        let back = read_servers(&doc);
        assert_eq!(back.iter().find(|s| s.name == "search").unwrap().transport, McpTransport::Http);
    }
}
