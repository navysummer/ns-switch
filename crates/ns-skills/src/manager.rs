//! Skills 管理器：跨工具读取/写入统一 [`Skill`] 列表。
//!
//! 面向 UI 的一次性入口：读某工具全部技能、写回（新增/更新/删除/启停）、
//! 跨工具同步、从本地目录/GitHub 仓库安装。每次写只动自己管的技能目录。

use std::path::{Path, PathBuf};

use ns_presets::Tool;

use crate::model::{Skill, SkillPaths, SyncStrategy};
use crate::tools;

/// 读某工具全部技能。
pub fn read_tool(tool: Tool, paths: &SkillPaths) -> Result<Vec<Skill>, String> {
    tools::read_tool(tool, paths)
}

/// 写一个技能到某工具。
pub fn write_tool(
    tool: Tool,
    paths: &SkillPaths,
    skill: &Skill,
    strategy: SyncStrategy,
) -> Result<(), String> {
    match strategy {
        SyncStrategy::Copy | SyncStrategy::Auto => tools::write_tool(tool, paths, skill).map(|_| ()),
        SyncStrategy::Symlink => {
            let target = tools::skills_dir(tool, paths).join(&skill.name);
            let skill_md = target.join("SKILL.md");
            if let Some(src) = &skill.source_path {
                if src.exists() {
                    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                    if skill_md.exists() {
                        std::fs::remove_file(&skill_md).map_err(|e| e.to_string())?;
                    }
                    std::os::unix::fs::symlink(src, &skill_md)
                        .map_err(|e| format!("符号链接 {} 失败: {e}", skill_md.display()))?;
                    return Ok(());
                }
            }
            tools::write_tool(tool, paths, skill).map(|_| ())
        }
    }
}

/// 删除某工具的一个技能。
pub fn remove_tool(tool: Tool, paths: &SkillPaths, name: &str) -> Result<(), String> {
    tools::remove_tool(tool, paths, name)
}

/// 同步一个技能到一组工具。
pub fn sync_to(
    skill: &Skill,
    source: Tool,
    targets: &[Tool],
    paths: &SkillPaths,
    strategy: SyncStrategy,
) -> Result<Vec<String>, String> {
    let mut applied = Vec::new();
    for &tool in targets {
        if tool == source {
            continue;
        }
        write_tool(tool, paths, skill, strategy)?;
        applied.push(tool.id_str().to_string());
    }
    Ok(applied)
}

/// 从本地目录安装一个技能（`<dir>/SKILL.md` 或 `<dir>/<name>.md`）。
pub fn install_from_dir(dir: &Path, paths: &SkillPaths, targets: &[Tool]) -> Result<String, String> {
    let skill_md = dir.join("SKILL.md");
    let file = if skill_md.exists() {
        skill_md
    } else {
        // 允许直接选一个 .md。
        let mut cands: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|e| {
                e.flatten()
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        cands.retain(|p| p.file_name().and_then(|s| s.to_str()) != Some("SKILL.md"));
        if cands.len() != 1 {
            return Err("目录需含一个 SKILL.md（或唯一 .md 文件）".into());
        }
        cands.pop().unwrap()
    };
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let (meta, body) = split_meta(&text);
    let name = meta
        .get("name")
        .cloned()
        .unwrap_or_else(|| {
            file.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });
    if name.is_empty() {
        return Err("技能缺少 name".into());
    }
    let skill = Skill {
        name,
        description: meta.get("description").cloned().unwrap_or_default(),
        allowed_tools: Vec::new(),
        body,
        source_path: Some(file.clone()),
        enabled: true,
    };
    for &tool in targets {
        write_tool(tool, paths, &skill, SyncStrategy::Auto)?;
    }
    Ok(skill.name)
}

/// 从 GitHub 仓库安装（`https://github.com/<owner>/<repo>`，取 `skills/<name>`）。
pub fn install_from_github(
    repo_url: &str,
    name: &str,
    paths: &SkillPaths,
    targets: &[Tool],
) -> Result<String, String> {
    // 依赖 git 二进制拉取；暂用浅克隆到临时目录。
    let slug = repo_url
        .trim_end_matches('/')
        .rsplit('/')
        .take(2)
        .collect::<Vec<_>>()
        .iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("-");
    let tmp = std::env::temp_dir().join(format!("ns-skills-git-{slug}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", repo_url])
        .arg(&tmp)
        .status()
        .map_err(|e| format!("git 不可用: {e}"))?;
    if !status.success() {
        return Err(format!("git clone 失败（{repo_url}）"));
    }
    let src = tmp.join("skills").join(name);
    let installed = if src.join("SKILL.md").exists() {
        install_from_dir(&src, paths, targets)?
    } else {
        // 仓库根直接是技能文件。
        install_from_dir(&tmp, paths, targets)?
    };
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(installed)
}

/// 简易前端元数据解析（与 tools 保持一致，避免循环依赖）。
fn split_meta(text: &str) -> (std::collections::BTreeMap<String, String>, String) {
    let trimmed = text.trim_start_matches('\u{feff}');
    let body_start = trimmed.find("---\n").map(|i| i + 4).unwrap_or(0);
    let body_end = if body_start > 0 {
        trimmed[body_start..].find("\n---").map(|i| body_start + i)
    } else {
        None
    };
    if body_start == 0 || body_end.is_none() {
        return (std::collections::BTreeMap::new(), trimmed.to_string());
    }
    let meta_text = &trimmed[4..body_end.unwrap()];
    let body = trimmed[body_end.unwrap() + 4..].to_string();
    let mut meta = std::collections::BTreeMap::new();
    for line in meta_text.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once(':') {
            meta.insert(
                k.trim().to_string(),
                v.trim().trim_matches('"').trim_matches('\'').to_string(),
            );
        }
    }
    (meta, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Skill;
    // (PathBuf 未直接使用；路径经 SkillPaths 解析。)
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static DIR_SEQ: AtomicU32 = AtomicU32::new(0);

    fn temp_paths() -> SkillPaths {
        let base = std::env::temp_dir().join(format!(
            "ns-skills-test-{}-{}",
            std::process::id(),
            DIR_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base);
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", base.join("claude"));
            std::env::set_var("CODEX_HOME", base.join("codex"));
            std::env::set_var("GEMINI_CLI_HOME", base.clone());
        }
        SkillPaths { home: base }
    }

    #[test]
    fn read_write_remove_sync() {
        let _guard = TEST_LOCK.lock().unwrap();
        let paths = temp_paths();
        // 初始为空。
        for tool in [Tool::Claude, Tool::Codex, Tool::Gemini] {
            assert!(read_tool(tool, &paths).unwrap().is_empty());
        }
        let skill = Skill {
            name: "code-review".into(),
            description: "审查代码".into(),
            body: "# 步骤\n\n1. 检查\n".into(),
            ..Default::default()
        };
        // 写入 Claude，读回验证。
        write_tool(Tool::Claude, &paths, &skill, SyncStrategy::Copy).unwrap();
        let got = read_tool(Tool::Claude, &paths).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "code-review");
        assert!(got[0].body.contains("1. 检查"));
        // 同步到 Codex + Gemini。
        let applied = sync_to(&skill, Tool::Claude, &[Tool::Codex, Tool::Gemini], &paths, SyncStrategy::Copy).unwrap();
        assert_eq!(applied.len(), 2);
        assert_eq!(read_tool(Tool::Codex, &paths).unwrap().len(), 1);
        assert_eq!(read_tool(Tool::Gemini, &paths).unwrap().len(), 1);
        // 删除。
        remove_tool(Tool::Codex, &paths, "code-review").unwrap();
        assert!(read_tool(Tool::Codex, &paths).unwrap().is_empty());
    }

    #[test]
    fn install_from_dir_roundtrip() {
        let _guard = TEST_LOCK.lock().unwrap();
        let paths = temp_paths();
        let src = std::env::temp_dir().join(format!("ns-skill-src-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("SKILL.md"),
            "---\nname: git-helper\ndescription: git 辅助\n---\n# 用法\n",
        )
        .unwrap();
        let name = install_from_dir(&src, &paths, &[Tool::Claude, Tool::Gemini]).unwrap();
        assert_eq!(name, "git-helper");
        let claude = read_tool(Tool::Claude, &paths).unwrap();
        assert!(claude.iter().any(|s| s.name == "git-helper"));
        let _ = std::fs::remove_dir_all(&src);
    }
}
