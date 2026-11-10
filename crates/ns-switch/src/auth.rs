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
    ("copilot", "GitHub Copilot"),
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
        "copilot" => Some(h.join(".config").join("github-copilot").join("hosts.json")),
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
        // hosts.json: {"github.com":{"oauth_token":"gho_...","user":"..."}}
        "copilot" => v
            .get("github.com")
            .and_then(|h| h.get("oauth_token"))
            .and_then(Value::as_str),
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

// ==================== OAuth Device Code 流程（参考 cc-switch 逻辑） ====================
//
// 协议移植自 cc-switch `proxy/providers/{xai_oauth_auth,codex_oauth_auth}.rs` 的
// `start_device_flow`/`poll_for_token`。纯 curl 子进程实现（ns-switch 无 reqwest 依赖），
// 流程与 cc-switch 一致：POST device_authorization_endpoint → 展示 user_code →
// 轮询 token_endpoint（处理 authorization_pending / slow_down）。

/// 一个进行中的 device-code 登录会话。
#[derive(Debug, Clone)]
pub struct DeviceCodeSession {
    /// provider：`xai` / `codex`。
    pub provider: &'static str,
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    /// token 轮询端点（发现后固定）。
    token_endpoint: String,
    /// 轮询间隔控制（slow_down 时增大）。
    next_poll_at_ms: u64,
    current_interval: u64,
}

/// OAuth provider 常量（参考 cc-switch）。
const XAI_DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_DEVICE_URL: &str = "https://auth.openai.com/oauth/device/code";
const CODEX_VERIFY_URL: &str = "https://auth.openai.com/codex/device";
// GitHub Copilot（参考 cc-switch copilot_auth）。
const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const GITHUB_DEVICE_URL: &str = "https://github.com/login/device/code";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_VERIFY_URL: &str = "https://github.com/login/device";

/// 启动 device-code 登录：获取 device_code/user_code 并返回会话。
/// provider：`xai`（xAI SuperGrok）/ `codex`（ChatGPT Plus/Pro）/ `copilot`（GitHub Copilot）。
pub fn start_device_flow(provider: &str) -> Result<DeviceCodeSession, String> {
    match provider {
        "xai" => start_xai_flow(),
        "codex" => start_codex_flow(),
        "copilot" => start_copilot_flow(),
        _ => Err(format!("不支持的 OAuth provider: {provider}")),
    }
}

///

fn start_xai_flow() -> Result<DeviceCodeSession, String> {
    // 发现端点（OIDC discovery）。
    let disc = curl_json(XAI_DISCOVERY_URL)?;
    let token_endpoint = disc
        .get("token_endpoint")
        .and_then(|v| v.as_str())
        .ok_or("xAI discovery 缺少 token_endpoint")?
        .to_string();
    let device_endpoint = disc
        .get("device_authorization_endpoint")
        .and_then(|v| v.as_str())
        .ok_or("xAI discovery 缺少 device_authorization_endpoint")?
        .to_string();
    // POST device authorization（form：client_id + scope）。
    let form = format!("client_id={XAI_CLIENT_ID}&scope={}", urlencode(XAI_SCOPE));
    let body = curl_post_form(&device_endpoint, &form, "xai-oauth")?;
    build_session("xai", &body, token_endpoint)
}

fn start_codex_flow() -> Result<DeviceCodeSession, String> {
    // Codex device code：POST JSON {client_id} 到固定端点。
    let body = curl_post_json(
        CODEX_DEVICE_URL,
        &format!(r#"{{"client_id":"{CODEX_CLIENT_ID}"}}"#),
        "codex-oauth",
    )?;
    build_session("codex", &body, "https://auth.openai.com/oauth/token".to_string())
}

fn start_copilot_flow() -> Result<DeviceCodeSession, String> {
    // GitHub device code：POST form {client_id, scope} 到固定端点。
    let form = format!("client_id={GITHUB_CLIENT_ID}&scope={}", urlencode("read:user"));
    let body = curl_post_form(GITHUB_DEVICE_URL, &form, "copilot-oauth")?;
    build_session("copilot", &body, GITHUB_TOKEN_URL.to_string())
}

/// 从 device-code 响应构造会话。
fn build_session(
    provider: &'static str,
    body: &str,
    token_endpoint: String,
) -> Result<DeviceCodeSession, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("解析 device 响应失败: {e}"))?;
    let device_code = v
        .get("device_code")
        .or_else(|| v.get("device_auth_id"))
        .and_then(|x| x.as_str())
        .ok_or("device 响应缺少 device_code")?
        .to_string();
    let user_code = v
        .get("user_code")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let verification_uri = v
        .get("verification_uri_complete")
        .or_else(|| v.get("verification_uri"))
        .and_then(|x| x.as_str())
        .unwrap_or(match provider {
            "codex" => CODEX_VERIFY_URL,
            "copilot" => GITHUB_VERIFY_URL,
            _ => "https://auth.x.ai/device",
        })
        .to_string();
    let expires_in = v
        .get("expires_in")
        .and_then(|x| x.as_u64())
        .unwrap_or(600);
    let interval = v
        .get("interval")
        .and_then(|x| x.as_u64())
        .unwrap_or(5)
        .max(1);
    let now = now_ms();
    Ok(DeviceCodeSession {
        provider,
        device_code,
        user_code,
        verification_uri,
        expires_in,
        interval,
        token_endpoint,
        next_poll_at_ms: now,
        current_interval: interval,
    })
}

impl DeviceCodeSession {
    /// 轮询 token。返回 Ok(Some(access_token)) 表示登录成功；
    /// Ok(None) 表示仍在等待（authorization_pending / slow_down）。
    pub fn poll(&mut self) -> Result<Option<String>, String> {
        let now = now_ms();
        if self.next_poll_at_ms > now {
            return Ok(None); // 未到轮询间隔
        }
        self.next_poll_at_ms = now + self.current_interval * 1000;
        let form = match self.provider {
            "codex" => format!(
                "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id={CODEX_CLIENT_ID}&device_code={}",
                urlencode(&self.device_code)
            ),
            "copilot" => format!(
                "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id={GITHUB_CLIENT_ID}&device_code={}",
                urlencode(&self.device_code)
            ),
            _ => format!(
                "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id={XAI_CLIENT_ID}&device_code={}",
                urlencode(&self.device_code)
            ),
        };
        let body = curl_post_form(&self.token_endpoint, &form, self.provider)?;
        let v: Value = serde_json::from_str(&body)
            .map_err(|e| format!("解析 token 响应失败: {e}"))?;
        if let Some(err) = v.get("error").and_then(|x| x.as_str()) {
            return match err {
                "authorization_pending" => Ok(None),
                "slow_down" => {
                    self.current_interval += 5;
                    Ok(None)
                }
                "access_denied" => Err("用户在浏览器中拒绝了授权".to_string()),
                "expired_token" => Err("设备码已过期，请重新登录".to_string()),
                other => Err(format!("OAuth 错误: {other}")),
            };
        }
        let token = v
            .get("access_token")
            .and_then(|x| x.as_str())
            .ok_or("token 响应缺少 access_token")?
            .to_string();
        Ok(Some(token))
    }
}

/// curl GET 返回 JSON。
fn curl_json(url: &str) -> Result<Value, String> {
    let out = std::process::Command::new("curl")
        .args(["-sS", "-m", "15", url])
        .output()
        .map_err(|e| format!("curl 执行失败: {e}"))?;
    if !out.status.success() {
        return Err(format!("HTTP 请求失败: {}", String::from_utf8_lossy(&out.stderr)));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("JSON 解析失败: {e}"))
}

/// curl POST form（x-www-form-urlencoded）。
fn curl_post_form(url: &str, form: &str, ua: &str) -> Result<String, String> {
    let out = std::process::Command::new("curl")
        .args(["-sS", "-m", "15", "-X", "POST"])
        .arg("-H")
        .arg("Content-Type: application/x-www-form-urlencoded")
        .arg("-H")
        .arg(format!("User-Agent: ns-switch-{ua}"))
        .arg("--data-binary")
        .arg(form)
        .arg(url)
        .output()
        .map_err(|e| format!("curl 执行失败: {e}"))?;
    if !out.status.success() {
        return Err(format!("HTTP 请求失败: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// curl POST JSON。
fn curl_post_json(url: &str, json: &str, ua: &str) -> Result<String, String> {
    let out = std::process::Command::new("curl")
        .args(["-sS", "-m", "15", "-X", "POST"])
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-H")
        .arg(format!("User-Agent: ns-switch-{ua}"))
        .arg("--data-binary")
        .arg(json)
        .arg(url)
        .output()
        .map_err(|e| format!("curl 执行失败: {e}"))?;
    if !out.status.success() {
        return Err(format!("HTTP 请求失败: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// URL 编码（application/x-www-form-urlencoded）。
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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
        assert!(!path.exists());
    }

    #[test]
    fn copilot_token_hint_extracted() {
        let path = std::env::temp_dir().join("ns-auth-test-copilot.json");
        std::fs::write(
            &path,
            r#"{"github.com":{"oauth_token":"gho_abcdef123456","user":"user1","username":"u1"}}"#,
        )
        .unwrap();
        let hint = read_token("copilot", &path).unwrap();
        assert_eq!(hint.as_deref(), Some("gho_abcd…"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn copilot_missing_file_not_logged_in() {
        let path = std::env::temp_dir().join("ns-auth-test-copilot-miss.json");
        let _ = std::fs::remove_file(&path);
        let r = read_token("copilot", &path).unwrap();
        assert_eq!(r, None);
        assert_eq!(r, None);
    }

    #[test]
    fn build_session_parses_xai_device_response() {
        let body = r#"{"device_code":"dc123","user_code":"ABCD-EF","verification_uri":"https://auth.x.ai/device","verification_uri_complete":"https://auth.x.ai/device?code=ABCD-EF","expires_in":600,"interval":5}"#;
        let s = build_session("xai", body, "https://auth.x.ai/token".to_string()).unwrap();
        assert_eq!(s.device_code, "dc123");
        assert_eq!(s.user_code, "ABCD-EF");
        assert_eq!(s.verification_uri, "https://auth.x.ai/device?code=ABCD-EF");
        assert_eq!(s.expires_in, 600);
        assert_eq!(s.interval, 5);
    }

    #[test]
    fn build_session_parses_codex_device_response() {
        // Codex 用 device_auth_id 字段。
        let body = r#"{"device_auth_id":"dev9","user_code":"WXYZ-12","expires_in":300,"interval":3}"#;
        let s = build_session("codex", body, "https://auth.openai.com/oauth/token".to_string()).unwrap();
        assert_eq!(s.device_code, "dev9");
        assert_eq!(s.verification_uri, "https://auth.openai.com/codex/device");
        assert_eq!(s.expires_in, 300);
    }

    #[test]
    fn poll_before_interval_returns_pending() {
        let body = r#"{"device_code":"dc1","user_code":"A","expires_in":600,"interval":5}"#;
        let mut s = build_session("xai", body, "https://auth.x.ai/token".to_string()).unwrap();
        // 强制下次轮询在未来，poll 应直接返回 Ok(None)（不发起网络请求）。
        s.next_poll_at_ms = now_ms() + 60_000;
        let r = s.poll();
        assert!(r.is_ok());
        assert_eq!(r.unwrap(), None);
    }

    #[test]
    fn urlencode_encodes_reserved() {
        assert_eq!(urlencode("a b/c"), "a+b%2Fc");
        assert_eq!(urlencode("openid profile"), "openid+profile");
    }
}
