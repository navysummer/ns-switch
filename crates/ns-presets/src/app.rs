//! 支持的 AI 编程助手工具（客户端）。

use serde::{Deserialize, Serialize};

/// 支持的客户端工具。`Switch` 类同一时刻只有一个在用供应商；
/// `Coexist` 类把多个供应商写入工具自身配置，工具内再选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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

    /// 切换后是否需要重启终端/进程：
    /// Claude Code 每次命令读取 settings.json（热切换）；
    /// Claude Desktop 读取启动时的 profile；Codex/Gemini/Grok 等在运行中的会话会缓存配置。
    pub fn needs_restart(self) -> bool {
        !matches!(self, Tool::Claude)
    }

    /// 工具的默认配置目录（不存在也返回约定路径）。
    ///
    /// 与 `switch_engine`/MCP 适配器使用的写入路径保持一致；界面「打开配置目录」
    /// 与设置页展示都走这里，避免各处硬编码漂移。
    pub fn default_config_dir(self, home: &std::path::Path) -> std::path::PathBuf {
        match self {
            Tool::Claude => home.join(".claude"),
            Tool::ClaudeDesktop => {
                #[cfg(target_os = "macos")]
                {
                    home.join("Library")
                        .join("Application Support")
                        .join("Claude-3p")
                }
                #[cfg(target_os = "windows")]
                {
                    std::env::var_os("LOCALAPPDATA")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| home.join("AppData").join("Local"))
                        .join("Claude-3p")
                }
                #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
                {
                    std::env::var_os("XDG_CONFIG_HOME")
                        .map(std::path::PathBuf::from)
                        .filter(|p| p.is_absolute())
                        .unwrap_or_else(|| home.join(".config"))
                        .join("Claude-3p")
                }
            }
            Tool::Codex => home.join(".codex"),
            Tool::Gemini => home.join(".gemini"),
            Tool::GrokBuild => home.join(".grok"),
            Tool::Opencode => home.join(".config").join("opencode"),
            Tool::Openclaw => home.join(".openclaw"),
            Tool::Pi => home.join(".pi").join("agent"),
            Tool::Hermes => home.join(".hermes"),
            Tool::Mcode => home.join(".minimax"),
            Tool::Kimi => home.join(".kimi-code"),
            Tool::Agy => home.join(".agy"),
            Tool::Omp => home.join(".omp").join("agent"),
        }
    }

    /// 工具的主配置文件路径（用于「打开配置目录」定位与展示）。
    pub fn primary_config_path(self, home: &std::path::Path) -> std::path::PathBuf {
        let dir = self.default_config_dir(home);
        dir.join(match self {
            Tool::Claude => "settings.json",
            Tool::ClaudeDesktop => "claude_desktop_config.json",
            Tool::Codex => "config.toml",
            Tool::Gemini => "settings.json",
            Tool::GrokBuild => "config.toml",
            Tool::Opencode => "opencode.json",
            Tool::Openclaw => "openclaw.json",
            Tool::Pi => "models.json",
            Tool::Hermes => "config.yaml",
            Tool::Mcode => "config.yaml",
            Tool::Kimi => "config.toml",
            Tool::Agy => "settings.json",
            Tool::Omp => "models.yml",
        })
    }

    /// 序列化用的 kebab-case id（与 `template_type` 一致）。
    pub fn id_str(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::ClaudeDesktop => "claude-desktop",
            Tool::Codex => "codex",
            Tool::Gemini => "gemini",
            Tool::GrokBuild => "grokbuild",
            Tool::Opencode => "opencode",
            Tool::Openclaw => "openclaw",
            Tool::Hermes => "hermes",
            Tool::Pi => "pi",
            Tool::Mcode => "mcode",
            Tool::Kimi => "kimi",
            Tool::Agy => "agy",
            Tool::Omp => "omp",
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_hot_switches_others_restart() {
        assert!(!Tool::Claude.needs_restart(), "Claude Code 热切换");
        assert!(Tool::ClaudeDesktop.needs_restart(), "Claude Desktop 需重启读取 profile");
        assert!(Tool::Codex.needs_restart());
        assert!(Tool::Gemini.needs_restart());
        assert!(Tool::GrokBuild.needs_restart());
        assert!(Tool::Opencode.needs_restart());
    }

    #[test]
    fn config_dirs_match_writer_paths() {
        let home = std::path::Path::new("/tmp/home");
        assert_eq!(Tool::Kimi.default_config_dir(home), home.join(".kimi-code"));
        assert_eq!(Tool::Mcode.default_config_dir(home), home.join(".minimax"));
        assert_eq!(Tool::Omp.default_config_dir(home), home.join(".omp/agent"));
        assert_eq!(Tool::GrokBuild.default_config_dir(home), home.join(".grok"));
        assert_eq!(Tool::Pi.default_config_dir(home), home.join(".pi/agent"));
        assert_eq!(
            Tool::Kimi.primary_config_path(home),
            home.join(".kimi-code/config.toml")
        );
        assert_eq!(
            Tool::Omp.primary_config_path(home),
            home.join(".omp/agent/models.yml")
        );
    }
}
