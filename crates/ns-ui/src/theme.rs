//! 暖墨金设计令牌（移植自 ns-embed-ide，见 AGENT.md 第 8 章）。

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
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::System => "system",
        }
    }

    /// 实际应用的模式（System 按当前系统外观解析；macOS 默认深色）。
    pub fn effective(&self) -> Self {
        match self {
            Self::System => {
                // macOS：`defaults read -g AppleInterfaceStyle` 含 "Dark" 为深色。
                let dark = std::process::Command::new("defaults")
                    .args(["read", "-g", "AppleInterfaceStyle"])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).contains("Dark"))
                    .unwrap_or(false);
                if dark {
                    Self::Dark
                } else {
                    Self::Light
                }
            }
            other => *other,
        }
    }
}

/// 当前模式的实际调色板（0 = 深色，1 = 浅色）。
pub fn palette(mode: ThemeMode) -> Palette {
    match mode.effective() {
        ThemeMode::Dark => Palette::dark(),
        ThemeMode::Light => Palette::light(),
        ThemeMode::System => Palette::dark(),
    }
}

/// 一组主题颜色。
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg_0: u32,
    pub bg_1: u32,
    pub bg_2: u32,
    pub panel: u32,
    pub text: u32,
    pub text_dim: u32,
    pub text_faint: u32,
    pub accent: u32,
    pub stroke: u32,
}

impl Palette {
    pub const fn dark() -> Self {
        Self {
            bg_0: BG_0,
            bg_1: BG_1,
            bg_2: BG_2,
            panel: PANEL_SOLID,
            text: TEXT,
            text_dim: TEXT_DIM,
            text_faint: TEXT_FAINT,
            accent: ACCENT,
            stroke: STROKE,
        }
    }

    pub const fn light() -> Self {
        Self {
            bg_0: LIGHT_BG_0,
            bg_1: LIGHT_BG_1,
            bg_2: LIGHT_BG_2,
            panel: LIGHT_PANEL,
            text: LIGHT_TEXT,
            text_dim: LIGHT_TEXT_DIM,
            text_faint: LIGHT_TEXT_FAINT,
            accent: LIGHT_ACCENT,
            stroke: LIGHT_STROKE,
        }
    }
}

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

pub fn app_background() -> Background {
    linear_gradient(
        160.,
        linear_color_stop(color(BG_1), 0.),
        linear_color_stop(color(BG_0), 1.),
    )
}

pub fn gold_gradient() -> Background {
    linear_gradient(
        135.,
        linear_color_stop(color(ACCENT_3), 0.),
        linear_color_stop(color(ACCENT_2), 1.),
    )
}
