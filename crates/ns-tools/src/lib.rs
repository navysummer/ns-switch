//! ns-tools - 各 CLI 工具版本探测、安装/升级、重复安装诊断。
//!
//! 支持工具：Claude Code / Codex / Gemini / Grok / GitHub Copilot。
//! 探测：`which` 定位 + `--version` 读版本；重复安装诊断枚举 PATH 中同名可执行。

use std::path::PathBuf;

/// 一个受管 CLI 工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    ClaudeCode,
    Codex,
    Gemini,
    Grok,
    Copilot,
}

impl Tool {
    /// 全部工具（显示顺序）。
    pub const ALL: [Tool; 5] = [
        Tool::ClaudeCode,
        Tool::Codex,
        Tool::Gemini,
        Tool::Grok,
        Tool::Copilot,
    ];

    /// 可执行名。
    pub fn bin(&self) -> &'static str {
        match self {
            Tool::ClaudeCode => "claude",
            Tool::Codex => "codex",
            Tool::Gemini => "gemini",
            Tool::Grok => "grok",
            Tool::Copilot => "copilot",
        }
    }

    /// 显示名。
    pub fn name(&self) -> &'static str {
        match self {
            Tool::ClaudeCode => "Claude Code",
            Tool::Codex => "Codex",
            Tool::Gemini => "Gemini CLI",
            Tool::Grok => "Grok",
            Tool::Copilot => "GitHub Copilot",
        }
    }

    /// 安装/升级命令（npm 默认）。
    pub fn install_command(&self) -> &'static str {
        match self {
            Tool::ClaudeCode => "npm install -g @anthropic-ai/claude-code",
            Tool::Codex => "npm install -g @openai/codex",
            Tool::Gemini => "npm install -g @google/gemini-cli",
            Tool::Grok => "npm install -g @grok-ai/cli",
            Tool::Copilot => "npm install -g @github/copilot",
        }
    }
}

/// 一个已安装的可执行。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolInstall {
    /// 可执行名。
    pub bin: String,
    /// PATH 中的解析路径。
    pub path: String,
    /// 解析后的版本（`--version` 首行，去前缀）。
    pub version: Option<String>,
}

/// 探测某工具：PATH 中所有同名可执行（重复安装诊断用）。
pub fn detect_tool(tool: Tool) -> Vec<ToolInstall> {
    let bin = tool.bin();
    let mut found = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(bin);
            if cand.is_file() && is_executable(&cand) {
                let version = read_version(&cand);
                found.push(ToolInstall {
                    bin: bin.to_string(),
                    path: cand.to_string_lossy().into_owned(),
                    version,
                });
            }
        }
    }
    found
}

/// 探测全部工具（每工具一个条目，取第一个命中）。
pub fn detect_all() -> Vec<(Tool, Option<ToolInstall>)> {
    Tool::ALL
        .iter()
        .map(|t| (*t, detect_tool(*t).into_iter().next()))
        .collect()
}


/// 诊断某工具是否重复安装（PATH 中出现 >1 个同名可执行）。
pub fn duplicate_installations(tool: Tool) -> Vec<ToolInstall> {
    let all = detect_tool(tool);
    if all.len() > 1 {
        all
    } else {
        Vec::new()
    }
}

/// 读版本：运行 `<bin> --version`，取首行，剥离 `name/` 前缀。
fn read_version(path: &PathBuf) -> Option<String> {
    let out = std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let line = text.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return None;
    }
    let version = line.rsplit('/').next().unwrap_or(line).trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

/// 是否可执行（Unix 权限位判断；Windows 靠扩展名）。
fn is_executable(path: &PathBuf) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_ascii_lowercase().as_str(), "exe" | "cmd" | "bat"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_metadata() {
        assert_eq!(Tool::ClaudeCode.bin(), "claude");
        assert_eq!(Tool::Codex.bin(), "codex");
        assert!(Tool::ClaudeCode.install_command().contains("claude-code"));
        assert_eq!(Tool::ALL.len(), 5);
    }

    #[test]
    fn version_parse_strips_prefix() {
        // read_version 需真实可执行；这里测剥离逻辑（模拟）。
        let line = "claude/2.1.3 (Claude Code)";
        let version = line.rsplit('/').next().unwrap_or(line).trim();
        assert_eq!(version, "2.1.3 (Claude Code)");
    }

    #[test]
    fn duplicate_detection_empty_when_single() {
        // 无 PATH 中工具时返回空（不 panic）。
        let dup = duplicate_installations(Tool::Grok);
        assert!(dup.len() <= 1);
    }
}
