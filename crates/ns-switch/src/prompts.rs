//! M7 统一 Prompts 面板。
//!
//! 以「条目」为第一视图：列出某工具的提示词条目，可新增/编辑/删除/启停，
//! 并一键写回该工具提示文件（先回存既有内容）。读写经 `ns_prompts`。

use gpui::{AppContext, Context, Entity};
use ns_presets::Tool;
use ns_prompts::model::{PromptEntry, PromptPaths};
use ns_prompts::store::PromptStore;

use crate::text_area::TextArea;
use crate::text_field::TextField;

/// Prompts 面板状态（挂在 SwitchApp 上）。
pub struct PromptsPanel {
    /// 当前查看的工具。
    pub tool: Tool,
    /// 加载后的条目列表。
    pub entries: Vec<PromptEntry>,
    /// 加载错误/状态消息。
    pub status: String,
    /// 新增/编辑对话框是否打开。
    pub dialog_open: bool,
    /// 编辑中的条目名（None = 新增）。
    pub editing_name: Option<String>,
    /// 表单字段（None = 对话框未打开）。
    pub form: Option<PromptForm>,
    /// 删除确认目标（条目名）。
    pub confirm_delete: Option<String>,
    /// 最近一次写回结果消息。
    pub write_msg: String,
    /// 未保存更改确认关闭（true = 显示确认浮层）。
    pub confirm_close: bool,
}

impl PromptsPanel {
    pub fn new(_cx: &mut Context<crate::app::SwitchApp>) -> Self {
        Self {
            tool: Tool::Claude,
            entries: Vec::new(),
            status: String::new(),
            dialog_open: false,
            editing_name: None,
            form: None,
            confirm_delete: None,
            write_msg: String::new(),
            confirm_close: false,
        }
    }

    /// 切换查看工具并重新加载。
    pub fn set_tool(&mut self, tool: Tool, cx: &mut Context<crate::app::SwitchApp>) {
        self.tool = tool;
        self.load(cx);
    }

    /// 加载当前工具的条目。
    pub fn load(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        match PromptStore::open() {
            Ok(store) => {
                self.entries = store.entries_for(self.tool);
                self.status = String::new();
            }
            Err(e) => {
                self.entries.clear();
                self.status = format!("读取失败: {e}");
            }
        }
        cx.notify();
    }

    /// 打开新增对话框。
    pub fn open_add(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        self.editing_name = None;
        self.form = Some(PromptForm::new(cx));
        self.dialog_open = true;
        cx.notify();
    }

    /// 打开编辑对话框（预填字段）。
    pub fn open_edit(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(entry) = self.entries.iter().find(|e| e.name == name) else {
            return;
        };
        let form = PromptForm::new(cx);
        form.fill(entry, cx);
        self.form = Some(form);
        self.editing_name = Some(name.to_string());
        self.dialog_open = true;
        cx.notify();
    }

    /// 保存表单（新增或编辑）。
    pub fn save(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(form) = &self.form else {
            return;
        };
        let name = form.name.read(cx).text();
        let name = name.trim().to_string();
        let body = form.body.read(cx).text();
        if name.is_empty() {
            self.write_msg = "条目名不能为空".into();
            cx.notify();
            return;
        }
        let entry = PromptEntry {
            name: name.clone(),
            body,
            enabled: form.enabled,
            updated_at: now_ms(),
        };
        let mut store = match PromptStore::open() {
            Ok(s) => s,
            Err(e) => {
                self.write_msg = format!("打开库失败: {e}");
                cx.notify();
                return;
            }
        };
        let is_new = match store.save_entry(self.tool, entry) {
            Ok(v) => v,
            Err(e) => {
                self.write_msg = format!("保存失败: {e}");
                cx.notify();
                return;
            }
        };
        self.write_msg = if is_new {
            format!("已添加条目 {name}")
        } else {
            format!("已更新条目 {name}")
        };
        self.dialog_open = false;
        self.load(cx);
    }

    /// 表单是否有未保存更改（与打开时快照比对）。
    pub fn form_dirty(&self, cx: &mut Context<crate::app::SwitchApp>) -> bool {
        let Some(form) = &self.form else {
            return false;
        };
        let name = form.name.read(cx).text();
        let body = form.body.read(cx).text();
        if let Some(orig) = &self.editing_name {
            let entry = self.entries.iter().find(|e| &e.name == orig);
            match entry {
                Some(e) => {
                    name.trim() != e.name
                        || body.trim() != e.body.trim()
                        || form.enabled != e.enabled
                }
                None => !name.trim().is_empty() || !body.trim().is_empty(),
            }
        } else {
            !name.trim().is_empty() || !body.trim().is_empty()
        }
    }

    /// 尝试关闭对话框：若有未保存更改则先弹确认。
    pub fn request_close(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        if self.form_dirty(cx) {
            self.confirm_close = true;
        } else {
            self.dialog_open = false;
        }
        cx.notify();
    }

    /// 删除确认。
    pub fn request_delete(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        self.confirm_delete = Some(name.to_string());
        cx.notify();
    }

    /// 执行删除。
    pub fn confirm_delete(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(name) = self.confirm_delete.clone() else {
            return;
        };
        let mut store = match PromptStore::open() {
            Ok(s) => s,
            Err(e) => {
                self.write_msg = format!("打开库失败: {e}");
                self.confirm_delete = None;
                cx.notify();
                return;
            }
        };
        match store.remove_entry(self.tool, &name) {
            Ok(true) => {
                self.write_msg = format!("已删除条目 {name}");
                self.load(cx);
            }
            Ok(false) => self.write_msg = format!("条目 {name} 不存在"),
            Err(e) => self.write_msg = format!("删除失败: {e}"),
        }
        self.confirm_delete = None;
        cx.notify();
    }

    /// 启停切换。
    pub fn toggle_enabled(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.name == name) else {
            return;
        };
        entry.enabled = !entry.enabled;
        let entry = entry.clone();
        let mut store = match PromptStore::open() {
            Ok(s) => s,
            Err(e) => {
                self.write_msg = format!("打开库失败: {e}");
                cx.notify();
                return;
            }
        };
        match store.save_entry(self.tool, entry) {
            Ok(_) => self.load(cx),
            Err(e) => self.write_msg = format!("启停失败: {e}"),
        }
        cx.notify();
    }

    /// 写回当前工具提示文件（先回存既有内容）。
    pub fn write_back(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let paths = PromptPaths::from_env();
        let mut store = match PromptStore::open() {
            Ok(s) => s,
            Err(e) => {
                self.write_msg = format!("打开库失败: {e}");
                cx.notify();
                return;
            }
        };
        match ns_prompts::tools::write_back(self.tool, &mut store, &paths) {
            Ok(file) => {
                self.write_msg = format!("已写回 {}", file.display());
                self.load(cx);
            }
            Err(e) => self.write_msg = format!("写回失败: {e}"),
        }
        cx.notify();
    }
}

/// 新增/编辑对话框表单。
pub struct PromptForm {
    pub name: Entity<TextField>,
    pub body: Entity<TextArea>,
    pub enabled: bool,
}

impl PromptForm {
    fn new(cx: &mut Context<crate::app::SwitchApp>) -> Self {
        Self {
            name: cx.new(|cx| TextField::new("条目名（如 项目规范）", cx)),
            body: cx.new(|cx| TextArea::new("Markdown 正文…", cx)),
            enabled: true,
        }
    }

    fn fill(&self, e: &PromptEntry, cx: &mut Context<crate::app::SwitchApp>) {
        self.name.update(cx, |f, _| f.set_text(e.name.as_str()));
        self.body.update(cx, |f, _| f.set_text(e.body.as_str()));
    }
}



fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
