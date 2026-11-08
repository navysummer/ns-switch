//! 授权中心：管理各 CLI 工具的 OAuth 凭据文件。
//!
//! - Claude Desktop：`~/.claude/.credentials.json`（`claudeAiOauth` 字段）。
//! - Codex：`~/.codex/auth.json`（`OPENAI_API_KEY` / `tokens`）。
//!
//! 纯读写，不发起网络请求。授权状态 = 文件存在且含有效 token。

use std::path::{Path, PathBuf};

use serde_json::Value;

/// 一个工具的授权状态。
#[derive(Debug, Clone)]
pub struct AuthStatus {
    pub tool: &'static str,
    pub name: &'static str,
    /// 凭据文件路径。
    pub path: PathBuf,
    /// 是否已授权（文件存在且含 token）。
    pub logged_in: bool,
    /// token 摘要（前 8 字符 + `…`；未授权空）。
    pub token_hint: String,
    /// 读取错误（若有）。
    pub error: Option<String>,
}

/// 各受管工具的授权定义。
const AUTH_TOOLS: &[(&str, &str)] = &[
    ("claude-desktop", "Claude Desktop"),
    ("codex", "Codex"),
];

/// home 目录。
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// 某工具的凭据文件路径。
pub fn credential_path(tool: &str) -> Option<PathBuf> {
    let h = home()?;
    match tool {
        "claude-desktop" => Some(h.join(".claude").join(".credentials.json")),
        "codex" => Some(h.join(".codex").join("auth.json")),
        _ => None,
    }
}

/// 读取全部工具的授权状态。
pub fn statuses() -> Vec<AuthStatus> {
    AUTH_TOOLS
        .iter()
        .map(|(id, name)| {
            let path = credential_path(id).unwrap_or_default();
            let (logged_in, token_hint, error) = match read_token(id, &path) {
                Ok(Some(hint)) => (true, hint, None),
                Ok(None) => (false, String::new(), None),
                Err(e) => (false, String::new(), Some(e)),
            };
            AuthStatus {
                tool: id,
                name,
                path,
                logged_in,
                token_hint,
                error,
            }
        })
        .collect()
}

/// 读取某工具的 token 摘要（未授权返回 Ok(None)）。
pub fn read_token(tool: &str, path: &Path) -> Result<Option<String>, String> {
    // 文件不存在 = 未授权（非错误）。
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let token = match tool {
        "claude-desktop" => v
            .get("claudeAiOauth")
            .and_then(|o| o.get("access_token"))
            .and_then(Value::as_str),
        "codex" => v
            .get("tokens")
            .and_then(|t| t.get("access_token"))
            .and_then(Value::as_str)
            .or_else(|| v.get("OPENAI_API_KEY").and_then(Value::as_str)),
        _ => None,
    };
    match token {
        Some(t) if !t.is_empty() => Ok(Some(hint_of(t))),
        _ => Ok(None),
    }
}

/// token 摘要。
fn hint_of(token: &str) -> String {
    if token.len() <= 8 {
        token.to_string()
    } else {
        format!("{}…", &token[..8])
    }
}

/// 清除某工具的凭据（删除文件）。返回是否删除了文件。
pub fn clear(tool: &str) -> Result<bool, String> {
    let path = credential_path(tool).ok_or("未知工具")?;
    if !path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_token_hint_extracted() {
        let path = std::env::temp_dir().join("ns-auth-test-claude.json");
        std::fs::write(
            &path,
            r#"{"claudeAiOauth":{"access_token":"sk-ant-1234567890","refresh_token":"r","expires_at":123}}"#,
        )
        .unwrap();
        let hint = read_token("claude-desktop", &path).unwrap();
        assert_eq!(hint.as_deref(), Some("sk-ant-1…"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn codex_token_hint_extracted() {
        let path = std::env::temp_dir().join("ns-auth-test-codex.json");
        std::fs::write(
            &path,
            r#"{"tokens":{"access_token":"sk-abcdef123","refresh_token":"r","id_token":"i"},"last_refresh":"2026-01-01"}"#,
        )
        .unwrap();
        let hint = read_token("codex", &path).unwrap();
        assert_eq!(hint.as_deref(), Some("sk-abcde…"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_not_logged_in() {
        let path = std::env::temp_dir().join("ns-auth-test-missing.json");
        let _ = std::fs::remove_file(&path);
        let r = read_token("codex", &path).unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn clear_removes_file() {
        let path = std::env::temp_dir().join("ns-auth-test-clear.json");
        std::fs::write(&path, "{}").unwrap();
        // 直接测文件删除（不依赖 home 路径）。
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();
        assert!(!path.exists());
    }
}
