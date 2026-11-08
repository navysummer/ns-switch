//! 地板清单（关键字段定义）。
//!
//! 地板回答四个问题：请求发到哪、凭什么鉴权、哪个模型名、说哪种协议。
//! 切换时清空再写目标供应商的值；其余键归用户和客户端，不写不删。
//!
//! M0 仅定义 Claude Code 的 `env` 地板谓词（后续里程碑扩展 Codex/Gemini 等）。

/// `env` 里整个前缀都属于连接和鉴权的前缀。
pub const CLAUDE_FLOOR_ENV_PREFIXES: &[&str] = &["ANTHROPIC_", "AWS_", "VERTEX_REGION_"];

/// `env` 里按名字列出的关键字段（协议选择器之外）。
pub const CLAUDE_FLOOR_ENV_KEYS: &[&str] = &[
    "CLAUDE_CODE_SUBAGENT_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL_FORCE",
    "CLOUD_ML_REGION",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
    "CLAUDE_CODE_OAUTH_SCOPES",
    "CLAUDE_CODE_API_KEY_HELPER_TTL_MS",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
];

/// Claude Code 协议选择器（不能按前缀匹配）。
pub const CLAUDE_PROTOCOL_SELECTORS: &[&str] = &[
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_GATEWAY",
    "CLAUDE_CODE_USE_MANTLE",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD",
];

/// Claude Code `settings.json` 的 `env` 里，这个键是否是关键字段。
pub fn claude_floor_env(key: &str) -> bool {
    CLAUDE_FLOOR_ENV_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
        || CLAUDE_PROTOCOL_SELECTORS.contains(&key)
        || CLAUDE_FLOOR_ENV_KEYS.contains(&key)
        || (key.starts_with("CLAUDE_CODE_SKIP_") && key.ends_with("_AUTH"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_prefix_floor() {
        assert!(claude_floor_env("ANTHROPIC_BASE_URL"));
        assert!(claude_floor_env("ANTHROPIC_AUTH_TOKEN"));
        assert!(claude_floor_env("AWS_REGION"));
        assert!(claude_floor_env("VERTEX_REGION_EU"));
    }

    #[test]
    fn claude_named_floor() {
        assert!(claude_floor_env("CLAUDE_CODE_SUBAGENT_MODEL"));
        assert!(claude_floor_env("GOOGLE_APPLICATION_CREDENTIALS"));
    }

    #[test]
    fn claude_protocol_selector() {
        assert!(claude_floor_env("CLAUDE_CODE_USE_BEDROCK"));
        assert!(!claude_floor_env("CLAUDE_CODE_USE_POWERSHELL_TOOL"));
    }

    #[test]
    fn claude_skip_auth_suffix() {
        assert!(claude_floor_env("CLAUDE_CODE_SKIP_VERIFY_AUTH"));
        assert!(claude_floor_env("CLAUDE_CODE_SKIP_SSRF_AUTH"));
        assert!(!claude_floor_env("CLAUDE_CODE_SKIP_VERIFY_OTHER"));
    }

    #[test]
    fn user_keys_kept() {
        assert!(!claude_floor_env("MY_CUSTOM_VAR"));
        assert!(!claude_floor_env("PATH"));
    }
}
