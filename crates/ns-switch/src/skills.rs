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
}

/// 新增/编辑对话框表单。
pub struct SkillForm {
    pub name: Entity<TextField>,
    pub description: Entity<TextField>,
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
            install_url: cx.new(|cx| TextField::new("https://github.com/<owner>/<repo>", cx)),
        }
    }

    pub fn set_tool(&mut self, tool: Tool, cx: &mut Context<crate::app::SwitchApp>) {
        self.tool = tool;
        self.load(cx);
    }

    pub fn load(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let paths = SkillPaths::from_env();
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

    pub fn open_add(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        self.editing_name = None;
        self.form = SkillForm::new(cx);
        self.dialog_open = true;
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
            enabled: true,
            allowed_tools: Vec::new(),
        };
        let mut remove = Vec::new();
        if let Some(old) = &self.editing_name {
            if old != &name {
                remove.push(old.clone());
            }
        }
        let paths = SkillPaths::from_env();
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
        let paths = SkillPaths::from_env();
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
        let paths = SkillPaths::from_env();
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
        let paths = SkillPaths::from_env();
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
}

impl SkillForm {
    fn new(cx: &mut Context<crate::app::SwitchApp>) -> Self {
        Self {
            name: cx.new(|cx| TextField::new("技能名（字母/数字/-/_）", cx)),
            description: cx.new(|cx| TextField::new("描述（一句话）", cx)),
            body: cx.new(|cx| TextField::new("Markdown 正文", cx)),
        }
    }

    fn fill(&self, s: &Skill, cx: &mut Context<crate::app::SwitchApp>) {
        self.name.update(cx, |f, _| f.set_text(&s.name));
        self.description
            .update(cx, |f, _| f.set_text(&s.description));
        self.body.update(cx, |f, _| f.set_text(&s.body));
    }
}
