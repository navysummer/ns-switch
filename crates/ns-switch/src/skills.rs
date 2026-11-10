//! M7 统一 Skills 面板。
//!
//! 以「技能」为第一视图：列出某工具的全部 Skills，可新增（本地/GitHub）、
//! 编辑、删除、启停，并一键同步到其他工具。读写经 `ns_skills::manager`。

// (安装经 GitHub URL，不直接选本地目录；无 PathBuf。)

use gpui::{AppContext, Context, Entity};
use ns_presets::Tool;
use ns_skills::manager;
use ns_skills::model::{Skill, SkillPaths, SyncStrategy};

use crate::text_field::TextField;

pub struct SkillsPanel {
    pub tool: Tool,
    pub skills: Vec<Skill>,
    pub status: String,
    pub dialog_open: bool,
    pub editing_name: Option<String>,
    pub form: SkillForm,
    pub confirm_delete: Option<String>,
    pub write_msg: String,
    pub strategy: SyncStrategy,
    /// 安装对话框是否打开（GitHub 仓库）。
    pub install_open: bool,
    pub install_url: Entity<TextField>,
    /// 本地目录安装输入。
    pub install_local_dir: Entity<TextField>,
    /// 当前安装来源。
    pub install_source: SkillInstallSource,
    /// 当前视图（已安装 / 发现）。
    pub view: SkillsView,
    /// 未保存更改确认关闭（true = 显示确认浮层）。
    pub confirm_close: bool,
    /// 是否使用共享 `~/.agents/skills` 目录（由 app 同步设置）。
    pub shared: bool,
    /// 发现页搜索输入。
    pub discover_query: Entity<TextField>,
    /// 发现页搜索结果（repo 描述行）。
    pub discover_results: Vec<(String, String)>,
    /// 发现页搜索状态/消息。
    pub discover_status: String,
}

/// Skills 安装来源（GitHub / 本地目录）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillInstallSource {
    Github,
    LocalDir,
}

/// Skills 面板视图（对齐 cc-switch 已安装/发现）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillsView {
    Installed,
    Discover,
}

/// 新增/编辑对话框表单。
pub struct SkillForm {
    pub name: Entity<TextField>,
    pub description: Entity<TextField>,
    pub enabled: bool,
    /// 允许工具列表（逗号分隔；空 = 全部允许）。
    pub allowed_tools: Entity<TextField>,
    pub body: Entity<TextField>,
}

impl SkillsPanel {
    pub fn new(cx: &mut Context<crate::app::SwitchApp>) -> Self {
        Self {
            tool: Tool::Claude,
            skills: Vec::new(),
            status: String::new(),
            dialog_open: false,
            editing_name: None,
            form: SkillForm::new(cx),
            confirm_delete: None,
            write_msg: String::new(),
            strategy: SyncStrategy::default(),
            install_open: false,
            view: SkillsView::Installed,
            install_url: cx.new(|cx| TextField::new("https://github.com/<owner>/<repo>", cx)),
            install_local_dir: cx.new(|cx| TextField::new("/path/to/skill-dir", cx)),
            install_source: SkillInstallSource::Github,
            confirm_close: false,
            shared: false,
            discover_query: cx.new(|cx| TextField::new("搜索 skills.sh / GitHub…", cx)),
            discover_results: Vec::new(),
            discover_status: String::new(),
        }
    }

    pub fn set_tool(&mut self, tool: Tool, cx: &mut Context<crate::app::SwitchApp>) {
        self.tool = tool;
        self.load(cx);
    }


    /// 构造 SkillPaths（按设置决定是否共享 `~/.agents/skills`）。
    fn paths(&self, _cx: &mut Context<crate::app::SwitchApp>) -> SkillPaths {
        let mut p = SkillPaths::from_env();
        p.shared = self.shared;
        p
    }
    pub fn load(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let paths = self.paths(cx);
        match manager::read_tool(self.tool, &paths) {
            Ok(mut skills) => {
                skills.sort_by(|a, b| a.name.cmp(&b.name));
                self.skills = skills;
                self.status = String::new();
            }
            Err(e) => {
                self.skills.clear();
                self.status = format!("读取失败: {e}");
            }
        }
        cx.notify();
    }


    /// 表单是否有未保存更改（新增时任一非空输入即 dirty；编辑时与快照对比）。
    pub fn form_dirty(&self, cx: &mut Context<crate::app::SwitchApp>) -> bool {
        if !self.dialog_open {
            return false;
        }
        let name = self.form.name.read(cx).text().trim().to_string();
        let desc = self.form.description.read(cx).text().trim().to_string();
        let body = self.form.body.read(cx).text();
        let allowed = self.form.allowed_tools.read(cx).text().trim().to_string();
        if let Some(old_name) = &self.editing_name {
            // 编辑：与打开时的技能对比（含 name 变更）。
            let Some(old) = self.skills.iter().find(|s| s.name == *old_name) else {
                return true;
            };
            name != old.name
                || desc != old.description.trim()
                || body != old.body
                || allowed != old.allowed_tools.join(", ")
                || self.form.enabled != old.enabled
        } else {
            // 新增：任一字段非空即视为有输入。
            !name.is_empty() || !desc.is_empty() || !body.is_empty() || !allowed.is_empty()
        }
    }

    /// 尝试关闭对话框：若有未保存更改则先弹确认浮层。
    pub fn request_close(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        if self.form_dirty(cx) {
            self.confirm_close = true;
            cx.notify();
        } else {
            self.dialog_open = false;
            cx.notify();
        }
    }
    pub fn open_add(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        self.editing_name = None;
        self.form = SkillForm::new(cx);
        self.dialog_open = true;
        self.confirm_close = false;
        cx.notify();
    }

    pub fn open_edit(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(skill) = self.skills.iter().find(|s| s.name == name) else {
            return;
        };
        self.form = SkillForm::new(cx);
        self.form.fill(skill, cx);
        self.editing_name = Some(name.to_string());
        self.dialog_open = true;
        self.confirm_close = false;
        cx.notify();
    }

    pub fn save(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let form = &self.form;
        let name = form.name.read(cx).text();
        let name = name.trim().to_string();
        if name.is_empty() {
            self.write_msg = "技能名不能为空".into();
            cx.notify();
            return;
        }
        let description = form.description.read(cx).text();
        let body = form.body.read(cx).text();
        let skill = Skill {
            name: name.clone(),
            description: description.trim().to_string(),
            body,
            source_path: None,
            repo_url: None,
            enabled: form.enabled,
            allowed_tools: form.allowed_tools.read(cx).text().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        };
        let mut remove = Vec::new();
        if let Some(old) = &self.editing_name {
            if old != &name {
                remove.push(old.clone());
            }
        }
        let paths = self.paths(cx);
        if let Err(e) = manager::write_tool(self.tool, &paths, &skill, self.strategy) {
            self.write_msg = format!("保存失败: {e}");
            cx.notify();
            return;
        }
        for old in remove {
            let _ = manager::remove_tool(self.tool, &paths, &old);
        }
        self.write_msg = format!("已保存技能 {name}");
        self.dialog_open = false;
        self.confirm_close = false;
        self.load(cx);
        cx.notify();
    }

    pub fn request_delete(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        self.confirm_delete = Some(name.to_string());
        cx.notify();
    }

    pub fn confirm_delete(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(name) = self.confirm_delete.clone() else {
            return;
        };
        let paths = self.paths(cx);
        match manager::remove_tool(self.tool, &paths, &name) {
            Ok(_) => {
                self.write_msg = format!("已删除技能 {name}");
                self.load(cx);
            }
            Err(e) => self.write_msg = format!("删除失败: {e}"),
        }
        self.confirm_delete = None;
        cx.notify();
    }

    pub fn sync_to(&mut self, name: &str, targets: &[Tool], cx: &mut Context<crate::app::SwitchApp>) {
        let Some(skill) = self.skills.iter().find(|s| s.name == name).cloned() else {
            return;
        };
        let paths = self.paths(cx);
        match manager::sync_to(&skill, self.tool, targets, &paths, self.strategy) {
            Ok(applied) if !applied.is_empty() => {
                self.write_msg = format!("已同步到 {}", applied.join(", "));
            }
            Ok(_) => self.write_msg = "无可同步目标".into(),
            Err(e) => self.write_msg = format!("同步失败: {e}"),
        }
        cx.notify();
    }

    /// 打开 GitHub 安装对话框。
    pub fn open_install(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        self.install_open = true;
        cx.notify();
    }

    /// 从 GitHub 仓库安装（取 `skills/<name>` 或仓库根技能文件）。
    pub fn install_github(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let url = self.install_url.read(cx).text();
        let url = url.trim().to_string();
        if !url.starts_with("http") {
            self.write_msg = "请输入 GitHub 仓库 URL".into();
            cx.notify();
            return;
        }
        let paths = self.paths(cx);
        let targets = [Tool::Claude, Tool::Codex, Tool::Gemini];
        match manager::install_from_github(&url, "", &paths, &targets) {
            Ok(name) => {
                self.write_msg = format!("已安装技能 {name}");
                self.install_open = false;
                self.load(cx);
            }
            Err(e) => self.write_msg = format!("安装失败: {e}"),
        }
        cx.notify();
    }

    /// 从本地目录安装（`<dir>/SKILL.md` 或 `<dir>/<name>.md`）。
    pub fn install_local(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let dir = self.install_local_dir.read(cx).text();
        let dir = dir.trim().to_string();
        if dir.is_empty() {
            self.write_msg = "请输入本地目录路径".into();
            cx.notify();
            return;
        }
        let path = std::path::PathBuf::from(&dir);
        if !path.exists() {
            self.write_msg = format!("目录不存在: {dir}");
            cx.notify();
            return;
        }
        let paths = self.paths(cx);
        let targets = [Tool::Claude, Tool::Codex, Tool::Gemini];
        match manager::install_from_dir(&path, &paths, &targets, None) {
            Ok(name) => {
                self.write_msg = format!("已安装技能 {name}");
                self.install_open = false;
                self.load(cx);
            }
            Err(e) => self.write_msg = format!("安装失败: {e}"),
        }
        cx.notify();
    }

    /// 行级启停：翻转某技能的 enabled 并写回当前工具。
    pub fn toggle_enabled(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(skill) = self.skills.iter_mut().find(|s| s.name == name) else {
            return;
        };
        skill.enabled = !skill.enabled;
        let skill = skill.clone();
        let paths = self.paths(cx);
        match manager::write_tool(self.tool, &paths, &skill, self.strategy) {
            Ok(_) => self.write_msg = format!("已{}技能 {}", if skill.enabled { "启用" } else { "禁用" }, name),
            Err(e) => self.write_msg = format!("写回失败: {e}"),
        }
        self.load(cx);
        cx.notify();
    }

    /// 全部启用/禁用当前工具的全部技能。
    pub fn set_all_enabled(&mut self, enabled: bool, cx: &mut Context<crate::app::SwitchApp>) {
        if self.skills.is_empty() {
            self.write_msg = "无技能可操作".into();
            cx.notify();
            return;
        }
        let mut changed = 0usize;
        let paths = self.paths(cx);
        for s in &self.skills {
            if s.enabled != enabled {
                let mut c = s.clone();
                c.enabled = enabled;
                match manager::write_tool(self.tool, &paths, &c, self.strategy) {
                    Ok(_) => changed += 1,
                    Err(e) => {
                        self.write_msg = format!("写回失败（{}）: {e}", c.name);
                        cx.notify();
                        return;
                    }
                }
            }
        }
        self.write_msg = if changed > 0 {
            if enabled {
                format!("已启用全部 {changed} 个技能")
            } else {
                format!("已禁用全部 {changed} 个技能")
            }
        } else if enabled {
            "已全部启用".into()
        } else {
            "已全部禁用".into()
        };
        self.load(cx);
        cx.notify();
    }

    /// 对话框表单启停开关。
    pub fn toggle_form_enabled(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        self.form.enabled = !self.form.enabled;
        cx.notify();
    }

    /// 检查全部已安装技能的 GitHub 来源是否有更新；更新 `write_msg`。
    pub fn check_updates(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let paths = self.paths(cx);
        let all = match manager::read_tool(self.tool, &paths) {
            Ok(v) => v,
            Err(e) => {
                self.write_msg = format!("检查失败: {e}");
                cx.notify();
                return;
            }
        };
        match manager::check_for_updates(&all) {
            Ok(updated) if !updated.is_empty() => {
                self.write_msg = format!("发现更新: {}", updated.join(", "));
            }
            Ok(_) => self.write_msg = "所有技能均为最新".into(),
            Err(e) => self.write_msg = format!("检查失败: {e}"),
        }
        cx.notify();
    }

    /// 全部更新：对有 GitHub 来源的技能逐个重新安装到各工具。
    pub fn update_all(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let paths = self.paths(cx);
        let all = match manager::read_tool(self.tool, &paths) {
            Ok(v) => v,
            Err(e) => {
                self.write_msg = format!("读取失败: {e}");
                cx.notify();
                return;
            }
        };
        let targets = [Tool::Claude, Tool::Codex, Tool::Gemini];
        let mut updated = Vec::new();
        let mut failed = Vec::new();
        for skill in &all {
            let Some(repo) = &skill.repo_url else {
                continue;
            };
            match manager::install_from_github(repo, &skill.name, &paths, &targets) {
                Ok(_) => updated.push(skill.name.clone()),
                Err(e) => failed.push(format!("{}: {e}", skill.name)),
            }
        }
        self.write_msg = if updated.is_empty() && failed.is_empty() {
            "无 GitHub 来源技能可更新".into()
        } else if updated.is_empty() {
            format!("全部更新失败: {}", failed.join("；"))
        } else if failed.is_empty() {
            format!("已更新 {} 个技能", updated.len())
        } else {
            format!("已更新 {} 个技能；失败: {}", updated.len(), failed.join("；"))
        };
        self.load(cx);
        cx.notify();
    }

    /// 发现页搜索：调用 GitHub 仓库搜索 API 列出含 skills 的仓库。
    pub fn search_discover(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let q = self.discover_query.read(cx).text().trim().to_string();
        if q.is_empty() {
            self.discover_status = "请输入搜索关键词".into();
            cx.notify();
            return;
        }
        self.discover_status = "搜索中…".into();
        cx.notify();
        let encoded = urlencode(&q);
        let url = format!(
            "https://api.github.com/search/repositories?q={encoded}+skill&sort=stars&per_page=10"
        );
        let out = std::process::Command::new("curl")
            .arg("-sS")
            .arg("--max-time")
            .arg("8")
            .arg("-H")
            .arg("User-Agent: ns-switch")
            .arg(&url)
            .output();
        let results = match out {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                let mut rows = Vec::new();
                if let Some(items) = v.get("items").and_then(|i| i.as_array()) {
                    for it in items.iter().take(10) {
                        let name = it.get("full_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let desc = it
                            .get("description")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() {
                            rows.push((name, desc));
                        }
                    }
                }
                if rows.is_empty() {
                    self.discover_status = "无结果（GitHub 搜索失败或为空）".into();
                } else {
                    self.discover_status = format!("{} 个结果", rows.len());
                }
                rows
            }
            _ => {
                self.discover_status = "搜索失败（网络或 GitHub API 不可用）".into();
                Vec::new()
            }
        };
        self.discover_results = results;
        cx.notify();
    }
}

impl SkillForm {
    fn new(cx: &mut Context<crate::app::SwitchApp>) -> Self {
        Self {
            name: cx.new(|cx| TextField::new("技能名（字母/数字/-/_）", cx)),
            description: cx.new(|cx| TextField::new("描述（一句话）", cx)),
            body: cx.new(|cx| TextField::new("Markdown 正文", cx)),
            enabled: true,
            allowed_tools: cx.new(|cx| TextField::new("允许工具（逗号分隔，留空=全部）", cx)),
        }
    }

    fn fill(&mut self, s: &Skill, cx: &mut Context<crate::app::SwitchApp>) {
        self.name.update(cx, |f, _| f.set_text(&s.name));
        self.description
            .update(cx, |f, _| f.set_text(&s.description));
        self.body.update(cx, |f, _| f.set_text(&s.body));
        self.allowed_tools.update(cx, |f, _| {
            f.set_text(&s.allowed_tools.join(", "))
        });
        self.enabled = s.enabled;
    }
}

/// 简易 URL 编码（`%XX`；空格→`+`，保留字母数字 `-_.~`）。
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_encodes_reserved() {
        assert_eq!(urlencode("a b/c"), "a+b%2Fc");
        assert_eq!(urlencode("claude-code"), "claude-code");
        assert_eq!(urlencode("skills 搜索"), "skills+%E6%90%9C%E7%B4%A2");
    }
}
