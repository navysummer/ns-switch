//! 环境变量冲突检测（对齐 cc-switch `env_checker`）。
//!
//! 客户端会优先读取 `ANTHROPIC_BASE_URL`、`OPENAI_API_KEY` 等环境变量，
//! 用户在 shell 配置里 `export` 的旧值会覆盖 ns-switch 写入的配置，表现为
//! 「切换了供应商但没生效」。这里扫描 `~/.zshrc` 等文件找出这些导出，
//! 并支持删除对应行（先留 `.bak` 备份，删除前界面二次确认）。

use std::path::{Path, PathBuf};

use ns_presets::Tool;

/// 一条环境变量冲突。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvConflict {
    /// 变量名（如 `ANTHROPIC_BASE_URL`）。
    pub var_name: String,
    /// 变量值（已去掉引号）。
    pub var_value: String,
    /// 定义该变量的文件路径。
    pub source_path: String,
    /// 行号（1 起）。
    pub line_no: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keyword {
    Exact(&'static str),
    Prefix(&'static str),
}

/// 工具相关的环境变量关键字（与各客户端读取的变量一致）。
fn keywords_for(tool: Tool) -> Vec<Keyword> {
    match tool {
        Tool::Claude | Tool::ClaudeDesktop => vec![Keyword::Prefix("ANTHROPIC")],
        Tool::Codex => vec![Keyword::Prefix("OPENAI")],
        Tool::Gemini => vec![
            Keyword::Prefix("GEMINI"),
            Keyword::Prefix("GOOGLE_GEMINI"),
        ],
        Tool::GrokBuild => vec![
            Keyword::Exact("XAI_API_KEY"),
            Keyword::Exact("GROK_DEFAULT_MODEL"),
        ],
        _ => Vec::new(),
    }
}

fn matches(name: &str, keywords: &[Keyword]) -> bool {
    let upper = name.to_uppercase();
    keywords.iter().any(|k| match k {
        Keyword::Exact(n) => upper == *n,
        Keyword::Prefix(p) => upper.starts_with(p),
    })
}

/// 待扫描的 shell 配置文件（不存在的会被跳过）。
pub fn shell_config_files(home: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for name in [
        ".zshrc",
        ".zprofile",
        ".zshenv",
        ".bashrc",
        ".bash_profile",
        ".profile",
    ] {
        let p = home.join(name);
        if p.is_file() {
            files.push(p);
        }
    }
    // Linux 全局 profile（macOS 通常不存在）。
    let etc = PathBuf::from("/etc/profile");
    if etc.is_file() {
        files.push(etc);
    }
    files
}

/// 从一行 shell 配置里解析 `export NAME=VALUE` / `NAME=VALUE`。
///
/// 返回 `(变量名, 值)`；值去引号与首尾空白。不支持的行返回 None。
pub fn parse_export(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let rest = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    let eq = rest.find('=')?;
    let name = rest[..eq].trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        || name.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        return None;
    }
    let mut value = rest[eq + 1..].trim().to_string();
    // 去掉包裹引号（简单场景：整段被同一个引号包住）。
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[value.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            value = value[1..value.len() - 1].to_string();
        } else if first == '"' {
            // 行内注释/拼接：取首个引号段。
            if let Some(end) = value[1..].find('"') {
                value = value[1..1 + end].to_string();
            }
        } else if first == '\'' {
            if let Some(end) = value[1..].find('\'') {
                value = value[1..1 + end].to_string();
            }
        }
    }
    Some((name.to_string(), value))
}

/// 扫描给定文件，返回匹配关键字的导出行。
pub fn scan_files(files: &[PathBuf], tool: Tool) -> Vec<EnvConflict> {
    let keywords = keywords_for(tool);
    if keywords.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for path in files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            let Some((name, value)) = parse_export(line) else {
                continue;
            };
            if matches(&name, &keywords) {
                out.push(EnvConflict {
                    var_name: name,
                    var_value: value,
                    source_path: path.display().to_string(),
                    line_no: i + 1,
                });
            }
        }
    }
    out
}

/// 扫描 `home` 下 shell 配置里与 `tool` 相关的环境变量冲突。
pub fn conflicts_for_tool(home: &Path, tool: Tool) -> Vec<EnvConflict> {
    scan_files(&shell_config_files(home), tool)
}

/// 删除某个文件的指定行（`var_name` 必须仍匹配，避免改错行）。
///
/// 写回前保留 `<file>.bak`；文件为空时也保留备份。
pub fn remove_line(path: &Path, var_name: &str, line_no: usize) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut lines: Vec<&str> = text.lines().collect();
    if line_no == 0 || line_no > lines.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("行号越界：{line_no}"),
        ));
    }
    let parsed = parse_export(lines[line_no - 1]);
    match parsed {
        Some((name, _)) if name.eq_ignore_ascii_case(var_name) => {}
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("第 {line_no} 行已不是 {var_name} 的定义，已中止"),
            ));
        }
    }
    let backup = path.with_extension(format!(
        "{}bak",
        path.extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    std::fs::copy(path, &backup)?;
    lines.remove(line_no - 1);
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, out)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{conflicts_for_tool, parse_export, remove_line};
    use ns_presets::Tool;

    #[test]
    fn parses_export_variants() {
        assert_eq!(
            parse_export("export ANTHROPIC_BASE_URL=https://x.example"),
            Some(("ANTHROPIC_BASE_URL".into(), "https://x.example".into()))
        );
        assert_eq!(
            parse_export("  OPENAI_API_KEY='sk-123'"),
            Some(("OPENAI_API_KEY".into(), "sk-123".into()))
        );
        assert_eq!(
            parse_export("export GEMINI_API_KEY=\"g-key\" # comment"),
            Some(("GEMINI_API_KEY".into(), "g-key".into()))
        );
        assert_eq!(parse_export("# export ANTHROPIC_API_KEY=x"), None);
        assert_eq!(parse_export("export PATH=$PATH:/usr/local/bin"), Some(("PATH".into(), "$PATH:/usr/local/bin".into())));
        assert_eq!(parse_export("echo hello"), None);
        assert_eq!(parse_export("export =x"), None);
    }

    #[test]
    fn scans_shell_config_for_keywords() {
        let dir = std::env::temp_dir().join(format!("ns-envcheck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".zshrc"),
            "export ANTHROPIC_BASE_URL=https://old.example\nexport PATH=/usr/bin\nexport OPENAI_API_KEY=sk-x\n",
        )
        .unwrap();
        std::fs::write(dir.join(".profile"), "export GEMINI_API_KEY=g\n").unwrap();
        let claude = conflicts_for_tool(&dir, Tool::Claude);
        assert_eq!(claude.len(), 1);
        assert_eq!(claude[0].var_name, "ANTHROPIC_BASE_URL");
        assert_eq!(claude[0].var_value, "https://old.example");
        assert_eq!(claude[0].line_no, 1);
        assert!(claude[0].source_path.ends_with(".zshrc"));
        // Codex 只看 OPENAI_*。
        let codex = conflicts_for_tool(&dir, Tool::Codex);
        assert_eq!(codex.len(), 1);
        assert_eq!(codex[0].var_name, "OPENAI_API_KEY");
        // Gemini 只看 GEMINI_*。
        let gemini = conflicts_for_tool(&dir, Tool::Gemini);
        assert_eq!(gemini.len(), 1);
        assert_eq!(gemini[0].source_path, dir.join(".profile").display().to_string());
        // 其他工具无关键字 → 空。
        assert!(conflicts_for_tool(&dir, Tool::Opencode).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_line_keeps_backup_and_refuses_mismatch() {
        let dir = std::env::temp_dir().join(format!("ns-envcheck-rm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(".zshrc");
        std::fs::write(
            &file,
            "export ANTHROPIC_BASE_URL=https://old.example\nexport ANTHROPIC_API_KEY=k\n",
        )
        .unwrap();
        remove_line(&file, "ANTHROPIC_API_KEY", 2).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("ANTHROPIC_API_KEY"));
        assert!(text.contains("ANTHROPIC_BASE_URL"));
        assert!(dir.join(".zshrc.bak").exists(), "删除前应留 .bak 备份");
        // 行号已不是该变量 → 拒绝。
        assert!(remove_line(&file, "ANTHROPIC_API_KEY", 2).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
