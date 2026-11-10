//! 供应商数据模型（与 cc-switch `types.ts` 对齐，SSOT 模式）。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProviderCategory, Tool};

/// 单个供应商。`settings_config` 是应用配置对象：Claude 为 settings.json 关键字段；
/// Codex 为 `{ auth, config }`。由预设模板 + 用户输入生成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(rename = "settingsConfig")]
    pub settings_config: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "websiteUrl")]
    pub website_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<ProviderCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAt")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sortIndex")]
    pub sort_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// 元数据（不写入 live 配置，仅存 config.json）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ProviderMeta>,
    /// 图标名称（如 "openai"、"anthropic"）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// 图标颜色（Hex）
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "iconColor")]
    pub icon_color: Option<String>,
    /// 是否加入故障转移队列
    #[serde(default)]
    #[serde(rename = "inFailoverQueue")]
    pub in_failover_queue: bool,
}

impl Provider {
    pub fn with_id(id: String, name: String, settings_config: Value) -> Self {
        Self {
            id,
            name,
            settings_config,
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }
}

/// 供应商元数据（用量脚本模板等）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "supports1m")]
    pub supports_1m: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    /// 用量脚本路径（JS 模板；`None` = 无脚本）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "usageScript")]
    pub usage_script: Option<String>,
    /// API 格式(anthropic / openai_chat / openai_responses / gemini_native)。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "apiFormat")]
    pub api_format: Option<String>,
    /// 是否启用用量查询（None/true = 启用；false = 卡片不显示配额脚注）。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    #[serde(rename = "usageEnabled")]
    pub usage_enabled: bool,
}

fn default_true() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Default for ProviderMeta {
    fn default() -> Self {
        Self {
            template_type: None,
            supports_1m: None,
            models: None,
            usage_script: None,
            api_format: None,
            usage_enabled: true,
        }
    }
}

/// 各应用是否启用（通用供应商同步目标）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UniversalApps {
    pub claude: bool,
    pub codex: bool,
    pub gemini: bool,
}

impl Default for UniversalApps {
    fn default() -> Self {
        Self {
            claude: true,
            codex: true,
            gemini: true,
        }
    }
}

/// Claude 模型映射（上游模型名）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UniversalClaudeModels {
    pub model: Option<String>,
    #[serde(rename = "haikuModel")]
    pub haiku_model: Option<String>,
    #[serde(rename = "sonnetModel")]
    pub sonnet_model: Option<String>,
    #[serde(rename = "opusModel")]
    pub opus_model: Option<String>,
}

impl Default for UniversalClaudeModels {
    fn default() -> Self {
        Self {
            model: Some("claude-sonnet-5".into()),
            haiku_model: Some("claude-haiku-4-5-20251001".into()),
            sonnet_model: Some("claude-sonnet-5".into()),
            opus_model: Some("claude-opus-5".into()),
        }
    }
}

/// Codex 模型配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UniversalCodexModels {
    pub model: Option<String>,
    #[serde(rename = "reasoningEffort")]
    pub reasoning_effort: Option<String>,
}

impl Default for UniversalCodexModels {
    fn default() -> Self {
        Self {
            model: Some("gpt-5.6-sol".into()),
            reasoning_effort: Some("high".into()),
        }
    }
}

/// Gemini 模型配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UniversalGeminiModels {
    pub model: Option<String>,
}

impl Default for UniversalGeminiModels {
    fn default() -> Self {
        Self {
            model: Some("gemini-3.6-flash".into()),
        }
    }
}

/// 各应用模型配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UniversalModels {
    pub claude: Option<UniversalClaudeModels>,
    pub codex: Option<UniversalCodexModels>,
    pub gemini: Option<UniversalGeminiModels>,
}

impl Default for UniversalModels {
    fn default() -> Self {
        Self {
            claude: Some(UniversalClaudeModels::default()),
            codex: Some(UniversalCodexModels::default()),
            gemini: Some(UniversalGeminiModels::default()),
        }
    }
}

/// 统一供应商（跨应用共享配置）：一份 baseUrl/apiKey 同步到 Claude/Codex/Gemini。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalProvider {
    pub id: String,
    pub name: String,
    #[serde(rename = "providerType")]
    pub provider_type: String,
    pub apps: UniversalApps,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    pub models: UniversalModels,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "websiteUrl")]
    pub website_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "iconColor")]
    pub icon_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAt")]
    pub created_at: Option<i64>,
}

impl UniversalProvider {
    /// 投影到 Claude 子供应商（apps.claude 关闭时返回 None）。
    pub fn to_claude_provider(&self) -> Option<Provider> {
        if !self.apps.claude {
            return None;
        }
        let m = self.models.claude.as_ref();
        let model = m
            .and_then(|m| m.model.clone())
            .unwrap_or_else(|| "claude-sonnet-4-20250514".into());
        let haiku = m.and_then(|m| m.haiku_model.clone()).unwrap_or_else(|| model.clone());
        let sonnet = m.and_then(|m| m.sonnet_model.clone()).unwrap_or_else(|| model.clone());
        let opus = m.and_then(|m| m.opus_model.clone()).unwrap_or_else(|| model.clone());
        let mut p = Provider::with_id(
            format!("universal-claude-{}", self.id),
            self.name.clone(),
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": self.base_url,
                    "ANTHROPIC_AUTH_TOKEN": self.api_key,
                    "ANTHROPIC_MODEL": model,
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": haiku,
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": sonnet,
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": opus,
                }
            }),
        );
        p.website_url = self.website_url.clone();
        p.category = Some(crate::ProviderCategory::Aggregator);
        p.created_at = self.created_at;
        p.icon = self.icon.clone();
        p.icon_color = self.icon_color.clone();
        p.meta = Some(ProviderMeta {
            template_type: Some("universal".into()),
            api_format: Some("anthropic".into()),
            ..Default::default()
        });
        Some(p)
    }

    /// 投影到 Codex 子供应商。
    pub fn to_codex_provider(&self) -> Option<Provider> {
        if !self.apps.codex {
            return None;
        }
        let m = self.models.codex.as_ref();
        let model = m
            .and_then(|m| m.model.clone())
            .unwrap_or_else(|| "gpt-4o".into());
        let effort = m
            .and_then(|m| m.reasoning_effort.clone())
            .unwrap_or_else(|| "high".into());
        let base_trimmed = self.base_url.trim_end_matches('/');
        let origin_only = match base_trimmed.split_once("://") {
            Some((_s, rest)) => !rest.contains('/'),
            None => !base_trimmed.contains('/'),
        };
        let codex_base = if base_trimmed.ends_with("/v1") {
            base_trimmed.to_string()
        } else if origin_only {
            format!("{base_trimmed}/v1")
        } else {
            base_trimmed.to_string()
        };
        let config_toml = format!(
            r#"model_provider = "custom"
model = "{model}"
model_reasoning_effort = "{effort}"
disable_response_storage = true

[model_providers.custom]
name = "{}"
base_url = "{codex_base}"
wire_api = "responses"
requires_openai_auth = true"#,
            self.name
        );
        let mut p = Provider::with_id(
            format!("universal-codex-{}", self.id),
            self.name.clone(),
            serde_json::json!({
                "auth": { "OPENAI_API_KEY": self.api_key },
                "config": config_toml,
            }),
        );
        p.website_url = self.website_url.clone();
        p.category = Some(crate::ProviderCategory::Aggregator);
        p.created_at = self.created_at;
        p.icon = self.icon.clone();
        p.icon_color = self.icon_color.clone();
        p.meta = Some(ProviderMeta {
            template_type: Some("universal".into()),
            api_format: Some("openai_responses".into()),
            ..Default::default()
        });
        Some(p)
    }

    /// 投影到 Gemini 子供应商。
    pub fn to_gemini_provider(&self) -> Option<Provider> {
        if !self.apps.gemini {
            return None;
        }
        let m = self.models.gemini.as_ref();
        let model = m
            .and_then(|m| m.model.clone())
            .unwrap_or_else(|| "gemini-2.5-pro".into());
        let mut p = Provider::with_id(
            format!("universal-gemini-{}", self.id),
            self.name.clone(),
            serde_json::json!({
                "env": {
                    "GOOGLE_GEMINI_BASE_URL": self.base_url,
                    "GEMINI_API_KEY": self.api_key,
                    "GEMINI_MODEL": model,
                }
            }),
        );
        p.website_url = self.website_url.clone();
        p.category = Some(crate::ProviderCategory::Aggregator);
        p.created_at = self.created_at;
        p.icon = self.icon.clone();
        p.icon_color = self.icon_color.clone();
        p.meta = Some(ProviderMeta {
            template_type: Some("universal".into()),
            api_format: Some("gemini_native".into()),
            ..Default::default()
        });
        Some(p)
    }
}


/// 每个工具的当前供应商选择。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrentSelection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "claude-desktop")]
    pub claude_desktop: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gemini: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "grokbuild")]
    pub grok_build: Option<String>,
}

impl CurrentSelection {
    pub fn get(&self, tool: Tool) -> Option<&String> {
        match tool {
            Tool::Claude => self.claude.as_ref(),
            Tool::ClaudeDesktop => self.claude_desktop.as_ref(),
            Tool::Codex => self.codex.as_ref(),
            Tool::Gemini => self.gemini.as_ref(),
            Tool::GrokBuild => self.grok_build.as_ref(),
            _ => None,
        }
    }

    pub fn set(&mut self, tool: Tool, id: String) {
        match tool {
            Tool::Claude => self.claude = Some(id),
            Tool::ClaudeDesktop => self.claude_desktop = Some(id),
            Tool::Codex => self.codex = Some(id),
            Tool::Gemini => self.gemini = Some(id),
            Tool::GrokBuild => self.grok_build = Some(id),
            _ => {}
        }
    }
}

impl CurrentSelection {
    /// 移除某工具的当前选择（删除供应商时用）。
    pub fn remove(&mut self, tool: Tool) {
        match tool {
            Tool::Claude => self.claude = None,
            Tool::ClaudeDesktop => self.claude_desktop = None,
            Tool::Codex => self.codex = None,
            Tool::Gemini => self.gemini = None,
            Tool::GrokBuild => self.grok_build = None,
            _ => {}
        }
    }
}

/// 从供应商提取该工具可见的模型名（`meta.models` 优先，否则从 settings_config 推断）。
pub fn provider_models(p: &Provider) -> Vec<String> {
    if let Some(meta) = &p.meta {
        if let Some(models) = &meta.models {
            if !models.is_empty() {
                return models.clone();
            }
        }
    }
    // settings_config 推断：Claude env.ANTHROPIC_MODEL、Codex config.model、Gemini env.GEMINI_MODEL。
    let mut out = Vec::new();
    if let Some(env) = p.settings_config.get("env").and_then(Value::as_object) {
        for k in ["ANTHROPIC_MODEL", "GEMINI_MODEL", "ANTHROPIC_SMALL_FAST_MODEL"] {
            if let Some(v) = env.get(k).and_then(Value::as_str) {
                if !v.is_empty() && !out.contains(&v.to_string()) {
                    out.push(v.to_string());
                }
            }
        }
    }
    if let Some(config) = p.settings_config.get("config").and_then(Value::as_object) {
        if let Some(v) = config.get("model").and_then(Value::as_str) {
            if !v.is_empty() && !out.contains(&v.to_string()) {
                out.push(v.to_string());
            }
        }
    }
    out
}

/// 生成 Codex 模型目录 JSON（顶层 `models` 数组）。聚合时合并所有供应商可见模型。
pub fn aggregation_model_catalog(providers: &[&Provider]) -> Value {
    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in providers {
        let provider_name = p.name.clone();
        for slug in provider_models(p) {
            if !seen.insert(slug.clone()) {
                continue;
            }
            models.push(serde_json::json!({
                "slug": slug,
                "display_name": format!("{provider_name} · {slug}"),
                "priority": models.len() as i64,
                "visibility": "visible",
                "context_window": 200_000,
            }));
        }
    }
    serde_json::json!({ "models": models })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_models_prefers_meta() {
        let mut p = Provider::with_id(
            "p1".into(),
            "P1".into(),
            serde_json::json!({
                "env": { "ANTHROPIC_MODEL": "claude-sonnet-4-5" }
            }),
        );
        p.meta = Some(ProviderMeta {
            models: Some(vec!["custom-model".into(), "other-model".into()]),
            ..Default::default()
        });
        assert_eq!(provider_models(&p), vec!["custom-model", "other-model"]);
    }

    #[test]
    fn provider_models_infers_from_settings() {
        let p = Provider::with_id(
            "p1".into(),
            "P1".into(),
            serde_json::json!({
                "env": { "ANTHROPIC_MODEL": "claude-sonnet-4-5" },
                "config": { "model": "gpt-6-sol" }
            }),
        );
        assert_eq!(provider_models(&p), vec!["claude-sonnet-4-5", "gpt-6-sol"]);
    }

    #[test]
    fn aggregate_catalog_dedupes_and_sorts() {
        let a = Provider::with_id(
            "a".into(),
            "Alpha".into(),
            serde_json::json!({ "env": { "ANTHROPIC_MODEL": "claude-sonnet-4-5" } }),
        );
        let b = Provider::with_id(
            "b".into(),
            "Beta".into(),
            serde_json::json!({ "config": { "model": "gpt-6-sol" } }),
        );
        // 重复模型（Alpha 与 Beta 都用 gpt-6-sol）应去重。
        let b2 = Provider::with_id(
            "b2".into(),
            "Beta2".into(),
            serde_json::json!({ "config": { "model": "gpt-6-sol" } }),
        );
        let catalog = aggregation_model_catalog(&[&a, &b, &b2]);
        let models = catalog["models"].as_array().expect("models array");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["slug"], "claude-sonnet-4-5");
        assert_eq!(models[1]["slug"], "gpt-6-sol");
    }

    #[test]
    fn provider_meta_usage_enabled_serde() {
        // 默认启用；显式停用时序列化 usageEnabled:false。
        let meta = ProviderMeta::default();
        assert!(meta.usage_enabled);
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("usageEnabled"));
        let disabled = ProviderMeta {
            usage_enabled: false,
            ..Default::default()
        };
        let json = serde_json::to_string(&disabled).unwrap();
        assert!(json.contains("usageEnabled"));
        assert!(json.contains("false"));
        let back: ProviderMeta = serde_json::from_str(&json).unwrap();
        assert!(!back.usage_enabled);
        // 缺失字段回退为 true（向后兼容旧 config）。
        let back: ProviderMeta = serde_json::from_str("{}").unwrap();
        assert!(back.usage_enabled);
    }
}
