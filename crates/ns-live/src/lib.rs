//! 切换引擎：地板清单、供应商→目标值投影、保序补丁改写（JSON/TOML/.env）、首写备份。

pub mod toml;
mod dotenv;
mod floor;
mod json;
mod jsonc;
mod patch;
mod project;
mod switcher;
mod yaml;
pub use yaml::{YamlClearScope, YamlListRemove, YamlListUpsert, YamlPatch};

pub use dotenv::EnvPatch;
pub use floor::*;
pub use json::{ClearScope, JsonPatch, JsonStyle};
pub use patch::{KeyPath, LivePatch, LiveWriteError, decode_utf8};
pub use project::*;
pub use switcher::*;
pub use toml::{TomlClearScope, TomlPatch};
#[cfg(test)]
mod tests;
