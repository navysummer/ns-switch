//! 品牌图标与配色预设（供应商卡片、对话框图标选择器共用）。
//!
//! 图标以「字形」表示（无 SVG 资源依赖）：卡片与选择器都按 id 查字形，
//! 未知 id 回退首字母。新增品牌只需在此表追加一行。

/// 品牌图标：`(id, 显示名, 字形)`。
pub const ICONS: &[(&str, &str, char)] = &[
    ("anthropic", "Anthropic", '❋'),
    ("openai", "OpenAI", '⬡'),
    ("google", "Google", '◆'),
    ("grok", "Grok", '✦'),
    ("copilot", "Copilot", '⌥'),
    ("api", "通用 API", '⌁'),
    ("router", "路由", '⛶'),
    ("cloud", "云服务", '☁'),
    ("code", "编码", '⌘'),
    ("sub", "订阅", '⧉'),
    ("flow", "聚合", '≋'),
    ("local", "本地", '⬢'),
    ("mini", "MiniMax", '▣'),
    ("kimi", "Kimi", '✳'),
    ("pi", "Pi", 'π'),
    ("hermes", "Hermes", '⚡'),
    ("claw", "OpenClaw", '✪'),
    ("omp", "Oh My Pi", '◉'),
];

/// 图标配色预设：`(十六进制, 名称)`。
pub const ICON_COLORS: &[(&str, &str)] = &[
    ("#c9a25f", "鎏金"),
    ("#6f9b8a", "玉"),
    ("#b0564a", "朱"),
    ("#cf6b5e", "绯"),
    ("#5b7fa6", "靛"),
    ("#8b6fa8", "紫"),
    ("#4f8a8b", "青"),
    ("#7a9b5f", "苔"),
    ("#c98a4b", "橙"),
    ("#8a7c63", "墨"),
];

/// 按 id 查字形（未知返回 None）。
pub fn icon_char(id: &str) -> Option<char> {
    ICONS
        .iter()
        .find(|(value, _, _)| *value == id)
        .map(|(_, _, glyph)| *glyph)
}

/// 按 id 查显示名（未知返回 None）。
pub fn icon_label(id: &str) -> Option<&'static str> {
    ICONS
        .iter()
        .find(|(value, _, _)| *value == id)
        .map(|(_, label, _)| *label)
}

#[cfg(test)]
mod tests {
    use super::{icon_char, icon_label, ICON_COLORS, ICONS};

    #[test]
    fn icons_are_unique_and_lookup_works() {
        let mut ids: Vec<&str> = ICONS.iter().map(|(id, _, _)| *id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "图标 id 不应重复");
        assert_eq!(icon_char("anthropic"), Some('❋'));
        assert_eq!(icon_char("nope"), None);
        assert_eq!(icon_label("openai"), Some("OpenAI"));
    }

    #[test]
    fn icon_colors_are_hex() {
        for (value, _) in ICON_COLORS {
            assert!(value.starts_with('#') && value.len() == 7, "{value}");
            assert!(value[1..].chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
}
