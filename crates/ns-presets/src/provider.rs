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
