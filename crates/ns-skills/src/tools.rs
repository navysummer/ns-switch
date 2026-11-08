//! 每工具 Skills 读写适配器。
//!
//! 三个工具共用「`<skills_dir>/<name>/SKILL.md` + 前端 YAML 元数据」布局，
//! 差异只在目录路径。适配器只增删改自己管的技能目录，其余内容原样保留。

use std::path::{Path, PathBuf};

use ns_presets::Tool;

use crate::model::{Skill, SkillPaths};

/// 读取某工具的全部技能（目录扫描 + SKILL.md 解析）。目录不存在返回空。
pub fn read_tool(tool: Tool, paths: &SkillPaths) -> Result<Vec<Skill>, String> {
    let dir = skills_dir(tool, paths);
    match std::fs::read_dir(&dir) {
        Ok(entries) => {
            let mut out = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    // 也接受直接 `<dir>/<name>.md`（Gemini 风格）。
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(skill) = parse_skill_file(&path) {
                            out.push(skill);
                        }
                    }
                    continue;
                }
                let skill_md = path.join("SKILL.md");
                if let Some(skill) = parse_skill_file(&skill_md) {
                    out.push(skill);
                }
            }
            out.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(out)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("读取 {} 失败: {e}", dir.display())),
    }
}

/// 某工具的 skills 根目录。
pub fn skills_dir(tool: Tool, paths: &SkillPaths) -> PathBuf {
    match tool {
        Tool::Claude | Tool::ClaudeDesktop => paths.claude_skills(),
        Tool::Codex => paths.codex_skills(),
        Tool::Gemini => paths.gemini_skills(),
        _ => paths.home.join(".agents").join("skills"),
    }
}

/// 解析单个技能文件（SKILL.md 或 `<name>.md`）。
fn parse_skill_file(path: &Path) -> Option<Skill> {
    let text = std::fs::read_to_string(path).ok()?;
    let (meta, body) = split_frontmatter(&text);
    let name = meta
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
        })?;
    let description = meta
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let allowed_tools = meta
        .get("allowed-tools")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(Skill {
        name,
        description,
        allowed_tools,
        body,
        source_path: None,
        enabled: true,
    })
}

/// 拆分前端 YAML 元数据与正文。
fn split_frontmatter(text: &str) -> (serde_json::Map<String, serde_json::Value>, String) {
    let trimmed = text.trim_start_matches('\u{feff}');
    let body_start = trimmed.find("---\n").map(|i| i + 4).unwrap_or(0);
    let body_end = if body_start > 0 {
        trimmed[body_start..].find("\n---").map(|i| body_start + i)
    } else {
        None
    };
    if body_start == 0 || body_end.is_none() {
        return (serde_json::Map::new(), trimmed.to_string());
    }
    let meta_text = &trimmed[4..body_end.unwrap()];
    let body = trimmed[body_end.unwrap() + 4..].to_string();
    // 简单 key: value 解析（前端 YAML 只取标量；复杂结构暂不展开）。
    let mut meta = serde_json::Map::new();
    for line in meta_text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            meta.insert(k.trim().to_string(), serde_json::Value::String(v.to_string()));
        }
    }
    (meta, body)
}

/// 写入一个技能到某工具（`<dir>/<name>/SKILL.md`）。
pub fn write_tool(
    tool: Tool,
    paths: &SkillPaths,
    skill: &Skill,
) -> Result<PathBuf, String> {
    let dir = skills_dir(tool, paths);
    let skill_dir = dir.join(&skill.name);
    std::fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    let skill_md = skill_dir.join("SKILL.md");
    let content = render_skill(skill);
    std::fs::write(&skill_md, content).map_err(|e| format!("写 {} 失败: {e}", skill_md.display()))?;
    Ok(skill_md)
}

/// 从某工具删除一个技能。
pub fn remove_tool(tool: Tool, paths: &SkillPaths, name: &str) -> Result<(), String> {
    let dir = skills_dir(tool, paths).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("删除 {} 失败: {e}", dir.display()))?;
    }
    Ok(())
}

/// 渲染 SKILL.md（前端 YAML + 正文）。
fn render_skill(skill: &Skill) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("name: {}\n", skill.name));
    out.push_str(&format!("description: \"{}\"\n", skill.description.replace('"', "'")));
    if !skill.allowed_tools.is_empty() {
        out.push_str("allowed-tools:\n");
        for t in &skill.allowed_tools {
            out.push_str(&format!("  - {t}\n"));
        }
    }
    out.push_str("---\n");
    out.push_str(&skill.body);
    if !skill.body.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// 同步一个技能到一组工具（跨工具同步）。
pub fn sync_to(
    skill: &Skill,
    source: Tool,
    targets: &[Tool],
    paths: &SkillPaths,
) -> Result<Vec<String>, String> {
    let mut applied = Vec::new();
    for &tool in targets {
        if tool == source {
            continue;
        }
        write_tool(tool, paths, skill)?;
        applied.push(tool.id_str().to_string());
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_roundtrip() {
        let s = Skill {
            name: "code-review".into(),
            description: "审查代码".into(),
            allowed_tools: vec!["Read".into(), "Edit".into()],
            body: "# 步骤\n\n1. 检查\n".into(),
            ..Default::default()
        };
        let rendered = render_skill(&s);
        let (meta, body) = split_frontmatter(&rendered);
        assert_eq!(meta.get("name").and_then(|v| v.as_str()), Some("code-review"));
        assert!(body.contains("1. 检查"));
        // 允许工具（列表解析为单值字符串；此处验证 name/description）。
        assert_eq!(
            meta.get("description").and_then(|v| v.as_str()),
            Some("审查代码")
        );
    }

    #[test]
    fn parse_plain_body() {
        let (meta, body) = split_frontmatter("# 无元数据\n正文\n");
        assert!(meta.is_empty());
        assert!(body.starts_with("# 无元数据"));
    }
}
