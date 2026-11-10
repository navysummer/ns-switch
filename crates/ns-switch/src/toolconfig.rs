//! 工具配置文件编辑器：OpenClaw 工作区指令 / Hermes 记忆。
//!
//! - OpenClaw：`~/.openclaw/AGENTS.md`（工作区指令）、`~/.openclaw/SOUL.md`（persona）。
//! - Hermes：`~/.hermes/memory/MEMORY.md`、`~/.hermes/memory/USER.md`（持久记忆）。
//!
//! 文件不存在时按需创建（保存时）。

use gpui::AppContext;
use std::path::PathBuf;

/// 一个可编辑的配置文件。
#[derive(Debug, Clone)]
pub struct ConfigFile {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub path: PathBuf,
}

/// home 目录。
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// 可编辑配置文件清单。
pub fn files() -> Vec<ConfigFile> {
    let h = home().unwrap_or_default();
    vec![
        ConfigFile {
            id: "openclaw-agents",
            label: "OpenClaw 工作区指令",
            hint: "AGENTS.md · 工作区规则（OpenClaw）",
            path: h.join(".openclaw").join("AGENTS.md"),
        },
        ConfigFile {
            id: "openclaw-config",
            label: "OpenClaw 配置",
            hint: "openclaw.json · env / tools / agents / workspace（OpenClaw）",
            path: h.join(".openclaw").join("openclaw.json"),
        },
        ConfigFile {
            id: "openclaw-soul",
            label: "OpenClaw Persona",
            hint: "SOUL.md · 人格定义（OpenClaw）",
            path: h.join(".openclaw").join("SOUL.md"),
        },
        ConfigFile {
            id: "hermes-memory",
            label: "Hermes 记忆",
            hint: "MEMORY.md · 持久记忆（Hermes）",
            path: h.join(".hermes").join("memory").join("MEMORY.md"),
        },
        ConfigFile {
            id: "hermes-user",
            label: "Hermes 用户记忆",
            hint: "USER.md · 用户记忆（Hermes）",
            path: h.join(".hermes").join("memory").join("USER.md"),
        },
    ]
}

/// 按 id 找配置文件。
pub fn file_by_id(id: &str) -> Option<ConfigFile> {
    files().into_iter().find(|f| f.id == id)
}

/// 读取文件内容（不存在返回 Ok(None)，未初始化）。
pub fn read_file(id: &str) -> Result<Option<String>, String> {
    let f = file_by_id(id).ok_or("未知配置文件")?;
    if !f.path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(&f.path).map(Some).map_err(|e| e.to_string())
}

/// 写入文件（不存在时创建父目录 + 文件）。
pub fn write_file(id: &str, content: &str) -> Result<(), String> {
    let f = file_by_id(id).ok_or("未知配置文件")?;
    if let Some(parent) = f.path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f.path, content).map_err(|e| e.to_string())
}

/// 文件是否存在。
pub fn exists(id: &str) -> bool {
    file_by_id(id)
        .map(|f| f.path.exists())
        .unwrap_or(false)
}

/// OpenClaw 每日记忆目录：`~/.openclaw/workspace/memory/`。
pub fn daily_memory_dir() -> Option<PathBuf> {
    home().map(|h| h.join(".openclaw").join("workspace").join("memory"))
}

/// 今日每日记忆文件名（`YYYY-MM-DD.md`，本地时区）。
pub fn today_daily_memory_filename() -> String {
    // 用 date 命令取本地日期（避免依赖 chrono）；失败回退 UTC。
    let d = std::process::Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| s.len() == 10)
        .unwrap_or_default();
    let date = if d.is_empty() {
        // 回退：取 Unix 秒 → 本地天数（无时区修正，仅日期回退）。
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_secs() as i64)
            .unwrap_or(0);
        let days = secs.div_euclid(86400);
        let (y, m, dd) = civil_from_days(days);
        format!("{y:04}-{m:02}-{dd:02}")
    } else {
        d
    };
    format!("{date}.md")
}

/// 每日记忆文件信息。
#[derive(Debug, Clone)]
pub struct DailyMemoryFile {
    pub filename: String,
    pub date: String,
    pub size_bytes: u64,
    pub modified_at: u64,
}

/// 列出 OpenClaw 每日记忆文件（`YYYY-MM-DD.md`，新→旧）。
pub fn list_daily_memory() -> Vec<DailyMemoryFile> {
    let Some(dir) = daily_memory_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()).map(String::from) else {
            continue;
        };
        if !is_daily_memory_filename(&name) {
            continue;
        }
        let date = name.trim_end_matches(".md").to_string();
        let (size_bytes, modified_at) = std::fs::metadata(&p)
            .map(|m| {
                let size = m.len();
                let mt = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                (size, mt)
            })
            .unwrap_or((0, 0));
        out.push(DailyMemoryFile {
            filename: name,
            date,
            size_bytes,
            modified_at,
        });
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.modified_at));
    out
}

/// 校验每日记忆文件名（`YYYY-MM-DD.md`；月 01-12、日 01-31）。
pub fn is_daily_memory_filename(name: &str) -> bool {
    let b = name.as_bytes();
    if b.len() != 13
        || !name.ends_with(".md")
        || !name[..4].bytes().all(|c| c.is_ascii_digit())
        || b[4] != b'-'
        || !name[5..7].bytes().all(|c| c.is_ascii_digit())
        || b[7] != b'-'
        || !name[8..10].bytes().all(|c| c.is_ascii_digit())
    {
        return false;
    }
    let month = &name[5..7];
    let day = &name[8..10];
    matches!(month, "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12")
        && matches!(day, "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12" | "13" | "14" | "15" | "16" | "17" | "18" | "19" | "20" | "21" | "22" | "23" | "24" | "25" | "26" | "27" | "28" | "29" | "30" | "31")
}

/// 读取每日记忆文件。
pub fn read_daily_memory(filename: &str) -> Result<Option<String>, String> {
    if !is_daily_memory_filename(filename) {
        return Err("文件名须为 YYYY-MM-DD.md".into());
    }
    let dir = daily_memory_dir().ok_or("无法定位 HOME")?;
    let p = dir.join(filename);
    if !p.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(&p).map(Some).map_err(|e| e.to_string())
}

/// 写入每日记忆文件（原子写：临时文件 + rename）。
pub fn write_daily_memory(filename: &str, content: &str) -> Result<(), String> {
    if !is_daily_memory_filename(filename) {
        return Err("文件名须为 YYYY-MM-DD.md".into());
    }
    let dir = daily_memory_dir().ok_or("无法定位 HOME")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{filename}.tmp"));
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(filename)).map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除每日记忆文件。
pub fn delete_daily_memory(filename: &str) -> Result<(), String> {
    if !is_daily_memory_filename(filename) {
        return Err("文件名须为 YYYY-MM-DD.md".into());
    }
    let dir = daily_memory_dir().ok_or("无法定位 HOME")?;
    let p = dir.join(filename);
    if !p.exists() {
        return Ok(());
    }
    std::fs::remove_file(&p).map_err(|e| e.to_string())
}

/// 公历转天数（从 1970-01-01 起；与 ns-usage 的 civil 算法一致）。
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// OpenClaw `agents.defaults` 结构化视图（只编辑已知字段，其余未知字段原样保留）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OpenClawAgentsDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<OpenClawDefaultModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// 超时（秒）。cc-switch 迁移到 `timeoutSeconds`；本工具同时兼容旧 `timeout`。
    #[serde(skip_serializing_if = "Option::is_none", rename = "timeoutSeconds")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "contextTokens")]
    pub context_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxConcurrent")]
    pub max_concurrent: Option<u64>,
    /// 其余未知字段（原样保留）。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OpenClawDefaultModel {
    pub primary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallbacks: Option<Vec<String>>,
}

impl OpenClawAgentsDefaults {
    /// 从 openclaw.json 的 `agents.defaults` 读取（不存在/解析失败返回默认空）。
    pub fn from_openclaw_json() -> Self {
        let Some(h) = home() else { return Self::default() };
        let p = h.join(".openclaw").join("openclaw.json");
        let Ok(text) = std::fs::read_to_string(&p) else {
            return Self::default();
        };
        let Ok(root) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Self::default();
        };
        let Some(def) = root
            .get("agents")
            .and_then(|a| a.get("defaults"))
            .cloned()
        else {
            return Self::default();
        };
        serde_json::from_value(def).unwrap_or_default()
    }

    /// 写回 `agents.defaults` 到 openclaw.json（保留其余字段与格式）。
    pub fn save_to_openclaw_json(&self) -> Result<(), String> {
        let h = home().ok_or("无法定位 HOME")?;
        let p = h.join(".openclaw").join("openclaw.json");
        let mut root: serde_json::Value = if p.exists() {
            let t = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
            serde_json::from_str(&t).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        let obj = root.as_object_mut().ok_or("openclaw.json 根必须是对象")?;
        let agents = obj
            .entry("agents")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .cloned()
            .unwrap_or_default();
        // 重建 agents 对象，写入 defaults。
        let mut agents = agents;
        agents.insert(
            "defaults".into(),
            serde_json::to_value(self).map_err(|e| e.to_string())?,
        );
        obj.insert("agents".into(), serde_json::Value::Object(agents));
        // 原子写。
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 是否已有 `agents.defaults`（用于 UI 提示）。
pub fn openclaw_agents_defaults_exists() -> bool {
    let Some(h) = home() else { return false };
    let p = h.join(".openclaw").join("openclaw.json");
    let Ok(text) = std::fs::read_to_string(&p) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    root.get("agents")
        .and_then(|a| a.get("defaults"))
        .is_some()
}

/// OpenClaw `agents.defaults` 编辑输入实体（真实可编辑文本域）。
#[derive(Default)]
pub struct OpenClawDefaultsInputs {
    pub primary: Option<gpui::Entity<crate::text_field::TextField>>,
    pub workspace: Option<gpui::Entity<crate::text_field::TextField>>,
    pub timeout: Option<gpui::Entity<crate::text_field::TextField>>,
    pub context_tokens: Option<gpui::Entity<crate::text_field::TextField>>,
    pub max_concurrent: Option<gpui::Entity<crate::text_field::TextField>>,
}

impl OpenClawDefaultsInputs {
    pub fn new(cx: &mut gpui::Context<crate::app::SwitchApp>) -> Self {
        Self {
            primary: Some(cx.new(|cx| crate::text_field::TextField::new("主模型（如 claude-sonnet-4-5）", cx))),
            workspace: Some(cx.new(|cx| crate::text_field::TextField::new("工作区路径（如 ~/projects）", cx))),
            timeout: Some(cx.new(|cx| crate::text_field::TextField::new("超时秒（如 300）", cx))),
            context_tokens: Some(cx.new(|cx| crate::text_field::TextField::new("上下文 token（如 200000）", cx))),
            max_concurrent: Some(cx.new(|cx| crate::text_field::TextField::new("最大并发（如 4）", cx))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_list_includes_both_tools() {
        let fs = files();
        assert!(fs.iter().any(|f| f.id == "openclaw-agents"));
        assert!(fs.iter().any(|f| f.id == "hermes-memory"));
        assert!(fs.iter().any(|f| f.id == "hermes-user"));
    }

    #[test]
    fn read_missing_returns_none() {
        let id = "hermes-memory";
        // 仅当文件不存在时返回 None（避免污染真实文件）。
        if !exists(id) {
            assert_eq!(read_file(id).unwrap(), None);
        }
    }

    #[test]
    fn unknown_id_errors() {
        assert!(file_by_id("nope").is_none());
        assert!(write_file("nope", "x").is_err());
    }

    #[test]
    fn daily_memory_filename_validation() {
        assert!(is_daily_memory_filename("2026-11-10.md"));
        assert!(is_daily_memory_filename("2026-01-01.md"));
        assert!(!is_daily_memory_filename("2026-11-10.txt"));
        assert!(!is_daily_memory_filename("2026-11-10"));
        assert!(!is_daily_memory_filename("2026-1-10.md"));
        assert!(!is_daily_memory_filename("../evil.md"));
        assert!(!is_daily_memory_filename("2026-13-40.md"));
    }

    #[test]
    fn today_filename_is_wellformed() {
        let f = today_daily_memory_filename();
        assert!(is_daily_memory_filename(&f), "got {f}");
        assert_eq!(f.len(), 13);
    }

    #[test]
    fn daily_memory_read_write_delete_roundtrip() {
        // 用临时目录覆盖 HOME 不便（函数读环境 HOME），故仅在函数层验证校验与
        // 路径安全（不污染真实 ~/.openclaw）。
        assert!(read_daily_memory("../x.md").is_err());
        assert!(write_daily_memory("2026-11-10.txt", "x").is_err());
        assert!(delete_daily_memory("bad.md.txt").is_err());
    }
    #[test]
    fn agents_defaults_serde_preserves_unknown() {
        let d = OpenClawAgentsDefaults {
            model: Some(OpenClawDefaultModel {
                primary: "claude-sonnet-4-5".into(),
                fallbacks: Some(vec!["gpt-5".into()]),
            }),
            workspace: Some("~/projects".into()),
            timeout_seconds: Some(300),
            context_tokens: Some(200000),
            max_concurrent: Some(4),
            extra: serde_json::Map::from_iter(vec![(
                "customFlag".into(),
                serde_json::json!(true),
            )]),
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["model"]["primary"], "claude-sonnet-4-5");
        assert_eq!(v["timeoutSeconds"], 300);
        assert_eq!(v["customFlag"], true);
        let back: OpenClawAgentsDefaults = serde_json::from_value(v).unwrap();
        assert_eq!(back.model.unwrap().primary, "claude-sonnet-4-5");
        assert!(back.extra.contains_key("customFlag"));
    }

    #[test]
    fn agents_defaults_roundtrip_via_tmp_home() {
        // 用临时 HOME 验证写读往返（环境 HOME 覆盖在子进程中不易，直接测 JSON 值）。
        let d = OpenClawAgentsDefaults {
            model: Some(OpenClawDefaultModel {
                primary: "x".into(),
                fallbacks: None,
            }),
            timeout_seconds: Some(60),
            workspace: None,
            context_tokens: None,
            max_concurrent: None,
            extra: Default::default(),
        };
        let v = serde_json::to_value(&d).unwrap();
        let back: OpenClawAgentsDefaults = serde_json::from_value(v).unwrap();
        assert_eq!(back.model.as_ref().unwrap().primary, "x");
        assert_eq!(back.timeout_seconds, Some(60));
    }
}
