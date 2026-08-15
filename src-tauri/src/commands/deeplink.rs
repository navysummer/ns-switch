use crate::db::AppState;
use crate::models::DeepLinkResult;
use crate::AppResult;
use serde_json::json;
use std::collections::HashMap;

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() + 1 && i + 2 <= bytes.len() - 0 => {
                if i + 2 < bytes.len() {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                    if let Some(hex) = hex {
                        if let Ok(v) = u8::from_str_radix(hex, 16) {
                            out.push(v);
                            i += 3;
                            continue;
                        }
                    }
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn parse_url(raw: &str) -> Option<(String, HashMap<String, String>)> {
    let s = raw.trim();
    let rest = s
        .strip_prefix("ns-switch://")
        .or_else(|| s.strip_prefix("ccswitch://"))?;
    let (head, query) = match rest.split_once('?') {
        Some((h, q)) => (h, q),
        None => (rest, ""),
    };
    let action = head.trim_end_matches('/').to_string();
    let mut params = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        if let Some((k, v)) = pair.split_once('=') {
            params.insert(percent_decode(k), percent_decode(v));
        }
    }
    Some((action, params))
}

fn insert_provider(conn: &rusqlite::Connection, p: HashMap<String, String>) -> AppResult<bool> {
    let tool = p.get("tool").cloned().unwrap_or_else(|| "claude_code".to_string());
    let name = p.get("name").cloned().unwrap_or_else(|| "Imported".to_string());
    let base = p.get("baseUrl").or_else(|| p.get("base_url")).cloned().unwrap_or_default();
    let universal = p.get("universal").map(|x| x == "1" || x == "true").unwrap_or(false);
    let shared = p.get("sharedConfig").or_else(|| p.get("shared")).cloned().unwrap_or_default();
    let headers = p.get("headers").cloned().unwrap_or_else(|| "{}".to_string());
    let model = p.get("model").cloned().unwrap_or_default();
    let key = p.get("apiKey").cloned().unwrap_or_default();
    let proto = p.get("protocol").cloned().unwrap_or_default();
    conn.execute(
        "INSERT INTO providers (tool_id, name, base_url, api_key, model, headers, is_preset, is_active, is_universal, shared_config, protocol)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7, ?8, ?9)",
        rusqlite::params![
            if universal { "universal" } else { &tool },
            name,
            base,
            key,
            model,
            headers,
            universal as i64,
            shared,
            proto
        ],
    )?;
    Ok(true)
}

fn insert_mcp(conn: &rusqlite::Connection, p: HashMap<String, String>) -> AppResult<bool> {
    let name = p.get("name").cloned().unwrap_or_else(|| "MCP".to_string());
    let command = p.get("command").cloned().unwrap_or_else(|| "npx".to_string());
    let args: Vec<String> = p
        .get("args")
        .map(|a| {
            if a.trim_start().starts_with('[') {
                serde_json::from_str::<Vec<String>>(a).unwrap_or_else(|_| {
                    a.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
                })
            } else {
                a.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
            }
        })
        .unwrap_or_default();
    let tools: Vec<String> = p
        .get("tools")
        .map(|x| x.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
        .unwrap_or_else(|| vec!["claude_code".to_string()]);
    conn.execute(
        "INSERT INTO mcp_servers (name, command, args, env, cwd, transport, is_active, tools)
         VALUES (?1, ?2, ?3, '{}', '', 'stdio', 1, ?4)",
        rusqlite::params![name, command, serde_json::to_string(&args).unwrap_or("[]".into()), serde_json::to_string(&tools).unwrap_or("[]".into())],
    )?;
    Ok(true)
}

fn insert_prompt(conn: &rusqlite::Connection, p: HashMap<String, String>) -> AppResult<bool> {
    let name = p.get("name").cloned().unwrap_or_else(|| "Prompt".to_string());
    let content = p.get("content").cloned().unwrap_or_default();
    let ft = p.get("fileType").cloned().unwrap_or_else(|| "CLAUDE_MD".to_string());
    conn.execute(
        "INSERT INTO prompts (name, content, file_type) VALUES (?1, ?2, ?3)",
        rusqlite::params![name, content, ft],
    )?;
    Ok(true)
}

/// accept one or multiple deep links (one per line) and perform the imports
#[tauri::command]
pub fn import_deep_links(
    state: tauri::State<AppState>,
    text: String,
) -> AppResult<DeepLinkResult> {
    import_deep_links_impl(&state, text)
}

pub fn import_deep_links_impl(state: &AppState, text: String) -> AppResult<DeepLinkResult> {
    let mut count = 0u32;
    let mut messages: Vec<String> = Vec::new();
    let mut skill_links: Vec<HashMap<String, String>> = Vec::new();
    {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parsed = match parse_url(line) {
                Some(p) => p,
                None => {
                    messages.push(format!("invalid link: {line}"));
                    continue;
                }
            };
            let (action, params) = parsed;
            let result: Result<bool, String> = (|| {
                match action.as_str() {
                    "provider" | "import" => {
                        insert_provider(&conn, params).map_err(|e| e.to_string())
                    }
                    "preset" => {
                        let tool = params.get("tool").cloned().unwrap_or_else(|| "claude_code".into());
                        let name = params.get("name").cloned().unwrap_or_default();
                        let found = crate::presets::presets_for(&tool)
                            .into_iter()
                            .find(|p| p.name == name)
                            .ok_or_else(|| "preset not found".to_string())?;
                        let mut m = HashMap::new();
                        m.insert("tool".into(), tool);
                        m.insert("name".into(), found.name.clone());
                        m.insert("baseUrl".into(), found.base_url.clone());
                        m.insert("model".into(), found.model.clone());
                        insert_provider(&conn, m).map_err(|e| e.to_string())
                    }
                    "mcp" => insert_mcp(&conn, params).map_err(|e| e.to_string()),
                    "prompt" => insert_prompt(&conn, params).map_err(|e| e.to_string()),
                    "skill" => {
                        skill_links.push(params);
                        Ok(true)
                    }
                    _ => Err(format!("unknown action: {action}")),
                }
            })();
            match result {
                Ok(true) => count += 1,
                Err(e) => messages.push(format!("{action}: {e}")),
                _ => {}
            }
        }
    }
    // skills need the full install pipeline (clone/extract + symlink), run outside the lock
    for p in skill_links {
        let source = p.get("source").cloned().unwrap_or_default();
        let mode = p.get("mode").cloned().unwrap_or_else(|| "symlink".to_string());
        match crate::commands::skills::skill_install_impl(state, source, mode) {
            Ok(_) => count += 1,
            Err(e) => messages.push(format!("skill: {e}")),
        }
    }
    Ok(DeepLinkResult {
        action: "import".into(),
        count,
        message: messages.join("\n"),
    })
}

/// register the `ns-switch://` URL scheme at the OS level so links can launch the app
#[tauri::command]
pub fn register_scheme() -> AppResult<String> {
    const SCHEME: &str = "ns-switch";
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string());
    #[cfg(target_os = "windows")]
    {
        let exe = exe.unwrap_or_default();
        run_cmd(&format!("reg add HKCU\\Software\\Classes\\{SCHEME} /ve /d \"URL:NS Switch\" /f"))?;
        run_cmd(&format!("reg add HKCU\\Software\\Classes\\{SCHEME} /v \"URL Protocol\" /t REG_SZ /d \"\" /f"))?;
        run_cmd(&format!("reg add \"HKCU\\Software\\Classes\\{SCHEME}\\shell\\open\\command\" /ve /d \"\\\"{exe}\\\" \\\"%1\\\"\" /f"))?;
        run_cmd(&format!("reg add \"HKCU\\Software\\Classes\\{SCHEME}\\DefaultIcon\" /ve /d \"\\\"{exe}\\\"\" /f"))?;
        return Ok(format!("registered {SCHEME}:// in HKCU; use ns-switch:// links to open the app"));
    }
    #[cfg(target_os = "linux")]
    {
        let exe = exe.unwrap_or_default();
        let app_dir = dirs::config_dir().map(|c| c.join("applications")).unwrap_or_default();
        std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
        let desktop = app_dir.join("ns-switch-handler.desktop");
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=NS Switch\nExec=\"{exe}\" %u\nNoDisplay=true\nMimeType=x-scheme-handler/{SCHEME};\n"
        );
        std::fs::write(&desktop, content).map_err(|e| e.to_string())?;
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&app_dir)
            .output();
        let _ = std::process::Command::new("xdg-mime")
            .args(["default", "ns-switch-handler.desktop", &format!("x-scheme-handler/{SCHEME}")])
            .output();
        return Ok(format!("registered {SCHEME}:// via {desktop}"));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = exe;
        Ok(format!(
            "On macOS the {SCHEME}:// scheme is registered by the installed app bundle (Info.plist CFBundleURLTypes). \
             Links work after packaging with `pnpm tauri:build`. In development, paste ns-switch:// links into Settings > Deep Link, or run: open -a \"NS Switch\" \"ns-switch://...\""
        ))
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Ok(format!("scheme registration is not supported on this platform; use the Settings > Deep Link paste import instead"))
    }
}

#[cfg(target_os = "windows")]
fn run_cmd(cmd: &str) -> AppResult<()> {
    let out = std::process::Command::new("cmd")
        .args(["/C", cmd])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("command failed: {cmd}: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

mod tests {
    #[test]
    fn parse() {
        let (a, p) = super::parse_url("ns-switch://provider?tool=claude_code&name=Hi%20X&baseUrl=https://a.com&apiKey=sk-1&universal=1").unwrap();
        assert_eq!(a, "provider");
        assert_eq!(p.get("name").unwrap(), "Hi X");
        assert_eq!(p.get("baseUrl").unwrap(), "https://a.com");
        assert_eq!(p.get("universal").unwrap(), "1");
    }
}

#[allow(unused)]
fn _json_helper(v: &str) -> serde_json::Value {
    json!(v)
}