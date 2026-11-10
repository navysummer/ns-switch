//! OpenCode MCP 配置适配器（`opencode.json` 的 `mcp` 对象）。
//!
//! OpenCode 的 MCP 约定（`~/.config/opencode/opencode.json`）：
//! ```json
//! { "mcp": { "<name>": { "type": "local", "command": ["npx", "-y", "..."], "enabled": true } } }
//! ```
//! - `type: "local"`：`command` 数组（命令 + 参数合并）；`env` 对象可选。
//! - `type: "remote"`：`url` 端点；`headers` 对象可选。
//! - `enabled: false` 保留条目但停用（OpenCode 支持持久 enable 字段）。
//!
//! 最小侵入：只增删改自己管的 `mcp` 键，其余内容与格式原样保留。

use std::path::Path;

use serde_json::{Map, Value};

use crate::model::{McpServer, McpTransport};

/// 读 `mcp` 对象为 [`McpServer`] 列表。
pub fn read_servers(doc: &Value) -> Vec<McpServer> {
    let Some(obj) = doc.get("mcp").and_then(Value::as_object) else {
        return Vec::new();
    };
    obj.iter().map(|(name, v)| from_json(name, v)).collect()
}

/// 把 JSON 值投影为 [`McpServer`]。
fn from_json(name: &str, v: &Value) -> McpServer {
    let get = |k: &str| v.get(k).cloned();
    let str_of = |k: &str| get(k).and_then(|x| x.as_str().map(str::to_string));
    // OpenCode `command` 是数组（命令 + 参数合并）。
    let cmd_vec = get("command")
        .and_then(|x| x.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let map_of = |k: &str| {
        get(k)
            .and_then(|x| x.as_object().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let transport = match str_of("type").as_deref() {
        Some("remote") | Some("http") | Some("sse") | Some("ws") => McpTransport::Http,
        _ => McpTransport::Stdio,
    };
    let (command, args) = if cmd_vec.is_empty() {
        (None, Vec::new())
    } else {
        (Some(cmd_vec[0].clone()), cmd_vec[1..].to_vec())
    };
    McpServer {
        name: name.to_string(),
        transport,
        command,
        args,
        env: map_of("env"),
        url: str_of("url"),
        headers: map_of("headers"),
        enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(true),
    }
}

/// 把 [`McpServer`] 投影为 OpenCode 的 `mcp` 条目。
pub fn to_json(server: &McpServer) -> Value {
    let mut m = Map::new();
    match server.transport {
        McpTransport::Stdio => {
            m.insert("type".into(), Value::String("local".into()));
            let mut cmd = Vec::new();
            if let Some(c) = &server.command {
                cmd.push(Value::String(c.clone()));
            }
            cmd.extend(server.args.iter().map(|a| Value::String(a.clone())));
            m.insert("command".into(), Value::Array(cmd));
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
            m.insert("type".into(), Value::String("remote".into()));
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

/// 写 `mcp` 到 JSON 文档（原地增删改；保留其他键）。
pub fn apply_servers(
    doc: &mut Value,
    remove_names: &[String],
    upserts: &[McpServer],
) -> bool {
    let root = doc.as_object_mut().expect("root object");
    if !root.get("mcp").is_some_and(|v| v.is_object()) {
        root.insert("mcp".into(), Value::Object(Map::new()));
    }
    let servers = root
        .get_mut("mcp")
        .and_then(Value::as_object_mut)
        .expect("mcp object");
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

/// 校验文件内容（顶层对象）。
pub fn validate_doc(text: &str) -> Result<Value, String> {
    let v: Value = serde_json_lenient::from_str(text).map_err(|e| e.to_string())?;
    if !v.is_object() {
        return Err("OpenCode 配置顶层应为对象".into());
    }
    Ok(v)
}

/// 从磁盘读 OpenCode MCP 配置。
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

    #[test]
    fn roundtrip_local_and_remote() {
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
            McpServer {
                name: "disabled".into(),
                transport: McpTransport::Stdio,
                command: Some("echo".into()),
                enabled: false,
                ..Default::default()
            },
        ];
        let mut doc = serde_json::json!({ "keep": 1 });
        assert!(apply_servers(&mut doc, &[], &servers));
        let back = read_servers(&doc);
        assert_eq!(back.len(), 3);
        let docs = back.iter().find(|s| s.name == "docs").unwrap();
        assert_eq!(docs.command.as_deref(), Some("npx"));
        assert_eq!(docs.args, vec!["-y".to_string(), "@docs/mcp".to_string()]);
        let remote = back.iter().find(|s| s.name == "remote").unwrap();
        assert_eq!(remote.transport, McpTransport::Http);
        assert_eq!(remote.url.as_deref(), Some("https://x/mcp"));
        let dis = back.iter().find(|s| s.name == "disabled").unwrap();
        assert!(!dis.enabled, "enabled=false 应保留为停用");
        // 删除保留其他键。
        assert!(apply_servers(&mut doc, &["docs".into()], &[]));
        assert_eq!(read_servers(&doc).len(), 2);
        assert_eq!(doc.get("keep"), Some(&serde_json::json!(1)));
        assert!(doc.get("mcp").is_some(), "mcp 对象应保留");
    }
}
