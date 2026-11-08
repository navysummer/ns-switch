//! Gemini CLI MCP 配置适配器（`~/.gemini/settings.json`）。
//!
//! `settings.json` 的 `mcpServers` 对象定义服务器；启停状态存在
//! `~/.gemini/mcp-server-enablement.json`（`{ "<id>": { "enabled": bool } }`，
//! 缺省 = 启用）。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::model::{McpServer, McpTransport};

/// 服务器名 → 启用状态（缺省 true）。
pub type Enablement = std::collections::BTreeMap<String, bool>;

fn get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    v.get(k)
}

fn str_of(v: &Value, k: &str) -> Option<String> {
    get(v, k).and_then(|v| v.as_str().map(str::to_string))
}

fn arr_of(v: &Value, k: &str) -> Vec<String> {
    get(v, k)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn map_of(v: &Value, k: &str) -> std::collections::BTreeMap<String, String> {
    get(v, k)
        .and_then(|v| v.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// 从 `settings.json` 的 `mcpServers` 对象读取服务器（结合启停表）。
pub fn read_servers(doc: &Value, enablement: &Enablement) -> Vec<McpServer> {
    let Some(obj) = doc.get("mcpServers").and_then(Value::as_object) else {
        return Vec::new();
    };
    obj.iter()
        .map(|(name, v)| {
            // Gemini：httpUrl > url > command；type 用于远程 disambiguate。
            let transport = match str_of(v, "type").as_deref() {
                Some("http") => McpTransport::Http,
                Some("sse") => McpTransport::Sse,
                _ => {
                    if str_of(v, "httpUrl").is_some() || str_of(v, "url").is_some() {
                        McpTransport::Http
                    } else {
                        McpTransport::Stdio
                    }
                }
            };
            let url = str_of(v, "httpUrl").or_else(|| str_of(v, "url"));
            let enabled = enablement.get(name).copied().unwrap_or(true);
            McpServer {
                name: name.clone(),
                transport,
                command: str_of(v, "command"),
                args: arr_of(v, "args"),
                env: map_of(v, "env"),
                url,
                headers: map_of(v, "headers"),
                enabled,
            }
        })
        .collect()
}

/// 把 [`McpServer`] 投影为 Gemini 的 JSON 条目。
pub fn to_json(server: &McpServer) -> Value {
    let mut m = Map::new();
    match server.transport {
        McpTransport::Stdio => {
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

/// 写 `mcpServers` 到 `settings.json` 文档；返回是否有变化。
///
/// 禁用的服务器从 `mcpServers` 移除并记入 `enablement`（`enabled: false`）；
/// 启用的从 enablement 清除。
pub fn apply_servers(
    doc: &mut Value,
    enablement: &mut Enablement,
    remove_names: &[String],
    upserts: &[McpServer],
) -> bool {
    let root = doc.as_object_mut().expect("settings.json 顶层对象");
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
            enablement.remove(name);
            changed = true;
        }
    }
    for s in upserts {
        if s.enabled {
            enablement.remove(&s.name);
            let v = to_json(s);
            if servers.get(&s.name) != Some(&v) {
                servers.insert(s.name.clone(), v);
                changed = true;
            }
        } else {
            if servers.remove(&s.name).is_some() {
                changed = true;
            }
            if enablement.get(&s.name) != Some(&false) {
                enablement.insert(s.name.clone(), false);
                changed = true;
            }
        }
    }
    changed
}

/// 读启停表（`~/.gemini/mcp-server-enablement.json`）。
pub fn load_enablement(path: &Path) -> Enablement {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return Enablement::new(),
    };
    let Ok(v) = serde_json_lenient::from_str::<Value>(&text) else {
        return Enablement::new();
    };
    let mut out = Enablement::new();
    if let Some(obj) = v.as_object() {
        for (name, e) in obj {
            let enabled = e
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            out.insert(name.clone(), enabled);
        }
    }
    out
}

/// 写启停表。
pub fn write_enablement(path: &Path, enablement: &Enablement) -> Result<(), String> {
    let mut obj = Map::new();
    for (name, enabled) in enablement {
        obj.insert(name.clone(), serde_json::json!({ "enabled": enabled }));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(obj)).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("写 {} 失败: {e}", path.display()))
}

/// Gemini 启停表路径。
pub fn enablement_path(settings: &Path) -> PathBuf {
    settings
        .parent()
        .map(|p| p.join("mcp-server-enablement.json"))
        .unwrap_or_else(|| PathBuf::from("mcp-server-enablement.json"))
}

/// 校验 `settings.json` 顶层对象。
pub fn validate_doc(text: &str) -> Result<Value, String> {
    let v: Value = serde_json_lenient::from_str(text).map_err(|e| e.to_string())?;
    if !v.is_object() {
        return Err("Gemini settings.json 顶层应为对象".into());
    }
    Ok(v)
}

/// 从磁盘读 Gemini MCP 配置。
pub fn load(settings: &Path) -> Result<(Vec<McpServer>, Enablement), String> {
    let text = match std::fs::read_to_string(settings) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), Enablement::new()));
        }
        Err(e) => return Err(format!("读取 {} 失败: {e}", settings.display())),
    };
    let doc = validate_doc(&text)?;
    let en = load_enablement(&enablement_path(settings));
    Ok((read_servers(&doc, &en), en))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_enablement() {
        let servers = vec![
            McpServer {
                name: "github".into(),
                transport: McpTransport::Stdio,
                command: Some("npx".into()),
                args: vec!["-y".into(), "@github/mcp".into()],
                env: [("TOKEN".into(), "$TOKEN".into())].into(),
                enabled: true,
                ..Default::default()
            },
            McpServer {
                name: "api".into(),
                transport: McpTransport::Http,
                url: Some("https://x/mcp".into()),
                enabled: true,
                ..Default::default()
            },
        ];
        let mut doc = serde_json::json!({ "mcp": { "allowed": [] } });
        let mut en = Enablement::new();
        assert!(apply_servers(&mut doc, &mut en, &[], &servers));
        let back = read_servers(&doc, &en);
        assert_eq!(back.len(), 2);
        let github = back.iter().find(|s| s.name == "github").expect("github");
        assert_eq!(github.transport, McpTransport::Stdio);
        let api = back.iter().find(|s| s.name == "api").expect("api");
        assert_eq!(api.url.as_deref(), Some("https://x/mcp"));
        // 禁用 api
        let mut api_off = servers[1].clone();
        api_off.enabled = false;
        assert!(apply_servers(&mut doc, &mut en, &[], &[api_off]));
        assert_eq!(en.get("api"), Some(&false));
        assert_eq!(read_servers(&doc, &en).len(), 1);
        // 保留 mcp 键
        assert!(doc.get("mcp").is_some());
    }
}
