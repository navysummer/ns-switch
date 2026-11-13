//! ns-sessions — 各工具会话日志解析、阅读视图、恢复命令。
//!
//! Claude Code 会话在 `~/.claude/projects/<proj>/<session>.jsonl`；每行一个事件，
//! `message` 对象含 `role`/`content`/`model`/`usage`。本 crate 提供会话枚举、
//! 消息解析、分组、批量删除与阅读视图所需的分节（每轮 = user→assistant 交换）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 会话来源工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTool {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Pi,
    Hermes,
    OpenClaw,
}

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
    /// 所属项目目录名/工作目录标识。
    pub project: String,
    /// 会话文件路径。
    pub path: String,
    /// 最后修改时间（Unix 秒）。
    pub mtime: i64,
    pub message_count: usize,
    pub last_model: Option<String>,
    pub title: String,
    /// 来源工具（claude/codex）。
    #[serde(default = "default_tool")]
    pub tool: String,
}

fn default_tool() -> String {
    "claude".into()
}
const _: fn() -> String = default_tool;

/// 按工具枚举会话（按 mtime 新→旧）。
pub fn list_sessions(home: &Path, tool: SessionTool) -> Vec<SessionMeta> {
    match tool {
        SessionTool::Claude => list_claude_sessions(home),
        SessionTool::Codex => list_codex_sessions(home),
        SessionTool::Gemini => list_gemini_sessions(home),
        SessionTool::OpenCode => list_opencode_sessions(home),
        SessionTool::Pi => list_pi_sessions(home),
        SessionTool::Hermes => list_hermes_sessions(home),
        SessionTool::OpenClaw => list_openclaw_sessions(home),
    }
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
            let mtime = file_mtime(&p);
            // 快速扫消息数/模型（读全文一次）。
            let (count, last_model) = quick_stats_claude(&p);
            // 尝试从会话内容提取标题（取第一条用户消息的前50字符）。
            let title = extract_claude_session_title(&p).unwrap_or_else(|| project.clone());
            out.push(SessionMeta {
                id,
                project: project.clone(),
                path: p.to_string_lossy().into_owned(),
                mtime,
                message_count: count,
                last_model,
                title,
                tool: "claude".into(),
            });
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.mtime));
    out
}

/// 枚举 Codex 会话（`~/.codex/sessions/**/rollout-*.jsonl`）。
pub fn list_codex_sessions(home: &Path) -> Vec<SessionMeta> {
    let root = home.join(".codex").join("sessions");
    let mut files = Vec::new();
    collect_rollout_files(&root, &mut files);
    let mut out = Vec::new();
    for p in files {
        if let Some(meta) = codex_meta_from_file(&p) {
            out.push(meta);
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.mtime));
    out
}

/// 枚举 Gemini CLI 会话（`~/.gemini/tmp/**/chats/session-*.jsonl`）。
pub fn list_gemini_sessions(home: &Path) -> Vec<SessionMeta> {
    let root = home.join(".gemini").join("tmp");
    let mut files = Vec::new();
    collect_gemini_chat_files(&root, &mut files);
    let mut out = Vec::new();
    for p in files {
        if let Some(meta) = gemini_meta_from_file(&p) {
            out.push(meta);
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.mtime));
    out
}

/// OpenCode SQLite 数据库路径（`~/.local/share/opencode/opencode.db`）。
fn opencode_db(home: &Path) -> PathBuf {
    home.join(".local").join("share").join("opencode").join("opencode.db")
}

/// 枚举 OpenCode 会话（SQLite `session_v2` 表；经 sqlite3 CLI）。
pub fn list_opencode_sessions(home: &Path) -> Vec<SessionMeta> {
    let db = opencode_db(home);
    if !db.exists() {
        return Vec::new();
    }
    // 仅列实际有 part 数据（可读消息）的会话，按创建时间新→旧。
    let query = "SELECT s.id, s.title, s.directory, s.time_created \
                 FROM session_v2 s JOIN part p ON p.session_id = s.id \
                 WHERE s.time_archived IS NULL \
                 GROUP BY s.id ORDER BY s.time_created DESC;";
    let Ok(out) = std::process::Command::new("sqlite3")
        .arg("-json")
        .arg(&db)
        .arg(query)
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let Ok(rows) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return Vec::new();
    };
    rows.into_iter()
        .filter_map(|r| {
            let id = r.get("id").and_then(|v| v.as_str())?.to_string();
            let title = r
                .get("title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| id.clone());
            let directory = r
                .get("directory")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let project = std::path::Path::new(&directory)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("opencode")
                .to_string();
            let mtime = r
                .get("time_created")
                .and_then(|v| v.as_i64())
                .map(|ms| ms / 1000)
                .unwrap_or(0);
            let path = format!("opencode:{}", id);
            Some(SessionMeta {
                id,
                project,
                path,
                mtime,
                message_count: 0,
                last_model: None,
                title,
                tool: "opencode".into(),
            })
        })
        .collect()
}

/// 枚举 Pi（Oh My Pi）会话（`~/.pi/agent/sessions/<project>/<ts>_<uuid>.jsonl`）。
pub fn list_pi_sessions(home: &Path) -> Vec<SessionMeta> {
    let base = home.join(".pi").join("agent").join("sessions");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for proj_entry in entries.flatten() {
        if !proj_entry.path().is_dir() {
            continue;
        }
        let project = proj_entry.file_name().to_string_lossy().into_owned();
        let Ok(files) = std::fs::read_dir(proj_entry.path()) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (id, timestamp, cwd) = parse_pi_session_header(&content);
            let mtime = f
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let title = timestamp.unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("pi")
                    .to_string()
            });
            let project_name = std::path::Path::new(&cwd.unwrap_or_default())
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| project.clone());
            out.push(SessionMeta {
                id: id.unwrap_or_default(),
                project: project_name,
                path: path.to_string_lossy().into_owned(),
                mtime,
                message_count: count_pi_messages(&content),
                last_model: None,
                title,
                tool: "pi".into(),
            });
        }
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    out
}

/// 解析 Pi 会话头（`session` 记录的 id/timestamp/cwd）。
fn parse_pi_session_header(content: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut id = None;
    let mut timestamp = None;
    let mut cwd = None;
    for line in content.lines().take(20) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) == Some("session") {
            id = v.get("id").and_then(|x| x.as_str()).map(str::to_string);
            timestamp = v.get("timestamp").and_then(|x| x.as_str()).map(str::to_string);
            cwd = v.get("cwd").and_then(|x| x.as_str()).map(str::to_string);
            break;
        }
    }
    (id, timestamp, cwd)
}

/// 统计 Pi 会话的 message 记录数。
fn count_pi_messages(content: &str) -> usize {
    content
        .lines()
        .filter(|l| l.contains("\"type\":\"message\""))
        .count()
}

/// 解析 Pi 会话消息（`type=message` 记录，`message.role`/`message.content[].text`）。
pub fn parse_pi_session(path: &Path) -> Vec<Message> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("message") {
            continue;
        }
        let Some(msg) = v.get("message") else {
            continue;
        };
        let role = msg
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("user")
            .to_string();
        let model = msg
            .get("model")
            .and_then(|m| m.as_str())
            .map(str::to_string)
            .or_else(|| {
                v.get("modelId")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            });
        let content_val = msg.get("content");
        let texts: Vec<String> = match content_val {
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(|t| t.as_str())
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                })
                .collect(),
            Some(serde_json::Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        };
        for text in texts {
            out.push(Message {
                role: role.clone(),
                text,
                model: model.clone(),
                is_tool_use: false,
                tool_name: None,
            });
        }
    }
    out
}

/// Hermes SQLite 数据库路径（`~/.hermes/state.db`）。
fn hermes_db(home: &Path) -> PathBuf {
    home.join(".hermes").join("state.db")
}

/// 枚举 Hermes 会话（SQLite `sessions` 表；经 sqlite3 CLI，无 SQLite 依赖）。
///
/// 主存储为 `~/.hermes/state.db`；`~/.hermes/sessions/<id>.jsonl` 为 state.db
/// 被替换时的兜底 transcript（`divert_session_transcript_jsonl`）。
pub fn list_hermes_sessions(home: &Path) -> Vec<SessionMeta> {
    let db = hermes_db(home);
    if !db.exists() {
        return Vec::new();
    }
    let query = "SELECT id, COALESCE(title,'') AS title, COALESCE(cwd,'') AS cwd, \
                 COALESCE(started_at,0) AS started_at, COALESCE(message_count,0) AS message_count, \
                 COALESCE(model,'') AS model \
                 FROM sessions WHERE COALESCE(archived,0)=0 \
                 ORDER BY COALESCE(last_activity_at, started_at) DESC;";
    let Ok(out) = std::process::Command::new("sqlite3")
        .arg("-json")
        .arg(&db)
        .arg(query)
        .output()
    else {
        return Vec::new();
    };
    let Ok(rows) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    let Some(arr) = rows.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|r| {
            let id = r.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if id.is_empty() {
                return None;
            }
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let cwd = r.get("cwd").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let started_at = r.get("started_at").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let message_count = r.get("message_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let model = r
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from);
            let project = std::path::Path::new(&cwd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("hermes")
                .to_string();
            Some(SessionMeta {
                id: id.clone(),
                project,
                path: db.to_string_lossy().into_owned(),
                mtime: started_at as i64,
                message_count,
                last_model: model,
                title,
                tool: "hermes".into(),
            })
        })
        .collect()
}

/// 解析 Hermes 会话消息（SQLite `messages` 表 `role`/`content`）。
pub fn parse_hermes_session(home: &Path, session_id: &str) -> Vec<Message> {
    let db = hermes_db(home);
    if !db.exists() {
        return Vec::new();
    }
    let query = format!(
        "SELECT role, COALESCE(content,'') AS content, COALESCE(tool_name,'') AS tool_name, \
         COALESCE(timestamp,0) AS timestamp \
         FROM messages WHERE session_id='{}' AND COALESCE(active,1)=1 \
         ORDER BY timestamp ASC;",
        session_id.replace('\'', "''")
    );
    let Ok(out) = std::process::Command::new("sqlite3")
        .arg("-json")
        .arg(&db)
        .arg(&query)
        .output()
    else {
        return Vec::new();
    };
    let Ok(rows) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    let Some(arr) = rows.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for r in arr {
        let role = r.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let text = r.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if text.trim().is_empty() {
            continue;
        }
        let tool_name = r
            .get("tool_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);
        out.push(Message {
            role,
            text,
            model: None,
            is_tool_use: tool_name.is_some(),
            tool_name,
        });
    }
    out
}

/// 解析 OpenCode 会话消息（SQLite `part` 表；text 类型内容）。
pub fn parse_opencode_session(home: &Path, session_id: &str) -> Vec<Message> {
    let db = opencode_db(home);
    if !db.exists() {
        return Vec::new();
    }
    // JOIN message 获取角色与模型；part.data 为 text/reasoning 等类型。
    let sid = session_id.replace('\'', "''");
    let query = format!(
        "SELECT p.data, m.data AS mdata FROM part p \
         LEFT JOIN message m ON p.message_id = m.id \
         WHERE p.session_id = '{sid}' ORDER BY p.time_created;"
    );
    let Ok(out) = std::process::Command::new("sqlite3")
        .arg("-json")
        .arg(&db)
        .arg(&query)
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let Ok(rows) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return Vec::new();
    };
    let mut msgs = Vec::new();
    for r in rows {
        let Some(data) = r.get("data").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        let mdata = r
            .get("mdata")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
        if v.get("type").and_then(|t| t.as_str()) == Some("text") {
            let text = v
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if !text.is_empty() {
                let role = mdata
                    .as_ref()
                    .and_then(|m| m.get("role").and_then(|x| x.as_str()))
                    .unwrap_or("user")
                    .to_string();
                let model = mdata
                    .as_ref()
                    .and_then(|m| m.get("model"))
                    .and_then(|m| m.get("modelID").and_then(|x| x.as_str()))
                    .map(str::to_string);
                msgs.push(Message {
                    role,
                    text,
                    model,
                    is_tool_use: false,
                    tool_name: None,
                });
            }
        }
    }
    msgs
}

/// 枚举 OpenClaw 会话（调 `openclaw sessions --json` 官方接口）。
///
/// 顶层 `{path, count, totalCount, sessions: [SessionListRow]}`；每行含
/// `sessionId`/`key`/`derivedTitle`/`label`/`execCwd`/`workspaceDir`/`model`/
/// `updatedAt`（ms）/`archived`。CLI 不可用或未运行时返回空。
pub fn list_openclaw_sessions(home: &Path) -> Vec<SessionMeta> {
    let _ = home; // 会话存储由 OpenClaw 管理，home 仅作兼容签名。
    let Ok(out) = std::process::Command::new("openclaw")
        .args(["sessions", "--json", "--limit", "100"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    let Some(arr) = v.get("sessions").and_then(|s| s.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|s| {
            if s.get("archived").and_then(|a| a.as_bool()).unwrap_or(false) {
                return None;
            }
            let id = s
                .get("sessionId")
                .and_then(|x| x.as_str())
                .filter(|x| !x.is_empty())
                .or_else(|| s.get("key").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                return None;
            }
            let title = s
                .get("derivedTitle")
                .and_then(|x| x.as_str())
                .filter(|x| !x.is_empty())
                .or_else(|| s.get("label").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string();
            let cwd = s
                .get("workspaceDir")
                .and_then(|x| x.as_str())
                .or_else(|| s.get("execCwd").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string();
            let project = std::path::Path::new(&cwd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("openclaw")
                .to_string();
            let updated_ms = s.get("updatedAt").and_then(|x| x.as_i64()).unwrap_or(0);
            let mtime = if updated_ms > 0 {
                updated_ms / 1000
            } else {
                0
            };
            let model = s
                .get("model")
                .and_then(|x| x.as_str())
                .filter(|x| !x.is_empty())
                .map(String::from);
            Some(SessionMeta {
                id: id.clone(),
                project,
                path: format!("openclaw:{}", id),
                mtime,
                message_count: 0,
                last_model: model,
                title,
                tool: "openclaw".into(),
            })
        })
        .collect()
}

/// 解析 OpenClaw 会话消息（调 `openclaw transcripts show <id> --json`）。
///
/// 尽力解析常见摘要结构；无真实会话数据时返回空（CLI 不可用或会话不存在）。
pub fn parse_openclaw_session(home: &Path, session_id: &str) -> Vec<Message> {
    let _ = home;
    let Ok(out) = std::process::Command::new("openclaw")
        .args(["transcripts", "show", session_id, "--json"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    // 尝试常见键：messages/entries/turns（数组）；每项含 role/type + text/content。
    let arr = ["messages", "entries", "turns", "items"]
        .iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_array()))
        .or_else(|| v.as_array());
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out_msgs = Vec::new();
    for item in arr {
        let role = item
            .get("role")
            .and_then(|x| x.as_str())
            .or_else(|| item.get("type").and_then(|x| x.as_str()))
            .unwrap_or("")
            .to_string();
        let text = extract_openclaw_text(item);
        if text.trim().is_empty() {
            continue;
        }
        let is_tool = role.contains("tool") || role.contains("function");
        let tool_name = if is_tool {
            item.get("name")
                .or_else(|| item.get("tool"))
                .and_then(|x| x.as_str())
                .map(String::from)
        } else {
            None
        };
        let model = item
            .get("model")
            .and_then(|x| x.as_str())
            .filter(|x| !x.is_empty())
            .map(String::from);
        out_msgs.push(Message {
            role,
            text,
            model,
            is_tool_use: is_tool,
            tool_name,
        });
    }
    out_msgs
}

/// 从 OpenClaw 消息项尽力提取文本（`text`/`content`/`message` 嵌套）。
fn extract_openclaw_text(item: &serde_json::Value) -> String {
    for key in ["text", "content", "message"] {
        if let Some(v) = item.get(key) {
            if let Some(s) = v.as_str() {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
            if let Some(arr) = v.as_array() {
                let joined: Vec<String> = arr
                    .iter()
                    .filter_map(|x| x.get("text").and_then(|t| t.as_str()).map(String::from))
                    .collect();
                if !joined.is_empty() {
                    return joined.join("\n");
                }
            }
            if v.is_object() {
                let s = extract_openclaw_text(v);
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    String::new()
}

fn file_mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn collect_rollout_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_rollout_files(&p, out);
        } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl")) {
            out.push(p);
        }
    }
}

/// 递归收集 Gemini 会话文件：父目录为 `chats` 且文件名以 `session-` 开头。
fn collect_gemini_chat_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_gemini_chat_files(&p, out);
        } else if p
            .parent()
            .and_then(|d| d.file_name())
            .map(|n| n == "chats")
            .unwrap_or(false)
            && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("session-") && n.ends_with(".jsonl")
            })
        {
            out.push(p);
        }
    }
}

fn codex_meta_from_file(path: &Path) -> Option<SessionMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut id = path.file_stem().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let mut cwd = String::new();
    let mut ts = file_mtime(path);
    let mut model = None;
    let mut count = 0usize;
    for (i, line) in content.lines().enumerate() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue; };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty == "session_meta" {
            let p = v.get("payload").and_then(|p| p.as_object());
            if let Some(p) = p {
                if let Some(s) = p.get("id").or_else(|| p.get("session_id")).and_then(|x| x.as_str()) {
                    id = s.to_string();
                }
                if let Some(s) = p.get("cwd").and_then(|x| x.as_str()) {
                    cwd = s.to_string();
                }
                if let Some(s) = p.get("timestamp").and_then(|x| x.as_str()) {
                    if let Some(sec) = parse_rfc3339_secs(s) { ts = sec; }
                }
                model = p.get("model_provider").and_then(|x| x.as_str()).map(str::to_string)
                    .or_else(|| model.clone());
            }
        } else if ty == "event_msg" {
            count += 1;
        } else if ty == "response_item" {
            count += 1;
        }
        if i > 400 { break; } // 列表场景：只扫头部估算
    }
    let project = if cwd.is_empty() {
        "codex".into()
    } else {
        cwd.rsplit('/').next().unwrap_or(&cwd).to_string()
    };
    let title = if cwd.is_empty() { id.clone() } else { project.clone() };
    Some(SessionMeta { id, project, path: path.to_string_lossy().into_owned(), mtime: ts, message_count: count, last_model: model, title, tool: "codex".into() })
}

/// 从 Gemini 会话文件提取元数据：`type == "gemini"` 记录含 model/tokens。
fn gemini_meta_from_file(path: &Path) -> Option<SessionMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    let id = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let mut ts = file_mtime(path);
    let mut model = None;
    let mut count = 0usize;
    for (i, line) in content.lines().enumerate() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue; };
        if v.get("type").and_then(|t| t.as_str()) != Some("gemini") {
            continue;
        }
        count += 1;
        if let Some(m) = v.get("model").and_then(|x| x.as_str()) {
            model = Some(m.to_string());
        }
        if let Some(s) = v.get("timestamp").and_then(|x| x.as_str()) {
            if let Some(sec) = parse_rfc3339_secs(s) {
                ts = sec;
            }
        }
        if i > 400 {
            break;
        }
    }
    let project = path
        .parent()
        .and_then(|d| d.parent())
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("gemini")
        .to_string();
    let title = project.clone();
    Some(SessionMeta {
        id,
        project,
        path: path.to_string_lossy().into_owned(),
        mtime: ts,
        message_count: count,
        last_model: model,
        title,
        tool: "gemini".into(),
    })
}

fn parse_rfc3339_secs(s: &str) -> Option<i64> {
    // 轻量解析 "2026-10-27T07:57:58.863Z" / "+00:00"
    let date = s.get(0..10)?;
    let time = s.get(11..19)?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let m: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let hh: i64 = t.next()?.parse().ok()?;
    let mm: i64 = t.next()?.parse().ok()?;
    let ss: i64 = t.next()?.parse().ok()?;
    // days since epoch (civil)
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

/// 从会话文件内容中提取标题（第一条非空用户消息的前50字符）。
fn extract_claude_session_title(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let v = serde_json::from_str::<serde_json::Value>(line).ok()?;
        let msg = v.get("message").and_then(|m| m.as_object())?;
        let role = msg.get("role")?.as_str()?;
        if role != "user" {
            continue;
        }
        if let Some(content_val) = msg.get("content") {
            let text = if let Some(arr) = content_val.as_array() {
                arr.iter()
                    .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else if let Some(s) = content_val.as_str() {
                s.to_string()
            } else {
                continue;
            };
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(
                    if trimmed.len() > 50 {
                        &trimmed[..50]
                    } else {
                        trimmed
                    }
                    .to_string(),
                );
            }
        }
    }
    None
}

fn quick_stats_claude(path: &Path) -> (usize, Option<String>) {
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

/// 解析会话文件为消息序列（自动识别 Claude / Codex）。
pub fn parse_session(path: &Path) -> Vec<Message> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let is_codex = content.lines().take(8).any(|l| {
        l.contains("\"type\":\"session_meta\"")
            || (l.contains("\"type\":\"response_item\"") && l.contains("\"payload\""))
            || (l.contains("\"type\":\"event_msg\"") && l.contains("\"payload\""))
            || l.contains("\"type\":\"turn_context\"")
    });
    if is_codex {
        parse_codex_session(&content)
    } else if content.lines().take(8).any(|l| {
        l.contains("\"type\":\"gemini\"")
    }) {
        parse_gemini_session(&content)
    } else {
        parse_claude_session(&content)
    }
}

fn parse_claude_session(content: &str) -> Vec<Message> {
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

/// Gemini CLI 会话 JSONL 的 best-effort 解析：提取 `type == "gemini"` 记录中的
/// `role`/`text`/`content`（含 `parts[]` 数组文本）作为消息；无文本的记录（纯
/// token 计数）跳过。
fn parse_gemini_session(content: &str) -> Vec<Message> {
    let mut out = Vec::new();
    let mut last_model: Option<String> = None;
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("gemini") {
            continue;
        }
        let role = v
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(m) = v.get("model").and_then(|x| x.as_str()) {
            last_model = Some(m.to_string());
        }
        let model = last_model.clone();
        let mut text = String::new();
        if let Some(s) = v.get("text").and_then(|x| x.as_str()) {
            text = s.to_string();
        } else if let Some(s) = v.get("content").and_then(|x| x.as_str()) {
            text = s.to_string();
        } else if let Some(parts) = v.get("parts").and_then(|x| x.as_array()) {
            for p in parts {
                if let Some(s) = p.get("text").and_then(|x| x.as_str()) {
                    text.push_str(s);
                    text.push('\n');
                }
            }
        } else if let Some(content) = v.get("content").and_then(|x| x.as_array()) {
            for p in content {
                if let Some(s) = p.get("text").and_then(|x| x.as_str()) {
                    text.push_str(s);
                    text.push('\n');
                }
            }
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let is_tool = v
            .get("tool_calls")
            .and_then(|x| x.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        out.push(Message {
            role: if role.is_empty() {
                "assistant".into()
            } else {
                role
            },
            text,
            model,
            is_tool_use: is_tool,
            tool_name: None,
        });
    }
    out
}

/// Codex rollout JSONL 的 best-effort 解析（提取 user/assistant 文本与工具事件）。
fn parse_codex_session(content: &str) -> Vec<Message> {
    let mut out = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue; };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "response_item" => {
                let p = match v.get("payload") { Some(p) => p, None => continue };
                let pty = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if pty == "message" {
                    let role = p.get("role").and_then(|r| r.as_str()).unwrap_or("");
                    if role != "user" && role != "assistant" {
                        continue; // 跳过 developer/system
                    }
                    let text = extract_codex_content_text(p.get("content"));
                    if !text.trim().is_empty() {
                        out.push(Message {
                            role: role.to_string(),
                            text,
                            model: None,
                            is_tool_use: false,
                            tool_name: None,
                        });
                    }
                } else if matches!(pty, "function_call" | "custom_tool_call" | "local_shell_call") {
                    let name = p.get("name").and_then(|n| n.as_str())
                        .or_else(|| p.get("action").and_then(|a| a.get("type")).and_then(|n| n.as_str()))
                        .unwrap_or(pty);
                    let args = p.get("arguments").and_then(|a| a.as_str())
                        .or_else(|| p.get("input").and_then(|a| a.as_str()))
                        .unwrap_or("");
                    let short = truncate_str(args, 400);
                    out.push(Message {
                        role: "assistant".into(),
                        text: format!("[{name}] {short}"),
                        model: None,
                        is_tool_use: true,
                        tool_name: Some(name.to_string()),
                    });
                } else if matches!(pty, "function_call_output" | "custom_tool_call_output") {
                    let out_text = p.get("output").and_then(|o| o.as_str()).unwrap_or("");
                    let short = truncate_str(out_text, 600);
                    if !short.trim().is_empty() {
                        out.push(Message {
                            role: "tool".into(),
                            text: format!("结果: {short}"),
                            model: None,
                            is_tool_use: true,
                            tool_name: Some("tool_result".into()),
                        });
                    }
                }
            }
            "event_msg" => {
                let p = match v.get("payload") { Some(p) => p, None => continue };
                let ety = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if ety == "user_message" {
                    if let Some(text) = p.get("message").and_then(|m| m.as_str()).filter(|s| !s.trim().is_empty()) {
                        out.push(Message { role: "user".into(), text: text.to_string(), model: None, is_tool_use: false, tool_name: None });
                    }
                } else if ety == "agent_message" {
                    if let Some(text) = p.get("message").and_then(|m| m.as_str()).filter(|s| !s.trim().is_empty()) {
                        out.push(Message { role: "assistant".into(), text: text.to_string(), model: None, is_tool_use: false, tool_name: None });
                    }
                } else if ety == "error" {
                    if let Some(text) = p.get("message").and_then(|m| m.as_str()).filter(|s| !s.trim().is_empty()) {
                        out.push(Message { role: "error".into(), text: text.to_string(), model: None, is_tool_use: false, tool_name: None });
                    }
                } else if ety == "agent_reasoning" {
                    if let Some(text) = p.get("message").and_then(|m| m.as_str()).filter(|s| !s.trim().is_empty()) {
                        out.push(Message { role: "assistant".into(), text: format!("推理: {text}"), model: None, is_tool_use: false, tool_name: None });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

fn extract_codex_content_text(content: Option<&serde_json::Value>) -> String {
    let Some(serde_json::Value::Array(items)) = content else {
        return content.and_then(|c| c.as_str()).unwrap_or("").to_string();
    };
    let mut parts = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else { continue; };
        let ty = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if matches!(ty, "input_text" | "output_text" | "text" | "reasoning") {
            if let Some(t) = obj.get("text").and_then(|t| t.as_str()) {
                if !t.trim().is_empty() {
                    parts.push(t.to_string());
                }
            }
        }
    }
    parts.join("\n")
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
            if m.text.contains("\"is_error\":true") || m.role == "error" {
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
        out.push_str(&round_markdown(r));
    }
    out
}

/// 单轮 Markdown（复制本轮用）。
pub fn round_markdown(r: &Round) -> String {
    let mut out = String::new();
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
    out
}

/// 生成跨平台一键在终端恢复会话的命令（Claude Code resume）。
pub fn recover_command(session_id: &str) -> String {
    recover_command_for("claude", session_id)
}

/// 按工具生成恢复命令（纯命令，不含终端启动包装；由界面用首选终端执行）。
pub fn recover_command_for(tool: &str, session_id: &str) -> String {
    let (bin, arg) = match tool {
        "codex" => ("codex", "resume"),
        "gemini" => ("gemini", "--session"),
        "opencode" => ("opencode", "session"),
        "pi" => ("pi", "--resume"),
        "hermes" => ("hermes", "resume"),
        "openclaw" => ("openclaw", "resume"),
        _ => ("claude", "--resume"),
    };
    format!("{bin} {arg} {session_id}")
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
                tool: "claude".into(),
            },
            SessionMeta {
                id: "b".into(),
                project: "proj-2".into(),
                path: "/x/b.jsonl".into(),
                mtime: 2,
                message_count: 1,
                last_model: None,
                title: "proj-2".into(),
                tool: "claude".into(),
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
                tool: "claude".into(),
            },
            SessionMeta {
                id: "b".into(),
                project: "p1".into(),
                path: "/x/b".into(),
                mtime: 2,
                message_count: 1,
                last_model: None,
                title: "p1".into(),
                tool: "claude".into(),
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
            tool: "claude".into(),
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
        // 单轮复制：只有该轮，不含会话头。
        let one = round_markdown(&build_rounds(&msgs)[0]);
        assert!(one.starts_with("## 第 1 轮"));
        assert!(one.contains("**用户**\n\nhi"));
        assert!(!one.contains("# 会话 sess-1"));
        // 恢复命令为纯命令，终端包装由界面按首选终端执行。
        assert_eq!(recover_command("abc"), "claude --resume abc");
        assert_eq!(recover_command_for("codex", "abc"), "codex resume abc");
    }

    #[test]
    fn default_tool_is_claude() {
        assert_eq!(default_tool(), "claude");
    }

    #[test]
    fn parses_codex_rollout_meta() {
        let dir = std::env::temp_dir().join(format!(
            "ns-sessions-codex-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let sess = dir.join(".codex/sessions/2026/10/27");
        std::fs::create_dir_all(&sess).unwrap();
        let f = sess.join("rollout-2026-10-27T15-57-58-01a17c5d.jsonl");
        std::fs::write(
            &f,
            r#"{"timestamp":"2026-10-27T07:57:58.863Z","type":"session_meta","payload":{"id":"01a17c5d","cwd":"/tmp/myproj","timestamp":"2026-10-27T07:57:58.863Z","model_provider":"openai"}}
{"timestamp":"2026-10-27T07:58:11.191Z","type":"event_msg","payload":{"type":"task_started"}}"#,
        )
        .unwrap();
        let list = list_codex_sessions(&dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "01a17c5d");
        assert_eq!(list[0].project, "myproj");
        assert_eq!(list[0].tool, "codex");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_codex_session_content() {
        let dir = std::env::temp_dir().join(format!(
            "ns-sessions-codex-content-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("rollout.jsonl");
        std::fs::write(
            &f,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"s1\",\"cwd\":\"/tmp/p\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"帮我看看\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"好的\"}]}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"shell_command\",\"arguments\":\"{\\\"command\\\":\\\"ls\\\"}\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"output\":\"file.txt\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"完成\"}}\n",
            ),
        )
        .unwrap();
        let msgs = parse_session(&f);
        assert!(msgs.iter().any(|m| m.role == "user" && m.text.contains("帮我看看")));
        assert!(msgs.iter().any(|m| m.role == "assistant" && m.text.contains("好的")));
        assert!(msgs.iter().any(|m| m.is_tool_use && m.tool_name.as_deref() == Some("shell_command")));
        assert!(msgs.iter().any(|m| m.role == "tool" && m.text.contains("file.txt")));
        assert!(msgs.iter().any(|m| m.role == "assistant" && m.text.contains("完成")));
        let rounds = build_rounds(&msgs);
        assert!(!rounds.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_codex_errors_and_reasoning() {
        let content = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"s2\",\"cwd\":\"/tmp/p\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"hi\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"reasoning\",\"text\":\"先想一下\"},{\"type\":\"output_text\",\"text\":\"答复\"}]}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"error\",\"message\":\"上游超时\"}}\n",
        );
        let msgs = parse_codex_session(content);
        // 推理内容被提取进 assistant 消息。
        assert!(msgs.iter().any(|m| m.role == "assistant" && m.text.contains("先想一下")));
        assert!(msgs.iter().any(|m| m.role == "assistant" && m.text.contains("答复")));
        // 错误事件成为 error 消息。
        assert!(msgs.iter().any(|m| m.role == "error" && m.text.contains("上游超时")));
        // 含错误消息的轮次被标记 failed。
        let rounds = build_rounds(&msgs);
        assert!(rounds.iter().any(|r| r.failed));
    }

    #[test]
    fn parses_gemini_session_content() {
        let content = concat!(
            "{\"type\":\"gemini\",\"model\":\"gemini-3-flash-preview\",\"tokens\":{\"input\":100,\"output\":20,\"total\":120}}\n",
            "{\"type\":\"gemini\",\"role\":\"user\",\"text\":\"你好\"}\n",
            "{\"type\":\"gemini\",\"role\":\"assistant\",\"parts\":[{\"text\":\"答复一\"}]}\n",
            "{\"type\":\"gemini\",\"role\":\"assistant\",\"content\":[{\"text\":\"答复二\"}]}\n",
        );
        let msgs = parse_gemini_session(content);
        assert!(msgs.iter().any(|m| m.role == "user" && m.text.contains("你好")));
        assert!(msgs.iter().any(|m| m.role == "assistant" && m.text.contains("答复一")));
        assert!(msgs.iter().any(|m| m.role == "assistant" && m.text.contains("答复二")));
        let first = msgs.iter().find(|m| m.role == "assistant").unwrap();
        assert_eq!(first.model.as_deref(), Some("gemini-3-flash-preview"));
    }

    #[test]
    fn lists_gemini_sessions_from_chats_dir() {
        let dir = std::env::temp_dir().join("ns-sessions-gemini-test");
        let chats = dir.join(".gemini").join("tmp").join("proj").join("chats");
        let _ = std::fs::create_dir_all(&chats);
        let f = chats.join("session-2026-05-03T15-01-test.jsonl");
        std::fs::write(
            &f,
            "{\"type\":\"gemini\",\"model\":\"gemini-3-flash-preview\",\"tokens\":{\"input\":1,\"output\":1,\"total\":2}}\n",
        )
        .unwrap();
        let metas = list_gemini_sessions(&dir);
        assert!(!metas.is_empty());
        assert_eq!(metas[0].tool, "gemini");
        assert_eq!(metas[0].project, "proj");
        assert!(metas[0].message_count >= 1);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opencode_no_db_returns_empty() {
        // 无 opencode.db 时 list/parse 均返回空（不 panic）。
        let dir = std::env::temp_dir().join("ns-sessions-opencode-test");
        let _ = std::fs::create_dir_all(&dir);
        let metas = list_opencode_sessions(&dir);
        assert!(metas.is_empty());
        let msgs = parse_opencode_session(&dir, "ses_none");
        assert!(msgs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extracts_openclaw_text() {
        assert_eq!(extract_openclaw_text(&serde_json::json!({"text": "hello"})), "hello");
        assert_eq!(
            extract_openclaw_text(&serde_json::json!({"content": [{"text": "a"}, {"text": "b"}]})),
            "a\nb"
        );
        assert_eq!(
            extract_openclaw_text(&serde_json::json!({"message": {"text": "nested"}})),
            "nested"
        );
        assert_eq!(extract_openclaw_text(&serde_json::json!({"type": "x"})), "");
    }

    #[test]
    fn openclaw_cli_lists_sessions() {
        // openclaw 二进制存在且 `sessions --json` 可用时返回（本机 count=0 为空数组）；
        // CLI 缺失时返回空。均不 panic。
        let dir = std::env::temp_dir().join("ns-sessions-openclaw-test");
        let metas = list_openclaw_sessions(&dir);
        // 仅断言不 panic 且结构合法（id 非空时 project/tool 正确）。
        for m in &metas {
            assert_eq!(m.tool, "openclaw");
            assert!(!m.id.is_empty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lists_and_parses_pi_sessions() {
        let dir = std::env::temp_dir().join("ns-sessions-pi-test");
        let proj = dir
            .join(".pi")
            .join("agent")
            .join("sessions")
            .join("--Users-navysummer--");
        let _ = std::fs::create_dir_all(&proj);
        let f = proj.join("2026-10-01T00-00-00-000Z_abc.jsonl");
        std::fs::write(
            &f,
            "{\"type\":\"session\",\"version\":3,\"id\":\"abc\",\"timestamp\":\"2026-10-01T00:00:00.000Z\",\"cwd\":\"/Users/navysummer/proj\"}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"你好\"}]}}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"答复\"}]}}\n",
        )
        .unwrap();
        let metas = list_pi_sessions(&dir);
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].tool, "pi");
        assert_eq!(metas[0].project, "proj");
        assert_eq!(metas[0].message_count, 2);
        let msgs = parse_pi_session(&f);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert!(msgs[0].text.contains("你好"));
        assert_eq!(msgs[1].role, "assistant");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pi_no_dir_returns_empty() {
        let dir = std::env::temp_dir().join("ns-sessions-pi-empty-test");
        let _ = std::fs::create_dir_all(&dir);
        assert!(list_pi_sessions(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hermes_no_db_returns_empty() {
        let dir = std::env::temp_dir().join("ns-sessions-hermes-empty-test");
        let _ = std::fs::create_dir_all(&dir);
        assert!(list_hermes_sessions(&dir).is_empty());
        assert!(parse_hermes_session(&dir, "sid_1").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hermes_sqlite_list_and_parse() {
        // sqlite3 CLI 可用时用临时 db 验证（不可用则跳过）。
        let ok = std::process::Command::new("sqlite3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            return;
        }
        let dir = std::env::temp_dir().join("ns-sessions-hermes-db-test");
        // 清理遗留 db（schema 变更后旧表不兼容）。
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join(".hermes");
        let _ = std::fs::create_dir_all(&home);
        let db = home.join("state.db");
        let schema = "CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT, cwd TEXT, \
                      started_at REAL, message_count INTEGER, model TEXT, archived INTEGER, \
                      last_activity_at REAL); \
                      CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, \
                      session_id TEXT, role TEXT, content TEXT, tool_name TEXT, \
                      timestamp REAL, active INTEGER);";
        let _ = std::process::Command::new("sqlite3")
            .arg(&db)
            .arg(schema)
            .output();
        let insert = "INSERT INTO sessions (id,title,cwd,started_at,message_count,model,archived) \
                      VALUES ('sid_1','我的会话','/Users/u/proj',100.0,2,'hermes-3.5',0), \
                             ('sid_2','隐藏','/Users/u/proj2',200.0,0,NULL,1); \
                      INSERT INTO messages (session_id,role,content,tool_name,timestamp,active) \
                      VALUES ('sid_1','user','你好',NULL,100.0,1), \
                             ('sid_1','assistant','答复',NULL,101.0,1), \
                             ('sid_1','tool','结果','bash',102.0,1);";
        let _ = std::process::Command::new("sqlite3")
            .arg(&db)
            .arg(insert)
            .output();
        let metas = list_hermes_sessions(&dir);
        assert_eq!(metas.len(), 1, "应只列出未归档会话");
        assert_eq!(metas[0].id, "sid_1");
        assert_eq!(metas[0].title, "我的会话");
        assert_eq!(metas[0].project, "proj");
        assert_eq!(metas[0].message_count, 2);
        assert_eq!(metas[0].last_model.as_deref(), Some("hermes-3.5"));
        let msgs = parse_hermes_session(&dir, "sid_1");
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "user");
        assert!(msgs[0].text.contains("你好"));
        assert!(msgs[2].is_tool_use);
        assert_eq!(msgs[2].tool_name.as_deref(), Some("bash"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
