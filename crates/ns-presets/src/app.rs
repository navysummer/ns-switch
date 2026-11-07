//! 支持的 AI 编程助手工具（客户端）。

use serde::{Deserialize, Serialize};

/// 支持的客户端工具。`Switch` 类同一时刻只有一个在用供应商；
/// `Coexist` 类把多个供应商写入工具自身配置，工具内再选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    Claude,
    ClaudeDesktop,
    Codex,
    Gemini,
    GrokBuild,
    Opencode,
    Openclaw,
    Hermes,
    Pi,
    Mcode,
    Kimi,
    Agy,
    Omp,
}

impl Tool {
    pub const ALL: [Tool; 13] = [
        Tool::Claude,
        Tool::ClaudeDesktop,
        Tool::Codex,
        Tool::Gemini,
        Tool::GrokBuild,
        Tool::Opencode,
        Tool::Openclaw,
        Tool::Hermes,
        Tool::Pi,
        Tool::Mcode,
        Tool::Kimi,
        Tool::Agy,
        Tool::Omp,
    ];

    /// 界面显示名。
    pub fn label(self) -> &'static str {
        match self {
            Tool::Claude => "Claude Code",
            Tool::ClaudeDesktop => "Claude Desktop",
            Tool::Codex => "Codex",
            Tool::Gemini => "Gemini CLI",
            Tool::GrokBuild => "Grok Build",
            Tool::Opencode => "OpenCode",
            Tool::Openclaw => "OpenClaw",
            Tool::Hermes => "Hermes",
            Tool::Pi => "Pi",
            Tool::Mcode => "MiniMax Code",
            Tool::Kimi => "Kimi Code",
            Tool::Agy => "Antigravity",
            Tool::Omp => "Oh My Pi",
        }
    }

    /// 图标字形（界面用，纯文本字形，后续可换 SVG）。
    pub fn glyph(self) -> &'static str {
        match self {
            Tool::Claude => "✦",
            Tool::ClaudeDesktop => "◈",
            Tool::Codex => "⌘",
            Tool::Gemini => "◆",
            Tool::GrokBuild => "▲",
            Tool::Opencode => "◇",
            Tool::Openclaw => "❖",
            Tool::Hermes => "☿",
            Tool::Pi => "π",
            Tool::Mcode => "♾",
            Tool::Kimi => "☾",
            Tool::Agy => "✦",
            Tool::Omp => "☯",
        }
    }

    /// 切换类：同一时刻只启用一个供应商。
    pub fn is_switch(self) -> bool {
        matches!(
            self,
            Tool::Claude
                | Tool::ClaudeDesktop
                | Tool::Codex
                | Tool::Gemini
                | Tool::GrokBuild
        )
    }

    /// 是否走本地路由/代理（路由模式可用）。
    pub fn supports_routing(self) -> bool {
        matches!(self, Tool::Claude | Tool::Codex | Tool::Gemini | Tool::GrokBuild)
    }

    /// 是否支持聚合模式。
    pub fn supports_aggregation(self) -> bool {
        matches!(self, Tool::Claude | Tool::Codex)
    }
}

/// 供应商分类（差异化提示/能力开关）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCategory {
    /// 官方（Claude Official 等）
    Official,
    /// 开源官方
    CnOfficial,
    /// 云服务商（AWS Bedrock 等）
    CloudProvider,
    /// 聚合网站
    Aggregator,
    /// 第三方供应商
    ThirdParty,
    /// 自定义
    Custom,
    /// 本机推理服务（Ollama、LM Studio 等）
    Local,
    /// Oh My OpenCode
    Omo,
    /// Oh My OpenCode Slim
    OmoSlim,
}

impl ProviderCategory {
    pub fn label(self) -> &'static str {
        match self {
            ProviderCategory::Official => "官方",
            ProviderCategory::CnOfficial => "开源官方",
            ProviderCategory::CloudProvider => "云服务商",
            ProviderCategory::Aggregator => "聚合",
            ProviderCategory::ThirdParty => "第三方",
            ProviderCategory::Custom => "自定义",
            ProviderCategory::Local => "本机",
            ProviderCategory::Omo => "OMO",
            ProviderCategory::OmoSlim => "OMO Slim",
        }
    }
}
