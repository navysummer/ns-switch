//! Claude Code MCP 配置适配器（`.mcp.json` / `~/.claude.json`）。
//!
//! 三个作用域共用同一 `mcpServers` 对象：项目共享（`<project>/.mcp.json`）、
//! 用户（`~/.claude.json` 顶层）、本地（`~/.claude.json` 的
//! `projects["<path>"].mcpServers`）。本适配器面向用户级与项目级。

use std::path::Path;

use serde_json::{Map, Value};

use crate::model::{McpServer, McpTransport};

/// 读 `mcpServers` 对象：从 JSON 文档中取 `mcpServers`（或按子路径取）。
pub fn read_servers(doc: &Value, nested: Option<&[String]>) -> Vec<McpServer> {
    let mut node = doc;
    for seg in nested.unwrap_or(&[]) {
        let Some(v) = node.get(seg) else {
            return Vec::new();
        };
        node = v;
    }
    let Some(obj) = node.get("mcpServers").and_then(Value::as_object) else {
        return Vec::new();
    };
    obj.iter().map(|(name, v)| from_json(name, v)).collect()
}

/// 把 JSON 值投影为 [`McpServer`]。
fn from_json(name: &str, v: &Value) -> McpServer {
    let get = |k: &str| v.get(k).cloned();
    let str_of = |k: &str| get(k).and_then(|v| v.as_str().map(str::to_string));
    let arr_of = |k: &str| {
        get(k)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect::<Vec<_>>()
    };
    let map_of = |k: &str| {
        get(k)
            .and_then(|v| v.as_object().cloned())
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
        enabled: true,
    }
}

/// 把 [`McpServer`] 投影为 Claude 的 JSON 条目。
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
                McpTransport::Http => "http",
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
    Value::Object(m)
}

/// 写 `mcpServers` 到 JSON 文档（原地增删改；保留其他键与格式）。
///
/// `remove_names`：要删除的服务器名；`upserts`：要写入/更新的服务器。
/// 返回 (新文档, 是否有变化)。
pub fn apply_servers(
    doc: &mut Value,
    nested: Option<&[String]>,
    remove_names: &[String],
    upserts: &[McpServer],
) -> bool {
    let mut node = doc;
    for seg in nested.unwrap_or(&[]) {
        if !node.get(seg).is_some_and(|v| v.is_object()) {
            node.as_object_mut()
                .expect("root object")
                .insert(seg.clone(), Value::Object(Map::new()));
        }
        node = node.get_mut(seg).expect("just inserted");
    }
    let root = node.as_object_mut().expect("mcpServers parent object");
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
        if !s.enabled {
            // 禁用：从配置中移除（Claude 无持久 enable 字段）。
            if servers.remove(&s.name).is_some() {
                changed = true;
            }
            continue;
        }
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

/// 从磁盘读 Claude MCP 配置（`.mcp.json` 或 `~/.claude.json` 的某作用域）。
pub fn load(path: &Path, nested: Option<&[String]>) -> Result<Vec<McpServer>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("读取 {} 失败: {e}", path.display())),
    };
    let doc = validate_doc(&text)?;
    Ok(read_servers(&doc, nested))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_stdio_and_remote() {
        let servers = vec![
            McpServer {
                name: "docs".into(),
                transport: McpTransport::Stdio,
                command: Some("npx".into()),
                args: vec!["-y".into(), "@docs/mcp".into()],
                env: [("API_KEY".into(), "sk".into())].into(),
                ..Default::default()
            },
            McpServer {
                name: "remote".into(),
                transport: McpTransport::Http,
                url: Some("https://x/mcp".into()),
                headers: [("Authorization".into(), "Bearer t".into())].into(),
                ..Default::default()
            },
        ];
        let mut doc = serde_json::json!({ "keep": 1 });
        assert!(apply_servers(&mut doc, None, &[], &servers));
        let back = read_servers(&doc, None);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].name, "docs");
        assert_eq!(back[0].command.as_deref(), Some("npx"));
        assert_eq!(back[1].transport, McpTransport::Http);
        assert_eq!(back[1].url.as_deref(), Some("https://x/mcp"));
        // 删除
        assert!(apply_servers(&mut doc, None, &["docs".into()], &[]));
        assert_eq!(read_servers(&doc, None).len(), 1);
        assert_eq!(doc.get("keep"), Some(&serde_json::json!(1)));
    }

    #[test]
    fn nested_project_scope() {
        let mut doc = serde_json::json!({});
        let nested = vec!["projects".to_string(), "/proj".to_string()];
        let s = McpServer {
            name: "git".into(),
            transport: McpTransport::Stdio,
            command: Some("gh".into()),
            ..Default::default()
        };
        assert!(apply_servers(&mut doc, Some(&nested), &[], &[s.clone()]));
        let back = read_servers(&doc, Some(&nested));
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "git");
    }
}
