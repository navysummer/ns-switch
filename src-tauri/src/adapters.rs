use crate::models::{McpServer, Provider};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ConfigKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigKind {
    JsonEnv,
    CodexToml,
    TomlEnv,
    YamlEnv,
}

pub fn tools() -> Vec<ToolDef> {
    vec![
        ToolDef { id: "claude_code", name: "Claude Code", kind: ConfigKind::JsonEnv },
        ToolDef { id: "claude_desktop", name: "Claude Desktop", kind: ConfigKind::JsonEnv },
        ToolDef { id: "codex", name: "Codex", kind: ConfigKind::CodexToml },
        ToolDef { id: "gemini", name: "Gemini CLI", kind: ConfigKind::JsonEnv },
        ToolDef { id: "grok", name: "Grok Build", kind: ConfigKind::TomlEnv },
        ToolDef { id: "opencode", name: "OpenCode", kind: ConfigKind::JsonEnv },
        ToolDef { id: "openclaw", name: "OpenClaw", kind: ConfigKind::JsonEnv },
        ToolDef { id: "hermes", name: "Hermes Agent", kind: ConfigKind::YamlEnv },
    ]
}

pub fn tool_by_id(id: &str) -> Option<ToolDef> {
    tools().into_iter().find(|t| t.id == id)
}

pub fn config_path(tool_id: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match tool_id {
        "claude_code" => Some(home.join(".claude").join("settings.json")),
        "claude_desktop" => desktop_config_dir().map(|d| d.join("claude_desktop_config.json")),
        "codex" => Some(home.join(".codex").join("config.toml")),
        "gemini" => Some(home.join(".gemini").join("settings.json")),
        "grok" => Some(home.join(".grok").join("config.toml")),
        "opencode" => Some(home.join(".config").join("opencode").join("opencode.json")),
        "openclaw" => Some(home.join(".openclaw").join("openclaw.json")),
        "hermes" => Some(home.join(".hermes").join("config.yaml")),
        _ => None,
    }
}

fn desktop_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library").join("Application Support").join("Claude"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::config_dir().map(|c| c.join("Claude"))
    }
    #[cfg(target_os = "linux")]
    {
        dirs::config_dir().map(|c| c.join("Claude"))
    }
}

pub fn config_exists(tool_id: &str) -> bool {
    config_path(tool_id).map(|p| p.exists()).unwrap_or(false)
}

/// env plan for the provider fields -> tool specific env var names
fn env_plan(tool_id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match tool_id {
        "claude_code" | "claude_desktop" | "hermes" => {
            Some(("ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_MODEL"))
        }
        "gemini" => Some(("GOOGLE_GENERATIVE_AI_BASE_URL", "GEMINI_API_KEY", "")),
        "grok" => Some(("XAI_BASE_URL", "XAI_API_KEY", "")),
        "opencode" => Some(("OPENAI_BASE_URL", "OPENAI_API_KEY", "OPENAI_MODEL")),
        "openclaw" => Some(("OPENAI_BASE_URL", "OPENAI_API_KEY", "")),
        _ => None,
    }
}

fn sanitize(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "provider".to_string()
    } else {
        out.to_string()
    }
}

pub fn switch_provider_to(tool_id: &str, p: &Provider) -> Result<(), String> {
    let def = tool_by_id(tool_id).ok_or("unknown tool")?;
    let path = config_path(tool_id).ok_or("unsupported tool")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match def.kind {
        ConfigKind::JsonEnv => write_json_env(&path, tool_id, p),
        ConfigKind::CodexToml => write_codex_toml(&path, p),
        ConfigKind::TomlEnv => write_toml_env(&path, tool_id, p),
        ConfigKind::YamlEnv => write_yaml_env(&path, tool_id, p),
    }
}

fn read_json(path: &PathBuf) -> Map<String, Value> {
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| Map::new()),
        Err(_) => Map::new(),
    }
}

fn write_json_env(path: &PathBuf, tool_id: &str, p: &Provider) -> Result<(), String> {
    let mut root = read_json(path);
    let plan = env_plan(tool_id).ok_or("unsupported env plan")?;
    let mut env = match root.get("env") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    let set_if = |env: &mut Map<String, Value>, key: &str, val: &str| {
        if !key.is_empty() {
            env.insert(key.to_string(), json!(val));
        }
    };
    set_if(&mut env, plan.0, &p.base_url);
    set_if(&mut env, plan.1, &p.api_key);
    set_if(&mut env, plan.2, &p.model);
    root.insert("env".to_string(), Value::Object(env));
    merge_shared_json(&mut root, &p.shared_config);
    atomic_write_json(path, &root)
}

/// merge a JSON object of shared config into the tool's root config, preserving
/// existing keys unless the shared value is an object nested at top level.
fn merge_shared_json(root: &mut Map<String, Value>, shared: &str) {
    if shared.trim().is_empty() {
        return;
    }
    let Ok(Value::Object(src)) = serde_json::from_str::<Value>(shared) else {
        return;
    };
    for (k, v) in src {
        // do not clobber keys that are core (env, mcpServers) managed by us
        if k == "env" || k == "mcpServers" {
            continue;
        }
        root.insert(k, v);
    }
}

fn json_to_toml(v: &Value) -> Option<toml::Value> {
    match v {
        Value::String(s) => Some(toml::Value::String(s.clone())),
        Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        Value::Number(n) => n
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| n.as_f64().map(toml::Value::Float)),
        Value::Array(arr) => {
            let items: Vec<toml::Value> = arr.iter().filter_map(json_to_toml).collect();
            Some(toml::Value::Array(items))
        }
        _ => None,
    }
}

fn write_codex_toml(path: &PathBuf, p: &Provider) -> Result<(), String> {
    let src = fs::read_to_string(path).unwrap_or_default();
    let mut root = if src.trim().is_empty() {
        toml::map::Map::new()
    } else {
        src.parse::<toml::Value>()
            .map(|v| match v {
                toml::Value::Table(t) => t,
                _ => toml::map::Map::new(),
            })
            .unwrap_or_default()
    };
    root.insert("model_provider".into(), toml::Value::String(sanitize(&p.name)));
    if let Some(model) = root.get_mut("model") {
        if !p.model.is_empty() {
            *model = toml::Value::String(p.model.clone());
        }
    } else if !p.model.is_empty() {
        root.insert("model".to_string(), toml::Value::String(p.model.clone()));
    }
    let mut providers_table = match root.remove("model_providers") {
        Some(toml::Value::Table(t)) => t,
        _ => toml::map::Map::new(),
    };
    let mut block = toml::map::Map::new();
    block.insert("name".into(), toml::Value::String(p.name.clone()));
    block.insert("base_url".into(), toml::Value::String(p.base_url.clone()));
    block.insert("env_key".into(), toml::Value::String("OPENAI_API_KEY".into()));
    block.insert("env_val".into(), toml::Value::String(p.api_key.clone()));
    block.insert("wire_api".into(), toml::Value::String("chat".into()));
    providers_table.insert(sanitize(&p.name), toml::Value::Table(block));
    root.insert("model_providers".into(), toml::Value::Table(providers_table));

    // shared config snippet -> toml root (best-effort primitives)
    if !p.shared_config.trim().is_empty() {
        if let Ok(Value::Object(src)) = serde_json::from_str::<Value>(&p.shared_config) {
            for (k, v) in src {
                if k == "model_providers" || k == "model_provider" {
                    continue;
                }
                if let Some(tv) = json_to_toml(&v) {
                    root.insert(k, tv);
                }
            }
        }
    }

    atomic_write_text(path, &toml::to_string(&toml::Value::Table(root)).map_err(|e| e.to_string())?)
}

fn write_toml_env(path: &PathBuf, tool_id: &str, p: &Provider) -> Result<(), String> {
    let plan = env_plan(tool_id).ok_or("unsupported env plan")?;
    let src = fs::read_to_string(path).unwrap_or_default();
    let mut root = if src.trim().is_empty() {
        toml::map::Map::new()
    } else {
        src.parse::<toml::Value>()
            .map(|v| match v {
                toml::Value::Table(t) => t,
                _ => toml::map::Map::new(),
            })
            .unwrap_or_default()
    };
    let set_if = |m: &mut toml::map::Map<String, toml::Value>, key: &str, val: &str| {
        if !key.is_empty() && !val.is_empty() {
            m.insert(key.to_string(), toml::Value::String(val.to_string()));
        } else if !key.is_empty() {
            m.remove(key);
        }
    };
    set_if(&mut root, plan.0, &p.base_url);
    set_if(&mut root, plan.1, &p.api_key);
    set_if(&mut root, plan.2, &p.model);
    atomic_write_text(path, &toml::to_string(&toml::Value::Table(root)).map_err(|e| e.to_string())?)
}

fn write_yaml_env(path: &PathBuf, tool_id: &str, p: &Provider) -> Result<(), String> {
    let plan = env_plan(tool_id).ok_or("unsupported env plan")?;
    let mut lines: Vec<String> = Vec::new();
    let push = |lines: &mut Vec<String>, key: &str, val: &str| {
        if !key.is_empty() {
            lines.push(format!("{key}: \"{}\"", val.replace('"', "\\\"")));
        }
    };
    push(&mut lines, plan.0, &p.base_url);
    push(&mut lines, plan.1, &p.api_key);
    if !plan.2.is_empty() {
        push(&mut lines, plan.2, &p.model);
    }
    atomic_write_text(path, &format!("{}\n", lines.join("\n")))
}

pub fn atomic_write_json(path: &PathBuf, val: &serde_json::Map<String, Value>) -> Result<(), String> {
    let body = serde_json::to_string_pretty(&Value::Object(val.clone())).map_err(|e| e.to_string())?;
    atomic_write_text(path, &format!("{body}\n"))
}

fn atomic_write_text(path: &PathBuf, body: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// build the mcpServers JSON fragment for a list of active servers
pub fn mcp_json_fragment(servers: &[McpServer]) -> Map<String, Value> {
    let mut out = Map::new();
    for s in servers {
        if !s.is_active {
            continue;
        }
        out.insert(
            s.name.clone(),
            json!({
                "command": s.command,
                "args": s.args,
                "env": s.env,
                "cwd": s.cwd,
                "transport": s.transport,
            }),
        );
    }
    out
}

pub fn write_mcp_to_tool(tool_id: &str, servers: &[McpServer]) -> Result<bool, String> {
    let some_active = servers.iter().any(|s| s.is_active);
    let path = match config_path(tool_id) {
        Some(p) => p,
        None => return Ok(false),
    };
    if !path.exists() && !some_active {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let fragment = mcp_json_fragment(servers);
    match tool_by_id(tool_id).map(|t| t.kind) {
        Some(ConfigKind::JsonEnv) => {
            let mut root = read_json(&path);
            root.insert("mcpServers".into(), Value::Object(fragment));
            atomic_write_json(&path, &root)?;
            Ok(true)
        }
        Some(ConfigKind::CodexToml) | Some(ConfigKind::TomlEnv) => {
            let src = fs::read_to_string(&path).unwrap_or_default();
            let mut root = if src.trim().is_empty() {
                toml::map::Map::new()
            } else {
                src.parse::<toml::Value>()
                    .map(|v| match v {
                        toml::Value::Table(t) => t,
                        _ => toml::map::Map::new(),
                    })
                    .unwrap_or_default()
            };
            let mut mcp = toml::map::Map::new();
            for (name, spec) in fragment {
                if let Value::Object(o) = spec {
                    let mut t = toml::map::Map::new();
                    if let Some(v) = o.get("command").and_then(|v| v.as_str()) {
                        t.insert("command".into(), toml::Value::String(v.into()));
                    }
                    if let Some(v) = o.get("args").and_then(|v| v.as_array()) {
                        let arr: Vec<toml::Value> = v
                            .iter()
                            .filter_map(|x| x.as_str().map(ToString::to_string))
                            .map(toml::Value::String)
                            .collect();
                        t.insert("args".into(), toml::Value::Array(arr));
                    }
                    if let Some(v) = o.get("env").and_then(|v| v.as_object()) {
                        let mut env = toml::map::Map::new();
                        for (k, val) in v {
                            env.insert(
                                k.clone(),
                                toml::Value::String(val.as_str().unwrap_or_default().to_string()),
                            );
                        }
                        t.insert("env".into(), toml::Value::Table(env));
                    }
                    if let Some(v) = o.get("cwd").and_then(|v| v.as_str()) {
                        t.insert("cwd".into(), toml::Value::String(v.into()));
                    }
                    if let Some(v) = o.get("transport").and_then(|v| v.as_str()) {
                        t.insert("transport".into(), toml::Value::String(v.into()));
                    }
                    mcp.insert(name, toml::Value::Table(t));
                }
            }
            if tool_id == "grok" {
                root.insert("mcpServers".into(), toml::Value::Table(mcp));
            } else {
                root.insert("mcp_servers".into(), toml::Value::Table(mcp));
            }
            atomic_write_text(&path, &toml::to_string(&toml::Value::Table(root)).map_err(|e| e.to_string())?)?;
            Ok(true)
        }
        Some(ConfigKind::YamlEnv) => Ok(false),
        None => Ok(false),
    }
}

pub fn all_tool_ids() -> Vec<String> {
    tools().into_iter().map(|t| t.id.to_string()).collect()
}

/// remove ns-managed provider entries so a tool can log back in officially
pub fn reset_config(tool_id: &str) -> Result<(), String> {
    let Some(path) = config_path(tool_id) else { return Ok(()) };
    if !path.exists() {
        return Ok(());
    }
    match tool_by_id(tool_id).map(|t| t.kind) {
        Some(ConfigKind::JsonEnv) => {
            let plan = match env_plan(tool_id) {
                Some(p) => p,
                None => return Ok(()),
            };
            let mut root = read_json(&path);
            if let Some(env) = root.get_mut("env").and_then(|e| e.as_object_mut()) {
                for key in [plan.0, plan.1, plan.2] {
                    if !key.is_empty() {
                        env.remove(key);
                    }
                }
                if env.is_empty() {
                    root.remove("env");
                }
            }
            atomic_write_json(&path, &root)
        }
        Some(ConfigKind::CodexToml) => {
            let src = fs::read_to_string(&path).unwrap_or_default();
            let mut root = match src.parse::<toml::Value>() {
                Ok(toml::Value::Table(t)) => t,
                _ => return Ok(()),
            };
            root.remove("model_provider");
            root.remove("model_providers");
            root.remove("model");
            atomic_write_text(&path, &toml::to_string(&toml::Value::Table(root)).map_err(|e| e.to_string())?)
        }
        Some(ConfigKind::TomlEnv) => {
            let plan = match env_plan(tool_id) {
                Some(p) => p,
                None => return Ok(()),
            };
            let src = fs::read_to_string(&path).unwrap_or_default();
            let mut root = match src.parse::<toml::Value>() {
                Ok(toml::Value::Table(t)) => t,
                _ => return Ok(()),
            };
            for key in [plan.0, plan.1, plan.2] {
                if !key.is_empty() {
                    root.remove(key);
                }
            }
            atomic_write_text(&path, &toml::to_string(&toml::Value::Table(root)).map_err(|e| e.to_string())?)
        }
        Some(ConfigKind::YamlEnv) => {
            let plan = match env_plan(tool_id) {
                Some(p) => p,
                None => return Ok(()),
            };
            let src = fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<String> = src.lines().filter(|l| {
                let trimmed = l.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    return true;
                }
                let key = trimmed.split(':').next().unwrap_or("").trim();
                !(key == plan.0 || key == plan.1 || key == plan.2)
            }).map(|l| l.to_string()).collect();
atomic_write_text(&path, &format!("{}\n", lines.join("\n")))
        }
        None => Ok(()),
    }
}

/// Read MCP servers back out of an existing tool config (bidirectional sync).
/// Returns (name, (command, args, env, cwd, transport, is_active))
pub fn read_mcp_from_config(
    tool_id: &str,
) -> Vec<(String, (String, Vec<String>, std::collections::HashMap<String, String>, String, String, bool))> {
    let mut out = Vec::new();
    let Some(path) = config_path(tool_id) else { return out };
    if !path.exists() {
        return out;
    }
    match tool_by_id(tool_id).map(|t| t.kind) {
        Some(ConfigKind::JsonEnv) => {
            let root = read_json(&path);
            let Some(map) = root.get("mcpServers").and_then(|v| v.as_object()) else { return out };
            for (name, v) in map {
                let (command, args, env, cwd, transport, active) = parse_mcp_value(v);
                out.push((name.clone(), (command, args, env, cwd, transport, active)));
            }
        }
        Some(ConfigKind::CodexToml) | Some(ConfigKind::TomlEnv) => {
            let Ok(src) = fs::read_to_string(&path) else { return out };
            let Ok(toml::Value::Table(root)) = src.parse::<toml::Value>() else { return out };
            let mcp_key = if tool_id == "grok" { "mcpServers" } else { "mcp_servers" };
            let v = match root.get(mcp_key) {
                Some(toml::Value::Table(t)) => {
                    let mut m = Map::new();
                    for (k, spec) in t {
                        m.insert(k.clone(), toml_to_json(spec));
                    }
                    m
                }
                _ => return out,
            };
            for (name, v) in v {
                let (command, args, env, cwd, transport, active) = parse_mcp_value(&v);
                out.push((name, (command, args, env, cwd, transport, active)));
            }
        }
        Some(ConfigKind::YamlEnv) => {}
        None => {}
    }
    out
}

fn parse_mcp_value(
    v: &Value,
) -> (String, Vec<String>, std::collections::HashMap<String, String>, String, String, bool) {
    let o = v.as_object().cloned().unwrap_or_default();
    let cmd = o.get("command").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let args: Vec<String> = o
        .get("args")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(ToString::to_string)).collect())
        .unwrap_or_default();
    let mut env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(e) = o.get("env").and_then(|x| x.as_object()) {
        for (k, val) in e {
            env.insert(k.clone(), val.as_str().unwrap_or_default().to_string());
        }
    }
    let cwd = o.get("cwd").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let transport = o
        .get("transport")
        .and_then(|x| x.as_str())
        .unwrap_or("stdio")
        .to_string();
    let active = o.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true);
    (cmd, args, env, cwd, transport, active)
}

fn toml_to_json(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => json!(s),
        toml::Value::Integer(i) => json!(i),
        toml::Value::Float(f) => json!(f),
        toml::Value::Boolean(b) => json!(b),
        toml::Value::Array(a) => Value::Array(a.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            let mut m = Map::new();
            for (k, x) in t {
                m.insert(k.clone(), toml_to_json(x));
            }
            Value::Object(m)
        }
        _ => Value::Null,
    }
}

/// Read (base_url, api_key, model) from an already-existing tool config, if any.
pub fn read_existing_config(tool_id: &str) -> (String, String, String) {
    let empty = (String::new(), String::new(), String::new());
    let Some(path) = config_path(tool_id) else { return empty };
    match tool_by_id(tool_id).map(|t| t.kind) {
        Some(ConfigKind::JsonEnv) => {
            let plan = match env_plan(tool_id) {
                Some(p) => p,
                None => return empty,
            };
            let root = read_json(&path);
            let env = root.get("env").and_then(|e| e.as_object());
            let get = |k: &str| {
                env.and_then(|m| m.get(k))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let base = get(plan.0);
            let key = get(plan.1);
            let model = get(plan.2);
            if base.is_empty() && key.is_empty() && model.is_empty() {
                empty
            } else {
                (base, key, model)
            }
        }
        Some(ConfigKind::CodexToml) => {
            let Ok(src) = fs::read_to_string(&path) else { return empty };
            let Ok(toml::Value::Table(root)) = src.parse::<toml::Value>() else { return empty };
            let Some(active) = root.get("model_provider").and_then(|v| v.as_str()) else {
                return empty;
            };
            let block = root
                .get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get(active))
                .and_then(|v| v.as_table());
            let Some(block) = block else { return empty };
            let Some(base) = block.get("base_url").and_then(|v| v.as_str()) else {
                return empty;
            };
            let key = block
                .get("env_val")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
                .unwrap_or_default();
            let model = root.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
            (base.to_string(), key, model)
        }
        Some(ConfigKind::TomlEnv) => {
            let plan = match env_plan(tool_id) {
                Some(p) => p,
                None => return empty,
            };
            let Ok(src) = fs::read_to_string(&path) else { return empty };
            let Ok(toml::Value::Table(root)) = src.parse::<toml::Value>() else { return empty };
            let get = |k: &str| {
                root.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string()
            };
            let base = get(plan.0);
            let key = get(plan.1);
            let model = get(plan.2);
            if base.is_empty() && key.is_empty() && model.is_empty() {
                empty
            } else {
                (base, key, model)
            }
        }
        Some(ConfigKind::YamlEnv) => {
            let plan = match env_plan(tool_id) {
                Some(p) => p,
                None => return empty,
            };
            let Ok(src) = fs::read_to_string(&path) else { return empty };
            let mut base = String::new();
            let mut key = String::new();
            let mut model = String::new();
            for line in src.lines() {
                let trimmed = line.trim();
                if let Some((k, v)) = trimmed.split_once(':') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if k == plan.0 { base = v.to_string(); }
                    if k == plan.1 { key = v.to_string(); }
                    if !plan.2.is_empty() && k == plan.2 { model = v.to_string(); }
                }
            }
            if base.is_empty() && key.is_empty() && model.is_empty() {
                empty
            } else {
                (base, key, model)
            }
        }
        None => empty,
    }
}