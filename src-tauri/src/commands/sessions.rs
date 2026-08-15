use crate::db::AppState;
use crate::models::{ProxyHealthItem, ProxyStatus, SessionItem};
use crate::AppResult;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::Mutex;

#[tauri::command]
pub fn list_sessions(state: tauri::State<AppState>, tool_id: String) -> AppResult<Vec<SessionItem>> {
    drop(state);
    match tool_id.as_str() {
        "claude_code" => Ok(list_claude_sessions()),
        "opencode" => Ok(list_opencode_sessions()),
        "codex" => Ok(list_codex_sessions()),
        _ => Ok(Vec::new()),
    }
}

fn list_claude_sessions() -> Vec<SessionItem> {
    let mut out = Vec::new();
    let Some(home) = dirs::home_dir() else { return out };
    let root = home.join(".claude").join("projects");
    let Ok(proj_dirs) = std::fs::read_dir(&root) else {
        return out;
    };
    for dir in proj_dirs.flatten() {
        if !dir.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(dir.path()) else { continue };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                if let Some(s) = parse_claude_session(&path) {
                    out.push(s);
                }
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out.truncate(200);
    out
}

fn parse_claude_session(path: &std::path::Path) -> Option<SessionItem> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let mut cwd = String::new();
    let mut title = String::new();
    let mut updated_at = String::new();
    let mut count = 0i64;
    for line in reader.lines().flatten() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        let ts = v.get("timestamp").and_then(|x| x.as_str()).unwrap_or("");
        if !ts.is_empty() && (updated_at.is_empty() || ts > updated_at.as_str()) {
            updated_at = ts.to_string();
        }
        if cwd.is_empty() {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                cwd = c.to_string();
            }
        }
        let msg_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        if msg_type == "user" || msg_type == "assistant" {
            count += 1;
            if title.is_empty() {
                if let Some(text) = extract_message_text(&v) {
                    let mut cleaned: String = text
                        .split_whitespace()
                        .take(12)
                        .collect::<Vec<_>>()
                        .join(" ");
                    cleaned = cleaned.trim().to_string();
                    if !cleaned.is_empty() {
                        title = cleaned;
                    }
                }
            }
        }
        if title.is_empty() && msg_type == "summary" {
            // summaries have no user-readable title; leave empty
        }
    }
    let id = path
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Some(SessionItem {
        id,
        tool_id: "claude_code".to_string(),
        path: path.to_string_lossy().to_string(),
        cwd,
        title,
        updated_at,
        message_count: count,
    })
}

fn extract_message_text(v: &serde_json::Value) -> Option<String> {
    let msg = v.get("message")?;
    if let Some(s) = msg.get("content").and_then(|c| c.as_str()) {
        return Some(s.to_string());
    }
    if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
        for item in arr {
            if let Some(t) = item.get("text").and_then(|x| x.as_str()) {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn opencode_sessions_root() -> Option<std::path::PathBuf> {
    let base = if let Ok(x) = std::env::var("OPENCODE_DATA") {
        Some(std::path::PathBuf::from(x))
    } else {
        dirs::data_local_dir().map(|d| d.join("opencode"))
    }?;
    Some(base.join("project"))
}

fn list_opencode_sessions() -> Vec<SessionItem> {
    let mut out = Vec::new();
    let Some(root) = opencode_sessions_root() else { return out };
    let Ok(proj_dirs) = std::fs::read_dir(root) else { return out };
    for proj in proj_dirs.flatten() {
        if !proj.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        // two possible storage layouts
        for sub in ["storage/session", "sessions"] {
            let dir = proj.path().join(sub);
            let Ok(files) = std::fs::read_dir(&dir) else { continue };
            for f in files.flatten() {
                let path = f.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Some(s) = parse_opencode_session(&path) {
                        out.push(s);
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out.truncate(200);
    out
}

fn parse_opencode_session(path: &std::path::Path) -> Option<SessionItem> {
    let raw = std::fs::read_to_string(path).ok()?;
    let mut v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if v.get("info").and_then(|x| x.as_object()).is_some() {
        v = v.get("info").unwrap().clone();
    }
    let directory = v.get("directory").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .map(ToString::to_string)
        .unwrap_or_default();
    let updated = v
        .get("time")
        .and_then(|t| t.get("updated"))
        .and_then(|x| x.as_str())
        .map(ToString::to_string)
        .unwrap_or_default();
    let messages = v
        .get("messages")
        .and_then(|x| x.as_array())
        .map(|a| a.len() as i64)
        .unwrap_or(0);
    let id = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Some(SessionItem {
        id,
        tool_id: "opencode".into(),
        path: path.to_string_lossy().to_string(),
        cwd: directory,
        title,
        updated_at: updated,
        message_count: messages,
    })
}

fn list_codex_sessions() -> Vec<SessionItem> {
    let mut out = Vec::new();
    let Some(home) = dirs::home_dir() else { return out };
    let root = home.join(".codex").join("sessions");
    let Ok(files) = std::fs::read_dir(&root) else { return out };
    for f in files.flatten() {
        let path = f.path();
        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
            if let Some(s) = parse_codex_session(&path) {
                out.push(s);
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out.truncate(200);
    out
}

fn parse_codex_session(path: &std::path::Path) -> Option<SessionItem> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let mut cwd = String::new();
    let mut title = String::new();
    let mut updated_at = String::new();
    let mut count = 0i64;
    for line in reader.lines().flatten() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        let ts = v.get("timestamp").and_then(|x| x.as_str()).unwrap_or("");
        if !ts.is_empty() && (updated_at.is_empty() || ts > updated_at.as_str()) {
            updated_at = ts.to_string();
        }
        if cwd.is_empty() {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                cwd = c.to_string();
            }
        }
        let ev = v.get("event").and_then(|x| x.as_str()).unwrap_or("");
        if matches!(ev, "message" | "input" | "user" | "assistant") {
            count += 1;
        }
        if title.is_empty() && ev == "input" {
            if let Some(t) = v
                .get("input")
                .and_then(|i| i.as_str())
                .or_else(|| {
                    v.get("input")
                        .and_then(|i| i.get("text"))
                        .and_then(|x| x.as_str())
                })
            {
                title = t.split_whitespace().take(12).collect::<Vec<_>>().join(" ");
            }
        }
    }
    let id = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Some(SessionItem {
        id,
        tool_id: "codex".into(),
        path: path.to_string_lossy().to_string(),
        cwd,
        title,
        updated_at,
        message_count: count,
    })
}

#[tauri::command]
pub fn open_path(state: tauri::State<AppState>, path: String) -> AppResult<()> {
    drop(state);
    opener::open(&path).map_err(|e| e.to_string())?;
    Ok(())
}

/// resume a previous Claude Code session by launching the CLI in that project
#[tauri::command]
pub fn restore_session(state: tauri::State<AppState>, path: String) -> AppResult<String> {
    drop(state);
    let file = std::path::PathBuf::from(&path);
    if !file.exists() {
        return Err("session file not found".into());
    }
    let path_str = file.to_string_lossy();
    if path_str.contains(".codex/") || path_str.contains("opencode/") {
        // resume is only supported for Claude Code sessions; open the folder instead
        let cwd = dir_of_first_cwd(&file)?.unwrap_or_else(|| std::path::PathBuf::from("."));
        opener::open(&cwd).map_err(|e| e.to_string())?;
        return Ok("restore is only supported for Claude Code sessions; opened the project folder".into());
    }
    let session_id = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let cwd = dir_of_first_cwd(&file)?.unwrap_or_else(|| std::path::PathBuf::from("."));
    std::process::Command::new("claude")
        .arg("--resume")
        .arg(&session_id)
        .current_dir(&cwd)
        .spawn()
        .map_err(|e| format!("failed to launch claude: {e}"))?;
    Ok(session_id)
}

fn dir_of_first_cwd(file: &std::path::Path) -> Result<Option<std::path::PathBuf>, String> {
    for line in read_first_400_lines(file) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(c) = v
                .get("cwd")
                .and_then(|x| x.as_str())
                .or_else(|| v.get("directory").and_then(|x| x.as_str()))
            {
                if !c.is_empty() {
                    return Ok(Some(std::path::PathBuf::from(c)));
                }
            }
        }
    }
    Ok(None)
}

fn read_first_400_lines(path: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(f) = std::fs::File::open(path) {
        let reader = std::io::BufReader::new(f);
        for line in reader.lines().flatten().take(400) {
            out.push(line);
        }
    }
    out
}

// ---------------- proxy ----------------

pub struct RunningProxy {
    pub task: tokio::task::JoinHandle<()>,
    pub port: u16,
}

#[derive(Clone, Copy, Default)]
pub struct CircuitState {
    pub failures: u32,
    pub open: bool,
    pub opened_at: Option<std::time::Instant>,
}

pub struct ProxyManager {
    pub handles: Mutex<HashMap<String, RunningProxy>>,
    pub health: Mutex<HashMap<String, Vec<ProxyHealthItem>>>,
    pub breaker: Mutex<HashMap<String, HashMap<i64, CircuitState>>>,
    pub monitor_started: Mutex<bool>,
}

impl Default for ProxyManager {
    fn default() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
            health: Mutex::new(HashMap::new()),
            breaker: Mutex::new(HashMap::new()),
            monitor_started: Mutex::new(false),
        }
    }
}

impl ProxyManager {
    fn is_open(&self, tool: &str, id: i64) -> bool {
        let Ok(breaker) = self.breaker.lock() else { return false };
        let Some(st) = breaker.get(tool).and_then(|m| m.get(&id)) else { return false };
        if st.open {
            if let Some(opened) = st.opened_at {
                if opened.elapsed() >= std::time::Duration::from_secs(30) {
                    return false;
                }
            }
            return true;
        }
        false
    }

    fn on_failure(&self, tool: &str, id: i64) {
        if let Ok(mut breaker) = self.breaker.lock() {
            let m = breaker.entry(tool.to_string()).or_default();
            let st = m.entry(id).or_default();
            st.failures += 1;
            if st.failures >= 3 {
                st.open = true;
                st.opened_at = Some(std::time::Instant::now());
            }
        }
    }

    fn on_success(&self, tool: &str, id: i64) {
        if let Ok(mut breaker) = self.breaker.lock() {
            let m = breaker.entry(tool.to_string()).or_default();
            let st = m.entry(id).or_default();
            st.failures = 0;
            st.open = false;
        }
    }
}

pub fn default_port_for_tool(tool_id: &str) -> u16 {
    let base = match tool_id {
        "claude_code" => 9301,
        "claude_desktop" => 9302,
        "codex" => 9303,
        "gemini" => 9304,
        "grok" => 9305,
        "opencode" => 9306,
        "openclaw" => 9307,
        "hermes" => 9308,
        _ => 9500,
    };
    base
}

#[derive(Clone)]
pub struct ProxyCtx {
    pub data_dir: PathBuf,
    pub tool_id: String,
    pub mgr: std::sync::Arc<ProxyManager>,
}

fn proxy_ports_map(state: &AppState) -> std::collections::HashMap<String, u16> {
    let Ok(conn) = state.db.lock() else { return std::collections::HashMap::new() };
    let v = crate::db::get_settings_value(&conn);
    let mut out = std::collections::HashMap::new();
    if let Some(p) = v.get("proxyPorts").and_then(|x| x.as_object()) {
        for (k, val) in p {
            if let Some(n) = val.as_u64() {
                out.insert(k.clone(), n.min(65535) as u16);
            }
        }
    }
    out
}

fn proxy_enabled_map(state: &AppState) -> std::collections::HashMap<String, bool> {
    let Ok(conn) = state.db.lock() else { return std::collections::HashMap::new() };
    let v = crate::db::get_settings_value(&conn);
    let mut out = std::collections::HashMap::new();
    if let Some(p) = v.get("proxyEnabled").and_then(|x| x.as_object()) {
        for (k, val) in p {
            if let Some(b) = val.as_bool() {
                out.insert(k.clone(), b);
            }
        }
    }
    out
}

/// effective port for a tool proxy (custom override or default)
pub fn port_for_tool(state: &AppState, tool: &str) -> u16 {
    proxy_ports_map(state)
        .get(tool)
        .copied()
        .unwrap_or_else(|| default_port_for_tool(tool))
}

/// whether the tool is marked for automatic takeover (auto-start on launch)
pub fn proxy_takeover_enabled(state: &AppState, tool: &str) -> bool {
    proxy_enabled_map(state).get(tool).copied().unwrap_or(true)
}

fn active_provider_for(ctx: &ProxyCtx) -> Option<crate::models::Provider> {
    let conn = rusqlite::Connection::open(ctx.data_dir.join("ns-switch.db")).ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort
             FROM providers WHERE tool_id = ?1 AND is_active = 1",
        )
        .ok()?;
    let mut rows = stmt.query_map(rusqlite::params![ctx.tool_id], crate::commands::providers::row_to_provider).ok()?;
    rows.next().transpose().ok().flatten()
}

/// all usable providers for a tool, active first (used for failover)
fn provider_candidates(ctx: &ProxyCtx) -> Vec<crate::models::Provider> {
    let Ok(conn) = rusqlite::Connection::open(ctx.data_dir.join("ns-switch.db")) else { return vec![] };
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort
         FROM providers WHERE tool_id = ?1 ORDER BY is_active DESC, sort ASC, id ASC",
    ) else {
        return vec![];
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![ctx.tool_id], |r| crate::commands::providers::row_to_provider(r)) else {
        return vec![];
    };
    rows.flatten().filter(|p| !p.base_url.trim().is_empty()).collect()
}

fn build_request(
    provider: &crate::models::Provider,
    method: &axum::http::Method,
    url: &str,
    headers: &axum::http::HeaderMap,
    body: &[u8],
    provider_proto: &str,
) -> reqwest::RequestBuilder {
    let client = reqwest::Client::new();
    let mut rb = client.request(method.clone(), url);
    for (name, value) in headers {
        let name_str = name.as_str().to_ascii_lowercase();
        if matches!(name_str.as_str(), "host" | "content-length" | "connection" | "transfer-encoding" | "accept-encoding") {
            continue;
        }
        // drop anthropic-specific headers when talking to an openai provider
        if name_str.starts_with("anthropic-") || name_str == "x-api-key" {
            if provider_proto == "openai" {
                continue;
            }
        }
        if name_str == "authorization" {
            if !provider.api_key.is_empty() {
                let auth = if provider.api_key.starts_with("Bearer ") {
                    provider.api_key.clone()
                } else {
                    format!("Bearer {}", provider.api_key)
                };
                rb = rb.header(axum::http::header::AUTHORIZATION, auth);
            }
        } else if let Ok(v) = value.to_str() {
            rb = rb.header(name, v);
        }
    }
    if let Ok(extra) = serde_json::from_str::<HashMap<String, String>>(&provider.headers) {
        for (k, v) in extra {
            rb = rb.header(&k, v);
        }
    }
    // request rectifier: forward json bodies with proper content-type
    rb = rb.header(
        axum::http::header::CONTENT_TYPE,
        headers
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json"),
    );
    rb.body(body.to_vec())
}

fn extract_model(body: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(m) = v.get("model").and_then(|m| m.as_str()) {
            return m.to_string();
        }
    }
    String::new()
}

fn extract_usage(body: &[u8]) -> (i64, i64) {
    // non-streaming JSON body (anthropic input_tokens/output_tokens or openai prompt_tokens/completion_tokens)
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(u) = v.get("usage") {
            let input = u
                .get("input_tokens")
                .or_else(|| u.get("prompt_tokens"))
                .and_then(|x| x.as_i64())
                .unwrap_or(0);
            let output = u
                .get("output_tokens")
                .or_else(|| u.get("completion_tokens"))
                .and_then(|x| x.as_i64())
                .unwrap_or(0);
            return (input, output);
        }
    }
    // SSE stream: scan lines for last usage object
    let text = String::from_utf8_lossy(body);
    let mut input = 0i64;
    let mut output = 0i64;
    for line in text.lines() {
        let line = line.trim();
        let json = line.strip_prefix("data:").map(str::trim).unwrap_or(line);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
            if let Some(u) = v.get("usage") {
                if let Some(i) = u.get("input_tokens").and_then(|x| x.as_i64()) {
                    input = i;
                }
                if let Some(o) = u.get("output_tokens").and_then(|x| x.as_i64()) {
                    output = o;
                }
            }
        }
    }
    (input, output)
}

async fn relay(
    axum::extract::State(ctx): axum::extract::State<ProxyCtx>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<axum::response::Response<axum::body::Body>, axum::http::StatusCode> {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());
    let method = req.method().clone();
    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), 128 * 1024 * 1024)
        .await
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let request_model = extract_model(&body_bytes);
    let source_proto = crate::conv::client_protocol(&ctx.tool_id);

    let mut last_err = axum::http::StatusCode::SERVICE_UNAVAILABLE;
    for provider in provider_candidates(&ctx) {
        if ctx.mgr.is_open(&ctx.tool_id, provider.id) {
            continue;
        }
        let provider_proto = provider.protocol.trim();
        let converting = (source_proto == "anthropic" || source_proto == "openai")
            && (provider_proto == "openai" || provider_proto == "anthropic")
            && provider_proto != source_proto;

        // convert the request body to the provider's protocol when needed
        let mut req_bytes: Vec<u8> = body_bytes.to_vec();
        let mut stream_req = false;
        if converting {
            if let Ok(json_body) = serde_json::from_slice::<serde_json::Value>(&req_bytes) {
                stream_req = crate::conv::is_stream_request(&json_body, &headers);
                let mut conv = json_body;
                if crate::conv::convert_request(source_proto, provider_proto, &mut conv).is_ok() {
                    if let Ok(enc) = serde_json::to_vec(&conv) {
                        req_bytes = enc;
                    }
                }
            }
        }

        let url = format!("{}{}", provider.base_url.trim_end_matches('/'), path_and_query);
        let rb = build_request(&provider, &method, &url, &headers, &req_bytes, provider_proto);
        let request_started = std::time::Instant::now();
        let resp = match rb.send().await {
            Ok(r) => r,
            Err(_) => {
                ctx.mgr.on_failure(&ctx.tool_id, provider.id);
                last_err = axum::http::StatusCode::BAD_GATEWAY;
                continue;
            }
        };
        if resp.status().as_u16() >= 500 {
            ctx.mgr.on_failure(&ctx.tool_id, provider.id);
            last_err = resp.status();
            continue;
        }
        let resp_headers = resp.headers().clone();
        let status = resp.status();
        let body = match resp.bytes().await {
            Ok(b) => b,
            Err(_) => {
                ctx.mgr.on_failure(&ctx.tool_id, provider.id);
                last_err = axum::http::StatusCode::BAD_GATEWAY;
                continue;
            }
        };
        ctx.mgr.on_success(&ctx.tool_id, provider.id);
        let latency_ms = request_started.elapsed().as_millis() as u64;

        let (inp, out) = extract_usage(&body);
        if inp + out > 0 {
            crate::commands::usage::record_usage_to_db(
                &ctx.data_dir.join("ns-switch.db"),
                &ctx.tool_id,
                provider.id,
                &provider.name,
                if request_model.is_empty() { &provider.model } else { &request_model },
                inp,
                out,
            );
        }
        crate::commands::usage::record_usage_event_to_db(
            &ctx.data_dir.join("ns-switch.db"),
            &ctx.tool_id,
            &provider.name,
            if request_model.is_empty() { &provider.model } else { &request_model },
            inp,
            out,
            latency_ms,
            status.as_u16(),
        );

        let mut builder = axum::http::Response::builder().status(status);
        for (name, value) in &resp_headers {
            if matches!(name.as_str(), "content-length" | "transfer-encoding" | "connection") {
                continue;
            }
            if let Ok(v) = value.to_str() {
                builder = builder.header(name, v);
            }
        }

        // convert the response body back to the client's protocol
        let mut out_body: Vec<u8> = body.to_vec();
        if converting && status.is_success() {
            if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&body) {
                if crate::conv::convert_response(source_proto, provider_proto, &mut v).is_ok() {
                    if stream_req {
                        let sse = if source_proto == "openai" {
                            crate::conv::to_openai_sse(&v)
                        } else {
                            crate::conv::to_anthropic_sse(&v)
                        };
                        out_body = sse.into_bytes();
                        builder = builder.header(axum::http::header::CONTENT_TYPE, "text/event-stream");
                    } else if let Ok(enc) = serde_json::to_vec(&v) {
                        out_body = enc;
                        builder = builder.header(axum::http::header::CONTENT_TYPE, "application/json");
                    }
                }
            }
        }

        return builder
            .body(axum::body::Body::from(out_body))
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }
    Err(last_err)
}

#[tauri::command(rename_all = "camelCase")]
pub fn proxy_statuses(
    state: tauri::State<AppState>,
    mgr: tauri::State<std::sync::Arc<ProxyManager>>,
) -> AppResult<Vec<ProxyStatus>> {
    let handles = mgr.handles.lock().map_err(|_| "proxy lock poisoned")?;
    let mut out = Vec::new();
    for (tid, _) in crate::db::ALL_TOOLS {
        let running = handles.contains_key(*tid);
        let active = active_provider_for(&crate::commands::sessions::ProxyCtx {
            data_dir: state.active_data_dir(),
            tool_id: tid.to_string(),
            mgr: mgr.inner().clone(),
        })
        .map(|p| p.name);
        out.push(ProxyStatus {
            tool_id: tid.to_string(),
            running,
            port: handles
                .get(*tid)
                .map(|rp| rp.port)
                .unwrap_or_else(|| port_for_tool(&state, tid)),
            active_provider: active,
            error: None,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn proxy_health(
    state: tauri::State<AppState>,
    mgr: tauri::State<std::sync::Arc<ProxyManager>>,
    tool_id: String,
) -> AppResult<Vec<ProxyHealthItem>> {
    drop(state);
    Ok(mgr.health.lock().map_err(|_| "proxy lock poisoned")?.get(&tool_id).cloned().unwrap_or_default())
}

#[tauri::command]
pub async fn proxy_start(
    state: tauri::State<'_, AppState>,
    mgr: tauri::State<'_, std::sync::Arc<ProxyManager>>,
    tool_id: String,
) -> AppResult<()> {
    if !proxy_takeover_enabled(&state, &tool_id) {
        return Err("proxy takeover is disabled for this tool — enable it in Settings".into());
    }
    let _port = port_for_tool(&state, &tool_id);
    {
        let handles = mgr.handles.lock().map_err(|_| "proxy lock poisoned")?;
        if handles.contains_key(&tool_id) {
            return Ok(());
        }
    }
    let data_dir = state.active_data_dir();
    let ctx = ProxyCtx { data_dir: data_dir.clone(), tool_id: tool_id.clone(), mgr: mgr.inner().clone() };
    let router = axum::Router::new().fallback(relay).with_state(ctx);

    // start the health monitor once (per manager lifetime)
    {
        let mut started = mgr.monitor_started.lock().map_err(|_| "proxy lock poisoned")?;
        if !*started {
            *started = true;
            let mon = mgr.inner().clone();
            let dir = data_dir.clone();
            tokio::spawn(health_monitor_loop(mon, dir));
        }
    }

    // bind on the configured port, falling back to the next free port if busy
    let desired = port_for_tool(&state, &tool_id);
    let mut bound: Option<(tokio::net::TcpListener, u16)> = None;
    for p in desired..desired.saturating_add(200) {
        match tokio::net::TcpListener::bind(("127.0.0.1", p)).await {
            Ok(l) => {
                bound = Some((l, p));
                break;
            }
            Err(_) => continue,
        }
    }
    let Some((listener, port)) = bound else {
        return Err(format!("cannot bind any port in {desired}..{}", desired + 200).into());
    };
    let server = axum::serve(listener, router);
    let task = tokio::spawn(async move {
        let _ = server.await;
    });
    let mut handles = mgr.handles.lock().map_err(|_| "proxy lock poisoned")?;
    handles.insert(tool_id, RunningProxy { task, port });
    Ok(())
}

async fn health_monitor_loop(mgr: std::sync::Arc<ProxyManager>, data_dir: PathBuf) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let tools: Vec<(String, u16)> = {
            let handles = match mgr.handles.lock() {
                Ok(h) => h,
                Err(_) => continue,
            };
            handles.iter().map(|(t, rp)| (t.clone(), rp.port)).collect()
        };
        for (tool, _port) in tools {
            let ctx = ProxyCtx { data_dir: data_dir.clone(), tool_id: tool.clone(), mgr: mgr.clone() };
            let mut items = Vec::new();
            for p in provider_candidates(&ctx) {
                let started = std::time::Instant::now();
                let client = reqwest::Client::new();
                let (ok, latency) = match client
                    .get(p.base_url.trim_end_matches('/'))
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await
                {
                    Ok(r) => {
                        let code = r.status().as_u16();
                        (code < 500, started.elapsed().as_millis() as u64)
                    }
                    Err(_) => (false, started.elapsed().as_millis() as u64),
                };
                items.push(ProxyHealthItem {
                    provider_id: p.id,
                    name: p.name.clone(),
                    ok,
                    latency_ms: latency,
                    error: None,
                });
            }
            if let Ok(mut health) = mgr.health.lock() {
                health.insert(tool, items);
            }
        }
    }
}

#[tauri::command]
pub fn proxy_stop(
    mgr: tauri::State<std::sync::Arc<ProxyManager>>,
    tool_id: String,
) -> AppResult<()> {
    let mut handles = mgr.handles.lock().map_err(|_| "proxy lock poisoned")?;
    if let Some(rp) = handles.remove(&tool_id) {
        rp.task.abort();
    }
    Ok(())
}