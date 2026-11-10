//! MCP 管理器：跨工具读取/写入统一 [`McpServer`] 列表。
//!
//! 面向 UI 的一次性入口：读某工具全部 MCP 服务器、写回（新增/更新/删除/
//! 启停）、跨工具同步。每个写操作只动自己管的 `mcpServers`/`mcp_servers`
//! 键，其余配置与注释原样保留。

use std::path::{Path, PathBuf};

use ns_presets::Tool;
use serde_json::Value;

use crate::model::McpServer;
use crate::tools::{claude, codex, gemini, grok, hermes, kimi, minimax, omp, opencode, openclaw, pi, McpPaths};

/// 一次读某工具 MCP 服务器（用户作用域）。未知工具返回空。
pub fn read_tool(tool: Tool, paths: &McpPaths) -> Result<Vec<McpServer>, String> {
    match tool {
        Tool::Claude | Tool::ClaudeDesktop => claude::load(&paths.claude_user_json(), None),
        Tool::Codex => codex::load(&paths.codex_config()),
        Tool::Gemini => {
            let (servers, _en) = gemini::load(&paths.gemini_settings())?;
            Ok(servers)
        }
        Tool::Opencode => opencode::load(&paths.opencode_config()),
        Tool::GrokBuild => grok::load(&paths.grok_config()),
        Tool::Kimi => kimi::load(&paths.kimi_config()),
        Tool::Openclaw => openclaw::load(&paths.openclaw_config()),
        Tool::Hermes => hermes::load(&paths.hermes_config()),
        Tool::Pi => pi::load(&paths.pi_mcp()),
        Tool::Mcode => minimax::load(&paths.minimax_mcp()),
        Tool::Omp => omp::load(&paths.omp_mcp()),
        _ => Ok(Vec::new()),
    }
}

/// 对某工具应用一次写：删除 `remove`、写入/更新 `upserts`。
///
/// 返回是否发生任何变更（用于避免无谓写盘）。写前会先读现有内容，
/// 保持「最小侵入」。
pub fn write_tool(
    tool: Tool,
    paths: &McpPaths,
    remove: &[String],
    upserts: &[McpServer],
) -> Result<bool, String> {
    match tool {
        Tool::Claude | Tool::ClaudeDesktop => {
            let path = paths.claude_user_json();
            let mut doc = read_json_or_empty(&path)?;
            let changed = claude::apply_servers(&mut doc, None, remove, upserts);
            if changed {
                write_json(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::Codex => {
            let path = paths.codex_config();
            let mut doc = read_toml_or_empty(&path)?;
            let changed = codex::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_toml(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::Gemini => {
            let path = paths.gemini_settings();
            let mut doc = read_json_or_empty(&path)?;
            let en_path = gemini::enablement_path(&path);
            let mut en = gemini::load_enablement(&en_path);
            let changed = gemini::apply_servers(&mut doc, &mut en, remove, upserts);
            if changed {
                write_json(&path, &doc)?;
                gemini::write_enablement(&en_path, &en)?;
            }
            Ok(changed)
        }
        Tool::Opencode => {
            let path = paths.opencode_config();
            let mut doc = read_json_or_empty(&path)?;
            let changed = opencode::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_json(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::GrokBuild => {
            let path = paths.grok_config();
            let mut doc = read_toml_or_empty(&path)?;
            let changed = grok::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_toml(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::Kimi => {
            let path = paths.kimi_config();
            let mut doc = read_toml_or_empty(&path)?;
            let changed = kimi::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_toml(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::Openclaw => {
            let path = paths.openclaw_config();
            let mut doc = read_json_or_empty(&path)?;
            let changed = openclaw::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_json(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::Hermes => {
            let path = paths.hermes_config();
            let mut doc = read_yaml_or_empty(&path)?;
            let changed = hermes::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_yaml(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::Pi => {
            let path = paths.pi_mcp();
            let mut doc = read_json_or_empty(&path)?;
            let changed = pi::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_json(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::Mcode => {
            let path = paths.minimax_mcp();
            let mut doc = read_json_or_empty(&path)?;
            let changed = minimax::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_json(&path, &doc)?;
            }
            Ok(changed)
        }
        Tool::Omp => {
            let path = paths.omp_mcp();
            let mut doc = read_json_or_empty(&path)?;
            let changed = omp::apply_servers(&mut doc, remove, upserts);
            if changed {
                write_json(&path, &doc)?;
            }
            Ok(changed)
        }
        _ => Ok(false),
    }
}

/// 读取 JSON 文件（不存在视为空对象）。
pub fn read_json_or_empty(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => claude::validate_doc(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(e) => Err(format!("读取 {} 失败: {e}", path.display())),
    }
}

/// 写 JSON 文件（2 空格缩进 + 尾换行；与各工具默认风格一致）。
pub fn write_json(path: &Path, doc: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut text = serde_json::to_string_pretty(doc).map_err(|e| e.to_string())?;
    text.push('\n');
    std::fs::write(path, text).map_err(|e| format!("写 {} 失败: {e}", path.display()))
}

/// 读取 TOML 文件（不存在视为空文档）。
pub fn read_toml_or_empty(path: &Path) -> Result<toml_edit::DocumentMut, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.parse().map_err(|e| format!("解析 {} 失败: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(toml_edit::DocumentMut::new())
        }
        Err(e) => Err(format!("读取 {} 失败: {e}", path.display())),
    }
}

/// 写 TOML 文件（保留格式）。
pub fn write_toml(path: &Path, doc: &toml_edit::DocumentMut) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, doc.to_string())
        .map_err(|e| format!("写 {} 失败: {e}", path.display()))
}

/// 读取 YAML 文件（不存在视为空映射）。
pub fn read_yaml_or_empty(path: &Path) -> Result<serde_yaml::Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => hermes::validate_doc(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
        }
        Err(e) => Err(format!("读取 {} 失败: {e}", path.display())),
    }
}

/// 写 YAML 文件。
pub fn write_yaml(path: &Path, doc: &serde_yaml::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_yaml::to_string(doc).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("写 {} 失败: {e}", path.display()))
}

/// 把某工具的服务器同步到另一组工具（跨工具同步）。
///
/// `targets`：目标工具集（不含 `source`）。写入前读每个目标现有列表，
/// 合并 `server`（同名覆盖、其余保留），再写回。
pub fn sync_server(
    server: &McpServer,
    source: Tool,
    targets: &[Tool],
    paths: &McpPaths,
) -> Result<Vec<String>, String> {
    let mut applied = Vec::new();
    for &tool in targets {
        if tool == source {
            continue;
        }
        let existing = read_tool(tool, paths)?;
        let mut upserts = Vec::new();
        let mut replaced = false;
        for e in existing {
            if e.name == server.name {
                upserts.push(server.clone());
                replaced = true;
            } else {
                upserts.push(e);
            }
        }
        if !replaced {
            upserts.push(server.clone());
        }
        write_tool(tool, paths, &[], &upserts)?;
        applied.push(tool.id_str().to_string());
    }
    Ok(applied)
}

/// 把某工具的全部服务器复制到另一组工具（一次性导入）。
pub fn import_tool(
    source: Tool,
    targets: &[Tool],
    paths: &McpPaths,
) -> Result<Vec<String>, String> {
    let servers = read_tool(source, paths)?;
    let mut applied = Vec::new();
    for &tool in targets {
        if tool == source {
            continue;
        }
        // 合并：目标已有同名则被源覆盖，其余保留。
        let existing = read_tool(tool, paths)?;
        let mut merged: Vec<McpServer> = existing
            .iter()
            .filter(|e| !servers.iter().any(|s| s.name == e.name))
            .cloned()
            .collect();
        merged.extend(servers.iter().cloned());
        write_tool(tool, paths, &[], &merged)?;
        applied.push(tool.id_str().to_string());
    }
    Ok(applied)
}

/// 项目级 Claude `.mcp.json` 的绝对路径提示（供 UI 展示作用域）。
pub fn claude_project_scope(project: &Path) -> PathBuf {
    project.join(".mcp.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{McpServer, McpTransport};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static DIR_SEQ: AtomicU32 = AtomicU32::new(0);
    /// 每个测试独立的临时 HOME（进程级唯一），避免跨测试污染。
    fn temp_paths() -> (McpPaths, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "ns-mcp-test-{}-{}",
            std::process::id(),
            DIR_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("claude")).unwrap();
        std::fs::create_dir_all(base.join("codex")).unwrap();
        std::fs::create_dir_all(base.join("gemini")).unwrap();
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", base.join("claude"));
            std::env::set_var("CODEX_HOME", base.join("codex"));
            std::env::set_var("GEMINI_CLI_HOME", base.join("gemini"));
        }
        (
            McpPaths {
                home: base.clone(),
            },
            base,
        )
    }

    fn server(name: &str, cmd: &str) -> McpServer {
        McpServer {
            name: name.into(),
            transport: McpTransport::Stdio,
            command: Some(cmd.into()),
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn read_write_delete_across_tools() {
        let _guard = TEST_LOCK.lock().unwrap();
        let (paths, _base) = temp_paths();
        // 初始为空。
        for tool in [Tool::Claude, Tool::Codex, Tool::Gemini] {
            assert!(read_tool(tool, &paths).unwrap().is_empty());
        }
        // 写一个 stdio 服务器到每个工具。
        let s = server("github", "npx");
        for tool in [Tool::Claude, Tool::Codex, Tool::Gemini] {
            assert!(write_tool(tool, &paths, &[], &[s.clone()]).unwrap());
            let got = read_tool(tool, &paths).unwrap();
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].name, "github");
            assert_eq!(got[0].command.as_deref(), Some("npx"));
        }
        // 再次写同一服务器：无变化（幂等）。
        assert!(!write_tool(Tool::Claude, &paths, &[], &[s.clone()]).unwrap());
        // 删除。
        assert!(write_tool(Tool::Codex, &paths, &["github".into()], &[]).unwrap());
        assert!(read_tool(Tool::Codex, &paths).unwrap().is_empty());
    }

    #[test]
    fn sync_and_import() {
        let _guard = TEST_LOCK.lock().unwrap();
        let (paths, _base) = temp_paths();
        let s = server("docs", "uvx");
        // 从 Claude 同步到 Codex + Gemini。
        write_tool(Tool::Claude, &paths, &[], &[s.clone()]).unwrap();
        let applied = sync_server(&s, Tool::Claude, &[Tool::Codex, Tool::Gemini], &paths).unwrap();
        assert_eq!(applied.len(), 2);
        assert_eq!(read_tool(Tool::Codex, &paths).unwrap().len(), 1);
        assert_eq!(read_tool(Tool::Gemini, &paths).unwrap().len(), 1);
        // 从 Codex 导入到 Claude（Codex 现有一个，导入不应清掉 Claude 已有的）。
        let s2 = server("codex-only", "npx");
        write_tool(Tool::Codex, &paths, &[], &[s2.clone()]).unwrap();
        import_tool(Tool::Codex, &[Tool::Claude], &paths).unwrap();
        let claude = read_tool(Tool::Claude, &paths).unwrap();
        assert!(claude.iter().any(|x| x.name == "docs"));
        assert!(claude.iter().any(|x| x.name == "codex-only"));
    }
}
