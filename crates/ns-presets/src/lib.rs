//! ns-presets — 工具枚举、供应商分类、供应商数据模型与预设表。
//!
//! 供应商预设即数据（Rust 常量表），不硬编码进逻辑；供应商的 `settings_config`
//! 由预设模板 + 用户输入生成。本 crate 无界面依赖。

mod app;
mod icons;
mod preset;
mod provider;

pub use app::*;
pub use icons::*;
pub use preset::*;
pub use provider::*;
