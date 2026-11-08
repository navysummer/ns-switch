//! `.env` 补丁器（Gemini 等以环境变量文件存密钥）。
//!
//! 逐行保序：保留注释、空行与键序；只改写点名键的值，删除命中键（含前导注释可选）。
use std::path::Path;

use super::{LivePatch, LiveWriteError, decode_utf8};
#[derive(Clone, Default)]
pub struct EnvPatch {
    /// 删除这些键。
    pub remove: Vec<String>,
    /// 写入这些键（已存在原位替换，不存在追加到末尾）。
    pub set: Vec<(String, String)>,
}

impl LivePatch for EnvPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        let text = match pre {
            None => String::new(),
            Some(bytes) => decode_utf8(path, bytes)?,
        };
        let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        // 先写入（原位替换，保留键序）；不存在则追加到末尾
        for (key, value) in &self.set {
            let mut replaced = false;
            for line in lines.iter_mut() {
                if line_key(line.trim()) == Some(key.as_str()) {
                    *line = format!("{key}={value}");
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                lines.push(format!("{key}={value}"));
            }
        }

        // 再删除不在 set 里的点名键（保留注释——注释归用户）
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            if self.remove.iter().any(|k| {
                !self.set.iter().any(|(sk, _)| sk == k) && line_key(line) == Some(k.as_str())
            }) {
                lines.remove(i);
            } else {
                i += 1;
            }
        }

        let mut out = lines.join("\n");
        if text.ends_with('\n') || text.is_empty() {
            out.push('\n');
        }
        Ok(out.into_bytes())
    }
}

/// 提取行首键名（忽略 `export ` 前缀）。
fn line_key(line: &str) -> Option<&str> {
    let line = line.strip_prefix("export ").unwrap_or(line);
    let eq = line.find('=')?;
    let key = line[..eq].trim();
    if key.is_empty() || key.starts_with('#') {
        return None;
    }
    Some(key)
}

/// 从 JSON 值构造 `.env` 字符串（用于 `set`）。
pub fn env_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => value.to_string(),
    }
}


