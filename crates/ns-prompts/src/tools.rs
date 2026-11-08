//! 每工具提示文件写回适配器。
//!
//! 把统一的 [`PromptEntry`] 写回该工具的用户级提示文件：
//! - Claude Code：`$CLAUDE_CONFIG_DIR/CLAUDE.md`（默认 `~/.claude/CLAUDE.md`）。
//! - Codex：`$CODEX_HOME/AGENTS.md`（默认 `~/.codex/AGENTS.md`）。
//! - Gemini CLI：`$GEMINI_CLI_HOME/.gemini/GEMINI.md`。
//!
//! 写回策略：「先回存」。写前读目标文件既有内容，作为一条无名字条目存回
//! 库（不覆盖用户在文件里手写的段落），再整文件写库内合并内容。这样切换
//! 工具提示文件时，手写内容不丢失。
//!
//! 合并规则：库内某工具的全部启用条目按名序拼成一份 Markdown；无名字
//! 条目（回存的既有内容）排最前，其余条目以 `## <name>` 分节。

use ns_presets::Tool;

use crate::model::{PromptEntry, PromptPaths};
use crate::store::{PromptStore, read_file_or_empty, write_file};

/// 某工具的提示文件路径（用户作用域）。
pub fn prompt_file(tool: Tool, paths: &PromptPaths) -> std::path::PathBuf {
    match tool {
        Tool::Claude | Tool::ClaudeDesktop => paths.claude_user(),
        Tool::Codex => paths.codex_user(),
        Tool::Gemini => paths.gemini_user(),
        _ => paths.claude_user(),
    }
}

/// 某工具是否支持提示文件写回。
pub fn supported(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Claude | Tool::ClaudeDesktop | Tool::Codex | Tool::Gemini
    )
}

/// 写回：先回存既有内容，再合并库内条目写文件。
///
/// `store`：提示词库（写回前读既有内容存入 `未命名` 条目并持久化）。
/// 返回写回的文件路径。
pub fn write_back(
    tool: Tool,
    store: &mut PromptStore,
    paths: &PromptPaths,
) -> Result<std::path::PathBuf, String> {
    let file = prompt_file(tool, paths);
    let existing = read_file_or_empty(&file)?;
    if !existing.trim().is_empty() {
        // 先回存：既有内容作为未命名条目（仅当库中还没有未命名条目时）。
        let has_anon = store
            .entries_for(tool)
            .iter()
            .any(|e| e.name.trim().is_empty());
        if !has_anon {
            let anon = PromptEntry {
                name: String::new(),
                body: existing.clone(),
                enabled: true,
                updated_at: now_ms(),
            };
            store.save_entry(tool, anon)?;
        }
    }

    let merged = render_tool(tool, store, paths)?;
    write_file(&file, &merged)?;
    Ok(file)
}


pub fn render_tool(
    tool: Tool,
    store: &PromptStore,
    _paths: &PromptPaths,
) -> Result<String, String> {
    let mut entries = store.entries_for(tool);
    entries.retain(|e| e.enabled);
    // 未命名条目（回存的既有内容）排最前，其余按名排序。
    entries.sort_by_key(|e| (!e.name.trim().is_empty(), e.name.clone()));
    let mut out = String::new();
    for (i, e) in entries.iter().enumerate() {
        let body = e.body.trim_end();
        if body.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        if e.name.trim().is_empty() {
            out.push_str(body);
        } else {
            out.push_str(&format!("## {}\n\n{}", e.name.trim(), body));
        }
        if i + 1 < entries.len() {
            out.push('\n');
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PromptPaths;
    use crate::store::PromptStore;

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static DIR_SEQ: AtomicU32 = AtomicU32::new(0);

    fn temp_paths() -> PromptPaths {
        let base = std::env::temp_dir().join(format!(
            "ns-prompts-tools-test-{}-{}",
            std::process::id(),
            DIR_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base);
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", base.join("claude"));
            std::env::set_var("CODEX_HOME", base.join("codex"));
            std::env::set_var("GEMINI_CLI_HOME", base.clone());
        }
        PromptPaths { home: base }
    }

    #[test]
    fn write_back_preserves_existing() {
        let _guard = TEST_LOCK.lock().unwrap();
        let paths = temp_paths();
        let mut store = PromptStore::open_at(paths.home.clone()).unwrap();
        let claude_file = paths.claude_user();
        std::fs::create_dir_all(claude_file.parent().unwrap()).unwrap();
        // 既有手写内容。
        std::fs::write(&claude_file, "# 手写段落\n\n保留我\n").unwrap();

        // 库中有两条命名条目。
        store
            .save_entry(
                Tool::Claude,
                PromptEntry {
                    name: "代码规范".into(),
                    body: "- 用 4 空格\n".into(),
                    enabled: true,
                    updated_at: 0,
                },
            )
            .unwrap();
        store
            .save_entry(
                Tool::Claude,
                PromptEntry {
                    name: "提交规范".into(),
                    body: "- conventional\n".into(),
                    enabled: true,
                    updated_at: 0,
                },
            )
            .unwrap();

        let file = write_back(Tool::Claude, &mut store, &paths).unwrap();
        assert_eq!(file, claude_file);
        let text = std::fs::read_to_string(&claude_file).unwrap();
        // 既有内容被回存为未命名条目，渲染在最前。
        assert!(text.contains("保留我"));
        // 命名条目按名序分节。
        let code_pos = text.find("## 代码规范").unwrap();
        let commit_pos = text.find("## 提交规范").unwrap();
        assert!(code_pos < commit_pos);
        // 未命名条目在命名条目之前。
        assert!(text.find("保留我").unwrap() < commit_pos);
        let mut store2 = PromptStore::open_at(paths.home.clone()).unwrap();
        write_back(Tool::Claude, &mut store2, &paths).unwrap();
        let text2 = std::fs::read_to_string(&claude_file).unwrap();
        assert_eq!(text, text2);
        let _ = std::fs::remove_dir_all(&paths.home);
    }

    #[test]
    fn render_respects_enabled() {
        let _guard = TEST_LOCK.lock().unwrap();
        let paths = temp_paths();
        let mut store = PromptStore::open_at(paths.home.clone()).unwrap();
        store
            .save_entry(
                Tool::Codex,
                PromptEntry {
                    name: "A".into(),
                    body: "aaa".into(),
                    enabled: true,
                    updated_at: 0,
                },
            )
            .unwrap();
        store
            .save_entry(
                Tool::Codex,
                PromptEntry {
                    name: "B".into(),
                    body: "bbb".into(),
                    enabled: false,
                    updated_at: 0,
                },
            )
            .unwrap();
        let rendered = render_tool(Tool::Codex, &store, &paths).unwrap();
        assert!(rendered.contains("aaa"));
        assert!(!rendered.contains("bbb"));
        let _ = std::fs::remove_dir_all(&paths.home);
    }
}
