//! Hermes MCP 配置适配器（`~/.hermes/config.yaml`）。
//!
//! Hermes 的 MCP 约定（`config.yaml` 顶层 `mcp_servers`，snake_case）：
//! ```yaml
//! mcp_servers:
//!   filesystem:
//!     command: npx
//!     args: ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/projects"]
//! ```
//! - stdio：`command` + `args`（数组）；`env` 映射可选。
//! - remote：`url` 端点；`headers` 映射可选。
//!
//! 最小侵入：只增删改 `mcp_servers` 键，其余内容与格式原样保留。
use std::path::Path;

use serde_yaml::Value;

use crate::model::{McpServer, McpTransport};

/// 读 `mcp_servers` 映射为 [`McpServer`] 列表。
pub fn read_servers(doc: &Value) -> Vec<McpServer> {
    let Some(obj) = doc.get("mcp_servers").and_then(Value::as_mapping) else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(k, v)| {
            let name = k.as_str()?;
            Some(from_yaml(name, v))
        })
        .collect()
}

/// 把 YAML 值投影为 [`McpServer`]。
fn from_yaml(name: &str, v: &Value) -> McpServer {
    let get = |k: &str| v.get(k).cloned();
    let str_of = |k: &str| get(k).and_then(|x| x.as_str().map(str::to_string));
    let arr_of = |k: &str| {
        get(k)
            .and_then(|x| x.as_sequence().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect::<Vec<_>>()
    };
    let map_of = |k: &str| {
        get(k)
            .and_then(|x| x.as_mapping().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.as_str().unwrap_or("").to_string(), s.to_string())))
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

/// 把 [`McpServer`] 投影为 Hermes 的 `mcp_servers` 条目。
pub fn to_yaml(server: &McpServer) -> Value {
    let mut m = serde_yaml::Mapping::new();
    match server.transport {
        McpTransport::Stdio => {
            m.insert(Value::String("command".into()), Value::String(server.command.clone().unwrap_or_default()));
            if !server.args.is_empty() {
                m.insert(
                    Value::String("args".into()),
                    Value::Sequence(server.args.iter().map(|a| Value::String(a.clone())).collect()),
                );
            }
            if !server.env.is_empty() {
                m.insert(
                    Value::String("env".into()),
                    Value::Mapping(
                        server
                            .env
                            .iter()
                            .map(|(k, v)| (Value::String(k.clone()), Value::String(v.clone())))
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
            m.insert(Value::String("type".into()), Value::String(ty.into()));
            if let Some(url) = &server.url {
                m.insert(Value::String("url".into()), Value::String(url.clone()));
            }
            if !server.headers.is_empty() {
                m.insert(
                    Value::String("headers".into()),
                    Value::Mapping(
                        server
                            .headers
                            .iter()
                            .map(|(k, v)| (Value::String(k.clone()), Value::String(v.clone())))
                            .collect(),
                    ),
                );
            }
        }
    }
    if !server.enabled {
        m.insert(Value::String("enabled".into()), Value::Bool(false));
    }
    Value::Mapping(m)
}

/// 写 `mcp_servers` 到 YAML 文档（原地增删改；保留其他键）。
pub fn apply_servers(
    doc: &mut Value,
    remove_names: &[String],
    upserts: &[McpServer],
) -> bool {
    if !doc.is_mapping() {
        *doc = Value::Mapping(serde_yaml::Mapping::new());
    }
    let root = doc.as_mapping_mut().expect("root mapping");
    let servers = root
        .entry(Value::String("mcp_servers".into()))
        .or_insert_with(|| Value::Mapping(serde_yaml::Mapping::new()));
    if !servers.is_mapping() {
        *servers = Value::Mapping(serde_yaml::Mapping::new());
    }
    let servers = servers.as_mapping_mut().expect("mcp_servers mapping");
    let mut changed = false;
    for name in remove_names {
        if servers.remove(Value::String(name.clone())).is_some() {
            changed = true;
        }
    }
    for s in upserts {
        let key = Value::String(s.name.clone());
        if !s.enabled {
            if servers.remove(key).is_some() {
                changed = true;
            }
            continue;
        }
        let v = to_yaml(s);
        if servers.get(&key) != Some(&v) {
            servers.insert(key, v);
            changed = true;
        }
    }
    changed
}

/// 校验文件内容是否合法（顶层映射）。
pub fn validate_doc(text: &str) -> Result<Value, String> {
    let v: Value = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    if !v.is_mapping() {
        return Err("MCP 配置顶层应为映射".into());
    }
    Ok(v)
}

/// 从磁盘读 Hermes MCP 配置。
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
    fn hermes_roundtrip_yaml() {
        let s = McpServer {
            name: "filesystem".into(),
            transport: McpTransport::Stdio,
            command: Some("npx".into()),
            args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into()],
            ..Default::default()
        };
        let mut doc: Value = serde_yaml::from_str("keep: true\n").unwrap();
        assert!(apply_servers(&mut doc, &[], &[s.clone()]));
        let back = read_servers(&doc);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "filesystem");
        assert_eq!(back[0].command.as_deref(), Some("npx"));
        assert_eq!(back[0].args.len(), 2);
        assert!(doc.get("keep").is_some(), "保留其他键");
        // 删除
        assert!(apply_servers(&mut doc, &["filesystem".into()], &[]));
        assert!(read_servers(&doc).is_empty());
    }

    #[test]
    fn hermes_remote_roundtrip() {
        let s = McpServer {
            name: "linear".into(),
            transport: McpTransport::Http,
            url: Some("https://mcp.example.com/mcp".into()),
            headers: [("Authorization".into(), "Bearer x".into())].into(),
            ..Default::default()
        };
        let mut doc: Value = serde_yaml::from_str("mcp_servers: {}\n").unwrap();
        assert!(apply_servers(&mut doc, &[], &[s.clone()]));
        let back = read_servers(&doc);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].transport, McpTransport::Http);
        assert_eq!(back[0].url.as_deref(), Some("https://mcp.example.com/mcp"));
        assert_eq!(back[0].headers.get("Authorization").map(String::as_str), Some("Bearer x"));
    }
}
