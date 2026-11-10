//! 会话日志扫描：从工具本地会话文件统计按模型请求/token。
//!
//! Claude Code 会话在 `~/.claude/projects/<proj>/<session>.jsonl`，每行一个 JSON 事件；
//! assistant 消息含 `model` 与 `usage{input_tokens, output_tokens, cache_read_input_tokens}`。

use std::collections::BTreeMap;
use std::path::Path;

/// 按模型的会话统计。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ModelStats {
    pub model: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    /// 估算花费（USD；价格未知时 `None`）。
    pub cost_usd: Option<f64>,
}

/// 扫描 Claude Code 会话目录，统计按模型。
/// `limit` = 最多扫描的会话文件（按 mtime 最新；0 = 全部）。
pub fn scan_claude_sessions(home: &Path, limit: usize) -> Vec<ModelStats> {
    let projects = home.join(".claude").join("projects");
    let Ok(entries) = std::fs::read_dir(&projects) else {
        return Vec::new();
    };
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let Ok(sub) = std::fs::read_dir(&dir) else {
            continue;
        };
        for f in sub.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                files.push(p);
            }
        }
    }
    // 按 mtime 最新优先，取前 limit 个。
    files.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
            .unwrap_or(0)
    });
    files.reverse();
    if limit > 0 && files.len() > limit {
        files.truncate(limit);
    }
    let mut stats: BTreeMap<String, ModelStats> = BTreeMap::new();
    for file in files {
        scan_claude_file(&file, &mut stats);
    }
    stats
        .into_values()
        .map(|mut s| {
            s.cost_usd = crate::pricing::cost(
                &s.model,
                s.input_tokens,
                s.output_tokens,
                s.cache_read_tokens,
            );
            s
        })
        .collect()
}
/// 单日请求统计（用于趋势图/热力图）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DailyCount {
    /// 日期 YYYY-MM-DD。
    pub date: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// 单条请求明细。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RequestDetail {
    pub ts_ms: u64,
    /// 来源工具（claude / codex / gemini）。
    pub tool: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// 扫描 Claude Code 会话，返回按日统计（`days` = 最近 N 天；0 = 全部）。
pub fn scan_claude_daily(home: &Path, days: usize) -> Vec<DailyCount> {
    let mut details = Vec::new();
    collect_claude_details(home, &mut details);
    let mut by_date: BTreeMap<String, DailyCount> = BTreeMap::new();
    for d in &details {
        let date = date_str(d.ts_ms);
        let e = by_date.entry(date.clone()).or_insert_with(|| DailyCount {
            date,
            ..Default::default()
        });
        e.requests += 1;
        e.input_tokens += d.input_tokens;
        e.output_tokens += d.output_tokens;
    }
    let mut out: Vec<DailyCount> = by_date.into_values().collect();
    if days > 0 && out.len() > days {
        out = out.split_off(out.len() - days);
    }
    out
}

/// 扫描 Claude Code 会话，返回请求明细（新→旧）。
pub fn scan_claude_details(home: &Path) -> Vec<RequestDetail> {
    let mut details = Vec::new();
    collect_claude_details(home, &mut details);
    details.sort_by_key(|d| std::cmp::Reverse(d.ts_ms));
    details
}

fn collect_claude_details(home: &Path, out: &mut Vec<RequestDetail>) {
    let projects = home.join(".claude").join("projects");
    let Ok(entries) = std::fs::read_dir(&projects) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let Ok(sub) = std::fs::read_dir(&dir) else {
            continue;
        };
        for f in sub.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                collect_claude_file(&p, out);
            }
        }
    }
}

fn collect_claude_file(path: &Path, out: &mut Vec<RequestDetail>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(msg) = v.get("message").and_then(|m| m.as_object()) else {
            continue;
        };
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let Some(model) = msg.get("model").and_then(|m| m.as_str()) else {
            continue;
        };
        let ts_ms = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_iso_ms)
            .unwrap_or(0);
        let num = |k: &str| -> u64 {
            msg.get("usage")
                .and_then(|u| u.get(k))
                .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)))
                .unwrap_or(0)
        };
        out.push(RequestDetail {
            ts_ms,
            tool: "claude".into(),
            model: model.to_string(),
            input_tokens: num("input_tokens"),
            output_tokens: num("output_tokens"),
        });
    }
}

/// 解析 ISO 8601 时间戳为 Unix 毫秒。
fn parse_iso_ms(s: &str) -> Option<u64> {
    // 期望 "2026-11-08T10:30:00.123Z"。
    let t = s.strip_suffix('Z').unwrap_or(s);
    let mut it = t.split(['T', '-', ':', '.']);
    let y: i64 = it.next()?.parse().ok()?;
    let mo: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    let h: i64 = it.next().unwrap_or("0").parse().ok()?;
    let mi: i64 = it.next().unwrap_or("0").parse().ok()?;
    let se: i64 = it.next().unwrap_or("0").parse().ok()?;
    let ms: i64 = it.next().unwrap_or("0").parse().ok()?;
    let days = days_from_civil(y, mo, d);
    let secs = days * 86400 + h * 3600 + mi * 60 + se;
    Some((secs as u64) * 1000 + ms as u64)
}

/// 日期（Unix 毫秒）→ "YYYY-MM-DD"。
fn date_str(ts_ms: u64) -> String {
    let secs = (ts_ms / 1000) as i64;
    let days = secs.div_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
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


fn scan_claude_file(path: &Path, stats: &mut BTreeMap<String, ModelStats>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(msg) = v.get("message").and_then(|m| m.as_object()) else {
            continue;
        };
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let Some(model) = msg.get("model").and_then(|m| m.as_str()) else {
            continue;
        };
        let Some(usage) = msg.get("usage").and_then(|u| u.as_object()) else {
            continue;
        };
        let num = |k: &str| -> u64 {
            usage
                .get(k)
                .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)))
                .unwrap_or(0)
        };
        let s = stats.entry(model.to_string()).or_insert_with(|| ModelStats {
            model: model.to_string(),
            ..Default::default()
        });
        s.requests += 1;
        s.input_tokens += num("input_tokens");
        s.output_tokens += num("output_tokens");
        s.cache_read_tokens += num("cache_read_input_tokens");
    }
}

/// 扫描 Codex 会话目录（`~/.codex/sessions/**/rollout-*.jsonl`），统计按模型。
/// `limit` = 最多扫描的会话文件（按 mtime 最新；0 = 全部）。
pub fn scan_codex_sessions(home: &Path, limit: usize) -> Vec<ModelStats> {
    let mut details = Vec::new();
    collect_codex_details(home, &mut details);
    if limit > 0 && details.len() > limit {
        // 保留最新 `limit` 条（明细已按时间排序由调用方处理；此处按 ts 截断）。
        details.sort_by_key(|d| std::cmp::Reverse(d.ts_ms));
        details.truncate(limit);
    }
    let mut stats: BTreeMap<String, ModelStats> = BTreeMap::new();
    for d in details {
        let e = stats.entry(d.model.clone()).or_insert(ModelStats {
            model: d.model.clone(),
            requests: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cost_usd: None,
        });
        e.requests += 1;
        e.input_tokens += d.input_tokens;
        e.output_tokens += d.output_tokens;
    }
    stats
        .into_values()
        .map(|mut s| {
            s.cost_usd = crate::pricing::cost(
                &s.model,
                s.input_tokens,
                s.output_tokens,
                s.cache_read_tokens,
            );
            s
        })
        .collect()
}

/// 扫描 Codex 会话，返回按日统计（`days` = 最近 N 天；0 = 全部）。
pub fn scan_codex_daily(home: &Path, days: usize) -> Vec<DailyCount> {
    let mut details = Vec::new();
    collect_codex_details(home, &mut details);
    let mut by_date: BTreeMap<String, DailyCount> = BTreeMap::new();
    for d in &details {
        let date = date_str(d.ts_ms);
        let e = by_date.entry(date.clone()).or_insert_with(|| DailyCount {
            date,
            ..Default::default()
        });
        e.requests += 1;
        e.input_tokens += d.input_tokens;
        e.output_tokens += d.output_tokens;
    }
    let mut out: Vec<DailyCount> = by_date.into_values().collect();
    if days > 0 && out.len() > days {
        out = out.split_off(out.len() - days);
    }
    out
}

/// 扫描 Codex 会话，返回请求明细（新→旧）。
pub fn scan_codex_details(home: &Path) -> Vec<RequestDetail> {
    let mut details = Vec::new();
    collect_codex_details(home, &mut details);
    details.sort_by_key(|d| std::cmp::Reverse(d.ts_ms));
    details
}

/// 递归收集 Codex rollout 的请求明细（assistant message 含 usage）。
fn collect_codex_details(home: &Path, out: &mut Vec<RequestDetail>) {
    let sessions = home.join(".codex").join("sessions");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_jsonl_files(&sessions, &mut files);
    for file in files {
        collect_codex_file(&file, out);
    }
}

fn collect_codex_file(path: &Path, out: &mut Vec<RequestDetail>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    // 先提取会话级 model（world_state.collaboration_mode.model；Codex 每请求无 model 字段）。
    let mut session_model: Option<String> = None;
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) == Some("world_state") {
            if let Some(m) = v
                .get("payload")
                .and_then(|p| p.get("state"))
                .and_then(|s| s.get("collaboration_mode"))
                .and_then(|c| c.get("model"))
                .and_then(|m| m.as_str())
            {
                session_model = Some(m.to_string());
            }
        }
    }
    let Some(model) = session_model else {
        return;
    };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("token_usage_record") {
            continue;
        }
        let Some(usage) = v
            .get("payload")
            .and_then(|p| p.get("usage"))
            .and_then(|u| u.as_object())
        else {
            continue;
        };
        let ts_ms = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_iso_ms)
            .unwrap_or(0);
        let num = |k: &str| -> u64 {
            usage
                .get(k)
                .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)))
                .unwrap_or(0)
        };
        let input = num("input_tokens");
        let output = num("output_tokens");
        if input == 0 && output == 0 {
            continue;
        }
        out.push(RequestDetail {
            ts_ms,
            tool: "codex".into(),
            model: model.clone(),
            input_tokens: input,
            output_tokens: output,
        });
    }
}

/// 递归收集目录下所有 `.jsonl` 文件。
fn collect_jsonl_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_jsonl_files(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(p);
        }
    }
}


/// 扫描 Gemini CLI 会话日志（`~/.gemini/tmp/<project>/chats/session-*.jsonl`），
/// 统计 `type == "gemini"` 记录的 tokens + model。
/// `limit` = 最多扫描的会话文件（按 mtime 最新；0 = 全部）。
pub fn scan_gemini_sessions(home: &Path, limit: usize) -> Vec<ModelStats> {
    let chats = home.join(".gemini").join("tmp");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_jsonl_files(&chats, &mut files);
    // 只取 `chats/session-*.jsonl` 会话转录。
    files.retain(|p| {
        p.parent()
            .and_then(|d| d.file_name())
            .map(|n| n == "chats")
            .unwrap_or(false)
            && p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("session-"))
                .unwrap_or(false)
    });
    files.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
            .unwrap_or(0)
    });
    files.reverse();
    if limit > 0 && files.len() > limit {
        files.truncate(limit);
    }
    let mut stats: BTreeMap<String, ModelStats> = BTreeMap::new();
    for file in files {
        scan_gemini_file(&file, &mut stats);
    }
    stats
        .into_values()
        .map(|mut s| {
            s.cost_usd = crate::pricing::cost(
                &s.model,
                s.input_tokens,
                s.output_tokens,
                s.cache_read_tokens,
            );
            s
        })
        .collect()
}

/// 扫描 Gemini CLI 会话日志，返回按日统计（`days` = 最近 N 天；0 = 全部）。
///
/// Gemini 记录无独立时间戳，用会话文件 mtime 归日（近似，趋势图/热力图用）。
pub fn scan_gemini_daily(home: &Path, days: usize) -> Vec<DailyCount> {
    let mut details = Vec::new();
    collect_gemini_details(home, &mut details);
    let mut by_date: BTreeMap<String, DailyCount> = BTreeMap::new();
    for d in &details {
        let date = date_str(d.ts_ms);
        let e = by_date.entry(date.clone()).or_insert_with(|| DailyCount {
            date,
            ..Default::default()
        });
        e.requests += 1;
        e.input_tokens += d.input_tokens;
        e.output_tokens += d.output_tokens;
    }
    let mut out: Vec<DailyCount> = by_date.into_values().collect();
    if days > 0 && out.len() > days {
        out = out.split_off(out.len() - days);
    }
    out
}

/// 扫描 Gemini CLI 会话日志，返回请求明细（新→旧；0 = 全部）。
pub fn scan_gemini_details(home: &Path) -> Vec<RequestDetail> {
    let mut details = Vec::new();
    collect_gemini_details(home, &mut details);
    details.sort_by_key(|d| std::cmp::Reverse(d.ts_ms));
    details
}

/// 收集 Gemini 会话请求明细（每条 `type=="gemini"` 记录 = 一次请求）。
fn collect_gemini_details(home: &Path, out: &mut Vec<RequestDetail>) {
    let chats = home.join(".gemini").join("tmp");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_jsonl_files(&chats, &mut files);
    files.retain(|p| {
        p.parent()
            .and_then(|d| d.file_name())
            .map(|n| n == "chats")
            .unwrap_or(false)
            && p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("session-"))
                .unwrap_or(false)
    });
    for file in files {
        let mtime_ms = std::fs::metadata(&file)
            .and_then(|m| m.modified())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in content.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v.get("type").and_then(|t| t.as_str()) != Some("gemini") {
                continue;
            }
            let Some(tokens) = v.get("tokens").and_then(|t| t.as_object()) else {
                continue;
            };
            let num = |k: &str| -> u64 {
                tokens
                    .get(k)
                    .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)))
                    .unwrap_or(0)
            };
            if num("input") == 0 && num("output") == 0 {
                continue;
            }
            let model = v
                .get("model")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            out.push(RequestDetail {
                ts_ms: mtime_ms,
                tool: "gemini".into(),
                model,
                input_tokens: num("input"),
                output_tokens: num("output"),
            });
        }
    }
}

/// 解析单个 Gemini 会话 jsonl：`type == "gemini"` 记录含 `tokens` 与 `model`。
fn scan_gemini_file(path: &Path, stats: &mut BTreeMap<String, ModelStats>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("gemini") {
            continue;
        }
        let model = v
            .get("model")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let Some(tokens) = v.get("tokens").and_then(|t| t.as_object()) else {
            continue;
        };
        let num = |k: &str| -> u64 {
            tokens
                .get(k)
                .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)))
                .unwrap_or(0)
        };
        let s = stats.entry(model.clone()).or_insert_with(|| ModelStats {
            model,
            ..Default::default()
        });
        s.requests += 1;
        s.input_tokens += num("input");
        s.output_tokens += num("output");
        s.cache_read_tokens += num("cached");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_accumulate_per_model() {
        let mut stats: BTreeMap<String, ModelStats> = BTreeMap::new();
        let dir = std::env::temp_dir().join("ns-usage-test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("s.jsonl");
        std::fs::write(
            &f,
            r#"{"message":{"role":"assistant","model":"m1","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5}}}
{"message":{"role":"assistant","model":"m1","usage":{"input_tokens":5,"output_tokens":7,"cache_read_input_tokens":0}}}
{"message":{"role":"assistant","model":"m2","usage":{"input_tokens":100,"output_tokens":1,"cache_read_input_tokens":0}}}
{"message":{"role":"user","content":"hi"}}
"#,
        )
        .unwrap();
        scan_claude_file(&f, &mut stats);
        let m1 = stats.get("m1").unwrap();
        assert_eq!(m1.requests, 2);
        assert_eq!(m1.input_tokens, 15);
        assert_eq!(m1.output_tokens, 27);
        assert_eq!(m1.cache_read_tokens, 5);
        let m2 = stats.get("m2").unwrap();
        assert_eq!(m2.requests, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stats_accumulate_codex_per_model() {
        let dir = std::env::temp_dir().join("ns-usage-codex-test");
        let sdir = dir.join(".codex").join("sessions").join("2026").join("10");
        std::fs::create_dir_all(&sdir).unwrap();
        let f = sdir.join("rollout-test.jsonl");
        std::fs::write(
            &f,
            r#"{"type":"world_state","payload":{"state":{"collaboration_mode":{"model":"ccs-sensenova/sensenova-6.8-flash-lite"}}}}
{"type":"token_usage_record","timestamp":"2026-10-27T07:58:11.191Z","payload":{"usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"token_usage_record","timestamp":"2026-10-27T07:59:11.191Z","payload":{"usage":{"input_tokens":20,"output_tokens":10}}}
"#,
        )
        .unwrap();
        let stats = scan_codex_sessions(&dir, 0);
        assert_eq!(stats.len(), 1);
        let m1 = &stats[0];
        assert_eq!(m1.model, "ccs-sensenova/sensenova-6.8-flash-lite");
        assert_eq!(m1.requests, 2);
        assert_eq!(m1.input_tokens, 120);
        assert_eq!(m1.output_tokens, 60);
        let _ = std::fs::remove_dir_all(&dir);
    }


    #[test]
    fn stats_accumulate_gemini_per_model() {
        let mut stats: BTreeMap<String, ModelStats> = BTreeMap::new();
        let dir = std::env::temp_dir().join("ns-usage-gemini-test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("session-2026-05-03T15-01-test.jsonl");
        std::fs::write(
            &f,
            r#"{"sessionId":"s1","projectHash":"h1","kind":"main"}
{"type":"user","content":[{"text":"hi"}]}
{"type":"gemini","model":"gemini-3-flash-preview","tokens":{"input":11410,"output":105,"cached":0,"thoughts":65,"tool":0,"total":11580}}
{"type":"gemini","model":"gemini-3-flash-preview","tokens":{"input":500,"output":50,"cached":10,"thoughts":0,"tool":0,"total":560}}
{"type":"gemini","model":"gemini-2.5-pro","tokens":{"input":1000,"output":200,"cached":100,"thoughts":0,"tool":0,"total":1300}}
"#,
        )
        .unwrap();
        scan_gemini_file(&f, &mut stats);
        let m1 = stats.get("gemini-3-flash-preview").unwrap();
        assert_eq!(m1.requests, 2);
        assert_eq!(m1.input_tokens, 11910);
        assert_eq!(m1.output_tokens, 155);
        assert_eq!(m1.cache_read_tokens, 10);
        let m2 = stats.get("gemini-2.5-pro").unwrap();
        assert_eq!(m2.requests, 1);
        assert_eq!(m2.input_tokens, 1000);
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn gemini_daily_and_details_collect() {
        let dir = std::env::temp_dir().join("ns-usage-gemini-dd-test");
        let chats = dir.join(".gemini").join("tmp").join("proj").join("chats");
        let _ = std::fs::create_dir_all(&chats);
        let f = chats.join("session-2026-05-03T15-01-test.jsonl");
        std::fs::write(
            &f,
            r#"{"type":"user","content":[{"text":"hi"}]}
{"type":"gemini","model":"gemini-3-flash-preview","tokens":{"input":11410,"output":105,"cached":0,"thoughts":65,"tool":0,"total":11580}}
{"type":"gemini","model":"gemini-3-flash-preview","tokens":{"input":500,"output":50,"cached":10,"thoughts":0,"tool":0,"total":560}}
{"type":"gemini","model":"gemini-2.5-pro","tokens":{"input":1000,"output":200,"cached":100,"thoughts":0,"tool":0,"total":1300}}
"#,
        )
        .unwrap();
        let daily = scan_gemini_daily(&dir, 0);
        assert_eq!(daily.len(), 1);
        assert_eq!(daily[0].requests, 3);
        assert_eq!(daily[0].input_tokens, 12910);
        assert_eq!(daily[0].output_tokens, 355);
        let details = scan_gemini_details(&dir);
        assert_eq!(details.len(), 3);
        // 两条 flash 一条 pro；明细按 mtime 同文件，顺序保留新→旧。
        assert!(details.iter().any(|d| d.model == "gemini-2.5-pro"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}




#[cfg(test)]
mod tool_tag_tests {
    use super::*;

    #[test]
    fn request_detail_tags_source_tool() {
        // Claude 扫描：创建临时 .claude/projects/<p>/<f>.jsonl。
        let dir = std::env::temp_dir().join("ns-usage-tooltag-test");
        let proj = dir.join(".claude").join("projects").join("p");
        let _ = std::fs::create_dir_all(&proj);
        let f = proj.join("s.jsonl");
        std::fs::write(
            &f,
            r#"{"timestamp":"2026-05-03T15:00:00Z","message":{"role":"assistant","model":"claude-sonnet-4-5","usage":{"input_tokens":10,"output_tokens":5}}}
"#,
        )
        .unwrap();
        let claude = scan_claude_details(&dir);
        assert_eq!(claude.len(), 1);
        assert_eq!(claude[0].tool, "claude");

        // Gemini 扫描：临时 .gemini/tmp/**/chats。
        let chats = dir.join(".gemini").join("tmp").join("p").join("chats");
        let _ = std::fs::create_dir_all(&chats);
        let g = chats.join("session-a.jsonl");
        std::fs::write(
            &g,
            r#"{"type":"gemini","model":"gemini-3-flash-preview","tokens":{"input":1,"output":1}}
"#,
        )
        .unwrap();
        let gem = scan_gemini_details(&dir);
        assert_eq!(gem.len(), 1);
        assert_eq!(gem[0].tool, "gemini");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
