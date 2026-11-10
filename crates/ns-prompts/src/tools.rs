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
        Tool::Pi => paths.omp_user(),
        _ => paths.claude_user(),
    }
}

/// 某工具是否支持提示文件写回。
pub fn supported(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Claude | Tool::ClaudeDesktop | Tool::Codex | Tool::Gemini | Tool::Pi
    )
}

/// 写回：先回存既有/手改内容，再合并库内条目写文件。
///
/// 写回前读取目标文件：若与当前库渲染结果不同，说明存在手工编辑或外部变更，
/// 将「去掉库内已知命名节后的剩余内容」更新到未命名条目中，避免两次写回间手改丢失。
pub fn write_back(
    tool: Tool,
    store: &mut PromptStore,
    paths: &PromptPaths,
) -> Result<std::path::PathBuf, String> {
    let file = prompt_file(tool, paths);
    let existing = read_file_or_empty(&file)?;
    if !existing.trim().is_empty() {
        let expected = render_tool(tool, store, paths)?;
        if normalize_prompt(&existing) != normalize_prompt(&expected) {
            // 命名节手改:先回存到库(保留手改正文),再提取未命名残余。
            merge_edited_sections(tool, store, &existing);
            // 文件与库渲染不一致：提取“未命名/手改”残余并回存。
            let anon_body = extract_unmanaged_body(tool, store, &existing);
            if !anon_body.trim().is_empty() || has_anon_entry(tool, store) {
                let anon = PromptEntry {
                    name: String::new(),
                    body: anon_body,
                    enabled: true,
                    updated_at: now_ms(),
                };
                store.save_entry(tool, anon)?;
            }
        }
    }

    let merged = render_tool(tool, store, paths)?;
    write_file(&file, &merged)?;
    Ok(file)
}

fn has_anon_entry(tool: Tool, store: &PromptStore) -> bool {
    store.entries_for(tool).iter().any(|e| e.name.trim().is_empty())
}

fn normalize_prompt(s: &str) -> String {
    s.replace("\r\n", "\n").trim().to_string()
}


/// 命名节内手工修改的精细合并：解析文件中的 `## <name>` 节，
/// 若节名匹配库内启用的命名条目且正文与库不同，则将手改正文回存到库（保留手改），
/// 避免两次写回间用户对命名节的编辑被当作未命名残余或丢失。
fn merge_edited_sections(tool: Tool, store: &mut PromptStore, existing: &str) {
    let text = existing.replace("\r\n", "\n");
    let mut entries = store.entries_for(tool);
    entries.retain(|e| e.enabled && !e.name.trim().is_empty());
    // 节名 -> 库内条目(用于比较正文是否被手改)
    for e in entries {
        let marker = format!("## {}\n", e.name.trim());
        if let Some(start) = text.find(&marker) {
            let body_start = start + marker.len();
            // 节正文:从 body_start 到下一个 `## ` 节头(或文件尾)。
            let rest = &text[body_start..];
            let body_end = rest
                .find("\n## ")
                .map(|p| body_start + p)
                .unwrap_or(text.len());
            let body = text[body_start..body_end].trim_end().to_string();
            let trimmed = body.trim();
            let store_body = e.body.trim();
            if !trimmed.is_empty() && trimmed != store_body {
                // 手改过:回存到库,保留手改正文。
                let mut updated = e.clone();
                updated.body = trimmed.to_string();
                updated.updated_at = now_ms();
                let _ = store.save_entry(tool, updated);
            }
        }
    }
}
/// 从文件内容中移除库内启用的命名条目对应块，返回剩余（未命名/手改）内容。
fn extract_unmanaged_body(tool: Tool, store: &PromptStore, existing: &str) -> String {
    let mut text = existing.replace("\r\n", "\n");
    let mut entries = store.entries_for(tool);
    entries.retain(|e| e.enabled && !e.name.trim().is_empty());
    for e in entries {
        let block = format!("## {}\n\n{}", e.name.trim(), e.body.trim_end());
        text = text.replace(&block, "\n");
        // 兼容可能的格式差异（标题后单换行等）。
        let alt = format!("## {}\n{}", e.name.trim(), e.body.trim_end());
        text = text.replace(&alt, "\n");
    }
    // 清理多余空行
    let mut out = String::new();
    let mut blank = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            if !blank && !out.is_empty() {
                out.push('\n');
                blank = true;
            }
        } else {
            out.push_str(line);
            out.push('\n');
            blank = false;
        }
    }
    out.trim().to_string()
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

    #[test]
    fn write_back_captures_manual_edits_between_writes() {
        let _guard = TEST_LOCK.lock().unwrap();
        let paths = temp_paths();
        let mut store = PromptStore::open_at(paths.home.clone()).unwrap();
        store
            .save_entry(
                Tool::Claude,
                PromptEntry {
                    name: "规范".into(),
                    body: "库内容".into(),
                    enabled: true,
                    updated_at: 0,
                },
            )
            .unwrap();
        let file = write_back(Tool::Claude, &mut store, &paths).unwrap();
        // 用户在两次写回之间手工追加内容。
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.push_str("\n# 手工追加\n请保留\n");
        std::fs::write(&file, text).unwrap();

        let mut store2 = PromptStore::open_at(paths.home.clone()).unwrap();
        write_back(Tool::Claude, &mut store2, &paths).unwrap();
        let text2 = std::fs::read_to_string(&file).unwrap();
        assert!(text2.contains("手工追加"), "手工编辑应被回存保留: {text2}");
        assert!(text2.contains("## 规范"), "库条目应仍存在: {text2}");
        assert!(text2.contains("库内容"), "库条目内容应仍存在: {text2}");
        let _ = std::fs::remove_dir_all(&paths.home);
    }

    #[test]
    fn write_back_merges_edited_named_section_into_library() {
        let _guard = TEST_LOCK.lock().unwrap();
        let paths = temp_paths();
        let mut store = PromptStore::open_at(paths.home.clone()).unwrap();
        store
            .save_entry(
                Tool::Claude,
                PromptEntry {
                    name: "规范".into(),
                    body: "库原始内容".into(),
                    enabled: true,
                    updated_at: 0,
                },
            )
            .unwrap();
        let file = write_back(Tool::Claude, &mut store, &paths).unwrap();
        // 用户手改命名节正文。
        let text = std::fs::read_to_string(&file)
            .unwrap()
            .replace("库原始内容", "用户手改后的内容");
        std::fs::write(&file, text).unwrap();

        let mut store2 = PromptStore::open_at(paths.home.clone()).unwrap();
        write_back(Tool::Claude, &mut store2, &paths).unwrap();
        // 库应保留手改正文。
        let lib_body = store2
            .entries_for(Tool::Claude)
            .into_iter()
            .find(|e| e.name == "规范")
            .map(|e| e.body)
            .unwrap_or_default();
        assert!(lib_body.contains("用户手改后的内容"), "库应回存手改正文: {lib_body}");
        // 文件应包含手改正文且不被当作未命名残余重复。
        let text2 = std::fs::read_to_string(&file).unwrap();
        assert!(text2.contains("用户手改后的内容"), "文件应含手改正文: {text2}");
        assert!(!text2.contains("库原始内容"), "旧正文不应残留: {text2}");
        let _ = std::fs::remove_dir_all(&paths.home);
    }

    #[test]
    fn pi_prompt_file_points_to_append_system() {
        let _guard = TEST_LOCK.lock().unwrap();
        let paths = temp_paths();
        // Pi 用户级提示文件 = `~/.omp/agent/APPEND_SYSTEM.md`（追加语义）。
        let f = prompt_file(Tool::Pi, &paths);
        assert_eq!(f, paths.home.join(".omp").join("agent").join("APPEND_SYSTEM.md"));
        assert!(supported(Tool::Pi), "Pi 应支持提示写回");
        assert!(!supported(Tool::Omp), "OMP 暂不支持（提示文件约定不同）");
        let _ = std::fs::remove_dir_all(&paths.home);
    }
}
