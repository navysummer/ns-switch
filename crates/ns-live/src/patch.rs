//! 补丁通用类型：键路径、错误、`LivePatch` trait。

use std::fmt;
use std::path::{Path, PathBuf};

/// 键路径（如 `["env", "ANTHROPIC_BASE_URL"]`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyPath(pub Vec<String>);

impl KeyPath {
    pub fn root() -> Self {
        Self(Vec::new())
    }

    pub fn child(&self, key: &str) -> Self {
        let mut p = self.0.clone();
        p.push(key.to_string());
        Self(p)
    }

    pub fn push(&mut self, key: impl Into<String>) {
        self.0.push(key.into());
    }
}

impl From<&str> for KeyPath {
    fn from(s: &str) -> Self {
        // "env.ANTHROPIC_BASE_URL" → 按 "." 分段；带引号/特殊段暂不支持（内部使用）。
        KeyPath(s.split('.').map(|s| s.to_string()).collect())
    }
}

impl fmt::Display for KeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("."))
    }
}

/// 补丁错误。
#[derive(Debug)]
pub enum LiveWriteError {
    /// 文件不是 UTF-8。
    Utf8 { path: PathBuf },
    /// 内容无法解析。
    Parse {
        path: PathBuf,
        line: usize,
        column: usize,
        message: String,
    },
    /// 形状不对（顶层不是对象/表）。
    Shape {
        path: PathBuf,
        key_path: KeyPath,
        expected: &'static str,
    },
    /// 路径中某个父节点不是对象/表。
    NotObject { path: PathBuf, key_path: KeyPath },
    /// 磁盘写入失败。
    Io { path: PathBuf, source: std::io::Error },
    /// 写后重读与计划不一致（外部并发写）。
    Verify { path: PathBuf },
    /// 内容/配置不满足写入前提（如缺少端点/密钥），不应猜测改写。
    Invalid { path: PathBuf, message: String },
}

impl std::error::Error for LiveWriteError {}

impl fmt::Display for LiveWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utf8 { path } => write!(f, "{} 不是 UTF-8", path.display()),
            Self::Parse {
                path,
                line,
                column,
                message,
            } => write!(f, "{}:{}:{} 解析失败: {message}", path.display(), line, column),
            Self::Shape {
                path,
                key_path,
                expected,
            } => write!(f, "{} 的 {} 应为 {expected}", path.display(), key_path),
            Self::NotObject { path, key_path } => {
                write!(f, "{} 的 {} 不是对象", path.display(), key_path)
            }
            Self::Io { path, source } => write!(f, "{} 写入失败: {source}", path.display()),
            Self::Verify { path } => write!(f, "{} 写后重读不一致（外部并发写）", path.display()),
            Self::Invalid { path, message } => {
                write!(f, "{} 无法写入: {message}", path.display())
            }
        }
    }
}

/// 解码 UTF-8（BOM 剥离由各格式处理）。
pub fn decode_utf8(path: &Path, bytes: &[u8]) -> Result<String, LiveWriteError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| LiveWriteError::Utf8 {
        path: path.to_path_buf(),
    })
}

/// 读取 JSON/JSONC 文档（宽松解析，允许注释/尾逗号）。用于导入当前客户端配置。
pub fn read_json_document(path: &Path) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("读取 {} 失败: {err}", path.display()))?;
    serde_json_lenient::from_str(&text)
        .map_err(|err| format!("解析 {} 失败: {err}", path.display()))
}

/// 一个能对文件内容做补丁的格式适配器。
pub trait LivePatch {
    /// 应用补丁到 `pre`（写前内容）；`None` 表示文件不存在（视为空底）。
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError>;
}
