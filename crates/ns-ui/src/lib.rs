//! ns-ui — GPUI 界面层：主题令牌、通用组件、侧栏、供应商卡片。
//!
//! 界面风格遵循 `AGENT.md` 第 8 章（暖墨金深色主题，参照 ns-embed-ide）。
//! 本 crate 依赖 GPUI，核心 crate（ns-store/ns-live/ns-presets）不依赖本 crate。

pub mod theme;
pub mod widgets;
