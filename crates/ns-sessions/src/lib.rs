//! ns-sessions — 各工具会话日志解析、阅读视图、恢复命令。
//!
//! Claude Code 会话在 `~/.claude/projects/<proj>/<session>.jsonl`；每行一个事件，
//! `message` 对象含 `role`/`content`/`model`/`usage`。本 crate 提供会话枚举、
//! 消息解析、分组、批量删除与阅读视图所需的分节（每轮 = user→assistant 交换）。

use std::collections::BTreeMap;
use std::path::Path;

/// 一条消息（已解析的显示用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Message {
    pub role: String,
    pub text: String,
    pub model: Option<String>,
    pub is_tool_use: bool,
    pub tool_name: Option<String>,
}

/// 一个会话的元数据。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionMeta {
    pub id: String,
    /// 所属项目目录名（如 `-Users-navysummer-Desktop-other-cc-switch`）。
    pub project: String,
    /// 会话文件路径。
    pub path: String,
    /// 最后修改时间（Unix 秒）。
    pub mtime: i64,
    pub message_count: usize,
    pub last_model: Option<String>,
    pub title: String,
}

/// 枚举 Claude Code 会话（按 mtime 新→旧）。
pub fn list_claude_sessions(home: &Path) -> Vec<SessionMeta> {
    let projects = home.join(".claude").join("projects");
    let Ok(entries) = std::fs::read_dir(&projects) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let project = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let Ok(sub) = std::fs::read_dir(&dir) else {
            continue;
        };
        for f in sub.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let id = p
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let mtime = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            // 快速扫消息数/模型（读全文一次）。
            let (count, last_model) = quick_stats(&p);
            let title = project.clone();
            out.push(SessionMeta {
                id,
                project: project.clone(),
                path: p.to_string_lossy().into_owned(),
                mtime,
                message_count: count,
                last_model,
                title,
            });
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.mtime));
    out
}

fn quick_stats(path: &Path) -> (usize, Option<String>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return (0, None);
    };
    let mut count = 0;
    let mut last_model = None;
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(msg) = v.get("message").and_then(|m| m.as_object()) {
            if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
                if role == "assistant" || role == "user" {
                    count += 1;
                    if role == "assistant" {
                        last_model = msg
                            .get("model")
                            .and_then(|m| m.as_str())
                            .map(str::to_string);
                    }
                }
            }
        }
    }
    (count, last_model)
}

/// 解析会话文件为消息序列（user/assistant，文本与工具调用）。
pub fn parse_session(path: &Path) -> Vec<Message> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(msg) = v.get("message").and_then(|m| m.as_object()) else {
            continue;
        };
        let Some(role) = msg.get("role").and_then(|r| r.as_str()) else {
            continue;
        };
        let model = msg
            .get("model")
            .and_then(|m| m.as_str())
            .map(str::to_string);
        let content_val = msg.get("content");
        match content_val {
            Some(serde_json::Value::Array(items)) => {
                for item in items {
                    if let Some(obj) = item.as_object() {
                        match obj.get("type").and_then(|t| t.as_str()) {
                            Some("text") => {
                                let text = obj
                                    .get("text")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("");
                                if !text.is_empty() {
                                    out.push(Message {
                                        role: role.to_string(),
                                        text: text.to_string(),
                                        model: model.clone(),
                                        is_tool_use: false,
                                        tool_name: None,
                                    });
                                }
                            }
                            Some("tool_use") => {
                                let name = obj
                                    .get("name")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("tool")
                                    .to_string();
                                let input = obj.get("input").map(|i| i.to_string()).unwrap_or_default();
                                out.push(Message {
                                    role: role.to_string(),
                                    text: format!("[{name}] {input}"),
                                    model: model.clone(),
                                    is_tool_use: true,
                                    tool_name: Some(name),
                                });
                            }
                            Some("tool_result") => {
                                let content_str = obj
                                    .get("content")
                                    .and_then(|c| {
                                        c.as_str()
                                            .map(str::to_string)
                                            .or_else(|| {
                                                c.as_array().map(|a| {
                                                    serde_json::Value::Array(a.clone()).to_string()
                                                })
                                            })
                                    })
                                    .unwrap_or_default();
                                let is_error = obj
                                    .get("is_error")
                                    .and_then(|e| e.as_bool())
                                    .unwrap_or(false);
                                out.push(Message {
                                    role: role.to_string(),
                                    text: format!(
                                        "{}结果: {content_str}",
                                        if is_error { "⚠ " } else { "" }
                                    ),
                                    model: model.clone(),
                                    is_tool_use: true,
                                    tool_name: Some("tool_result".into()),
                                });
                            }
                            Some("thinking") => {
                                let thinking = obj
                                    .get("thinking")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("");
                                out.push(Message {
                                    role: role.to_string(),
                                    text: format!("思考: {thinking}"),
                                    model: model.clone(),
                                    is_tool_use: false,
                                    tool_name: Some("thinking".into()),
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some(serde_json::Value::String(s)) => {
                if !s.is_empty() {
                    out.push(Message {
                        role: role.to_string(),
                        text: s.clone(),
                        model,
                        is_tool_use: false,
                        tool_name: None,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// 按项目分组会话（项目名 → 会话列表，新→旧）。
pub fn group_by_project(sessions: &[SessionMeta]) -> BTreeMap<String, Vec<SessionMeta>> {
    let mut map: BTreeMap<String, Vec<SessionMeta>> = BTreeMap::new();
    for s in sessions {
        map.entry(s.project.clone()).or_default().push(s.clone());
    }
    map
}

/// 按搜索词过滤会话（标题/项目/id 包含）。
pub fn filter_sessions(sessions: &[SessionMeta], query: &str) -> Vec<SessionMeta> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return sessions.to_vec();
    }
    sessions
        .iter()
        .filter(|s| {
            s.title.to_lowercase().contains(&q)
                || s.project.to_lowercase().contains(&q)
                || s.id.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

/// 删除会话文件（返回是否成功）。
pub fn delete_session(path: &Path) -> anyhow::Result<()> {
    std::fs::remove_file(path).map_err(|e| anyhow::anyhow!("删除会话失败: {e}"))
}

/// 一轮对话：1 条用户消息 + 其后的助手消息（直到下一条用户消息）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Round {
    pub index: usize,
    pub user_text: String,
    pub assistant: Vec<Message>,
    /// 步数（助手消息数）。
    pub steps: usize,
    /// 文件改动数（含 Read/Write/Edit 工具调用）。
    pub file_changes: usize,
    /// 是否有失败（is_error）。
    pub failed: bool,
}

/// 把消息序列按“用户消息为界”分组为多轮。
pub fn build_rounds(messages: &[Message]) -> Vec<Round> {
    let mut rounds: Vec<Round> = Vec::new();
    for m in messages {
        if m.role == "user" {
            rounds.push(Round {
                index: rounds.len(),
                user_text: m.text.clone(),
                assistant: Vec::new(),
                steps: 0,
                file_changes: 0,
                failed: false,
            });
        } else if let Some(r) = rounds.last_mut() {
            r.assistant.push(m.clone());
            r.steps = r.assistant.len();
            if m.is_tool_use
                && matches!(
                    m.tool_name.as_deref(),
                    Some("Read" | "Write" | "Edit" | "MultiEdit")
                )
            {
                r.file_changes += 1;
            }
            if m.text.contains("\"is_error\":true") {
                r.failed = true;
            }
        }
    }
    rounds
}

/// 导出整会话为 Markdown。
pub fn export_markdown(meta: &SessionMeta, rounds: &[Round]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 会话 {}\n\n", meta.id));
    out.push_str(&format!("- 项目: {}\n", meta.project));
    out.push_str(&format!("- 消息数: {}\n", meta.message_count));
    if let Some(m) = &meta.last_model {
        out.push_str(&format!("- 模型: {m}\n"));
    }
    out.push_str("\n---\n\n");
    for r in rounds {
        out.push_str(&format!("## 第 {} 轮\n\n", r.index + 1));
        out.push_str(&format!("**用户**\n\n{}\n\n", r.user_text));
        for m in &r.assistant {
            let tag = if m.is_tool_use {
                format!("工具 {}", m.tool_name.as_deref().unwrap_or(""))
            } else {
                "助手".to_string()
            };
            out.push_str(&format!("**{tag}**\n\n{}\n\n", m.text));
        }
    }
    out
}

/// 生成 macOS 一键在终端恢复会话的命令（Claude Code resume）。
pub fn recover_command(session_id: &str) -> String {
    format!(
        "osascript -e 'tell application \"Terminal\" to do script \"claude --resume {}\"'",
        session_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_messages_from_jsonl() {
        let dir = std::env::temp_dir().join("ns-sessions-test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("s.jsonl");
        std::fs::write(
            &f,
            r#"{"message":{"role":"user","content":[{"type":"text","text":"hello"}]}}
{"message":{"role":"assistant","model":"m1","content":[{"type":"text","text":"hi there"}]}}
{"message":{"role":"assistant","model":"m1","content":[{"type":"tool_use","name":"Read","input":{"path":"a.rs"}}]}}
"#,
        )
        .unwrap();
        let msgs = parse_session(&f);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].text, "hello");
        assert_eq!(msgs[1].text, "hi there");
        assert!(msgs[2].is_tool_use);
        assert_eq!(msgs[2].tool_name.as_deref(), Some("Read"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filters_by_query() {
        let sessions = vec![
            SessionMeta {
                id: "a".into(),
                project: "proj-1".into(),
                path: "/x/a.jsonl".into(),
                mtime: 1,
                message_count: 2,
                last_model: None,
                title: "proj-1".into(),
            },
            SessionMeta {
                id: "b".into(),
                project: "proj-2".into(),
                path: "/x/b.jsonl".into(),
                mtime: 2,
                message_count: 1,
                last_model: None,
                title: "proj-2".into(),
            },
        ];
        assert_eq!(filter_sessions(&sessions, "proj-1").len(), 1);
        assert_eq!(filter_sessions(&sessions, "").len(), 2);
    }

    #[test]
    fn groups_by_project() {
        let sessions = vec![
            SessionMeta {
                id: "a".into(),
                project: "p1".into(),
                path: "/x/a".into(),
                mtime: 1,
                message_count: 1,
                last_model: None,
                title: "p1".into(),
            },
            SessionMeta {
                id: "b".into(),
                project: "p1".into(),
                path: "/x/b".into(),
                mtime: 2,
                message_count: 1,
                last_model: None,
                title: "p1".into(),
            },
        ];
        let g = group_by_project(&sessions);
        assert_eq!(g.get("p1").map(|v| v.len()), Some(2));
    }

    #[test]
    fn builds_rounds_by_user_boundary() {
        let msgs = vec![
            Message {
                role: "user".into(),
                text: "q1".into(),
                model: None,
                is_tool_use: false,
                tool_name: None,
            },
            Message {
                role: "assistant".into(),
                text: "a1".into(),
                model: Some("m1".into()),
                is_tool_use: false,
                tool_name: None,
            },
            Message {
                role: "user".into(),
                text: "q2".into(),
                model: None,
                is_tool_use: false,
                tool_name: None,
            },
            Message {
                role: "assistant".into(),
                text: "{\"is_error\":true}".into(),
                model: None,
                is_tool_use: true,
                tool_name: Some("Edit".into()),
            },
        ];
        let rounds = build_rounds(&msgs);
        assert_eq!(rounds.len(), 2);
        assert_eq!(rounds[0].user_text, "q1");
        assert_eq!(rounds[0].steps, 1);
        assert_eq!(rounds[1].file_changes, 1);
        assert!(rounds[1].failed);
    }

    #[test]
    fn export_markdown_contains_rounds() {
        let meta = SessionMeta {
            id: "sess-1".into(),
            project: "proj".into(),
            path: "/x/s".into(),
            mtime: 0,
            message_count: 2,
            last_model: Some("m1".into()),
            title: "proj".into(),
        };
        let msgs = vec![
            Message {
                role: "user".into(),
                text: "hi".into(),
                model: None,
                is_tool_use: false,
                tool_name: None,
            },
            Message {
                role: "assistant".into(),
                text: "hello".into(),
                model: Some("m1".into()),
                is_tool_use: false,
                tool_name: None,
            },
        ];
        let md = export_markdown(&meta, &build_rounds(&msgs));
        assert!(md.contains("# 会话 sess-1"));
        assert!(md.contains("第 1 轮"));
        assert!(md.contains("hello"));
        assert_eq!(recover_command("abc"), "osascript -e 'tell application \"Terminal\" to do script \"claude --resume abc\"'");
    }
}
