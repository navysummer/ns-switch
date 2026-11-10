//! 暖墨金设计令牌（移植自 ns-embed-ide，见 AGENT.md 第 8 章）。

use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{Background, Hsla, linear_color_stop, linear_gradient, rgb};

pub const BG_0: u32 = 0x0f0d0a;
pub const BG_1: u32 = 0x17130d;
pub const BG_2: u32 = 0x211a11;
pub const PANEL_SOLID: u32 = 0x201a11;
pub const TEXT: u32 = 0xf0e6d2;
pub const TEXT_DIM: u32 = 0xb8a98c;
pub const TEXT_FAINT: u32 = 0x8a7c63;
pub const ACCENT: u32 = 0xc9a25f;
pub const ACCENT_2: u32 = 0xa57a3d;
pub const ACCENT_3: u32 = 0xd8b97f;
pub const JADE: u32 = 0x6f9b8a;
pub const CINNABAR: u32 = 0xb0564a;
pub const DANGER: u32 = 0xcf6b5e;
pub const STROKE: u32 = 0xdbb579;
pub const DARK_TEXT: u32 = 0x221407;

// ===========================================================================
// 浅色主题令牌（暖金浅色变体）。
// ===========================================================================
pub const LIGHT_BG_0: u32 = 0xf7f3ea;
pub const LIGHT_BG_1: u32 = 0xfdfaf3;
pub const LIGHT_BG_2: u32 = 0xefe8d9;
pub const LIGHT_PANEL: u32 = 0xf2ecdf;
pub const LIGHT_TEXT: u32 = 0x2a2118;
pub const LIGHT_TEXT_DIM: u32 = 0x5c5142;
pub const LIGHT_TEXT_FAINT: u32 = 0x8a7c66;
pub const LIGHT_ACCENT: u32 = 0x8a6a2f;
pub const LIGHT_ACCENT_2: u32 = 0xa57a3d;
pub const LIGHT_ACCENT_3: u32 = 0xb8914e;
pub const LIGHT_STROKE: u32 = 0xc9a86a;

/// 主题模式（设置页切换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
}

impl ThemeMode {
    /// 从存储字符串解析（未知 → Dark）。
    pub fn parse(s: &str) -> Self {
        match s {
            "light" => Self::Light,
            "system" => Self::System,
            _ => Self::Dark,
        }
    }

    /// 存储字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::System => "system",
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Light,
            2 => Self::System,
            _ => Self::Dark,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Dark => 0,
            Self::Light => 1,
            Self::System => 2,
        }
    }

    /// 实际应用的模式（System 按当前系统外观解析）。
    pub fn effective(self) -> Self {
        match self {
            Self::System => {
                // macOS：`defaults read -g AppleInterfaceStyle` 含 "Dark" 为深色；失败默认浅色。
                let dark = std::process::Command::new("defaults")
                    .args(["read", "-g", "AppleInterfaceStyle"])
                    .output()
                    .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("Dark"))
                    .unwrap_or(false);
                if dark { Self::Dark } else { Self::Light }
            }
            other => other,
        }
    }
}

/// 全局活动主题模式（配置值：dark/light/system）。
static ACTIVE_MODE: AtomicU8 = AtomicU8::new(0);
/// 当前有效模式（解析 system 后：0=dark,1=light），避免渲染时重复探测系统外观。
static ACTIVE_EFFECTIVE: AtomicU8 = AtomicU8::new(0);

/// 设置全局活动主题模式（由应用在初始化/设置变更时调用）。
pub fn set_active_mode(mode: ThemeMode) {
    ACTIVE_MODE.store(mode.to_u8(), Ordering::Relaxed);
    let eff = match mode.effective() {
        ThemeMode::Light => 1,
        _ => 0,
    };
    ACTIVE_EFFECTIVE.store(eff, Ordering::Relaxed);
}

/// system 模式下按 OS 外观更新有效模式（P1-1 运行时跟随）。
///
/// 仅在 `ACTIVE_MODE == System` 时由外观变化回调调用；dark=true 表示系统
/// 切到深色。设置页后续切换模式会经 `set_active_mode` 重算。
pub fn set_system_effective(dark: bool) {
    if ACTIVE_MODE.load(Ordering::Relaxed) != ThemeMode::System.to_u8() {
        return;
    }
    ACTIVE_EFFECTIVE.store(if dark { 0 } else { 1 }, Ordering::Relaxed);
 }

/// 读取全局活动主题模式（配置值）。
pub fn active_mode() -> ThemeMode {
    ThemeMode::from_u8(ACTIVE_MODE.load(Ordering::Relaxed))
}

/// 当前有效调色板（使用缓存的 effective 结果，渲染安全）。
pub fn active_palette() -> Palette {
    if ACTIVE_EFFECTIVE.load(Ordering::Relaxed) == 1 {
        Palette::light()
    } else {
        Palette::dark()
    }
}

/// 按模式返回调色板（解析 system；用于显式场景）。
pub fn palette(mode: ThemeMode) -> Palette {
    match mode.effective() {
        ThemeMode::Dark | ThemeMode::System => Palette::dark(),
        ThemeMode::Light => Palette::light(),
    }
}

/// 一组主题颜色。
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg_0: u32,
    pub bg_1: u32,
    pub bg_2: u32,
    pub panel: u32,
    /// 卡片/输入等次级表面（深色暖棕 / 浅色暖米）。
    pub surface: u32,
    /// 次级表面强调态。
    pub surface_alt: u32,
    pub text: u32,
    pub text_dim: u32,
    pub text_faint: u32,
    pub accent: u32,
    pub stroke: u32,
    pub jade: u32,
    pub danger: u32,
    pub cinnabar: u32,
}

impl Palette {
    pub const fn dark() -> Self {
        Self {
            bg_0: BG_0,
            bg_1: BG_1,
            bg_2: BG_2,
            panel: PANEL_SOLID,
            surface: 0x261d12,
            surface_alt: 0x2c2314,
            text: TEXT,
            text_dim: TEXT_DIM,
            text_faint: TEXT_FAINT,
            accent: ACCENT,
            stroke: STROKE,
            jade: JADE,
            danger: DANGER,
            cinnabar: CINNABAR,
        }
    }

    pub const fn light() -> Self {
        Self {
            bg_0: LIGHT_BG_0,
            bg_1: LIGHT_BG_1,
            bg_2: LIGHT_BG_2,
            panel: LIGHT_PANEL,
            surface: 0xf5f0e6,
            surface_alt: 0xece4d4,
            text: LIGHT_TEXT,
            text_dim: LIGHT_TEXT_DIM,
            text_faint: LIGHT_TEXT_FAINT,
            accent: LIGHT_ACCENT,
            stroke: LIGHT_STROKE,
            jade: JADE,
            danger: DANGER,
            cinnabar: CINNABAR,
        }
    }
}

// === 动态颜色访问器（读取全局活动主题）===

pub fn bg_0() -> Hsla { color(active_palette().bg_0) }
pub fn bg_1() -> Hsla { color(active_palette().bg_1) }
pub fn bg_2() -> Hsla { color(active_palette().bg_2) }
pub fn panel() -> Hsla { color(active_palette().panel) }
pub fn text() -> Hsla { color(active_palette().text) }
pub fn text_dim() -> Hsla { color(active_palette().text_dim) }
pub fn text_faint() -> Hsla { color(active_palette().text_faint) }
pub fn accent() -> Hsla { color(active_palette().accent) }
pub fn stroke() -> Hsla { color(active_palette().stroke) }
pub fn jade() -> Hsla { color(active_palette().jade) }
pub fn danger() -> Hsla { color(active_palette().danger) }
pub fn cinnabar() -> Hsla { color(active_palette().cinnabar) }
/// 金色按钮上的深色文字（两种主题均保持深色以保证对比度）。
pub fn dark_text() -> Hsla { color(DARK_TEXT) }

pub fn stroke_a(opacity: f32) -> Hsla { alpha(active_palette().stroke, opacity) }
pub fn accent_a(opacity: f32) -> Hsla { alpha(active_palette().accent, opacity) }
pub fn text_a(opacity: f32) -> Hsla { alpha(active_palette().text, opacity) }
pub fn text_dim_a(opacity: f32) -> Hsla { alpha(active_palette().text_dim, opacity) }
pub fn bg_1_a(opacity: f32) -> Hsla { alpha(active_palette().bg_1, opacity) }
pub fn bg_2_a(opacity: f32) -> Hsla { alpha(active_palette().bg_2, opacity) }
pub fn panel_a(opacity: f32) -> Hsla { alpha(active_palette().panel, opacity) }
pub fn jade_a(opacity: f32) -> Hsla { alpha(active_palette().jade, opacity) }
pub fn danger_a(opacity: f32) -> Hsla { alpha(active_palette().danger, opacity) }
pub fn surface_a(opacity: f32) -> Hsla { alpha(active_palette().surface, opacity) }
pub fn surface_alt_a(opacity: f32) -> Hsla { alpha(active_palette().surface_alt, opacity) }

/// 按模式返回背景（渐变）。
pub fn app_background_for(mode: ThemeMode) -> Background {
    let p = palette(mode);
    linear_gradient(
        160.,
        linear_color_stop(color(p.bg_1), 0.),
        linear_color_stop(color(p.bg_0), 1.),
    )
}

pub const UI_FONT: &str = "PingFang SC";
pub const SERIF_FONT: &str = "Songti SC";
pub const MONO_FONT: &str = "Menlo";

pub fn color(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn alpha(hex: u32, opacity: f32) -> Hsla {
    color(hex).opacity(opacity)
}

/// 当前活动主题的应用背景（渐变）。
pub fn app_background() -> Background {
    let p = active_palette();
    linear_gradient(
        160.,
        linear_color_stop(color(p.bg_1), 0.),
        linear_color_stop(color(p.bg_0), 1.),
    )
}

pub fn gold_gradient() -> Background {
    linear_gradient(
        135.,
        linear_color_stop(color(ACCENT_3), 0.),
        linear_color_stop(color(ACCENT_2), 1.),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_effective_tracks_os_appearance() {
        // 设为 system 后，set_system_effective 更新有效模式。
        set_active_mode(ThemeMode::System);
        set_system_effective(true);
        assert_eq!(ACTIVE_EFFECTIVE.load(Ordering::Relaxed), 0);
        set_system_effective(false);
        assert_eq!(ACTIVE_EFFECTIVE.load(Ordering::Relaxed), 1);
        // 显式 dark 时忽略系统外观回调。
        set_active_mode(ThemeMode::Dark);
        set_system_effective(false);
        assert_eq!(ACTIVE_EFFECTIVE.load(Ordering::Relaxed), 0);
        // 复位为默认 dark，避免影响其他测试。
        set_active_mode(ThemeMode::Dark);
    }

    #[test]
    fn mode_parse_roundtrip() {
        assert_eq!(ThemeMode::parse("light"), ThemeMode::Light);
        assert_eq!(ThemeMode::parse("system"), ThemeMode::System);
        assert_eq!(ThemeMode::parse("dark"), ThemeMode::Dark);
        assert_eq!(ThemeMode::parse("unknown"), ThemeMode::Dark);
        assert_eq!(ThemeMode::Dark.as_str(), "dark");
        assert_eq!(ThemeMode::Light.as_str(), "light");
        assert_eq!(ThemeMode::System.as_str(), "system");
    }
}
