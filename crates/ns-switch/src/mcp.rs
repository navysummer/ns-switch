//! M7 统一 MCP 面板。
//!
//! 以「服务器」为第一视图：列出某工具的全部 MCP 服务器，可新增/编辑/
//! 删除/启停，并一键同步到其他工具。读写经 `ns_mcp::manager`，每次操作
//! 只改该工具的 `mcpServers`/`mcp_servers` 键。

use std::collections::BTreeMap;

use gpui::{AppContext, Context, Entity};
use ns_mcp::manager;
use ns_mcp::model::{McpServer, McpTransport};
use ns_mcp::tools::McpPaths;
use ns_presets::Tool;

use crate::text_field::TextField;

/// MCP 面板状态（挂在 SwitchApp 上）。
pub struct McpPanel {
    /// 当前查看的工具。
    pub tool: Tool,
    /// 加载后的服务器列表（按名排序）。
    pub servers: Vec<McpServer>,
    /// 加载错误/状态消息。
    pub status: String,
    /// 新增/编辑对话框是否打开。
    pub dialog_open: bool,
    /// 编辑中的服务器名（None = 新增）。
    pub editing_name: Option<String>,
    /// 表单字段。
    pub form: McpForm,
    /// 删除确认目标（服务器名）。
    pub confirm_delete: Option<String>,
    /// 最近一次写回结果消息。
    pub write_msg: String,
}

impl McpPanel {
    pub fn new(cx: &mut Context<crate::app::SwitchApp>) -> Self {
        Self {
            tool: Tool::Claude,
            servers: Vec::new(),
            status: String::new(),
            dialog_open: false,
            editing_name: None,
            form: McpForm::new(cx),
            confirm_delete: None,
            write_msg: String::new(),
        }
    }
}

/// 新增/编辑对话框表单。
pub struct McpForm {
    pub name: Entity<TextField>,
    pub command: Entity<TextField>,
    pub args: Entity<TextField>,
    pub url: Entity<TextField>,
    pub enabled: bool,
    pub transport: McpTransport,
}

// (McpForm 无 Default：实体字段需 cx 创建，见 McpForm::new/empty。)


impl McpForm {
    fn new(cx: &mut Context<crate::app::SwitchApp>) -> Self {
        Self {
            name: cx.new(|cx| TextField::new("服务器名（字母/数字/-/_）", cx)),
            command: cx.new(|cx| TextField::new("启动命令（如 npx）", cx)),
            args: cx.new(|cx| TextField::new("参数（空格分隔）", cx)),
            url: cx.new(|cx| TextField::new("https://.../mcp", cx)),
            enabled: true,
            transport: McpTransport::Stdio,
        }
    }

    fn fill(&self, s: &McpServer, cx: &mut Context<crate::app::SwitchApp>) {
        self.name.update(cx, |f, _| f.set_text(s.name.as_str()));
        if let Some(cmd) = &s.command {
            self.command.update(cx, |f, _| f.set_text(cmd));
        }
        if !s.args.is_empty() {
            self.args.update(cx, |f, _| f.set_text(&s.args.join(" ")));
        }
        if let Some(url) = &s.url {
            self.url.update(cx, |f, _| f.set_text(url));
        }
    }

}

impl McpPanel {
    /// 切换查看工具并重新加载。
    pub fn set_tool(&mut self, tool: Tool, cx: &mut Context<crate::app::SwitchApp>) {
        self.tool = tool;
        self.load(cx);
    }

    /// 加载当前工具的服务器列表。
    pub fn load(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let paths = McpPaths::from_env();
        match manager::read_tool(self.tool, &paths) {
            Ok(mut servers) => {
                servers.sort_by(|a, b| a.name.cmp(&b.name));
                self.servers = servers;
                self.status = String::new();
            }
            Err(e) => {
                self.servers.clear();
                self.status = format!("读取失败: {e}");
            }
        }
        cx.notify();
    }

    /// 打开新增对话框。
    pub fn open_add(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        self.editing_name = None;
        self.form = McpForm::new(cx);
        self.dialog_open = true;
        cx.notify();
    }

    /// 打开编辑对话框（预填字段）。
    pub fn open_edit(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(server) = self.servers.iter().find(|s| s.name == name) else {
            return;
        };
        self.form = McpForm::new(cx);
        self.form.fill(server, cx);
        self.form.transport = server.transport;
        self.form.enabled = server.enabled;
        self.editing_name = Some(name.to_string());
        self.dialog_open = true;
        cx.notify();
    }

    /// 保存表单（新增或编辑）。
    pub fn save(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let form = &self.form;
        let name = form.name.read(cx).text();
        let name = name.trim().to_string();
        let command = form.command.read(cx).text();
        let command = command.trim().to_string();
        let args: Vec<String> = form
            .args
            .read(cx)
            .text()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let url = form.url.read(cx).text();
        let url = url.trim().to_string();

        let server = match form.transport {
            McpTransport::Stdio => McpServer {
                name: name.clone(),
                transport: McpTransport::Stdio,
                command: Some(command),
                args,
                env: BTreeMap::new(),
                url: None,
                headers: BTreeMap::new(),
                enabled: form.enabled,
            },
            McpTransport::Http | McpTransport::Sse | McpTransport::Ws => McpServer {
                name: name.clone(),
                transport: form.transport,
                command: None,
                args: Vec::new(),
                env: BTreeMap::new(),
                url: Some(url),
                headers: BTreeMap::new(),
                enabled: form.enabled,
            },
        };

        if let Err(e) = server.validate() {
            self.write_msg = e;
            cx.notify();
            return;
        }

        let mut remove = Vec::new();
        if let Some(old) = &self.editing_name {
            if old != &name {
                remove.push(old.clone());
            }
        }
        let paths = McpPaths::from_env();
        match manager::write_tool(self.tool, &paths, &remove, &[server]) {
            Ok(_) => {
                self.write_msg = format!("已保存服务器 {name}");
                self.dialog_open = false;
                self.load(cx);
            }
            Err(e) => {
                self.write_msg = format!("保存失败: {e}");
            }
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
        let paths = McpPaths::from_env();
        match manager::write_tool(self.tool, &paths, &[name.clone()], &[]) {
            Ok(_) => {
                self.write_msg = format!("已删除服务器 {name}");
                self.load(cx);
            }
            Err(e) => {
                self.write_msg = format!("删除失败: {e}");
            }
        }
        self.confirm_delete = None;
        cx.notify();
    }

    /// 启停切换。
    pub fn toggle_enabled(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(server) = self.servers.iter_mut().find(|s| s.name == name) else {
            return;
        };
        server.enabled = !server.enabled;
        let server = server.clone();
        let paths = McpPaths::from_env();
        match manager::write_tool(self.tool, &paths, &[], &[server]) {
            Ok(_) => self.load(cx),
            Err(e) => self.write_msg = format!("启停失败: {e}"),
        }
        cx.notify();
    }

    /// 同步到其他工具。
    pub fn sync_to(&mut self, name: &str, targets: &[Tool], cx: &mut Context<crate::app::SwitchApp>) {
        let Some(server) = self.servers.iter().find(|s| s.name == name).cloned() else {
            return;
        };
        let paths = McpPaths::from_env();
        match manager::sync_server(&server, self.tool, targets, &paths) {
            Ok(applied) if !applied.is_empty() => {
                self.write_msg = format!("已同步到 {}", applied.join(", "));
            }
            Ok(_) => self.write_msg = "无可同步目标".into(),
            Err(e) => self.write_msg = format!("同步失败: {e}"),
        }
        cx.notify();
    }
}
