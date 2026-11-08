//! ns-prompts：每工具提示词库 + Markdown 编辑、写回提示文件。
//!
//! 每个工具有一份「提示文件」（CLAUDE.md / AGENTS.md / GEMINI.md）。本 crate
//! 提供统一 [`PromptEntry`] 模型、库存储（config.json 内的 prompts 节）与
//! 写回适配器：写回前先把文件既有内容存回库，不丢失。

pub mod model;
pub mod store;
pub mod tools;
