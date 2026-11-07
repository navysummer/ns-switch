//! 供应商预设表。
//!
//! 每个预设提供工具、分类、名称/别名/域名、图标与一个构建函数：
//! 用户输入（endpoint/key/model）→ `settings_config` 关键字段对象。
//! 预设即数据，逻辑不感知具体供应商。

use serde_json::{Value, json};

use crate::{ProviderCategory, Tool};

/// 供应商预设。
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    /// 预设标识（生成供应商 id 时加时间戳/序号后缀）。
    pub id: &'static str,
    /// 显示名（可含中文名）。
    pub name: &'static str,
    pub tool: Tool,
    pub category: ProviderCategory,
    /// 搜索别名（中文名、域名等）。
    pub aliases: &'static [&'static str],
    /// 图标名称。
    pub icon: &'static str,
    /// 图标颜色（Hex）。
    pub icon_color: &'static str,
    /// 网站（可选）。
    pub website_url: Option<&'static str>,
    /// 供应商行 → 目标关键字段。
    pub build: fn(endpoint: &str, key: &str, model: &str) -> Value,
}

/// 生成 `settingsConfig`：`env` 对象（Claude Code 风格）。
fn claude_env(endpoint: &str, key: &str, model: &str) -> Value {
    json!({
        "env": {
            "ANTHROPIC_BASE_URL": endpoint,
            "ANTHROPIC_AUTH_TOKEN": key,
            "ANTHROPIC_MODEL": model,
            "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku-4-5",
        }
    })
}

/// 生成 `settingsConfig`：Codex 风格 `{ auth, config }`。
fn codex_auth(endpoint: &str, key: &str, model: &str) -> Value {
    json!({
        "auth": {
            "OPENAI_API_KEY": key,
        },
        "config": {
            "model_provider": "custom",
            "model": model,
            "model_providers": {
                "custom": {
                    "name": "自定义",
                    "base_url": endpoint,
                    "wire_api": "chat",
                }
            }
        }
    })
}

/// 生成 `settingsConfig`：Gemini 风格（`settings.json` + `.env`）。
fn gemini_config(endpoint: &str, key: &str, model: &str) -> Value {
    json!({
        "env": {
            "GEMINI_API_KEY": key,
            "GEMINI_BASE_URL": endpoint,
            "GEMINI_MODEL": model,
        }
    })
}

const fn preset(
    id: &'static str,
    name: &'static str,
    tool: Tool,
    category: ProviderCategory,
    aliases: &'static [&'static str],
    icon: &'static str,
    icon_color: &'static str,
    website_url: Option<&'static str>,
    build: fn(&str, &str, &str) -> Value,
) -> Preset {
    Preset {
        id,
        name,
        tool,
        category,
        aliases,
        icon,
        icon_color,
        website_url,
        build,
    }
}

/// 全部预设（首批：Claude Code / Codex / Gemini CLI 常用供应商）。
/// 后续按需扩展至 90+（参照 cc-switch 预设表）。
pub static PRESETS: &[Preset] = &[
    // ── Claude Code 官方 ──────────────────────────────
    preset(
        "claude-official",
        "Claude Official",
        Tool::Claude,
        ProviderCategory::Official,
        &["官方", "anthropic", "claude.ai"],
        "anthropic",
        "#D97757",
        Some("https://claude.ai"),
        |_, _, model| {
            json!({
                "env": { "ANTHROPIC_MODEL": model, "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku-4-5" }
            })
        },
    ),
    // ── Claude Code 聚合/第三方 ────────────────────────
    preset(
        "claude-88api",
        "88API",
        Tool::Claude,
        ProviderCategory::Aggregator,
        &["88api", "聚合", "token"],
        "api",
        "#5B8DEF",
        Some("https://88api.ai"),
        claude_env,
    ),
    preset(
        "claude-openrouter",
        "OpenRouter",
        Tool::Claude,
        ProviderCategory::Aggregator,
        &["openrouter", "聚合"],
        "router",
        "#8A5CF6",
        Some("https://openrouter.ai"),
        claude_env,
    ),
    preset(
        "claude-ppio",
        "PPIO",
        Tool::Claude,
        ProviderCategory::Aggregator,
        &["ppio", "聚合", "token plan"],
        "cloud",
        "#F05A3C",
        Some("https://ppio.com"),
        claude_env,
    ),
    preset(
        "claude-aicode",
        "AIGoCode",
        Tool::Claude,
        ProviderCategory::Aggregator,
        &["aigocode", "聚合"],
        "code",
        "#22C55E",
        Some("https://aigocode.net"),
        claude_env,
    ),
    preset(
        "claude-ccsub",
        "CCSub",
        Tool::Claude,
        ProviderCategory::Aggregator,
        &["ccsub", "聚合"],
        "sub",
        "#EAB308",
        Some("https://www.ccsub.net"),
        claude_env,
    ),
    preset(
        "claude-siliconflow",
        "SiliconFlow",
        Tool::Claude,
        ProviderCategory::CloudProvider,
        &["siliconflow", "硅基流动", "云"],
        "flow",
        "#38BDF8",
        Some("https://cloud.siliconflow.cn"),
        claude_env,
    ),
    preset(
        "claude-local",
        "Ollama（本机）",
        Tool::Claude,
        ProviderCategory::Local,
        &["ollama", "本地", "lmstudio"],
        "local",
        "#94A3B8",
        Some("https://ollama.com"),
        claude_env,
    ),
    // ── Codex 官方 ───────────────────────────────────
    preset(
        "codex-official",
        "OpenAI Official",
        Tool::Codex,
        ProviderCategory::Official,
        &["官方", "openai", "codex"],
        "openai",
        "#10A37F",
        Some("https://chatgpt.com"),
        |_, _, model| {
            json!({
                "config": { "model": model }
            })
        },
    ),
    // ── Codex 聚合/第三方 ─────────────────────────────
    preset(
        "codex-88api",
        "88API",
        Tool::Codex,
        ProviderCategory::Aggregator,
        &["88api", "聚合", "token"],
        "api",
        "#5B8DEF",
        Some("https://88api.ai"),
        codex_auth,
    ),
    preset(
        "codex-openrouter",
        "OpenRouter",
        Tool::Codex,
        ProviderCategory::Aggregator,
        &["openrouter", "聚合"],
        "router",
        "#8A5CF6",
        Some("https://openrouter.ai"),
        codex_auth,
    ),
    preset(
        "codex-ppio",
        "PPIO",
        Tool::Codex,
        ProviderCategory::Aggregator,
        &["ppio", "聚合"],
        "cloud",
        "#F05A3C",
        Some("https://ppio.com"),
        codex_auth,
    ),
    preset(
        "codex-fenno",
        "Fenno.ai",
        Tool::Codex,
        ProviderCategory::Aggregator,
        &["fenno", "聚合"],
        "f",
        "#6366F1",
        Some("https://api.fenno.ai"),
        codex_auth,
    ),
    // ── Gemini CLI 官方 ───────────────────────────────
    preset(
        "gemini-official",
        "Google Official",
        Tool::Gemini,
        ProviderCategory::Official,
        &["官方", "google", "gemini"],
        "google",
        "#4285F4",
        Some("https://gemini.google.com"),
        |_, _, model| {
            json!({ "env": { "GEMINI_MODEL": model } })
        },
    ),
    // ── Gemini CLI 聚合/第三方 ─────────────────────────
    preset(
        "gemini-88api",
        "88API",
        Tool::Gemini,
        ProviderCategory::Aggregator,
        &["88api", "聚合"],
        "api",
        "#5B8DEF",
        Some("https://88api.ai"),
        gemini_config,
    ),
    preset(
        "gemini-openrouter",
        "OpenRouter",
        Tool::Gemini,
        ProviderCategory::Aggregator,
        &["openrouter", "聚合"],
        "router",
        "#8A5CF6",
        Some("https://openrouter.ai"),
        gemini_config,
    ),
    preset(
        "gemini-ppio",
        "PPIO",
        Tool::Gemini,
        ProviderCategory::Aggregator,
        &["ppio", "聚合"],
        "cloud",
        "#F05A3C",
        Some("https://ppio.com"),
        gemini_config,
    ),
];

/// 按工具列出预设。
pub fn presets_for(tool: Tool) -> impl Iterator<Item = &'static Preset> {
    PRESETS.iter().filter(move |p| p.tool == tool)
}

/// 按名称/别名/域名搜索预设（大小写不敏感）。
pub fn search_presets(query: &str, tool: Option<Tool>) -> Vec<&'static Preset> {
    let q = query.trim().to_lowercase();
    PRESETS
        .iter()
        .filter(|p| tool.is_none_or(|t| p.tool == t))
        .filter(|p| {
            if q.is_empty() {
                return true;
            }
            p.name.to_lowercase().contains(&q)
                || p.aliases.iter().any(|a| a.to_lowercase().contains(&q))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;

    #[test]
    fn presets_have_unique_ids() {
        let mut seen = std::collections::HashSet::new();
        for p in PRESETS {
            assert!(seen.insert(p.id), "重复预设 id: {}", p.id);
        }
    }

    #[test]
    fn claude_presets_build_env() {
        for p in presets_for(Tool::Claude) {
            let v = (p.build)("https://api.example.com", "sk-test", "claude-sonnet-4-5");
            let obj = v.as_object().expect("Claude 预设应返回对象");
            // 官方无端点键，第三方应有 ANTHROPIC_BASE_URL/AUTH_TOKEN
            if p.category != ProviderCategory::Official {
                let env = obj["env"].as_object().expect("应有 env");
                assert!(env.contains_key("ANTHROPIC_BASE_URL"));
                assert!(env.contains_key("ANTHROPIC_AUTH_TOKEN"));
                assert!(env.contains_key("ANTHROPIC_MODEL"));
            }
        }
    }

    #[test]
    fn codex_presets_build_auth() {
        for p in presets_for(Tool::Codex) {
            let v = (p.build)("https://api.example.com", "sk-test", "gpt-5");
            let obj = v.as_object().expect("Codex 预设应返回对象");
            assert!(obj.contains_key("config"));
            // 官方登录无 key；第三方应有 auth.OPENAI_API_KEY
            if p.category != ProviderCategory::Official {
                assert!(
                    obj["auth"]["OPENAI_API_KEY"] == "sk-test",
                    "{} 缺少 auth.OPENAI_API_KEY",
                    p.name
                );
            }
        }
    }

    #[test]
    fn search_matches_name_and_alias() {
        assert!(search_presets("88api", Some(Tool::Claude)).len() >= 1);
        assert!(search_presets("聚合", None).len() >= 1);
        assert!(search_presets("Ollama", None).iter().any(|p| p.tool == Tool::Claude));
    }

    #[test]
    fn each_switch_tool_has_official() {
        for tool in [Tool::Claude, Tool::Codex, Tool::Gemini] {
            assert!(
                presets_for(tool).any(|p| p.category == ProviderCategory::Official),
                "{} 缺少官方预设",
                tool.label()
            );
        }
    }
}
