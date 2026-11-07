//! ns-store — `~/.ns-switch/config.json` SSOT 读写。
//!
//! 所有状态以 config.json 为准（SSOT 模式，不写供应商副本文件）。
//! 原子写：临时文件 + rename；启动时校验并回退损坏文件。

mod store;

pub use store::*;
