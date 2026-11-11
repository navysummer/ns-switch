//! M7 统一 MCP 面板。

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

/// 支持 MCP 统一管理的工具（MCP 页工具页签与同步目标一致）。
pub const MCP_TOOLS: [Tool; 11] = [
    Tool::Claude,
    Tool::Codex,
    Tool::Gemini,
    Tool::Opencode,
    Tool::GrokBuild,
    Tool::Kimi,
    Tool::Openclaw,
    Tool::Hermes,
    Tool::Pi,
    Tool::Mcode,
    Tool::Omp,
];

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
    /// 项目作用域选择对话框是否打开。
    pub scope_open: bool,
    /// 项目作用域路径输入。
    pub scope_path: Entity<TextField>,
    /// 项目作用域 MCP 读取结果。
    pub scope_result: String,
    /// 同步目标选择浮层（Some = 服务器名）。
    pub sync_targets_open: Option<String>,
    /// 同步目标勾选（target tool id_str → 是否选中）。
    pub sync_target_selected: BTreeMap<String, bool>,
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
            scope_open: false,
            scope_path: cx.new(|cx| TextField::new("/path/to/project", cx)),
            scope_result: String::new(),
            sync_targets_open: None,
            sync_target_selected: BTreeMap::new(),
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
    pub env_raw: Entity<TextField>,
    pub headers_raw: Entity<TextField>,
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
            env_raw: cx.new(|cx| TextField::new("环境变量（每行 key=value）", cx)),
            headers_raw: cx.new(|cx| TextField::new("请求头（每行 key: value）", cx)),
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
        // Populate env and headers from server data
        let env_lines: Vec<String> = s.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        self.env_raw.update(cx, |f, _| f.set_text(&env_lines.join("\n")));
        let header_lines: Vec<String> = s.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect();
        self.headers_raw.update(cx, |f, _| f.set_text(&header_lines.join("\n")));
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

    /// 打开项目作用域选择对话框。
    pub fn open_scope(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        self.scope_open = true;
        self.scope_result = String::new();
        cx.notify();
    }

    /// 读取指定项目目录的项目作用域 MCP（`.mcp.json`，Claude Code 约定）。
    pub fn read_scope(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let path = self.scope_path.read(cx).text();
        let path = path.trim().to_string();
        if path.is_empty() {
            self.scope_result = "请输入项目目录路径".into();
            cx.notify();
            return;
        }
        // Claude 项目级 `.mcp.json`（mcpServers 对象）优先；Codex 项目级
        // `.codex/config.toml`（[mcp_servers.<name>] 节）为备选。
        let base = std::path::Path::new(&path);
        let mcp_file = base.join(".mcp.json");
        let codex_file = base.join(".codex").join("config.toml");
        if !mcp_file.exists() && !codex_file.exists() {
            self.scope_result = format!(
                "未找到项目级 MCP 配置（{} / {}）",
                mcp_file.display(),
                codex_file.display()
            );
            cx.notify();
            return;
        }
        if mcp_file.exists() {
            match std::fs::read_to_string(&mcp_file) {
                Ok(text) => {
                    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&text);
                    match parsed {
                        Ok(v) => {
                            let servers = v
                                .get("mcpServers")
                                .and_then(|s| s.as_object())
                                .map(|o| o.keys().cloned().collect::<Vec<_>>())
                                .unwrap_or_default();
                            self.scope_result = if servers.is_empty() {
                                format!("已读取 {}（无 mcpServers 条目）", mcp_file.display())
                            } else {
                                format!(
                                    "{}：{} 个项目作用域服务器",
                                    mcp_file.display(),
                                    servers.join(", ")
                                )
                            };
                        }
                        Err(e) => {
                            self.scope_result = format!("{} 解析失败: {e}", mcp_file.display());
                        }
                    }
                }
                Err(e) => {
                    self.scope_result = format!("读取失败: {e}");
                }
            }
        } else {
            // Codex 项目级 `[mcp_servers.<name>]` 节名扫描。
            match std::fs::read_to_string(&codex_file) {
                Ok(text) => {
                    let servers = codex_project_mcp_names(&text);
                    self.scope_result = if servers.is_empty() {
                        format!("已读取 {}（无 mcp_servers 条目）", codex_file.display())
                    } else {
                        format!(
                            "{}：{} 个项目作用域服务器",
                            codex_file.display(),
                            servers.join(", ")
                        )
                    };
                }
                Err(e) => {
                    self.scope_result = format!("读取失败: {e}");
                }
            }
        }
        cx.notify();
    }

    /// 打开原生目录选择器（macOS `osascript`），回填 scope_path。
    ///
    /// Linux/Windows 无内建选择器，提示手输路径。
    pub fn choose_scope_dir(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        #[cfg(target_os = "macos")]
        {
            let script = "POSIX path of (choose folder with prompt \"选择项目目录\")";
            let out = std::process::Command::new("osascript")
                .args(["-e", script])
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let dir = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if !dir.is_empty() {
                        self.scope_path.update(cx, |tf, _| tf.set_text(&dir));
                    } else {
                        self.scope_result = "未选择目录".into();
                    }
                }
                Ok(o) => {
                    self.scope_result =
                        format!("选择目录失败: {}", String::from_utf8_lossy(&o.stderr).trim());
                }
                Err(e) => {
                    self.scope_result = format!("无法打开选择器: {e}");
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.scope_result = "请在输入框手动输入项目目录路径".into();
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

        // Parse env and headers from raw text
        let env: BTreeMap<String, String> = form.env_raw.read(cx).text()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|line| {
                let mut parts = line.splitn(2, '=');
                match (parts.next(), parts.next()) {
                    (Some(k), Some(v)) if !k.trim().is_empty() && !v.trim().is_empty() => {
                        Some((k.trim().to_string(), v.trim().to_string()))
                    }
                    _ => None,
                }
            })
            .collect();
        let headers: BTreeMap<String, String> = form.headers_raw.read(cx).text()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|line| {
                let mut parts = line.splitn(2, ':');
                match (parts.next(), parts.next()) {
                    (Some(k), Some(v)) if !k.trim().is_empty() && !v.trim().is_empty() => {
                        Some((k.trim().to_string(), v.trim().to_string()))
                    }
                    _ => None,
                }
            })
            .collect();

        let server = match form.transport {
            McpTransport::Stdio => McpServer {
                name: name.clone(),
                transport: McpTransport::Stdio,
                command: Some(command),
                args,
                env,
                url: None,
                headers,
                enabled: form.enabled,
            },
            McpTransport::Http | McpTransport::Sse | McpTransport::Ws => McpServer {
                name: name.clone(),
                transport: form.transport,
                command: None,
                args: Vec::new(),
                env,
                url: Some(url),
                headers,
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

    /// 打开同步目标选择浮层（默认选中除当前工具外的全部工具）。
    pub fn open_sync_targets(&mut self, name: &str, cx: &mut Context<crate::app::SwitchApp>) {
        self.sync_targets_open = Some(name.to_string());
        self.sync_target_selected.clear();
        for t in MCP_TOOLS {
            if t != self.tool {
                self.sync_target_selected.insert(t.id_str().to_string(), true);
            }
        }
        cx.notify();
    }

    /// 切换某个同步目标工具的勾选状态。
    pub fn toggle_sync_target(&mut self, tool: Tool, cx: &mut Context<crate::app::SwitchApp>) {
        let entry = self
            .sync_target_selected
            .entry(tool.id_str().to_string())
            .or_insert(false);
        *entry = !*entry;
        cx.notify();
    }

    /// 全选/全不选同步目标。
    pub fn set_all_sync_targets(&mut self, on: bool, cx: &mut Context<crate::app::SwitchApp>) {
        for t in MCP_TOOLS {
            if t != self.tool {
                self.sync_target_selected.insert(t.id_str().to_string(), on);
            }
        }
        cx.notify();
    }

    /// 确认同步到所选目标工具（至少选一个）。
    pub fn confirm_sync_targets(&mut self, cx: &mut Context<crate::app::SwitchApp>) {
        let Some(name) = self.sync_targets_open.clone() else {
            return;
        };
        let targets = selected_targets(self.tool, &self.sync_target_selected);
        if targets.is_empty() {
            self.write_msg = "请至少选择一个目标工具".into();
            cx.notify();
            return;
        }
        self.sync_targets_open = None;
        self.sync_to(&name, &targets, cx);
    }

    /// 全部启用/禁用当前工具的全部 MCP 服务器。
    pub fn set_all_enabled(&mut self, enabled: bool, cx: &mut Context<crate::app::SwitchApp>) {
        if self.servers.is_empty() {
            self.write_msg = "无服务器可操作".into();
            cx.notify();
            return;
        }
        let mut changed: Vec<McpServer> = Vec::new();
        for s in &self.servers {
            if s.enabled != enabled {
                let mut c = s.clone();
                c.enabled = enabled;
                changed.push(c);
            }
        }
        if changed.is_empty() {
            self.write_msg = if enabled { "已全部启用".into() } else { "已全部禁用".into() };
            cx.notify();
            return;
        }
        let paths = McpPaths::from_env();
        match manager::write_tool(self.tool, &paths, &[], &changed) {
            Ok(_) => {
                self.load(cx);
                self.write_msg = if enabled {
                    format!("已启用全部 {} 个服务器", changed.len())
                } else {
                    format!("已禁用全部 {} 个服务器", changed.len())
                };
            }
            Err(e) => self.write_msg = format!("操作失败: {e}"),
        }
        cx.notify();
    }

    /// 从其他工具导入 MCP 服务器。
    pub fn import_from(&mut self, source: Tool, cx: &mut Context<crate::app::SwitchApp>) {
        if source == self.tool {
            return;
        }
        let paths = McpPaths::from_env();
        match manager::import_tool(source, &[self.tool], &paths) {
            Ok(applied) if !applied.is_empty() => {
                self.write_msg = format!("已从 {} 导入服务器", applied.join(", "));
                self.load(cx);
            }
            Ok(_) => {
                self.write_msg = "源工具无可用服务器".into();
                cx.notify();
            }
            Err(e) => {
                self.write_msg = format!("导入失败: {e}");
                cx.notify();
            }
        }
    }
    }

/// 从 Codex 项目级 `config.toml` 文本提取 `[mcp_servers.<name>]` 节名。
///
/// 纯函数（不读文件），供 `read_scope` 与测试复用。
pub fn codex_project_mcp_names(text: &str) -> Vec<String> {
    let mut servers = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("[mcp_servers.") {
            if let Some(name) = rest.strip_suffix(']') {
                let name = name.trim().trim_matches('"').trim();
                if !name.is_empty() {
                    servers.push(name.to_string());
                }
            }
        }
    }
    servers.sort();
    servers.dedup();
    servers
}

/// 从同步目标勾选表筛出目标工具（排除当前源工具、未勾选项）。
pub fn selected_targets(current: Tool, selected: &BTreeMap<String, bool>) -> Vec<Tool> {
    MCP_TOOLS
        .iter()
        .copied()
        .filter(|t| *t != current && selected.get(t.id_str()).copied().unwrap_or(false))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{codex_project_mcp_names, selected_targets, MCP_TOOLS};
    use ns_presets::Tool;
    use std::collections::BTreeMap;

    #[test]
    fn selected_targets_filters_current_and_unchecked() {
        let mut map = BTreeMap::new();
        for t in MCP_TOOLS {
            map.insert(t.id_str().to_string(), true);
        }
        // 当前工具被排除；全选时目标是其余全部工具。
        let targets = selected_targets(Tool::Claude, &map);
        assert_eq!(targets.len(), MCP_TOOLS.len() - 1);
        assert!(!targets.contains(&Tool::Claude));
        // 未勾选的目标不参与同步。
        map.insert(Tool::Codex.id_str().to_string(), false);
        let targets = selected_targets(Tool::Claude, &map);
        assert!(!targets.contains(&Tool::Codex));
        // 空勾选 → 无目标。
        assert!(selected_targets(Tool::Claude, &BTreeMap::new()).is_empty());
    }

    #[test]
    fn extracts_codex_project_mcp_names() {
        let text = r#"
[model_providers.anthropic]
name = "Anthropic"

[mcp_servers.fs]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

[mcp_servers."my server"]
command = "uvx"
args = ["mcp-server-fetch"]

[other]
x = 1
"#;
        let names = codex_project_mcp_names(text);
        assert_eq!(names, vec!["fs", "my server"]);
    }

    #[test]
    fn codex_project_mcp_names_empty() {
        assert!(codex_project_mcp_names("model_provider = \"openai\"").is_empty());
    }
}


