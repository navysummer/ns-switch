//! 请求日志：记录每次代理转发的请求（工具、请求模型、实际模型、状态、耗时）。
//!
//! 内存环形缓冲（有上限），供设置页/用量页展示。串行写入经 `Mutex`。

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 一条请求日志。
#[derive(Debug, Clone)]
pub struct RequestLogEntry {
    /// 工具 id（`claude`/`codex`/`gemini`…）。
    pub tool: String,
    /// 请求方指定的模型（Anthropic 请求的 `model`）。
    pub requested_model: String,
    /// 实际转发到的模型（上游响应或请求模型）。
    pub actual_model: String,
    /// HTTP 状态码。
    pub status: u16,
    /// 是否成功（2xx）。
    pub ok: bool,
    /// 耗时（毫秒）。
    pub elapsed_ms: u64,
    /// Unix 毫秒时间戳。
    pub ts_ms: u64,
}

/// 环形请求日志。
pub struct RequestLog {
    entries: Mutex<Vec<RequestLogEntry>>,
    cap: usize,
}

impl Default for RequestLog {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl RequestLog {
    pub fn new(cap: usize) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            cap: cap.max(1),
        }
    }

    /// 记录一次请求。
    pub fn record(
        &self,
        tool: &str,
        requested_model: &str,
        actual_model: &str,
        status: u16,
        elapsed: Duration,
    ) {
        let ts_ms = now_ms();
        let mut entries = self.entries.lock().unwrap();
        entries.push(RequestLogEntry {
            tool: tool.to_string(),
            requested_model: requested_model.to_string(),
            actual_model: actual_model.to_string(),
            status,
            ok: (200..300).contains(&status),
            elapsed_ms: elapsed.as_millis() as u64,
            ts_ms,
        });
        if entries.len() > self.cap {
            let excess = entries.len() - self.cap;
            entries.drain(0..excess);
        }
    }

    /// 当前日志副本（新→旧）。
    pub fn snapshot(&self) -> Vec<RequestLogEntry> {
        let mut entries = self.entries.lock().unwrap().clone();
        entries.reverse();
        entries
    }

    /// 按工具统计：请求数、成功数。
    pub fn summary_by_tool(&self) -> Vec<(String, usize, usize)> {
        use std::collections::BTreeMap;
        let mut map: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for e in self.entries.lock().unwrap().iter() {
            let entry = map.entry(e.tool.clone()).or_insert((0, 0));
            entry.0 += 1;
            if e.ok {
                entry.1 += 1;
            }
        }
        map.into_iter().map(|(t, (total, ok))| (t, total, ok)).collect()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 计时起点。
pub fn start() -> Instant {
    Instant::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_records_and_summarizes() {
        let log = RequestLog::new(10);
        log.record("claude", "claude-sonnet-4-5", "claude-sonnet-4-5", 200, Duration::from_millis(5));
        log.record("claude", "claude-opus-4-1", "claude-opus-4-1", 500, Duration::from_millis(3));
        log.record("codex", "gpt-5", "gpt-5", 200, Duration::from_millis(7));
        let snap = log.snapshot();
        assert_eq!(snap.len(), 3);
        assert!(snap[0].ok);
        assert_eq!(snap[0].requested_model, "gpt-5");
        assert!(!snap[1].ok);
        let summary = log.summary_by_tool();
        let claude = summary.iter().find(|(t, _, _)| t == "claude").unwrap();
        assert_eq!(claude.1, 2); // 请求数
        assert_eq!(claude.2, 1); // 成功数
        let codex = summary.iter().find(|(t, _, _)| t == "codex").unwrap();
        assert_eq!(codex.1, 1);
        assert_eq!(codex.2, 1);
    }

    #[test]
    fn log_caps_at_limit() {
        let log = RequestLog::new(3);
        for i in 0..5 {
            log.record("claude", &format!("m{i}"), &format!("m{i}"), 200, Duration::ZERO);
        }
        assert_eq!(log.snapshot().len(), 3);
    }
}
