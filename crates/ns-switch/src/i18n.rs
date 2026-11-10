//! 国际化：语言选择 + 核心 UI 字符串翻译。
//!
//! 语言：简中 / 繁中 / 英 / 日。未收录的字符串回退简中。
//! 字符串表覆盖侧栏、顶栏、状态栏、设置页等核心 UI。

/// 语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    ZhCn,
    ZhTw,
    En,
    Ja,
}

impl Lang {
    /// 从存储字符串解析（未知 → 简中）。
    pub fn parse(s: &str) -> Self {
        match s {
            "zh-tw" => Self::ZhTw,
            "en" => Self::En,
            "ja" => Self::Ja,
            _ => Self::ZhCn,
        }
    }


    /// 显示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::ZhTw => "繁體中文",
            Self::En => "English",
            Self::Ja => "日本語",
        }
    }
}

/// 翻译字符串（按 key）。未收录回退 `zh-cn` 值。
pub fn t(lang: Lang, key: &str) -> &'static str {
    let (en, ja, zh_tw) = match key {
        "sidebar.providers" => ("Providers", "プロバイダー", "供應商"),
        "sidebar.routing" => ("Routing", "ルーティング", "路由"),
        "sidebar.mcp" => ("MCP", "MCP", "MCP"),
        "sidebar.skills" => ("Skills", "スキル", "Skills"),
        "sidebar.prompts" => ("Prompts", "プロンプト", "提示詞"),
        "sidebar.usage" => ("Usage", "使用量", "用量"),
        "sidebar.tools" => ("Tools", "ツール", "工具"),
        "sidebar.projects" => ("Projects", "プロジェクト", "項目"),
        "sidebar.sessions" => ("Sessions", "セッション", "會話"),
        "sidebar.accounts" => ("Accounts", "アカウント", "帳號"),
        "sidebar.apps" => ("Apps", "アプリ", "應用"),
        "sidebar.universal" => ("Universal", "汎用", "通用"),
        "sidebar.toolconfig" => ("Tool Config", "ツール設定", "工具配置"),
        "sidebar.settings" => ("Settings", "設定", "設定"),
        "topbar.tagline" => (
            "One-click API provider switch",
            "APIプロバイダをワンクリック切替",
            "一鍵切換 API 供應商",
        ),
        "status.current" => ("Current", "現在", "當前"),
        "settings.title" => ("Settings", "設定", "設定"),
        "settings.theme" => ("Theme", "テーマ", "主題"),
        "settings.language" => ("Language", "言語", "語言"),
        "settings.autostart" => ("Launch at login", "ログイン時起動", "開機自啟"),
        "settings.autostart.on" => ("Launch at login: On", "ログイン時起動：有効", "開機自啟：已開啟"),
        "settings.autostart.off" => ("Launch at login: Off", "ログイン時起動：無効", "開機自啟：關閉"),
        "settings.autostart.enable" => ("Enable", "有効にする", "開啟自啟"),
        "settings.autostart.disable" => ("Disable", "無効にする", "關閉自啟"),
        "theme.dark" => ("Dark", "ダーク", "深色"),
        "theme.light" => ("Light", "ライト", "淺色"),
        "theme.system" => ("System", "システム", "跟隨系統"),
        "auth.title" => ("Auth Center", "認証センター", "授權中心"),
        "auth.subtitle" => (
            "CLI credentials & hosted subscriptions",
            "CLI認証とホスト型サブスクリプション",
            "CLI 工具憑證與託管訂閱",
        ),
        _ => ("", "", ""),
    };
    match lang {
        Lang::En => non_empty(en),
        Lang::Ja => non_empty(ja),
        Lang::ZhTw => non_empty(zh_tw),
        Lang::ZhCn => default_for(key),
    }
}

fn non_empty(s: &'static str) -> &'static str {
    if s.is_empty() { "" } else { s }
}

fn default_for(key: &str) -> &'static str {
    // 简中默认值（覆盖关键 key；未收录返回 key 本身）。
    match key {
        "sidebar.providers" => "供应商",
        "sidebar.routing" => "路由",
        "sidebar.mcp" => "MCP",
        "sidebar.skills" => "Skills",
        "sidebar.prompts" => "提示词",
        "sidebar.usage" => "用量",
        "sidebar.tools" => "工具",
        "sidebar.projects" => "项目",
        "sidebar.sessions" => "会话",
        "sidebar.accounts" => "账号",
        "sidebar.apps" => "应用",
        "sidebar.universal" => "通用",
        "sidebar.toolconfig" => "工具配置",
        "sidebar.settings" => "设置",
        "topbar.brand" => "ns-switch",
        "topbar.tagline" => "一键切换 API 供应商",
        "status.current" => "当前",
        "settings.title" => "设置",
        "settings.theme" => "主题",
        "settings.language" => "语言",
        "settings.autostart" => "开机自启",
        "settings.autostart.on" => "开机自启：已开启",
        "settings.autostart.off" => "开机自启：关闭",
        "settings.autostart.enable" => "开启自启",
        "settings.autostart.disable" => "关闭自启",
        "theme.dark" => "深色",
        "theme.light" => "浅色",
        "theme.system" => "跟随系统",
        "auth.title" => "授权中心",
        "auth.subtitle" => "CLI 工具凭据与托管订阅（Claude Desktop / Codex / Copilot / ChatGPT / xAI）",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        assert_eq!(Lang::parse("zh-tw"), Lang::ZhTw);
        assert_eq!(Lang::parse("en"), Lang::En);
        assert_eq!(Lang::parse("ja"), Lang::Ja);
        assert_eq!(Lang::parse("unknown"), Lang::ZhCn);
    }

    #[test]
    fn t_returns_translations() {
        assert_eq!(t(Lang::En, "sidebar.providers"), "Providers");
        assert_eq!(t(Lang::Ja, "sidebar.providers"), "プロバイダー");
        assert_eq!(t(Lang::ZhCn, "sidebar.providers"), "供应商");
        assert_eq!(t(Lang::ZhTw, "sidebar.providers"), "供應商");
    }

    #[test]
    fn unknown_key_falls_back() {
        assert_eq!(t(Lang::ZhCn, "nope.key"), "");
        assert_eq!(t(Lang::En, "nope.key"), "");
    }

    #[test]
    fn lang_labels() {
        assert_eq!(Lang::ZhCn.label(), "简体中文");
        assert_eq!(Lang::Ja.label(), "日本語");
    }
}
