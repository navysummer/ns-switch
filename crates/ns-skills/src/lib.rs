//! ns-skills：Skills 统一管理、跨工具同步。
//!
//! 每个工具把自己的 skills 存在 `~/.<tool>/skills/<name>/SKILL.md`（目录 +
//! 前端 YAML 元数据）。本 crate 提供统一 [`Skill`] 模型与每工具读写适配器，
//! 供 M7 统一 Skills 面板使用。

pub mod manager;
pub mod model;
pub mod tools;
