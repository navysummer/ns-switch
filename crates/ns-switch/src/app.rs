//! 主界面：顶栏、侧栏（工具列表）、供应商页、状态栏。

use std::sync::{Arc, RwLock};

use gpui::{
    Context, Div, FocusHandle, KeyBinding, Render, Stateful, div, prelude::*, px, rgb, Hsla,
    ClipboardItem, actions,
};
use ns_presets::Tool;
use ns_store::Store;
use crate::dialog;
use crate::tray;
use ns_ui::{theme, widgets};
use ns_mcp::model::McpTransport;

actions!(
    ns_switch,
    [
        QuitApp,
        ToggleSidebar,
        RefreshProviders,
        TogglePage0,
        TogglePage1,
        TogglePage2,
        TogglePage3,
        TogglePage4,
        TogglePage5,
        TogglePage6,
        TogglePage7,
        TogglePage8,
        EscapeDialog,
        FocusProviderSearch,
        SaveCurrent,
    ]
);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-q", QuitApp, None),
        KeyBinding::new("ctrl-q", QuitApp, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("ctrl-b", ToggleSidebar, None),
        KeyBinding::new("cmd-r", RefreshProviders, None),
        KeyBinding::new("cmd-1", TogglePage0, None),
        KeyBinding::new("cmd-2", TogglePage1, None),
        KeyBinding::new("cmd-3", TogglePage2, None),
        KeyBinding::new("cmd-4", TogglePage3, None),
        KeyBinding::new("cmd-5", TogglePage4, None),
        KeyBinding::new("cmd-6", TogglePage5, None),
        KeyBinding::new("cmd-f", FocusProviderSearch, None),
        KeyBinding::new("ctrl-f", FocusProviderSearch, None),
        KeyBinding::new("cmd-7", TogglePage6, None),
        KeyBinding::new("cmd-8", TogglePage7, None),
        KeyBinding::new("cmd-9", TogglePage8, None),
        KeyBinding::new("escape", EscapeDialog, None),
        KeyBinding::new("cmd-s", SaveCurrent, None),
        KeyBinding::new("ctrl-s", SaveCurrent, None),
    ]
}


/// 供应商卡片拖拽载荷（用于排序）。
#[derive(Clone)]
pub(crate) struct ProviderDrag {
    id: String,
}

/// 拖拽跟随幽灵（显示被拖供应商名）。
pub(crate) struct DragGhost {
    label: String,
    offset: gpui::Point<gpui::Pixels>,
}

impl Render for DragGhost {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .left(self.offset.x)
            .top(self.offset.y)
            .px(px(10.))
            .py(px(6.))
            .rounded(px(8.))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::stroke_a(0.4))
            .shadow_lg()
            .text_size(px(11.))
            .text_color(theme::text())
            .child(self.label.clone())
    }
}

/// 界面页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Providers,
    Routing,
    Mcp,
    Skills,
    Prompts,
    Usage,
    Tools,
    Projects,
    Sessions,
    Accounts,
    Apps,
    Universal,
    ToolConfig,
    Settings,

}

impl Page {
    const ALL: [Page; 14] = [
        Page::Providers,
        Page::Routing,
        Page::Mcp,
        Page::Skills,
        Page::Prompts,
        Page::Usage,
        Page::Tools,
        Page::Projects,
        Page::Sessions,
        Page::Accounts,
        Page::Apps,
        Page::Universal,
        Page::ToolConfig,
        Page::Settings,
    ];
}

/// Page → i18n key（侧栏翻译用）。
fn page_i18n_key(page: Page) -> &'static str {
    match page {
        Page::Providers => "sidebar.providers",
        Page::Routing => "sidebar.routing",
        Page::Mcp => "sidebar.mcp",
        Page::Skills => "sidebar.skills",
        Page::Prompts => "sidebar.prompts",
        Page::Usage => "sidebar.usage",
        Page::Tools => "sidebar.tools",
        Page::Projects => "sidebar.projects",
        Page::Sessions => "sidebar.sessions",
        Page::Accounts => "sidebar.accounts",
        Page::Apps => "sidebar.apps",
        Page::Universal => "sidebar.universal",
        Page::ToolConfig => "sidebar.toolconfig",
        Page::Settings => "sidebar.settings",
    }
}

/// token 数格式化（>1M 显示 M，>1k 显示 k，否则原样）。
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// 缓存命中率（%）：缓存读 /（输入 + 缓存读）。两者都为 0 时返回 0。
///
/// Anthropic/Codex 的 `input_tokens` 不含缓存读，所以缓存读要计入分母。
fn cache_hit_rate(cache_read: u64, input: u64) -> f64 {
    let total = cache_read.saturating_add(input);
    if total == 0 {
        0.0
    } else {
        cache_read as f64 / total as f64 * 100.0
    }
}

/// 合并两组按日统计（同日期累加），按日期排序。
fn merge_daily(
    a: Vec<ns_usage::DailyCount>,
    b: Vec<ns_usage::DailyCount>,
) -> Vec<ns_usage::DailyCount> {
    let mut map: std::collections::BTreeMap<String, ns_usage::DailyCount> = std::collections::BTreeMap::new();
    for d in a.into_iter().chain(b) {
        let e = map.entry(d.date.clone()).or_insert(ns_usage::DailyCount {
            date: d.date.clone(),
            ..Default::default()
        });
        e.requests += d.requests;
        e.input_tokens += d.input_tokens;
        e.output_tokens += d.output_tokens;
    }
    map.into_values().collect()
}

/// 合并两组请求明细（拼接后按时间新→旧排序）。
fn merge_details(
    mut a: Vec<ns_usage::RequestDetail>,
    b: Vec<ns_usage::RequestDetail>,
) -> Vec<ns_usage::RequestDetail> {
    a.extend(b);
    a.sort_by_key(|d| std::cmp::Reverse(d.ts_ms));
    a
}

/// 在系统终端执行恢复命令（命令由 `ns_sessions::recover_command_for` 生成）。
fn run_terminal_command(cmd: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let spawn = std::process::Command::new("cmd").arg("/C").arg(cmd).spawn();
    #[cfg(not(target_os = "windows"))]
    let spawn = std::process::Command::new("sh").arg("-c").arg(cmd).spawn();
    spawn.map(|_| ())
}

/// Page → 侧栏图标字形（纯文本，后续可换 SVG）。
fn page_glyph(page: Page) -> &'static str {
    match page {
        Page::Providers => "▤",
        Page::Routing => "⇄",
        Page::Mcp => "⛁",
        Page::Skills => "✦",
        Page::Prompts => "✎",
        Page::Usage => "▥",
        Page::Tools => "⌬",
        Page::Projects => "▦",
        Page::Sessions => "☰",
        Page::Accounts => "◉",
        Page::Apps => "▦",
        Page::Universal => "◈",
        Page::ToolConfig => "⚙",
        Page::Settings => "⚙",
    }
}

/// 供应商页模式视图（cc-switch 对应 direct/route/stack 三格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderMode {
    Direct,
    Route,
    Stack,
}

impl ProviderMode {
    fn label(self) -> &'static str {
        match self {
            ProviderMode::Direct => "直连",
            ProviderMode::Route => "路由",
            ProviderMode::Stack => "聚合",
        }
    }
}

/// 状态栏语义色：中性默认（就绪/加载）、成功（玉色）、错误（朱色）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusTone {
    Neutral,
    Success,
    Error,
}

impl StatusTone {
    pub fn color(&self) -> Hsla {
        match self {
            StatusTone::Neutral => theme::text_dim(),
            StatusTone::Success => theme::jade(),
            StatusTone::Error => theme::danger(),
        }
    }
}

/// 备份恢复来源：live 配置文件首写备份 / 手动 config.json 备份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupRestoreKind {
    Live,
    Manual,
}

impl BackupRestoreKind {
    fn label(self) -> &'static str {
        match self {
            BackupRestoreKind::Live => "首写备份",
            BackupRestoreKind::Manual => "手动备份",
        }
    }
}

/// 设置页分区（对齐 cc-switch SettingsSection）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    General,
    AppConfig,
    Routing,
    Network,
    Data,
    About,
}

impl SettingsSection {
    const ALL: [SettingsSection; 6] = [
        SettingsSection::General,
        SettingsSection::AppConfig,
        SettingsSection::Routing,
        SettingsSection::Network,
        SettingsSection::Data,
        SettingsSection::About,
    ];

    fn label(self) -> &'static str {
        match self {
            SettingsSection::General => "通用",
            SettingsSection::AppConfig => "应用配置",
            SettingsSection::Routing => "路由",
            SettingsSection::Network => "网络",
            SettingsSection::Data => "数据",
            SettingsSection::About => "关于",
        }
    }
}


/// 应用状态。
pub struct SwitchApp {
    pub(crate) store: Arc<RwLock<Store>>,
    pub(crate) active_tool: Tool,
    pub(crate) page: Page,
    pub(crate) sidebar_visible: bool,
    pub(crate) status: String,
    /// 状态栏语义色（中性默认、成功玉色、错误朱色）。
    pub(crate) status_tone: StatusTone,
    pub(crate) switch_conflicts: Vec<String>,
    /// 待用户确认的切换请求（存在写入冲突时暂存；确认后强制执行）。
    pub(crate) pending_switch: Option<(Tool, ns_presets::Provider, String)>,
    /// 启动时扫描到的遗留冲突（用户尚未处理），用于启动横幅展示。
    pub(crate) startup_conflicts: Vec<String>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) open_menu: Option<String>,
    pub(crate) add_open: bool,
    pub(crate) dialog_preset: Option<String>,
    /// 正在编辑的供应商 id（None = 无）。
    pub(crate) edit_id: Option<String>,
    /// 待确认删除的供应商 id（None = 无）。
    pub(crate) confirm_delete: Option<String>,
    /// 复制供应商后的高亮反馈（新副本卡片短暂高亮；None = 无）。
    pub(crate) copied_provider: Option<String>,
    /// 连通性测试结果（provider id → (成功, 摘要)）。健康芯片显示依据。
    pub(crate) health: std::collections::BTreeMap<String, (bool, String)>,

    pub(crate) dialog_fields: dialog::DialogFields,
    /// 对话框表单是否有未保存更改（用于关闭守卫）。
    pub(crate) dialog_dirty: bool,
    pub(crate) dialog_inputs: dialog::DialogInputs,
    /// 供应商页搜索输入。
    pub(crate) provider_search: gpui::Entity<crate::text_field::TextField>,
    /// 拖拽中的供应商 id（用于排序预览）。
    pub(crate) drag_provider: Option<String>,
    /// 拖拽悬停位置（排序落点 id；None = 拖拽结束）。
    pub(crate) drop_target: Option<String>,
    /// 每工具路由代理运行时。
    pub(crate) routing: crate::routing::RoutingRuntime,
    /// 路由代理状态文本（每工具，key = tool id_str）。
    pub(crate) routing_status: std::collections::BTreeMap<String, String>,
    /// 系统托盘（None = 未创建成功）。
    pub(crate) tray: Option<std::sync::Arc<tray::Tray>>,
    /// 托盘菜单/点击事件接收端（共享给后台轮询任务）。
    pub(crate) tray_rx: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<tray::TrayAction>>>,
    /// 单实例深链接收端（后台监听线程转发；None = 未启用单实例）。
    pub(crate) deeplink_rx:
        Option<std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<String>>>>,
    /// 主窗口句柄（托盘“打开主窗口”用；共享给后台轮询任务）。
    pub(crate) window_handle:
        std::sync::Arc<std::sync::Mutex<Option<gpui::WindowHandle<SwitchApp>>>>,
    /// M7 统一 MCP 面板状态。
    pub(crate) mcp: crate::mcp::McpPanel,
    /// M7 统一 Skills 面板状态。
    pub(crate) skills: crate::skills::SkillsPanel,
    /// 聚合模式当前编辑的工具。
    pub(crate) agg_tool: Tool,
    /// M7 统一 Prompts 面板状态。

    pub(crate) prompts: crate::prompts::PromptsPanel,
    /// M9 用量结果（tool id_str → Usage）。
    pub(crate) usage: std::collections::BTreeMap<String, ns_usage::Usage>,
    /// 用量查询中标志（tool id_str → bool）。
    pub(crate) usage_loading: std::collections::BTreeMap<String, bool>,
    /// 会话日志扫描结果（按模型统计）。
    pub(crate) session_stats: Vec<ns_usage::ModelStats>,
    /// 会话扫描中。

    pub(crate) session_scanning: bool,
    /// 按日统计（趋势图/热力图）。
    pub(crate) usage_daily: Vec<ns_usage::DailyCount>,
    /// 请求明细（新→旧）。

    pub(crate) usage_details: Vec<ns_usage::RequestDetail>,
    /// 按日统计（按工具拆分：key = tool id_str）。用于用量图表按工具筛选。
    pub(crate) usage_daily_by_tool: std::collections::BTreeMap<String, Vec<ns_usage::DailyCount>>,
    /// 用量图表包含的工具（key = tool id_str；空 = 全部）。
    pub(crate) usage_chart_tools: std::collections::BTreeMap<String, bool>,
    /// 用量图表日期范围（天数；0 = 全部）。对齐 cc-switch UsageDateRangePicker。
    pub(crate) usage_range_days: usize,
    /// M10 会话列表。
    pub(crate) sessions: Vec<ns_sessions::SessionMeta>,
    /// 会话页搜索输入。
    pub(crate) session_search: gpui::Entity<crate::text_field::TextField>,
    /// 待确认删除的会话路径（None = 无）。
    pub(crate) session_delete: Option<String>,
    /// 会话加载中。
    pub(crate) sessions_loading: bool,
    /// 会话来源工具（Claude/Codex）。
    pub(crate) session_tool: ns_sessions::SessionTool,
    /// 正在阅读的会话路径（None = 列表）。
    pub(crate) session_viewing: Option<String>,
    /// 阅读会话的分轮消息。
    pub(crate) session_rounds: Vec<ns_sessions::Round>,
    /// 阅读会话的元数据。
    pub(crate) session_meta: Option<ns_sessions::SessionMeta>,
    /// 每轮折叠状态（true = 展开）。

    pub(crate) session_collapsed: Vec<bool>,
    /// 阅读页会话内搜索。
    pub(crate) session_filter: gpui::Entity<crate::text_field::TextField>,
    /// M11 工具扫描结果（工具, 安装）。
    pub(crate) tools_result: Vec<(ns_tools::Tool, Option<ns_tools::ToolInstall>)>,
    pub(crate) tools_scanning: bool,
    /// 工具安装/卸载结果消息。
    pub(crate) tool_msg: String,
    /// 项目快照新项目名输入。
    pub(crate) project_name: gpui::Entity<crate::text_field::TextField>,
    /// 自动更新检查结果（None = 未检查）。

    pub(crate) update_status: Option<Result<String, String>>,
    /// 云同步配置输入。
    pub(crate) sync_url: gpui::Entity<crate::text_field::TextField>,
    pub(crate) sync_user: gpui::Entity<crate::text_field::TextField>,
    pub(crate) sync_pass: gpui::Entity<crate::text_field::TextField>,
    /// 云同步状态。
    pub(crate) sync_status: String,
    /// 云同步提供方（webdav / s3）。
    pub(crate) sync_provider: String,
    /// S3 bucket。
    pub(crate) sync_bucket: gpui::Entity<crate::text_field::TextField>,
    /// 自动同步间隔（分钟）。
    pub(crate) sync_interval: gpui::Entity<crate::text_field::TextField>,
    pub(crate) price_model: gpui::Entity<crate::text_field::TextField>,
    pub(crate) price_input: gpui::Entity<crate::text_field::TextField>,
    pub(crate) price_output: gpui::Entity<crate::text_field::TextField>,
    pub(crate) price_cache: gpui::Entity<crate::text_field::TextField>,
    /// 自定义定价操作状态。
    pub(crate) pricing_status: String,
    /// 全局出站代理 URL 输入。
    pub(crate) global_proxy_url: gpui::Entity<crate::text_field::TextField>,
    /// 全局代理操作状态（保存/测试/扫描反馈）。
    pub(crate) proxy_status: String,
    /// 全局代理测试结果（(成功, 延迟毫秒, 错误)）。
    pub(crate) proxy_test: Option<(bool, u64, Option<String>)>,
    /// 扫描到的本地代理（(URL, 协议, 端口)）。
    pub(crate) proxy_scan: Vec<(String, String, u16)>,
    pub(crate) provider_mode: ProviderMode,
    /// 每工具配置目录覆盖输入（key = 工具 `id_str()`）。
    pub(crate) config_dir_inputs: std::collections::BTreeMap<String, gpui::Entity<crate::text_field::TextField>>,
    /// 配置目录覆盖操作状态。
    pub(crate) config_dir_status: String,
    /// 备份管理操作状态。
    pub(crate) backup_status: String,
    /// 待确认的备份恢复（来源、路径、显示名）。
    pub(crate) backup_restore_pending: Option<(BackupRestoreKind, String, String)>,
    /// 设置页当前分区。
    pub(crate) settings_section: SettingsSection,
    /// 通用供应商对话框打开。
    pub(crate) universal_open: bool,
    /// 正在编辑的通用供应商 id（None = 添加）。
    pub(crate) universal_edit: Option<String>,
    /// 待确认删除的通用供应商 id。
    pub(crate) universal_confirm_delete: Option<String>,
    /// 通用供应商表单字段。
    pub(crate) universal_form: crate::universal::UniversalForm,
    /// 通用供应商对话框输入。
    pub(crate) universal_inputs: crate::universal::UniversalInputs,
    /// 工具配置编辑器打开。
    pub(crate) toolconfig_open: bool,
    /// 正在编辑的配置文件 id。
    pub(crate) toolconfig_id: Option<String>,
    /// 工具配置编辑器输入实体。
    pub(crate) toolconfig_editor: Option<gpui::Entity<crate::text_area::TextArea>>,
    /// 工具配置保存状态文本。
    pub(crate) toolconfig_status: String,
    /// OpenClaw 每日记忆文件列表。
    pub(crate) daily_memory_files: Vec<crate::toolconfig::DailyMemoryFile>,
    /// 正在编辑的每日记忆文件名。
    pub(crate) daily_memory_editing: Option<String>,
    /// 每日记忆编辑器输入实体。
    pub(crate) daily_memory_editor: Option<gpui::Entity<crate::text_area::TextArea>>,
    /// 每日记忆保存状态文本。
    pub(crate) daily_memory_status: String,
    /// 待确认删除的每日记忆文件名。
    pub(crate) daily_memory_confirm_delete: Option<String>,
    /// OpenClaw agents.defaults 结构化编辑打开。
    pub(crate) openclaw_defaults_open: bool,
    /// 正在编辑的 agents.defaults 字段。
    pub(crate) oc_defaults: crate::toolconfig::OpenClawAgentsDefaults,
    /// agents.defaults 编辑输入实体。
    pub(crate) oc_defaults_inputs: crate::toolconfig::OpenClawDefaultsInputs,
    /// agents.defaults 保存状态文本。
    pub(crate) oc_defaults_status: String,
    /// 首次运行欢迎页是否显示。
    pub(crate) welcome_visible: bool,
    /// 更新说明（WhatsNew）浮层是否显示。
    pub(crate) whats_new_visible: bool,
    /// 待确认的深链导入动作（action + 脱敏摘要；None = 无）。
    pub(crate) deeplink_pending:
        Option<(crate::deeplink::DeepLinkAction, String)>,
    /// 进行中的 OAuth device-code 登录（provider + session + 账号 id + 显示名）。
    pub(crate) oauth_login:
        Option<(String, crate::auth::DeviceCodeSession, String, String)>,
}

impl SwitchApp {
    pub fn new(
        cx: &mut Context<Self>,
        deeplink_rx: Option<std::sync::mpsc::Receiver<String>>,
    ) -> Self {
        let store = Arc::new(RwLock::new(
            Store::open().unwrap_or_else(|e| {
                eprintln!("打开存储失败，使用默认: {e}");
                Store::open_at(std::env::temp_dir().join("ns-switch-fallback")).unwrap_or_else(|_| {
                    // 极端情况：临时目录也不可写；用内存态（不落盘）。
                    Store::open_at(std::env::temp_dir()).expect("临时目录不可写")
                })
            }),
        ));
        // 同步全局主题（P1-1：light/system 实际应用）。
        let init_mode = theme::ThemeMode::parse(
            &store
                .read()
                .map(|s| s.config().settings.theme.clone())
                .unwrap_or_else(|_| "dark".into()),
        );
        ns_ui::theme::set_active_mode(init_mode);
        let welcome_visible = !store
            .read()
            .map(|s| s.config().settings.welcome_dismissed)
            .unwrap_or(false);
        // WhatsNew：上次版本非空且不同于当前版本 → 显示更新说明。
        let last_seen = store
            .read()
            .map(|s| s.config().settings.last_seen_version.clone())
            .unwrap_or_default();
        let whats_new_visible = !last_seen.is_empty() && last_seen != crate::update::CURRENT_VERSION;
        let (tray_tx, tray_rx) = std::sync::mpsc::channel();
        crate::tray::register_event_handler(tray_tx);
        let tray = Self::build_tray(store.clone());
        let tray_rx = std::sync::Arc::new(std::sync::Mutex::new(tray_rx));
        let window_handle = std::sync::Arc::new(std::sync::Mutex::new(None));
        let mut app = Self {
            store: store.clone(),
            active_tool: Tool::Claude,
            page: Page::Providers,
            sidebar_visible: true,
            status: "就绪".into(),
            status_tone: StatusTone::Neutral,
            switch_conflicts: Vec::new(),
            pending_switch: None,
            startup_conflicts: Vec::new(),
            focus_handle: cx.focus_handle(),
            open_menu: None,
            add_open: false,
            dialog_preset: None,
            edit_id: None,
            confirm_delete: None,
            copied_provider: None,
            health: std::collections::BTreeMap::new(),

            dialog_fields: dialog::DialogFields::default(),
            dialog_dirty: false,
            dialog_inputs: dialog::DialogInputs::new(cx),
            provider_search: cx.new(|cx| crate::text_field::TextField::new("搜索供应商…", cx)),
            drag_provider: None,
            drop_target: None,
            routing: crate::routing::RoutingRuntime::new(),
            routing_status: Default::default(),
            tray,
            tray_rx: tray_rx.clone(),
            deeplink_rx: deeplink_rx
                .map(|rx| std::sync::Arc::new(std::sync::Mutex::new(rx))),

            window_handle: window_handle.clone(),
            mcp: crate::mcp::McpPanel::new(cx),
            skills: crate::skills::SkillsPanel::new(cx),
            prompts: crate::prompts::PromptsPanel::new(cx),
            agg_tool: Tool::Claude,
            usage: Default::default(),
            usage_loading: Default::default(),
            session_stats: Vec::new(),
            session_scanning: false,
            usage_daily: Vec::new(),
            usage_details: Vec::new(),
            usage_daily_by_tool: Default::default(),
            usage_chart_tools: Default::default(),
            usage_range_days: 30,
            sessions: Vec::new(),
            session_search: cx.new(|cx| crate::text_field::TextField::new("搜索会话…", cx)),
            session_delete: None,
            sessions_loading: false,
            session_tool: ns_sessions::SessionTool::Claude,
            session_viewing: None,
            session_rounds: Vec::new(),
            session_meta: None,
            session_collapsed: Vec::new(),
            session_filter: cx.new(|cx| crate::text_field::TextField::new("会话内搜索…", cx)),
            tools_result: Vec::new(),
            tools_scanning: false,
            tool_msg: String::new(),
            project_name: cx.new(|cx| crate::text_field::TextField::new("项目名…", cx)),
            update_status: None,
            sync_url: cx.new(|cx| crate::text_field::TextField::new("WebDAV URL…", cx)),
            sync_user: cx.new(|cx| crate::text_field::TextField::new("用户名…", cx)),
            sync_pass: cx.new(|cx| crate::text_field::TextField::new("密码…", cx)),
            sync_status: String::new(),
            sync_provider: "webdav".into(),
            sync_bucket: cx.new(|cx| crate::text_field::TextField::new("S3 bucket（S3 用）", cx)),
            sync_interval: cx.new(|cx| crate::text_field::TextField::new("自动同步间隔（分钟，0=关）", cx)),
            price_model: cx.new(|cx| crate::text_field::TextField::new("模型前缀（如 claude-sonnet-4-5）", cx)),
            price_input: cx.new(|cx| crate::text_field::TextField::new("输入 $/1M", cx)),
            price_output: cx.new(|cx| crate::text_field::TextField::new("输出 $/1M", cx)),
            price_cache: cx.new(|cx| crate::text_field::TextField::new("缓存读 $/1M（可选）", cx)),
            pricing_status: String::new(),
            global_proxy_url: cx.new(|cx| crate::text_field::TextField::new("http://127.0.0.1:7890", cx)),
            proxy_status: String::new(),
            proxy_test: None,
            proxy_scan: Vec::new(),
            provider_mode: ProviderMode::Direct,
            config_dir_inputs: Default::default(),
            config_dir_status: String::new(),
            backup_status: String::new(),
            backup_restore_pending: None,
            settings_section: SettingsSection::General,
            universal_open: false,
            universal_edit: None,
            universal_confirm_delete: None,
            universal_form: crate::universal::UniversalForm::default(),
            universal_inputs: crate::universal::UniversalInputs::new(cx),
            toolconfig_open: false,
            toolconfig_id: None,
            toolconfig_editor: None,
            toolconfig_status: String::new(),
            daily_memory_files: Vec::new(),
            daily_memory_editing: None,
            daily_memory_editor: None,
            daily_memory_status: String::new(),
            daily_memory_confirm_delete: None,
            openclaw_defaults_open: false,
            oc_defaults: Default::default(),
            oc_defaults_inputs: crate::toolconfig::OpenClawDefaultsInputs::new(cx),
            oc_defaults_status: String::new(),
            welcome_visible,
            whats_new_visible,
            deeplink_pending: None,
            oauth_login: None,
        };
        // 记录本次版本（供下次启动 WhatsNew 检测）。
        if let Ok(mut s) = store.write() {
            let _ = s.set_last_seen_version(crate::update::CURRENT_VERSION);
        }
        // 回填全局出站代理到输入框，并应用进程级代理环境（curl/git 子进程继承）。
        {
            let proxy = store
                .read()
                .map(|s| s.config().settings.global_proxy().map(|p| p.to_string()))
                .unwrap_or(None);
            if let Some(p) = proxy.as_deref() {
                app.global_proxy_url.update(cx, |f, _cx| f.set_text(p));
                unsafe {
                    for (k, v) in crate::proxy_env::curl_env(p) {
                        std::env::set_var(k, v);
                    }
                }
            }
        }
        // 深链导入：启动时处理 `nsswitch://`（env 或命令行参数）。
        let deeplink = std::env::var("NS_SWITCH_DEEPLINK")
            .ok()
            .or_else(|| std::env::args().nth(1).filter(|a| a.starts_with("nsswitch://")));
        if let Some(url) = deeplink {
            if let Some(action) = crate::deeplink::parse(&url) {
                app.apply_deeplink(&action, cx);
            }
        }
        // 后台轮询托盘事件：窗口最小化/关闭后 render 不再运行，需独立消费。
        let poller = app.tray_rx.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let actions: Vec<tray::TrayAction> = {
                    let rx = poller.lock().unwrap();
                    let mut v = Vec::new();
                    while let Ok(a) = rx.try_recv() {
                        v.push(a);
                    }
                    v
                };
                for action in actions {
                    match action {
                        tray::TrayAction::OpenWindow => {
                            // 在 app 上下文更新窗口（避免实体重入）。
                            let wh = window_handle.lock().unwrap().clone();
                            if let Some(wh) = wh {
                                let _ = cx.update(|app| {
                                    let _ = wh.update(app, |_v, w, _c| w.activate_window());
                                });
                            }
                        }
                        other => {
                            this.update(cx, |this, cx| this.handle_tray_action(other, cx))
                                .ok();
                        }
                    }
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
            }
        })
        .detach();
        // 后台轮询单实例转发的深链 URL（窗口最小化时 render 不运行）。
        let deeplink_rx = app.deeplink_rx.clone();
        if let Some(rx) = deeplink_rx {
            cx.spawn(async move |this, cx| {
                loop {
                    let urls: Vec<String> = {
                        let rx = rx.lock().unwrap();
                        let mut v = Vec::new();
                        while let Ok(u) = rx.try_recv() {
                            v.push(u);
                        }
                        v
                    };
                    for url in urls {
                        if let Some(action) = crate::deeplink::parse(&url) {
                            this.update(cx, |this, cx| this.apply_deeplink(&action, cx))
                                .ok();
                        }
                    }
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                }
            })
            .detach();
        }
        // 启动后台定期自动检查更新（启动后 3 秒首次检查，此后每 6 小时循环；
        // 不阻塞，发现新版写入 update_status 供设置页显示）。
        cx.spawn(async move |this, cx| {
            loop {
                // 延迟：首次 3 秒，之后 6 小时。
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                let result = crate::update::check_update();
                if let Some(v) = result {
                    this.update(cx, |this, cx| {
                        if this.update_status.is_none() {
                            this.update_status = Some(Ok(format!("发现新版本 {v}")));
                            cx.notify();
                        }
                    })
                    .ok();
                }
                // 等待下一个周期（6 小时）。
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(6 * 3600))
                    .await;
            }
        })
        .detach();
        // 启动后台定期自动同步（按 settings.sync.auto_sync_minutes；0 = 关闭）。
        let sync_store = app.store.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let minutes = {
                    let s = sync_store.read().expect("store lock");
                    s.config().settings.sync.auto_sync_minutes
                };
                if minutes == 0 {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(60))
                        .await;
                    continue;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(minutes as u64 * 60))
                    .await;
                let r = {
                    let s = sync_store.read().expect("store lock");
                    if s.config().settings.sync.enabled {
                        s.sync_push()
                    } else {
                        Ok(())
                    }
                };
                if let Err(e) = r {
                    this.update(cx, |this, cx| {
                        this.sync_status = format!("自动同步失败: {e}");
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
        // 启动后异步扫描所有工具的遗留配置冲突，在供应商页首屏提示用户。
        let scan_store = app.store.clone();
        cx.spawn(async move |this, _cx| {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let conflicts: Vec<String> = if let Some(h) = &home {
                let mut out = Vec::new();
                let s = scan_store.read().expect("store lock");
                let cdirs = s.config().settings.config_dirs.clone();
                for tool in Tool::ALL.iter() {
                    let cfg = s.config();
                    if let Some(current_id) = cfg.current.get(*tool) {
                        if let Some(current_provider) = cfg.providers.get(current_id) {
                            let cfs = crate::switch_engine::check_conflicts_dirs(
                                *tool, current_provider, None, h, &cdirs
                            );
                            for c in cfs {
                                out.push(format!("{}：{}", c.path.display(), c.reason));
                            }
                        }
                    }
                }
                out
            } else {
                Vec::new()
            };
            this.update(_cx, |this, _| {
                if !conflicts.is_empty() {
                    this.startup_conflicts = conflicts;
                }
            }).ok();
        })
        .detach();
        app
    }

    /// 从存储快照构建托盘（含初始菜单）。
    fn build_tray(store: Arc<RwLock<Store>>) -> Option<std::sync::Arc<tray::Tray>> {
        let entries = tray_entries(&store, &std::collections::BTreeMap::new());
        match tray::Tray::new(&entries) {
            Ok(t) => Some(std::sync::Arc::new(t)),
            Err(e) => {
                eprintln!("托盘创建失败（继续无托盘运行）: {e}");
                None
            }
        }
    }

    /// 处理单个托盘动作（后台轮询任务调用）。
    fn handle_tray_action(&mut self, action: tray::TrayAction, cx: &mut Context<Self>) {
        match action {
            tray::TrayAction::SwitchProvider { tool, provider_id } => {
                self.switch_to_provider_for(tool, &provider_id, cx);
                self.refresh_tray_menu(cx);
            }
            tray::TrayAction::Quit => {
                self.write_back_routing_direct();
                self.routing.stop_all();
                cx.quit();
            }
            tray::TrayAction::OpenWindow => {
                // 由后台轮询任务在 app 上下文处理（避免实体重入）。
            }
        }
    }

    /// 消费托盘事件（非阻塞；由渲染循环调用，窗口可见时即时响应）。
    fn poll_tray(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let actions: Vec<tray::TrayAction> = {
            let rx = self.tray_rx.lock().unwrap();
            let mut v = Vec::new();
            while let Ok(a) = rx.try_recv() {
                v.push(a);
            }
            v
        };
        for action in actions {
            match action {
                tray::TrayAction::OpenWindow => window.activate_window(),
                other => self.handle_tray_action(other, cx),
            }
        }
    }

    /// 供应商集合变化后重建托盘菜单。
    pub(crate) fn refresh_tray_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(tray) = &self.tray {
            let entries = tray_entries(&self.store, &self.usage);
            if let Err(e) = tray.rebuild_menu(&entries) {
                eprintln!("托盘菜单重建失败: {e}");
            }
        }
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        // 按当前页分发真实刷新（cmd-r）：用量页重查统计+各工具用量、会话页重扫、供应商页重绘重读。
        match self.page {
            Page::Usage => {
                self.refresh_session_stats(cx);
                for &tool in Tool::ALL.iter() {
                    self.query_usage(tool, cx);
                }
                self.status = "用量已刷新".into();
            }
            Page::Sessions => {
                self.load_sessions(cx);
                self.status = "会话已刷新".into();
            }
            _ => {
                self.status = "已刷新".into();
            }
        }
        cx.notify();
    }

    /// 保存全局出站代理：写 settings + 应用/清除进程级代理环境（curl/git 继承）。
    fn save_global_proxy(&mut self, cx: &mut Context<Self>) {
        let url = self.global_proxy_url.read(cx).text().trim().to_string();
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("socks") {
            self.proxy_status = "代理 URL 需以 http://、https:// 或 socks5:// 开头".into();
            self.status_tone = StatusTone::Error;
            cx.notify();
            return;
        }
        let env = crate::proxy_env::curl_env(&url);
        {
            let mut s = self.store.write().expect("store lock");
            let _ = s.set_global_proxy(&url);
        }
        // 应用/清除进程级代理环境。
        unsafe {
            for (k, _) in crate::proxy_env::curl_env("x") {
                std::env::remove_var(k);
            }
            for (k, v) in env {
                std::env::set_var(k, v);
            }
        }
        self.proxy_test = None;
        self.proxy_status = if url.is_empty() {
            "已清除全局代理（直连）".into()
        } else {
            format!("已保存全局代理：{url}")
        };
        self.status_tone = StatusTone::Success;
        cx.notify();
    }

    /// 保存某工具的配置目录覆盖（空 = 恢复默认）。
    fn save_config_dir(&mut self, tool: ns_presets::Tool, cx: &mut Context<Self>) {
        let Some(e) = self.config_dir_inputs.get(tool.id_str()) else {
            return;
        };
        let dir = e.read(cx).text().trim().to_string();
        let result = {
            let mut s = self.store.write().expect("store lock");
            s.set_config_dir(tool, &dir)
        };
        self.config_dir_status = match result {
            Ok(()) => {
                if dir.is_empty() {
                    format!("{} 配置目录已恢复默认", tool.label())
                } else {
                    format!("{} 配置目录已设为 {dir}", tool.label())
                }
            }
            Err(e) => format!("保存失败: {e}"),
        };
        self.status_tone = if self.config_dir_status.contains("失败") {
            StatusTone::Error
        } else {
            StatusTone::Success
        };
        cx.notify();
    }

    /// 测试全局代理连接（后台 curl 经代理请求公共端点）。
    fn test_global_proxy(&mut self, cx: &mut Context<Self>) {
        let url = self.global_proxy_url.read(cx).text().trim().to_string();
        self.proxy_status = "正在测试代理…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (ok, ms, err) = crate::proxy_env::test_proxy(&url);
            this.update(cx, |this, cx| {
                this.proxy_test = Some((ok, ms, err.clone()));
                this.proxy_status = if ok {
                    format!("代理可用（{ms}ms）")
                } else {
                    format!("代理不可用：{}", err.unwrap_or_default())
                };
                this.status_tone = if ok { StatusTone::Success } else { StatusTone::Error };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 扫描本地常见代理端口。
    fn scan_proxies(&mut self, cx: &mut Context<Self>) {
        let found = crate::proxy_env::scan_local_proxies();
        self.proxy_scan = found.clone();
        self.proxy_status = if found.is_empty() {
            "未发现本地代理".into()
        } else {
            format!("发现 {} 个本地代理", found.len())
        };
        cx.notify();
    }

    fn select_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.active_tool = tool;
        self.page = Page::Providers;
        cx.notify();
    }

    fn select_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        // 进入 Sessions 页即自动加载会话列表（否则首屏空态，需手动点「刷新」）。
        if page == Page::Sessions && self.sessions.is_empty() && !self.sessions_loading {
            self.load_sessions(cx);
        }
        // MCP/Skills/Prompts 面板进入即预加载当前工具（否则首屏空态）。
        if page == Page::Mcp && self.mcp.servers.is_empty() {
            self.mcp.load(cx);
        }
        if page == Page::Skills && self.skills.skills.is_empty() {
            self.skills.shared = self
                .store
                .read()
                .map(|s| s.config().settings.shared_skills)
                .unwrap_or(false);
            self.skills.load(cx);
        }
        if page == Page::Prompts && self.prompts.entries.is_empty() {
            self.prompts.load(cx);
        }
        if page == Page::ToolConfig && self.daily_memory_files.is_empty() {
            self.daily_memory_files = crate::toolconfig::list_daily_memory();
        }
        cx.notify();
    }

    fn render_topbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(44.))
            .px(px(14.))
            .flex()
            .items_center()
            .justify_between()
            .bg(theme::bg_1().opacity(0.78))
            .border_b_1()
            .border_color(theme::stroke_a(0.1))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(widgets::brand_mark(22.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("ns-switch"),
                    )
                    .child(
                        div()
                            .id("topbar-collapse")
                            .cursor_pointer()
                            .size(px(26.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.))
                            .text_color(theme::text_dim())
                            .hover(|s| s.bg(theme::stroke_a(0.08)))
                            .child(if self.sidebar_visible { "«" } else { "»" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar_visible = !this.sidebar_visible;
                                cx.notify();
                            })),
                    ),
            )
            .when(
                self.store
                    .read()
                    .map(|s| s.config().settings.project_switcher)
                    .unwrap_or(false),
                |el| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("项目"))
                            .child(
                                widgets::ghost_button("top-project", "切换项目…")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_menu = Some("project".to_string());
                                        cx.notify();
                                    })),
                            ),
                    )
                },
            )
    }

    /// 侧栏（cc-switch v7 布局）：顶条 → 应用列表滚动区 → 分隔线 → 全局页面项（贴底）→ 分隔线 → 底栏（应用/设置）。
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = crate::i18n::Lang::parse(
            &self
                .store
                .read()
                .map(|s| s.config().settings.language.clone())
                .unwrap_or_else(|_| "zh-cn".into()),
        );
        // 全局页面项（cc-switch MainDirectory 的 globals：MCP/Skills/Prompts/Usage/Sessions 等）。
        const GLOBAL_PAGES: [Page; 11] = [
            Page::Routing,
            Page::Tools,
            Page::Mcp,
            Page::Skills,
            Page::Prompts,
            Page::Usage,
            Page::Sessions,
            Page::Projects,
            Page::Accounts,
            Page::Universal,
            Page::ToolConfig,
        ];
        // 当前生效模式缓存（每工具：路由/聚合/直连）。
        let store = self.store.read().expect("store lock");
        let tools = store
            .tool_order()
            .into_iter()
            .filter(|&t| store.tool_visible(t))
            .collect::<Vec<_>>();
        let mut mode_map = std::collections::BTreeMap::new();
        let mut usage_map = std::collections::BTreeMap::new();
        for &t in &tools {
            let rc = store.config().settings.routing_for(t);
            let agg = store.config().aggregation_for(t);
            mode_map.insert(
                t.id_str().to_string(),
                if rc.enabled {
                    ProviderMode::Route
                } else if agg.enabled {
                    ProviderMode::Stack
                } else {
                    ProviderMode::Direct
                },
            );
            if let Some(u) = self.usage.get(t.id_str()) {
                if u.remaining.is_some() || u.total.is_some() {
                    usage_map.insert(t.id_str().to_string(), u.clone());
                }
            }
        }
        drop(store);

        div()
            .w(px(200.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::bg_1().opacity(0.56))
            .border_r_1()
            .border_color(theme::stroke_a(0.1))
            // 顶条
            .child(
                div()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(widgets::section_label("AI 工具"))
                    .child(
                        div()
                            .text_size(px(9.5))
                            .text_color(theme::text_faint())
                            .child(format!("{} 个", tools.len())),
                    ),
            )
            // 应用列表（唯一滚动区）
            .child(
                div()
                    .id("tool-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .py(px(6.))
                    .px(px(6.))
                    .flex()
                    .flex_col()
                    .children({
                        tools.into_iter().enumerate().map(|(i, tool)| {
                            let active = tool == self.active_tool;
                            let mode = mode_map.get(tool.id_str()).copied().unwrap_or(ProviderMode::Direct);
                            let usage = usage_map.get(tool.id_str()).cloned();
                            div()
                                .id(("tool-item", i))
                                .h(px(30.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .rounded(px(6.))
                                .bg(if active {
                                    theme::accent_a(0.12)
                                } else {
                                    theme::alpha(0xffffff, 0.0)
                                })
                                .cursor_pointer()
                                .hover(|style| style.bg(theme::stroke_a(0.07)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.select_tool(tool, cx);
                                }))
                                .child(
                                    div()
                                        .w(px(18.))
                                        .text_size(px(12.))
                                        .text_color(rgb(if active {
                                            theme::ACCENT
                                        } else {
                                            theme::TEXT_FAINT
                                        }))
                                        .child(tool.glyph()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_size(px(11.5))
                                        .text_color(if active {
                                            theme::text()
                                        } else {
                                            theme::text_dim()
                                        })
                                        .child(tool.label()),
                                )
                                .child(if !active {
                                    // 模式状态标签（对齐 cc-switch AppNavItem modeTag）。
                                    match mode {
                                        ProviderMode::Route => widgets::pill("路由", theme::jade()),
                                        ProviderMode::Stack => widgets::pill("聚合", theme::accent()),
                                        ProviderMode::Direct => div(),
                                    }
                                } else {
                                    div()
                                })
                                .child(if let Some(u) = usage {
                                    // 用量尾注（对齐 cc-switch 今日用量）。
                                    let mut line = String::new();
                                    let unit = if u.unit.is_empty() { String::new() } else { format!(" {}", u.unit) };
                                    let currency = u.currency.as_deref().unwrap_or("");
                                    if let Some(total) = u.total {
                                        line.push_str(&format!("{total}{unit}{currency}"));
                                    } else if let Some(rem) = u.remaining {
                                        line.push_str(&format!("剩 {rem}{unit}{currency}"));
                                    }
                                    if !line.is_empty() {
                                        div()
                                            .text_size(px(9.))
                                            .text_color(theme::text_faint())
                                            .child(line)
                                    } else {
                                        div()
                                    }
                                } else {
                                    div()
                                })
                        })
                    }),
            )
            // 分隔线
            .child(
                div()
                    .h(px(1.))
                    .flex_shrink_0()
                    .mx(px(10.))
                    .my(px(4.))
                    .bg(theme::stroke_a(0.1)),
            )
            // 全局页面项（贴底）
            .child(
                div()
                    .flex_none()
                    .flex_col()
                    .children(GLOBAL_PAGES.into_iter().enumerate().map(|(i, page)| {
                        let active = page == self.page;
                        div()
                            .id(("page-item", i))
                            .h(px(26.))
                            .px(px(8.))
                            .mx(px(4.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(6.))
                            .bg(if active {
                                theme::accent_a(0.12)
                            } else {
                                theme::alpha(0xffffff, 0.0)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::stroke_a(0.07)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_page(page, cx);
                            }))
                            .child(
                                div()
                                    .w(px(18.))
                                    .text_size(px(11.))
                                    .text_color(rgb(if active {
                                        theme::ACCENT
                                    } else {
                                        theme::TEXT_FAINT
                                    }))
                                    .child(page_glyph(page)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(if active {
                                        theme::text()
                                    } else {
                                        theme::text_dim()
                                    })
                                    .child(crate::i18n::t(lang, page_i18n_key(page))),
                            )
                    })),
            )
            // 分隔线
            .child(
                div()
                    .h(px(1.))
                    .flex_shrink_0()
                    .mx(px(10.))
                    .my(px(4.))
                    .bg(theme::stroke_a(0.1)),
            )
            // 底栏：应用管理 + 设置
            .child(
                div()
                    .h(px(38.))
                    .px(px(8.))
                    .pb(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id(("page-item", 99usize))
                            .flex_1()
                            .h(px(28.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(6.))
                            .bg(if self.page == Page::Apps {
                                theme::accent_a(0.12)
                            } else {
                                theme::alpha(0xffffff, 0.0)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::stroke_a(0.07)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.select_page(Page::Apps, cx);
                            }))
                            .child(
                                div()
                                    .w(px(18.))
                                    .text_size(px(11.))
                                    .text_color(rgb(if self.page == Page::Apps {
                                        theme::ACCENT
                                    } else {
                                        theme::TEXT_FAINT
                                    }))
                                    .child("▦"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(if self.page == Page::Apps {
                                        theme::text()
                                    } else {
                                        theme::text_dim()
                                    })
                                    .child(crate::i18n::t(lang, page_i18n_key(Page::Apps))),
                            ),
                    )
                    .child(
                        div()
                            .id(("page-item", 100usize))
                            .flex_1()
                            .h(px(28.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(6.))
                            .bg(if self.page == Page::Settings {
                                theme::accent_a(0.12)
                            } else {
                                theme::alpha(0xffffff, 0.0)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::stroke_a(0.07)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.select_page(Page::Settings, cx);
                            }))
                            .child(
                                div()
                                    .w(px(18.))
                                    .text_size(px(11.))
                                    .text_color(rgb(if self.page == Page::Settings {
                                        theme::ACCENT
                                    } else {
                                        theme::TEXT_FAINT
                                    }))
                                    .child("⚙"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(if self.page == Page::Settings {
                                        theme::text()
                                    } else {
                                        theme::text_dim()
                                    })
                                    .child(crate::i18n::t(lang, page_i18n_key(Page::Settings))),
                            ),
                    ),
            )
    }

    fn render_provider_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.provider_search.read(cx).text().to_lowercase();
        let providers = {
            let store = self.store.read().expect("store lock");
            let providers = store.config().providers_for(self.active_tool);
            providers
                .into_iter()
                .filter(|p| {
                    if query.is_empty() {
                        return true;
                    }
                    p.name.to_lowercase().contains(&query)
                        || p.id.to_lowercase().contains(&query)
                        || p
                            .website_url
                            .as_deref()
                            .is_some_and(|u| u.to_lowercase().contains(&query))
                })
                .map(|p| p.clone())
                .collect::<Vec<_>>()
        };

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            // 页头（cc-switch AppPageHeader：高 52，左图标+标题+副标题，右操作区）
            .child(
                div()
                    .h(px(52.))
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(20.))
                    .border_b_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                // 工具图标（对齐 AppPageHeader icon）
                                div()
                                    .size(px(22.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.))
                                    .text_color(theme::accent())
                                    .child(self.active_tool.glyph()),
                            )
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(self.active_tool.label()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_faint())
                                    .child("供应商"),
                            )
                            .child(widgets::pill(
                                if self.active_tool.is_switch() { "切换" } else { "共存" },
                                theme::jade(),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::text_faint())
                                    .child(format!("{} 个供应商", providers.len())),
                            )
                            .child(
                                widgets::ghost_button("page-export", "导出")
                                    .on_click(cx.listener(Self::export_providers)),
                            )
                            .child(
                                widgets::ghost_button("page-import-current", "从当前配置导入")
                                    .on_click(cx.listener(Self::import_current_provider)),
                            )
                            .child(
                                widgets::ghost_button("page-import", "导入")
                                    .on_click(cx.listener(Self::import_providers)),
                            )
                            .child(
                                widgets::primary_button("page-add", "＋ 添加供应商")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_add_dialog(cx);
                                    })),
                            ),
                    ),
            )
            .child(self.render_provider_mode_tabs(cx))
            .when(!self.switch_conflicts.is_empty() || !self.startup_conflicts.is_empty(), |el| {
                let mut text = String::new();
                if !self.startup_conflicts.is_empty() {
                    text.push_str(&format!("启动扫描（{}）：{}", self.startup_conflicts.len(), self.startup_conflicts.join("；")));
                }
                if !self.switch_conflicts.is_empty() {
                    if !text.is_empty() {
                        text.push_str(" | ");
                    }
                    text.push_str(&format!("切换检测（{}）：{}", self.switch_conflicts.len(), self.switch_conflicts.join("；")));
                }
                el.child(
                    div()
                        .mx(px(16.))
                        .mt(px(10.))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .bg(theme::danger().opacity(0.12))
                        .border_1()
                        .border_color(theme::danger().opacity(0.35))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(10.))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme::danger())
                                .child(text),
                        )
                        .child(
                            widgets::ghost_button("dismiss-conflict", "忽略").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.switch_conflicts.clear();
                                    this.startup_conflicts.clear();
                                    cx.notify();
                                }),
                            ),
                        ),
                )
            })
            .child(
                div()
                    .id("provider-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .on_drop(cx.listener(move |this, drag: &ProviderDrag, _, cx| {
                        if let Some(target) = this.drop_target.clone() {
                            this.move_provider(&drag.id, &target, cx);
                        }
                        this.drag_provider = None;
                        this.drop_target = None;
                        cx.notify();
                    }))
                    .child(
                        // 列表首行：分区标题（左）+ 搜索框（右），对齐 cc-switch searchField
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text_dim())
                                    .child(if providers.is_empty() {
                                        "无供应商".to_string()
                                    } else {
                                        "全部供应商".to_string()
                                    }),
                            )
                            .child(
                                div()
                                    .w(px(200.))
                                    .h(px(30.))
                                    .px(px(10.))
                                    .rounded(px(8.))
                                    .bg(theme::alpha(0x000000, 0.25))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.14))
                                    .flex()
                                    .items_center()
                                    .child(self.provider_search.clone()),
                            ),
                    )
                    .child(if providers.is_empty() {
                        self.render_empty_state(cx).into_any_element()
                    } else {
                        div()
                            .flex_col()
                            .gap(px(10.))
                            .children(
                                providers
                                    .iter()
                                    .enumerate()
                                    .map(|(i, p)| {
                                        let p = p.clone();
                                        let drag_info = ProviderDrag { id: p.id.clone() };
                                        self.render_provider_card(i, p.clone(), cx)
                                            .on_drag(drag_info, |info, position, _, cx| {
                                                let label = info.id.clone();
                                                cx.new(|_| DragGhost {
                                                    label,
                                                    offset: position,
                                                })
                                            })
                                            .on_drag_move(
                                                cx.listener(
                                                    move |this,
                                                          ev: &gpui::DragMoveEvent<ProviderDrag>,
                                                          _,
                                                          cx| {
                                                        let id = ev.drag(cx).id.clone();
                                                        this.drop_target = Some(id);
                                                        cx.notify();
                                                    },
                                                ),
                                            )
                                            .into_any_element()
                                    })
                                    .collect::<Vec<_>>(),
                            )
                            .into_any_element()
                    })
            )
    }

    /// 供应商页模式标签（直连/路由/聚合，对齐 cc-switch ModeTabs）。
    fn render_provider_mode_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = self.active_tool;
        let supports_stack = tool.supports_aggregation();
        let modes: Vec<ProviderMode> = if supports_stack {
            vec![ProviderMode::Direct, ProviderMode::Route, ProviderMode::Stack]
        } else {
            vec![ProviderMode::Direct, ProviderMode::Route]
        };
        // 当前生效模式（从配置读取）。
        let route_active = self
            .store
            .read()
            .map(|s| s.config().settings.routing_for(tool).enabled)
            .unwrap_or(false);
        let stack_active = self
            .store
            .read()
            .map(|s| s.config().aggregation_for(tool).enabled)
            .unwrap_or(false);
        let active_mode = if route_active {
            ProviderMode::Route
        } else if stack_active {
            ProviderMode::Stack
        } else {
            ProviderMode::Direct
        };

        div()
            .px(px(16.))
            .py(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .children(modes.into_iter().enumerate().map(|(i, mode)| {
                        let selected = self.provider_mode == mode;
                        let is_active = active_mode == mode;
                        div()
                            .id(("provider-mode", i))
                            .px(px(14.))
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .rounded(px(8.))
                            .cursor_pointer()
                            .bg(if selected {
                                theme::accent_a(0.18)
                            } else {
                                theme::surface_a(0.4)
                            })
                            .border_1()
                            .border_color(if selected {
                                theme::accent_a(0.5)
                            } else {
                                theme::stroke_a(0.1)
                            })
                            .text_color(if selected {
                                theme::accent()
                            } else {
                                theme::text_dim()
                            })
                            .text_size(px(12.))
                            .child(mode.label())
                            .child(if is_active {
                                widgets::status_dot(theme::jade(), 6.)
                            } else {
                                div()
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.provider_mode = mode;
                                cx.notify();
                            }))
                    })),
            )
    }

    /// 拖拽排序：把 `from` 移到 `to` 之前（列表其余顺延）。
    fn move_provider(&mut self, from: &str, to: &str, cx: &mut Context<Self>) {
        if from == to {
            return;
        }
        let mut ids: Vec<String> = {
            let store = self.store.read().expect("store lock");
            store
                .config()
                .providers_for(self.active_tool)
                .into_iter()
                .map(|p| p.id.clone())
                .collect()
        };
        let Some(from_ix) = ids.iter().position(|id| id == from) else {
            return;
        };
        let id = ids.remove(from_ix);
        let insert_at = ids.iter().position(|id| id == to).unwrap_or(ids.len());
        ids.insert(insert_at, id);
        let result = {
            let mut store = self.store.write().expect("store lock");
            store.reorder_providers(self.active_tool, &ids)
        };
        self.status = match result {
            Ok(()) => format!("已调整顺序（{} 个供应商）", ids.len()),
            Err(e) => format!("排序失败: {e}"),
        };
        cx.notify();
    }

    /// 导出当前工具供应商到 JSON 文件（通过系统保存对话框）。
    fn export_providers(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let tool = self.active_tool;
        let json = match self.store.read().expect("store lock").export_providers(tool) {
            Ok(s) => s,
            Err(e) => {
                self.status = format!("导出失败: {e}");
                cx.notify();
                return;
            }
        };
        let dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
        let suggested = format!("ns-switch-{}-providers.json", tool.id_str());
        let receiver = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(p))) => Some(p),
                Ok(Ok(None)) => None,
                Ok(Err(e)) => {
                    this.update(cx, |this, cx| {
                        this.status = format!("导出失败: {e}");
                        cx.notify();
                    })
                    .ok();
                    None
                }
                Err(_) => None,
            };
            if let Some(path) = path {
                let res = std::fs::write(&path, &json);
                this.update(cx, |this, cx| {
                    this.status = match res {
                        Ok(()) => format!("已导出 {} 个供应商 → {}", tool.id_str(), path.display()),
                        Err(e) => format!("导出失败: {e}"),
                    };
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// 从 JSON 文件导入供应商（通过系统打开对话框）。
    fn import_providers(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let tool = self.active_tool;
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择供应商 JSON".into()),
        });
        cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                Ok(Ok(None)) => None,
                Ok(Err(e)) => {
                    this.update(cx, |this, cx| {
                        this.status = format!("导入失败: {e}");
                        cx.notify();
                    })
                    .ok();
                    None
                }
                Err(_) => None,
            };
            if let Some(path) = path {
                let json = std::fs::read_to_string(&path);
                this.update(cx, |this, _cx| {
                    let result = match json {
                        Ok(text) => this
                            .store
                            .write()
                            .expect("store lock")
                            .import_providers(tool, &text),
                        Err(e) => Err(anyhow::anyhow!("读取失败: {e}")),
                    };
                    let is_ok = result.is_ok();
                    this.status = match result {
                        Ok(n) => format!("已导入 {n} 个供应商"),
                        Err(e) => format!("导入失败: {e}"),
                    };
                    this.status_tone = if is_ok { StatusTone::Success } else { StatusTone::Error };
                })
                .ok();
            }
        })
        .detach();
    }

    /// 从当前客户端 live 配置导入为供应商（Claude/Desktop/Codex/Gemini/Grok）。
    fn import_current_provider(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let tool = self.active_tool;
        let cdirs = self.config_dirs();
        let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
            self.status = "无法定位 HOME".into();
            self.status_tone = StatusTone::Error;
            cx.notify();
            return;
        };
        let imported = match crate::switch_engine::import_current_config(tool, &home, &cdirs) {
            Ok(v) => v,
            Err(e) => {
                self.status = format!("导入当前配置失败: {e}");
                self.status_tone = StatusTone::Error;
                cx.notify();
                return;
            }
        };
        let api_format = match tool {
            Tool::Codex => Some("openai_chat".to_string()),
            Tool::Gemini => Some("gemini_native".to_string()),
            _ => None,
        };
        let mut provider = ns_presets::Provider::with_id(
            format!("{}-import-{}", tool.id_str(), now_ts_suffix()),
            format!("{} 当前配置", tool.label()),
            imported.settings_config(tool),
        );
        provider.category = Some(ns_presets::ProviderCategory::Custom);
        provider.meta = Some(ns_presets::ProviderMeta {
            api_format,
            ..Default::default()
        });
        provider.icon = Some("api".to_string());
        provider.icon_color = Some("#c9a25f".to_string());
        let saved = self
            .store
            .write()
            .expect("store lock")
            .add_provider(provider);
        match saved {
            Ok(()) => {
                self.status = format!(
                    "已导入 {} 当前配置（端点 {}，模型 {}）",
                    tool.label(),
                    imported.endpoint,
                    if imported.model.is_empty() {
                        "默认"
                    } else {
                        imported.model.as_str()
                    },
                );
                self.status_tone = StatusTone::Success;
            }
            Err(e) => {
                self.status = format!("导入当前配置失败: {e}");
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    /// 一键切换到目标供应商：写 live 配置 + 更新 current。
    fn switch_to_provider(&mut self, id: &str, cx: &mut Context<Self>) {
        let tool = self.active_tool;
        self.switch_to_provider_for(tool, id, cx);
    }

    /// 指定工具切换到目标供应商（托盘菜单复用）。
    fn switch_to_provider_for(&mut self, tool: Tool, id: &str, cx: &mut Context<Self>) {
        let (provider, current_provider) = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            let provider = cfg.providers.get(id).cloned();
            let current_provider = cfg
                .current
                .get(tool)
                .and_then(|cur| cfg.providers.get(cur).cloned());
            (provider, current_provider)
        };
        let Some(provider) = provider else {
            self.status = "供应商不存在".into();
            cx.notify();
            return;
        };
        self.write_provider(tool, &provider, current_provider.as_ref(), cx);
    }

    /// 写回指定供应商到客户端 live 配置并更新 current。
    fn write_provider(
        &mut self,
        tool: Tool,
        provider: &ns_presets::Provider,
        current_provider: Option<&ns_presets::Provider>,
        cx: &mut Context<Self>,
    ) {
        // 写入前冲突检测：目标 live 文件被手动改动/他方占用 → 暂停并请求确认。
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let conflicts = home
            .as_deref()
            .map(|h| {
                crate::switch_engine::check_conflicts_dirs(
                    tool,
                    provider,
                    current_provider,
                    h,
                    &self.config_dirs(),
                )
            })
            .unwrap_or_default();
        self.switch_conflicts = conflicts
            .iter()
            .map(|c| format!("{}：{}", c.path.display(), c.reason))
            .collect();
        if !conflicts.is_empty() {
            // 存在冲突：暂存切换请求，等待用户确认（确认后强制执行）。
            let note = format!(
                "{} 处配置被占用/修改（{}：{}）",
                conflicts.len(),
                conflicts[0].path.display(),
                conflicts[0].reason
            );
            self.pending_switch = Some((tool, provider.clone(), note));
            self.status = format!("检测到写入冲突（{}），请确认是否继续", conflicts.len());
            self.status_tone = StatusTone::Error;
            cx.notify();
            return;
        }
        self.apply_switch(tool, provider, cx);
    }

    /// 实际执行切换（写入 live 配置 + 更新 current）。冲突确认后也走此路径。
    fn apply_switch(
        &mut self,
        tool: Tool,
        provider: &ns_presets::Provider,
        cx: &mut Context<Self>,
    ) {
        let backup_dir = self.store.read().expect("store lock").backups_dir();
        let cdirs = self.config_dirs();
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let home = home.as_deref().unwrap_or_else(|| std::path::Path::new("/"));
        match crate::switch_engine::switch_to_in_dirs(tool, provider, &backup_dir, home, &cdirs) {
            Ok(outcome) => {
                let result = {
                    let mut store = self.store.write().expect("store lock");
                    store.set_current(tool, provider.id.clone())
                };
                self.status = match result {
                    Ok(()) => {
                        let restart_hint = if tool.needs_restart() {
                            format!("；请重启 {} 终端/会话以生效", tool.label())
                        } else {
                            "；已热切换（下次命令生效）".to_string()
                        };
                        if outcome.has_no_live_files() {
                            format!(
                                "已选择 {}（{} 无独立客户端配置文件，仅记录当前选择）",
                                provider.name,
                                tool.label()
                            )
                        } else {
                            format!(
                                "已切换到 {}（写入 {} 个文件{}）{}",
                                provider.name,
                                outcome.written,
                                if outcome.noop { "，无内容变化" } else { "" },
                                restart_hint,
                            )
                        }
                    }
                    Err(e) => format!("切换失败: {e}"),
                };
                self.status_tone = StatusTone::Success;
            }
            Err(e) => {
                self.status = format!("切换失败: {e}");
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    /// 聚合写回客户端：写默认供应商配置；Codex 额外合并模型目录（model_catalog_json）。
    fn agg_apply_live(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let (default_provider, current_provider) = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            let agg = cfg.aggregation_for(tool);
            if !agg.enabled {
                return;
            }
            let Some(default_id) = agg.default_id.clone() else {
                return;
            };
            let provider = cfg.providers.get(&default_id).cloned();
            let current_provider = cfg
                .current
                .get(tool)
                .and_then(|cur| cfg.providers.get(cur).cloned());
            (provider, current_provider)
        };
        let Some(provider) = default_provider else {
            return;
        };
        // Codex 聚合：生成合并模型目录并注入 model_catalog_json 路径。
        let provider = if tool == Tool::Codex {
            match self.write_codex_model_catalog() {
                Some(catalog_path) => {
                    let mut p = provider.clone();
                    let config = p
                        .settings_config
                        .as_object_mut()
                        .and_then(|obj| obj.get_mut("config"))
                        .and_then(serde_json::Value::as_object_mut);
                    if let Some(config) = config {
                        config.insert(
                            "model_catalog_json".into(),
                            serde_json::Value::String(catalog_path),
                        );
                    }
                    p
                }
                None => provider,
            }
        } else {
            provider
        };
        self.write_provider(tool, &provider, current_provider.as_ref(), cx);
        if tool == Tool::Codex {
            self.status = format!(
                "{}；Codex 聚合已写入：请完全重启 Codex（所有终端/CLI/桌面应用共用后台 daemon，需重启后新模型列表才生效）",
                self.status
            );
            cx.notify();
        }
    }

    /// 写 Codex 聚合模型目录（合并所有聚合供应商可见模型），返回目录文件路径。
    fn write_codex_model_catalog(&mut self) -> Option<String> {
        let providers = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            let agg = cfg.aggregation_for(Tool::Codex);
            let mut v = Vec::new();
            for id in &agg.provider_ids {
                if let Some(p) = cfg.providers.get(id) {
                    v.push(p.clone());
                }
            }
            v
        };
        if providers.is_empty() {
            return None;
        }
        let catalog = ns_presets::aggregation_model_catalog(&providers.iter().collect::<Vec<_>>());
        let path = self
            .store
            .read()
            .expect("store lock")
            .dir()
            .join("codex-aggregate-models.json");
        let pretty = serde_json::to_string_pretty(&catalog).ok()?;
        std::fs::write(&path, pretty).ok()?;
        Some(path.to_string_lossy().into_owned())
    }

    /// 查询某工具当前供应商的用量（异步执行脚本）。
    fn query_usage(&mut self, tool: Tool, cx: &mut Context<Self>) {

        // Gemini 官方订阅：优先走 Cloud Code RPC 配额查询（需 OAuth token）。
        if tool == Tool::Gemini {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let token = home
                .as_deref()
                .and_then(ns_usage::read_gemini_access_token);
            if let Some(token) = token {
                self.usage_loading
                    .insert(tool.id_str().to_string(), true);
                cx.notify();
                let tool_id = tool.id_str().to_string();
                cx.spawn(async move |this, cx| {
                    let result = ns_usage::query_gemini_quota(&token);
                    this.update(cx, |this, cx| {
                        match result {
                            Ok(tiers) => {
                                this.usage.insert(
                                    tool_id.clone(),
                                    ns_usage::gemini_quota_to_usage(&tiers),
                                );
                            }
                            Err(e) => {
                                this.usage.insert(
                                    tool_id.clone(),
                                    ns_usage::Usage {
                                        source: format!("Gemini 配额查询失败: {e}"),
                                        ..Default::default()
                                    },
                                );
                            }
                        }
                        this.usage_loading.remove(&tool_id);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
                return;
            }
        }
        let (script, usage_enabled) = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            let p = cfg.current.get(tool).and_then(|id| cfg.providers.get(id));
            let script = p
                .and_then(|p| p.meta.as_ref())
                .and_then(|m| m.usage_script.clone());
            let enabled = p
                .and_then(|p| p.meta.as_ref())
                .map(|m| m.usage_enabled)
                .unwrap_or(true);
            (script, enabled)
        };
        if !usage_enabled {
            self.usage.insert(
                tool.id_str().to_string(),
                ns_usage::Usage {
                    source: "用量查询已停用".into(),
                    ..Default::default()
                },
            );
            cx.notify();
            return;
        }
        let Some(script) = script else {
            self.usage.insert(
                tool.id_str().to_string(),
                ns_usage::Usage {
                    source: "未配置用量脚本".into(),
                    ..Default::default()
                },
            );
            cx.notify();
            return;
        };
        self.usage_loading
            .insert(tool.id_str().to_string(), true);
        cx.notify();
        let tool_id = tool.id_str().to_string();
        cx.spawn(async move |this, cx| {
            let result = {
                let path = std::path::PathBuf::from(&script);
                match ns_usage::detect_runtime() {
                    Some(rt) => ns_usage::execute_script(&path, rt),
                    None => Err(anyhow::anyhow!(
                        "未找到脚本运行时（需要 node/deno/bun 之一）"
                    )),
                }
            };
            this.update(cx, |this, cx| {
                match result {
                    Ok(u) => {
                        this.usage.insert(tool_id.clone(), u);
                    }
                    Err(e) => {
                        this.usage.insert(
                            tool_id.clone(),
                            ns_usage::Usage {
                                source: format!("查询失败: {e}"),
                                ..Default::default()
                            },
                        );
                    }
                }
                this.usage_loading.remove(&tool_id);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 从定价表单添加/覆盖自定义模型定价。
    fn add_custom_price(&mut self, cx: &mut Context<Self>) {
        let model = self.price_model.read(cx).text().trim().to_string();
        if model.is_empty() {
            self.pricing_status = "请输入模型前缀".into();
            cx.notify();
            return;
        }
        let parse = |f: &gpui::Entity<crate::text_field::TextField>| -> Option<f64> {
            let t = f.read(cx).text().trim().to_string();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok()
            }
        };
        let Some(input) = parse(&self.price_input) else {
            self.pricing_status = "请输入有效输入价".into();
            cx.notify();
            return;
        };
        let Some(output) = parse(&self.price_output) else {
            self.pricing_status = "请输入有效输出价".into();
            cx.notify();
            return;
        };
        let cache = parse(&self.price_cache);
        let ok = {
            let mut s = self.store.write().expect("store lock");
            s.set_custom_price(&model, input, output, cache).is_ok()
        };
        self.pricing_status = if ok {
            format!("已设置 {model} 定价")
        } else {
            "保存定价失败".into()
        };
        // 清空表单。
        self.price_input.update(cx, |f, _| f.set_text(""));
        self.price_output.update(cx, |f, _| f.set_text(""));
        self.price_cache.update(cx, |f, _| f.set_text(""));
        cx.notify();
    }

    /// 从 models.dev API 拉取定价并批量写入自定义定价表。
    fn import_models_dev_pricing(&mut self, cx: &mut Context<Self>) {
        self.pricing_status = "正在从 models.dev 拉取定价…".into();
        cx.notify();
        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let result = ns_usage::import_models_dev();
            this.update(cx, |this, cx| {
                match result {
                    Ok(map) => {
                        let mut store_w = store.write().expect("store lock");
                        let mut count = 0usize;
                        for (model, price) in &map {
                            let _ = store_w.set_custom_price(
                                model,
                                price.input_per_mtok,
                                price.output_per_mtok,
                                price.cache_read_per_mtok,
                            );
                            count += 1;
                        }
                        drop(store_w);
                        this.pricing_status =
                            format!("已从 models.dev 导入 {count} 条模型定价");
                    }
                    Err(e) => {
                        this.pricing_status = format!("导入失败: {e}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 扫描 Claude Code + Codex 会话日志，统计按模型请求/token。
    fn refresh_session_stats(&mut self, cx: &mut Context<Self>) {
        self.session_scanning = true;
        cx.notify();
        // 自定义定价表（settings.custom_pricing 的 JSON → CustomPrice）。
        let custom: std::collections::BTreeMap<String, ns_usage::CustomPrice> = {
            let g = self.store.read().ok();
            g.map(|g| {
                let c = g.config().settings.custom_pricing.clone();
                c.iter()
                    .filter_map(|(k, v)| {
                        let input = v.get("input_per_mtok").and_then(|x| x.as_f64())?;
                        let output = v.get("output_per_mtok").and_then(|x| x.as_f64())?;
                        let cache = v.get("cache_read_per_mtok").and_then(|x| x.as_f64());
                        Some((k.clone(), ns_usage::CustomPrice::new(input, output, cache)))
                    })
                    .collect()
            })
            .unwrap_or_default()
        };
        let this_usage_range = self.usage_range_days;
        cx.spawn(async move |this, cx| {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);

            let (stats, daily, details, by_tool) = match home.as_deref() {
                Some(h) => {
                    let mut s = ns_usage::scan_claude_sessions(h, 0);
                    // Codex 统计合并进同一列表（按模型聚合）。
                    for cs in ns_usage::scan_codex_sessions(h, 0) {
                        if let Some(existing) = s.iter_mut().find(|e| e.model == cs.model) {
                            existing.requests += cs.requests;
                            existing.input_tokens += cs.input_tokens;
                            existing.output_tokens += cs.output_tokens;
                            existing.cache_read_tokens += cs.cache_read_tokens;
                            // 花费按合并后 token 重算。
                            existing.cost_usd = ns_usage::cost_with_custom(
                                &existing.model,
                                existing.input_tokens,
                                existing.output_tokens,
                                existing.cache_read_tokens,
                                &custom,
                            );
                        } else {
                            s.push(cs);
                        }
                    }
                    // Gemini 统计合并（Claude/Codex 列表同模型聚合）。
                    for gs in ns_usage::scan_gemini_sessions(h, 0) {
                        if let Some(existing) = s.iter_mut().find(|e| e.model == gs.model) {
                            existing.requests += gs.requests;
                            existing.input_tokens += gs.input_tokens;
                            existing.output_tokens += gs.output_tokens;
                            existing.cache_read_tokens += gs.cache_read_tokens;
                            existing.cost_usd = ns_usage::cost_with_custom(
                                &existing.model,
                                existing.input_tokens,
                                existing.output_tokens,
                                existing.cache_read_tokens,
                                &custom,
                            );
                        } else {
                            s.push(gs);
                        }
                    }
                    s.sort_by(|a, b| b.requests.cmp(&a.requests));
                    let range = this_usage_range;
                    let claude_daily = ns_usage::scan_claude_daily(h, range);
                    let codex_daily = ns_usage::scan_codex_daily(h, range);
                    let gemini_daily = ns_usage::scan_gemini_daily(h, range);
                    let by_tool = {
                        let mut m = std::collections::BTreeMap::new();
                        m.insert("claude".to_string(), claude_daily.clone());
                        m.insert("codex".to_string(), codex_daily.clone());
                        m.insert("gemini".to_string(), gemini_daily.clone());
                        m
                    };
                    (
                        s,
                        merge_daily(
                            merge_daily(claude_daily, codex_daily),
                            gemini_daily,
                        ),
                        merge_details(
                            merge_details(
                                ns_usage::scan_claude_details(h),
                                ns_usage::scan_codex_details(h),
                            ),
                            ns_usage::scan_gemini_details(h),
                        ),
                        by_tool,
                    )
                }
                None => (Vec::new(), Vec::new(), Vec::new(), Default::default()),
            };
            this.update(cx, |this, cx| {
                this.session_stats = stats;
                this.usage_daily = daily;
                this.usage_details = details;
                this.usage_daily_by_tool = by_tool;
                // 首次扫描时默认全选。
                if this.usage_chart_tools.is_empty() {
                    for k in ["claude", "codex", "gemini"] {
                        this.usage_chart_tools.insert(k.to_string(), true);
                    }
                }
                this.session_scanning = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 渲染 M9 用量页：每工具当前供应商的用量卡片 + 查询。
    fn render_usage_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tools = [Tool::Claude, Tool::Codex, Tool::Gemini, Tool::GrokBuild];
        let (usage, loading) = (self.usage.clone(), self.usage_loading.clone());
        let rows: Vec<(Tool, Option<ns_presets::Provider>)> = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            tools
                .into_iter()
                .map(|t| {
                    let p = cfg
                        .current
                        .get(t)
                        .and_then(|id| cfg.providers.get(id).cloned());
                    (t, p)
                })
                .collect()
        };
        div()
            .id("usage-scroll")
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("用量统计"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_faint())
                                    .child("配额 / 余额 / 重置倒计时"),
                            ),
                    )
                    .child(
                        widgets::ghost_button(("usage-refresh-all", 0usize), "全部刷新").on_click(
                            cx.listener(move |this, _, _, cx| {
                                for t in [Tool::Claude, Tool::Codex, Tool::Gemini, Tool::GrokBuild] {
                                    this.query_usage(t, cx);
                                }
                            }),
                        ),
                    ),
            )
            .children(rows.into_iter().enumerate().map(|(i, (tool, provider))| {
                let key = tool.id_str().to_string();
                let u = usage.get(&key).cloned();
                let is_loading = loading.get(&key).copied().unwrap_or(false);
                let provider_name = provider
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "未选择供应商".into());
                div()
                    .id(("usage-card", i))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(14.))
                    .rounded(px(10.))
                    .bg(theme::panel_a(0.6))
                    .border_1()
                    .border_color(theme::stroke_a(0.12))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(format!("{} · {}", tool.label(), provider_name)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        widgets::ghost_button(("usage-config", i), "配置脚本")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                let pid = this
                                                    .store
                                                    .read()
                                                    .expect("store lock")
                                                    .config()
                                                    .current
                                                    .get(tool)
                                                    .cloned();
                                                if let Some(p) = pid {
                                                    this.configure_usage(&p, cx);
                                                }
                                            })),
                                    )
                                    .child(
                                        widgets::ghost_button(("usage-query", i), "查询").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.query_usage(tool, cx);
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(match (u, is_loading) {
                        (_, true) => div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child("查询中…"),
                        (Some(u), false) => {
                            let mut line = String::new();
                            let unit = if u.unit.is_empty() {
                                String::new()
                            } else {
                                format!(" {}", u.unit)
                            };
                            let currency = u.currency.as_deref().unwrap_or("");
                            if let Some(total) = u.total {
                                line.push_str(&format!("总量 {total}{unit}{currency}"));
                                if let Some(used) = u.used {
                                    line.push_str(&format!(" · 已用 {used}{unit}{currency}"));
                                }
                                if let Some(remaining) = u.remaining {
                                    line.push_str(&format!(
                                        " · 剩余 {remaining}{unit}{currency}"
                                    ));
                                }
                            } else if let Some(remaining) = u.remaining {
                                line.push_str(&format!("剩余 {remaining}{unit}{currency}"));
                            } else {
                                line.push_str("无用量数据");
                            }
                            if let Some(reset_at) = u.reset_at {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs() as i64)
                                    .unwrap_or(0);
                                if reset_at > now {
                                    let secs = reset_at - now;
                                    let h = secs / 3600;
                                    let m = (secs % 3600) / 60;
                                    line.push_str(&format!(" · 重置倒计时 {h}h{m:02}m"));
                                } else {
                                    line.push_str(" · 已到重置点");
                                }
                            }
                            let mut col = div()
                                .text_size(px(12.))
                                .text_color(theme::text_dim())
                                .child(line)
                                .child(if u.source.is_empty() {
                                    div()
                                } else {
                                    div()
                                        .text_size(px(11.))
                                        .text_color(theme::text_faint())
                                        .child(format!("来源: {}", u.source))
                                });
                            // 配额进度条（总量+已用 → 显示剩余比例）。
                            if let (Some(total), Some(used)) = (u.total, u.used) {
                                if total > 0.0 {
                                    let pct = (used / total).clamp(0.0, 1.0);
                                    let color = if pct >= 0.9 {
                                        theme::danger()
                                    } else if pct >= 0.7 {
                                        theme::cinnabar()
                                    } else {
                                        theme::jade()
                                    };
                                    col = col.child(
                                        div()
                                            .w(px(240.))
                                            .mt(px(6.))
                                            .h(px(6.))
                                            .rounded_full()
                                            .bg(theme::stroke_a(0.12))
                                            .child(
                                                div()
                                                    .h_full()
                                                    .w(px((240. * pct) as f32))
                                                    .rounded_full()
                                                    .bg(color),
                                            ),
                                    );
                                }
                            }
                            col
                        }
                        (None, false) => div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child("点击“查询”获取用量"),
                    })
            }))
            .child(
                div()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(14.))
                    .rounded(px(10.))
                    .bg(theme::panel_a(0.6))
                    .border_1()
                    .border_color(theme::stroke_a(0.12))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("会话日志统计"),
                    )
                    .child(if self.session_scanning {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child("扫描中…")
                    } else if self.session_stats.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child("无会话日志（Claude/Codex/Gemini）")
                    } else {
                        let total_requests: u64 =
                            self.session_stats.iter().map(|s| s.requests).sum();
                        let total_input: u64 =
                            self.session_stats.iter().map(|s| s.input_tokens).sum();
                        let total_output: u64 =
                            self.session_stats.iter().map(|s| s.output_tokens).sum();
                        let total_cache: u64 =
                            self.session_stats.iter().map(|s| s.cache_read_tokens).sum();
                        let total_cost: f64 = self
                            .session_stats
                            .iter()
                            .filter_map(|s| s.cost_usd)
                            .sum();
                        let any_cost = self.session_stats.iter().any(|s| s.cost_usd.is_some());
                        let model_count = self.session_stats.len();
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(10.))
                                    .text_size(px(10.5))
                                    .text_color(theme::text_faint())
                                    .child(
                                        div()
                                            .flex_1()
                                            .child("模型"),
                                    )
                                    .child(div().w(px(56.)).child("请求"))
                                    .child(div().w(px(72.)).child("输入"))
                                    .child(div().w(px(72.)).child("输出"))
                                    .child(div().w(px(72.)).child("缓存读"))
                                    .child(div().w(px(56.)).child("命中率"))
                                    .child(div().w(px(72.)).child("花费")),
                            )
                            .children(self.session_stats.iter().take(12).map(|s| {
                                div()
                                    .flex()
                                    .gap(px(10.))
                                    .text_size(px(11.))
                                    .text_color(theme::text_dim())
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_color(theme::text())
                                            .child(s.model.clone()),
                                    )
                                    .child(
                                        div()
                                            .w(px(56.))
                                            .child(s.requests.to_string()),
                                    )
                                    .child(
                                        div()
                                            .w(px(72.))
                                            .child(format_tokens(s.input_tokens)),
                                    )
                                    .child(
                                        div()
                                            .w(px(72.))
                                            .child(format_tokens(s.output_tokens)),
                                    )
                                    .child(
                                        div()
                                            .w(px(72.))
                                            .child(format_tokens(s.cache_read_tokens)),
                                    )
                                    .child(div().w(px(56.)).child(format!(
                                        "{:.0}%",
                                        cache_hit_rate(s.cache_read_tokens, s.input_tokens)
                                    )))
                                    .child(
                                        div()
                                            .w(px(72.))
                                            .child(
                                                s.cost_usd
                                                    .map(|c| format!("${:.4}", c))
                                                    .unwrap_or_else(|| "—".into()),
                                            ),
                                    )
                            }))
                            .child(
                                div()
                                    .mt(px(4.))
                                    .pt(px(6.))
                                    .border_t_1()
                                    .border_color(theme::stroke_a(0.12))
                                    .flex()
                                    .gap(px(10.))
                                    .text_size(px(11.))
                                    .text_color(theme::text())
                                    .child(
                                        div()
                                            .flex_1()
                                            .child(format!("合计（{model_count} 个模型）")),
                                    )
                                    .child(
                                        div()
                                            .w(px(56.))
                                            .child(total_requests.to_string()),
                                    )
                                    .child(
                                        div()
                                            .w(px(72.))
                                            .child(format_tokens(total_input)),
                                    )
                                    .child(
                                        div()
                                            .w(px(72.))
                                            .child(format_tokens(total_output)),
                                    )
                                    .child(
                                        div()
                                            .w(px(72.))
                                            .child(format_tokens(total_cache)),
                                    )
                                    .child(div().w(px(56.)).child(format!(
                                        "{:.0}%",
                                        cache_hit_rate(total_cache, total_input)
                                    )))
                                    .child(
                                        div().w(px(72.)).child(if any_cost {
                                            format!("${total_cost:.4}")
                                        } else {
                                            "—".to_string()
                                        }),
                                    ),
                            )
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("stats-refresh", "刷新统计").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.refresh_session_stats(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .child(self.render_usage_charts(cx))
        }

    /// 渲染 M9 趋势图 + 热力图 + 请求明细（按工具筛选）。
    fn render_usage_charts(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let by_tool = self.usage_daily_by_tool.clone();
        let tools = self.usage_chart_tools.clone();
        // 按筛选聚合每日数据；未初始化时全选。
        let mut merged: std::collections::BTreeMap<String, ns_usage::DailyCount> =
            std::collections::BTreeMap::new();
        for (tool_key, vec) in &by_tool {
            let selected = tools.get(tool_key).copied().unwrap_or(true);
            if !selected {
                continue;
            }
            for d in vec {
                let e = merged.entry(d.date.clone()).or_insert_with(|| {
                    ns_usage::DailyCount {
                        date: d.date.clone(),
                        ..Default::default()
                    }
                });
                e.requests += d.requests;
                e.input_tokens += d.input_tokens;
                e.output_tokens += d.output_tokens;
            }
        }
        let daily: Vec<ns_usage::DailyCount> = merged.into_values().collect();
        let details = self.usage_details.clone();
        let max_requests = daily
            .iter()
            .map(|d| d.requests)
            .max()
            .unwrap_or(1)
            .max(1);
        div()
            .flex_col()
            .gap(px(10.))
            .child(
                // 按工具筛选（对齐 cc-switch 用量面板 app filter）。
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(widgets::section_label("图表包含"))
                    .children(
                        [
                            ("claude", "Claude"),
                            ("codex", "Codex"),
                            ("gemini", "Gemini"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (key, label))| {
                            let selected = tools.get(key).copied().unwrap_or(true);
                            widgets::ghost_button(
                                gpui::ElementId::from(("usage-filter", i)),
                                label,
                            )
                            .when(selected, |b| {
                                b.bg(theme::accent_a(0.18))
                                    .border_1()
                                    .border_color(theme::accent_a(0.5))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let k = key.to_string();
                                let cur = this
                                    .usage_chart_tools
                                    .get(&k)
                                    .copied()
                                    .unwrap_or(true);
                                this.usage_chart_tools.insert(k, !cur);
                                cx.notify();
                            }))
                        }),
                    ),
            )
            .child(
                // 日期范围预设（对齐 cc-switch UsageDateRangePicker）。
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(widgets::section_label("日期范围"))
                    .children(
                        [(7usize, "7 天"), (14, "14 天"), (30, "30 天"), (0, "全部")]
                            .into_iter()
                            .enumerate()
                            .map(|(i, (days, label))| {
                                let selected = self.usage_range_days == days;
                                widgets::ghost_button(
                                    gpui::ElementId::from(("usage-range", i)),
                                    label,
                                )
                                .when(selected, |b| {
                                    b.bg(theme::accent_a(0.18))
                                        .border_1()
                                        .border_color(theme::accent_a(0.5))
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.usage_range_days != days {
                                        this.usage_range_days = days;
                                        this.refresh_session_stats(cx);
                                    }
                                }))
                            }),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(if self.usage_range_days == 0 {
                                "全部请求趋势".to_string()
                            } else {
                                format!("近 {} 日请求趋势", self.usage_range_days)
                            }),
                    )
                    .child(if daily.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child("扫描后显示趋势")
                    } else {
                        div()
                            .flex()
                            .items_end()
                            .gap(px(2.))
                            .h(px(64.))
                            .children(daily.iter().enumerate().map(|(i, d)| {
                                let h =
                                    ((d.requests as f64 / max_requests as f64) * 56.0).max(2.0) as f32;
                                div()
                                    .id(("usage-trend-bar", i))
                                    .flex_1()
                                    .h(px(h))
                                    .rounded(px(2.))
                                    .bg(theme::accent_a(0.55))
                                    .child(
                                        div()
                                            .text_size(px(9.))
                                            .text_color(theme::text_faint())
                                            .child(d.date.clone()),
                                    )
                            }))
                    }),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(if self.usage_range_days == 0 {
                                "全部热力图".to_string()
                            } else {
                                format!("近 {} 日热力图", self.usage_range_days)
                            }),
                    )
                    .child(if daily.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child("扫描后显示热力图")
                    } else {
                        let max = daily.iter().map(|d| d.requests).max().unwrap_or(1).max(1);
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(3.))
                            .children(daily.iter().enumerate().map(|(i, d)| {
                                let alpha = if d.requests == 0 {
                                    0.04
                                } else {
                                    0.15 + 0.7 * (d.requests as f64 / max as f64)
                                } as f32;
                                div()
                                    .id(("usage-heat-cell", i))
                                    .w(px(16.))
                                    .h(px(16.))
                                    .rounded(px(3.))
                                    .bg(theme::accent_a(alpha))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.08))
                            }))
                    }),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("请求明细"),
                    )
                    .child(if details.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child("扫描后显示明细")
                    } else {
                        div()
                            .flex_col()
                            .children(details.iter().take(20).enumerate().map(|(i, d)| {
                                div()
                                    .id(("usage-detail-row", i))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .text_size(px(12.))
                                    .text_color(theme::text_dim())
                                    .child(
                                        div()
                                            .w(px(56.))
                                            .text_color(theme::text_faint())
                                            .child(match d.tool.as_str() {
                                                "claude" => "Claude",
                                                "codex" => "Codex",
                                                "gemini" => "Gemini",
                                                _ => "-",
                                            }),
                                    )
                                    .child(
                                        div()
                                            .font_family(theme::MONO_FONT)
                                            .text_color(theme::accent())
                                            .child(d.model.clone()),
                                    )
                                    .child(format!(
                                        "输入 {} · 输出 {}",
                                        d.input_tokens, d.output_tokens
                                    ))
                            }))
                    }),
            )
    }


    /// 加载会话列表（异步扫描，按当前 session_tool）。
    fn load_sessions(&mut self, cx: &mut Context<Self>) {
        self.sessions_loading = true;
        cx.notify();
        let tool = self.session_tool;
        cx.spawn(async move |this, cx| {
            let sessions = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .as_deref()
                .map(|h| ns_sessions::list_sessions(h, tool))
                .unwrap_or_default();
            this.update(cx, |this, cx| {
                this.sessions = sessions;
                this.sessions_loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 打开会话阅读视图（异步解析）。
    fn open_session_view(&mut self, path: String, cx: &mut Context<Self>) {
        let session_tool = self.session_tool;
        cx.spawn(async move |this, cx| {
            let inferred_tool = if path.contains("/.codex/") {
                "codex"
            } else if path.contains("/.gemini/") {
                "gemini"
            } else if path.starts_with("opencode:") {
                "opencode"
            } else if path.contains("/.pi/") {
                "pi"
            } else if path.contains("/.hermes/") {
                "hermes"
            } else if path.starts_with("openclaw:") {
                "openclaw"
            } else {
                "claude"
            };
            // OpenCode 会话 path 形如 `opencode:<session_id>`。
            let session_id = path
                .strip_prefix("opencode:")
                .or_else(|| path.strip_prefix("openclaw:"))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    std::path::Path::new(&path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("session")
                        .to_string()
                });
            let meta = ns_sessions::SessionMeta {
                id: session_id.clone(),
                project: String::new(),
                path: path.clone(),
                mtime: 0,
                message_count: 0,
                last_model: None,
                title: String::new(),
                tool: inferred_tool.into(),
            };
            let msgs = if session_tool == ns_sessions::SessionTool::OpenCode {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
                home.as_deref()
                    .map(|h| ns_sessions::parse_opencode_session(h, &session_id))
                    .unwrap_or_default()
            } else if inferred_tool == "pi" {
                ns_sessions::parse_pi_session(std::path::Path::new(&path))
            } else if inferred_tool == "hermes" {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
                home.as_deref()
                    .map(|h| ns_sessions::parse_hermes_session(h, &session_id))
                    .unwrap_or_default()
            } else if inferred_tool == "openclaw" {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
                home.as_deref()
                    .map(|h| ns_sessions::parse_openclaw_session(h, &session_id))
                    .unwrap_or_default()
            } else {
                ns_sessions::parse_session(std::path::Path::new(&path))
            };
            let rounds = ns_sessions::build_rounds(&msgs);
            let count = rounds.len();
            this.update(cx, |this, cx| {
                this.session_viewing = Some(path);
                this.session_meta = Some(meta);
                this.session_rounds = rounds;
                this.session_collapsed = vec![true; count];
                cx.notify();
            })
            .ok();
        })
        .detach();
    }


    /// 渲染会话阅读视图：分节展示、每轮摘要折叠、大纲、导出、恢复命令。
    fn render_session_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(meta) = &self.session_meta else {
            return div().id("session-view").child("加载中…");
        };
        let filter = self.session_filter.read(cx).text();
        let rounds: Vec<&ns_sessions::Round> = if filter.is_empty() {
            self.session_rounds.iter().collect()
        } else {
            self.session_rounds
                .iter()
                .filter(|r| {
                    r.user_text.contains(&filter)
                        || r.assistant.iter().any(|m| m.text.contains(&filter))
                })
                .collect()
        };
        let collapsed = &self.session_collapsed;
        let session_id = meta.id.clone();
        let sid_recover = session_id.clone();
        let sid_terminal = session_id.clone();
        div()
            .id("session-view")
            .flex_1()
            .h_full()
            .flex()
            .gap(px(0.))
            .child(
                // 左侧：会话正文（分节折叠）。
                div()
                    .id("session-view-body")
                    .flex_1()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(20.))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .text_size(px(16.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(theme::text())
                                            .child(format!("会话 {}", session_id)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(theme::text_faint())
                                            .child(format!("{} 轮", rounds.len())),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        widgets::ghost_button(("sess-export", 0usize), "导出 Markdown").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                if let Some(m) = &this.session_meta {
                                                    let md = ns_sessions::export_markdown(
                                                        m,
                                                        &this.session_rounds,
                                                    );
                                                    let out = std::env::var_os("HOME")
                                                        .map(std::path::PathBuf::from)
                                                        .unwrap_or_default()
                                                        .join("Desktop")
                                                        .join(format!("{}.md", m.id));
                                                    let _ = std::fs::write(&out, md);
                                                    this.status = format!("已导出 {}", out.display());
                                                    cx.notify();
                                                }
                                            }),
                                        ),
                                    )
                                    .child(
                                        widgets::ghost_button(("sess-copy-all", 0usize), "复制整段").on_click(
                                            cx.listener(|this, _, _, cx| {
                                                let Some(meta) = this.session_meta.clone() else {
                                                    return;
                                                };
                                                let md = ns_sessions::export_markdown(
                                                    &meta,
                                                    &this.session_rounds,
                                                );
                                                cx.write_to_clipboard(
                                                    ClipboardItem::new_string(md.clone()),
                                                );
                                                this.status =
                                                    format!("已复制整段会话（{} 字符）", md.chars().count());
                                                this.status_tone = StatusTone::Success;
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                    .child(
                                        widgets::ghost_button(("sess-recover", 0usize), "恢复命令").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                let tool = this
                                                    .session_meta
                                                    .as_ref()
                                                    .map(|m| m.tool.as_str())
                                                    .unwrap_or("claude");
                                                let cmd = ns_sessions::recover_command_for(tool, &sid_recover);
                                                this.status = cmd.clone();
                                                this.status_tone = StatusTone::Neutral;
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                    .child(
                                        widgets::ghost_button(("sess-restore-terminal", 0usize), "在终端恢复").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                let tool = this
                                                    .session_meta
                                                    .as_ref()
                                                    .map(|m| m.tool.as_str())
                                                    .unwrap_or("claude");
                                                let cmd = ns_sessions::recover_command_for(tool, &sid_terminal);
                                                match run_terminal_command(&cmd) {
                                                    Ok(()) => {
                                                        this.status = format!("已在终端恢复会话：{cmd}");
                                                        this.status_tone = StatusTone::Success;
                                                    }
                                                    Err(e) => {
                                                        this.status = format!("打开终端失败: {e}；可手动执行：{cmd}");
                                                        this.status_tone = StatusTone::Error;
                                                    }
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                    .child(
                                        widgets::ghost_button(("sess-back", 0usize), "返回列表").on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.session_viewing = None;
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(8.))
                            .children(rounds.iter().enumerate().map(|(i, r)| {
                                let is_open = collapsed.get(i).copied().unwrap_or(true);
                                let failed_color = if r.failed {
                                    theme::DANGER
                                } else {
                                    theme::TEXT_DIM
                                };
                                let round_md = ns_sessions::round_markdown(r);
                                div()
                                    .id(("round", i))
                                    .flex_col()
                                    .gap(px(4.))
                                    .p(px(10.))
                                    .rounded(px(8.))
                                    .bg(theme::panel_a(0.4))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.08))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .text_size(px(12.))
                                                    .text_color(rgb(failed_color))
                                                    .child(format!(
                                                        "第 {} 轮 · {} 步 · {} 文件改动{}",
                                                        i + 1,
                                                        r.steps,
                                                        r.file_changes,
                                                        if r.failed { " · 失败" } else { "" }
                                                    )),
                                            )
                                            .child(
                                                widgets::ghost_button(("round-copy", i), "复制本轮").on_click(
                                                    cx.listener(move |this, _, _, cx| {
                                                        cx.write_to_clipboard(
                                                            ClipboardItem::new_string(round_md.clone()),
                                                        );
                                                        this.status = format!(
                                                            "已复制第 {} 轮（{} 字符）",
                                                            i + 1,
                                                            round_md.chars().count()
                                                        );
                                                        this.status_tone = StatusTone::Success;
                                                        cx.notify();
                                                    }),
                                                ),
                                            )
                                            .child(
                                                widgets::ghost_button(("round-toggle", i), if is_open { "收起" } else { "展开" }).on_click(
                                                    cx.listener(move |this, _, _, cx| {
                                                        if let Some(c) = this.session_collapsed.get_mut(i) {
                                                            *c = !*c;
                                                        }
                                                        cx.notify();
                                                    }),
                                                ),
                                            ),
                                    )
                                    .child(if is_open {
                                        div()
                                            .flex_col()
                                            .gap(px(6.))
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(theme::text())
                                                    .child(format!("用户：{}", r.user_text)),
                                            )
                                            .children(r.assistant.iter().enumerate().map(|(_, m)| {
                                                div()
                                                    .flex_col()
                                                    .gap(px(2.))
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .text_color(theme::accent())
                                                            .child(if m.is_tool_use {
                                                                format!(
                                                                    "工具 {}",
                                                                    m.tool_name.as_deref().unwrap_or("")
                                                                )
                                                            } else {
                                                                "助手".to_string()
                                                            }),
                                                    )
                                                    .child(
                                                        div()
                                                            .font_family(theme::MONO_FONT)
                                                            .text_size(px(11.))
                                                            .text_color(theme::text_dim())
                                                            .child(m.text.clone()),
                                                    )
                                            }))
                                    } else {
                                        div()
                                    })
                            })),
                    ),
            )
            .child(
                // 右侧：大纲（轮次目录 + 会话内搜索）。
                div()
                    .id("session-outline")
                    .w(px(220.))
                    .h_full()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(12.))
                    .border_l_1()
                    .border_color(theme::stroke_a(0.1))
                    .child(div().w_full().child(self.session_filter.clone()))
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .children(rounds.iter().enumerate().map(|(i, r)| {
                                let is_open = collapsed.get(i).copied().unwrap_or(true);
                                div()
                                    .id(("outline-round", i))
                                    .p(px(6.))
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::stroke_a(0.06)))
                                    .text_size(px(11.))
                                    .text_color(rgb(if is_open {
                                        theme::TEXT
                                    } else {
                                        theme::TEXT_DIM
                                    }))
                                    .child(format!("第 {} 轮 · {} 步", i + 1, r.steps))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(c) = this.session_collapsed.get_mut(i) {
                                            *c = true;
                                        }
                                        cx.notify();
                                    }))
                            })),
                    ),
            )
    }

    /// 接收深链动作：暂存待确认（含脱敏摘要），由 UI 确认浮层决定执行。
    fn apply_deeplink(&mut self, action: &crate::deeplink::DeepLinkAction, cx: &mut Context<Self>) {
        self.deeplink_pending = Some((action.clone(), Self::deeplink_summary(action)));
        cx.notify();
    }

    /// 确认后执行深链动作（原直接导入逻辑）。
    fn execute_deeplink(&mut self, action: &crate::deeplink::DeepLinkAction, cx: &mut Context<Self>) {
        self.deeplink_pending = None;
        match action {
            crate::deeplink::DeepLinkAction::ImportProvider {
                preset_id,
                endpoint,
                key,
                model,
            } => {
                if let Some(preset) =
                    ns_presets::presets_for(ns_presets::Tool::Claude)
                        .find(|p| p.id == *preset_id)
                {
                    let fields = crate::dialog::DialogFields {
                        name: preset.name.to_string(),
                        endpoint: endpoint.clone(),
                        key: key.clone(),
                        model: model.clone(),
                        usage_script: String::new(),
                        api_format: preset.api_format.to_string(),
                        icon: preset.icon.to_string(),
                        icon_color: preset.icon_color.to_string(),
                        usage_enabled: true,
                    };
                    let provider = crate::dialog::build_provider(preset, &fields);
                    let id = provider.id.clone();
                    {
                        let mut s = self.store.write().expect("store lock");
                        let _ = s.add_provider(provider);
                        let _ = s.set_current(ns_presets::Tool::Claude, id);
                    }
                    // P6-5：导入后定位到 Claude 供应商页。
                    self.active_tool = ns_presets::Tool::Claude;
                    self.page = Page::Providers;
                    cx.notify();
                }
            }
            crate::deeplink::DeepLinkAction::SwitchProvider { tool, provider_id } => {
                if let Some(t) = ns_presets::Tool::ALL.iter().find(|t| t.id_str() == tool) {
                    // 真实切换：写 live 配置 + 更新 current。
                    self.switch_to_provider_for(*t, provider_id, cx);
                }
            }
            crate::deeplink::DeepLinkAction::ImportRepo { kind, url } => {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
                let Some(home) = home else {
                    return;
                };
                let target = match kind.as_str() {
                    "skill" | "skills" => home.join(".claude").join("skills"),
                    "prompt" | "prompts" => home.join(".claude").join("prompts"),
                    "mcp" => home.join(".claude"),
                    _ => return,
                };
                let tmp = std::env::temp_dir().join(format!("ns-repo-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&tmp);
                let status = std::process::Command::new("git")
                    .args(["clone", "--depth", "1", url])
                    .arg(&tmp)
                    .status();
                match status {
                    Ok(s) if s.success() => {
                        // 浅克隆成功：复制到目标（若目标为目录则复制内容）。
                        let _ = std::fs::create_dir_all(&target);
                        let _ = Self::copy_dir_recursive(&tmp, &target);
                        eprintln!("[deeplink] 已导入 {kind} 仓库 → {}", target.display());
                    }
                    _ => {
                        eprintln!("[deeplink] 仓库导入失败: {url}");
                    }
                }
                let _ = std::fs::remove_dir_all(&tmp);
            }
            crate::deeplink::DeepLinkAction::ImportMcp { name, command } => {
                if name.trim().is_empty() || command.trim().is_empty() {
                    eprintln!("[deeplink] MCP 导入参数不完整");
                    return;
                }
                let server = ns_mcp::model::McpServer {
                    name: name.clone(),
                    transport: ns_mcp::model::McpTransport::Stdio,
                    command: Some(command.clone()),
                    args: Vec::new(),
                    ..Default::default()
                };
                let paths = ns_mcp::tools::McpPaths::from_env();
                match ns_mcp::manager::write_tool(
                    ns_presets::Tool::Claude,
                    &paths,
                    &[],
                    &[server],
                ) {
                    Ok(_) => {
                        eprintln!("[deeplink] 已导入 MCP {name}");
                        self.page = Page::Mcp;
                        cx.notify();
                    }
                    Err(e) => eprintln!("[deeplink] MCP 导入失败: {e}"),
                }
            }
            crate::deeplink::DeepLinkAction::ImportSkill { name, source } => {
                if name.trim().is_empty() {
                    eprintln!("[deeplink] Skill 导入参数不完整");
                    return;
                }
                let skill = ns_skills::model::Skill {
                    name: name.clone(),
                    description: String::new(),
                    body: if source.trim().is_empty() {
                        format!("# {name}\n")
                    } else {
                        source.clone()
                    },
                    ..Default::default()
                };
                let paths = ns_skills::model::SkillPaths::from_env();
                match ns_skills::tools::write_tool(
                    ns_presets::Tool::Claude,
                    &paths,
                    &skill,
                ) {
                    Ok(_) => {
                        eprintln!("[deeplink] 已导入 Skill {name}");
                        self.page = Page::Skills;
                        cx.notify();
                    }
                    Err(e) => eprintln!("[deeplink] Skill 导入失败: {e}"),
                }
            }
            crate::deeplink::DeepLinkAction::ImportPrompt { name, content } => {
                if name.trim().is_empty() {
                    eprintln!("[deeplink] Prompt 导入参数不完整");
                    return;
                }
                let mut store = match ns_prompts::store::PromptStore::open() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[deeplink] Prompt 库打开失败: {e}");
                        return;
                    }
                };
                let entry = ns_prompts::model::PromptEntry {
                    name: name.clone(),
                    body: content.clone(),
                    enabled: true,
                    updated_at: now_ts() as u64 * 1000,
                };
                match store.save_entry(ns_presets::Tool::Claude, entry) {
                    Ok(_) => {
                        eprintln!("[deeplink] 已导入 Prompt {name}");
                        self.page = Page::Prompts;
                        cx.notify();
                    }
                    Err(e) => eprintln!("[deeplink] Prompt 导入失败: {e}"),
                }
            }
        }
    }

    /// 递归复制目录内容（深链仓库导入用）。
    fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                Self::copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }

/// 构建深链动作的脱敏摘要（用于确认对话框展示，key/content 脱敏）。
fn deeplink_summary(action: &crate::deeplink::DeepLinkAction) -> String {
    fn mask(s: &str) -> String {
        if s.len() <= 8 {
            "****".to_string()
        } else {
            let mut out = s[..4].to_string();
            out.push_str("…");
            out.push_str(&s[s.len() - 4..]);
            out
        }
    }
    match action {
        crate::deeplink::DeepLinkAction::ImportProvider {
            preset_id,
            endpoint,
            key,
            model,
        } => format!(
            "导入供应商：{}（端点 {}，模型 {}，密钥 {}）",
            preset_id,
            endpoint,
            model,
            mask(key)
        ),
        crate::deeplink::DeepLinkAction::SwitchProvider { tool, provider_id } => {
            format!("切换 {tool} 到供应商 {provider_id}")
        }
        crate::deeplink::DeepLinkAction::ImportRepo { kind, url } => {
            format!("导入 {kind} 仓库：{url}")
        }
        crate::deeplink::DeepLinkAction::ImportMcp { name, command } => {
            format!("导入 MCP 服务器：{name}（命令 {}）", mask(command))
        }
        crate::deeplink::DeepLinkAction::ImportSkill { name, source } => {
            format!("导入 Skill：{name}（正文 {} 字）", source.chars().count())
        }
        crate::deeplink::DeepLinkAction::ImportPrompt { name, content } => {
            format!("导入 Prompt：{name}（内容 {} 字）", content.chars().count())
        }
    }
}

    /// 切到第 idx 页（快捷键 cmd-1..6）。
    fn goto_page(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(p) = Page::ALL.get(idx) {
            self.page = *p;
            cx.notify();
        }
    }

    /// 检测添加/编辑对话框是否有未保存修改（字段与初始快照比对）。
    pub(crate) fn dialog_is_dirty(&self, cx: &mut Context<Self>) -> bool {
        let f = &self.dialog_fields;
        let name = self.dialog_inputs.name.read(cx).text();
        let endpoint = self.dialog_inputs.endpoint.read(cx).text();
        let key = self.dialog_inputs.key.read(cx).text();
        let model = self.dialog_inputs.model.read(cx).text();
        let usage = self.dialog_inputs.usage_script.read(cx).text();
        let icon = self.dialog_inputs.icon.read(cx).text();
        let icon_color = self.dialog_inputs.icon_color.read(cx).text();
        name != f.name
            || endpoint != f.endpoint
            || key != f.key
            || model != f.model
            || usage != f.usage_script
            || icon != f.icon
            || icon_color != f.icon_color
    }

    /// Esc：关闭当前打开的对话框/菜单。
    fn escape_dialog(&mut self, cx: &mut Context<Self>) {
        if self.backup_restore_pending.is_some() {
            self.backup_restore_pending = None;
        } else if self.pending_switch.is_some() {
            self.pending_switch = None;
            self.status = "已取消切换".into();
            self.status_tone = StatusTone::Neutral;
        } else if self.deeplink_pending.is_some() {
        } else if self.confirm_delete.is_some() {
        } else if self.edit_id.is_some() || self.add_open {
            if self.dialog_is_dirty(cx) {
                // 有未保存修改：显示守卫确认浮层，不直接关闭。
                self.dialog_dirty = true;
                cx.notify();
                return;
            }
            self.add_open = false;
            self.edit_id = None;
        } else if self.open_menu.is_some() {
            self.open_menu = None;
        } else if self.mcp.sync_targets_open.is_some() {
            self.mcp.sync_targets_open = None;
        } else if self.mcp.dialog_open {
            self.mcp.dialog_open = false;
        } else if self.mcp.scope_open {
            self.mcp.scope_open = false;
        } else if self.skills.dialog_open {
            self.skills.request_close(cx);
        } else if self.skills.install_open {
            self.skills.install_open = false;
        } else if self.prompts.dialog_open {
            self.prompts.request_close(cx);
        }
        cx.notify();
    }

    /// Cmd-S：当前页/对话框的保存动作。
    ///
    /// 供应商对话框 → `dialog::save_provider`；Prompts/Skills/MCP 对话框 →
    /// 各自 `save`；工具配置编辑器 → `save_toolconfig`。无打开对话框时提示。
    fn save_current(&mut self, cx: &mut Context<Self>) {
        // 供应商添加/编辑对话框（有预设时走标准保存路径）。
        if self.add_open || self.edit_id.is_some() {
            let preset = self
                .dialog_preset
                .as_deref()
                .and_then(|id| {
                    ns_presets::search_presets("", None)
                        .into_iter()
                        .find(|p| p.id == id)
                });
            if let Some(preset) = preset {
                crate::dialog::save_provider(self, preset, &self.dialog_fields.clone(), cx);
            } else {
                self.status = "请先选择预设".into();
                self.status_tone = StatusTone::Error;
                cx.notify();
            }
            return;
        }
        if self.prompts.dialog_open {
            self.prompts.save(cx);
            return;
        }
        if self.skills.dialog_open || self.skills.install_open {
            if self.skills.dialog_open {
                self.skills.save(cx);
            }
            return;
        }
        if self.mcp.dialog_open {
            self.mcp.save(cx);
            return;
        }
        if self.toolconfig_id.is_some() {
            self.save_toolconfig(cx);
            return;
        }
        self.status = "当前无可保存的表单".into();
        self.status_tone = StatusTone::Neutral;
        cx.notify();
    }

    /// 关闭首次运行欢迎页并持久化。
    fn dismiss_welcome(&mut self, cx: &mut Context<Self>) {
        self.welcome_visible = false;
        if let Ok(mut s) = self.store.write() {
            let _ = s.set_welcome_dismissed(true);
        }
        cx.notify();
    }

    /// 首次运行欢迎页（全屏遮罩 + 引导卡片）。
    fn render_welcome(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = ns_ui::theme::active_palette();
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x080604, 0.78))
            .child(
                div()
                    .w(px(520.))
                    .flex_col()
                    .gap(px(16.))
                    .p(px(28.))
                    .rounded(px(20.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.3))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(widgets::brand_mark(44.))
                            .child(
                                div()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(18.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(theme::text())
                                            .child("欢迎使用 ns-switch"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(theme::text_faint())
                                            .child("一键切换 AI 编程助手 API 供应商"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(10.))
                            .child(
                                welcome_row(
                                    "①",
                                    "添加供应商",
                                    "从 90+ 预设中选择，或自定义端点/密钥/模型",
                                    pal,
                                ),
                            )
                            .child(welcome_row(
                                "②",
                                "一键切换",
                                "只改写客户端配置关键字段，其余原样保留",
                                pal,
                            ))
                            .child(welcome_row(
                                "③",
                                "管理 MCP / Skills / Prompts",
                                "统一面板，跨工具同步",
                                pal,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::primary_button("welcome-start", "开始使用").on_click(
                                    cx.listener(|this, _, _, cx| this.dismiss_welcome(cx)),
                                ),
                            ),
                    ),
            )
    }

    /// 更新说明（WhatsNew）浮层：上次版本 ≠ 当前版本时显示（对齐 cc-switch WhatsNewDialog）。
    fn render_whats_new(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = ns_ui::theme::active_palette();
        let last_seen = self
            .store
            .read()
            .map(|s| s.config().settings.last_seen_version.clone())
            .unwrap_or_default();
        let version = crate::update::CURRENT_VERSION;
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x080604, 0.72))
            .child(
                div()
                    .w(px(560.))
                    .flex_col()
                    .gap(px(16.))
                    .p(px(28.))
                    .rounded(px(20.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.3))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(widgets::brand_mark(44.))
                            .child(
                                div()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(18.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(theme::text())
                                            .child(format!("更新到 {version}")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(theme::text_faint())
                                            .child(if last_seen.is_empty() {
                                                "欢迎使用 ns-switch".to_string()
                                            } else {
                                                format!("上次版本 {last_seen}")
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(10.))
                            .child(welcome_row(
                                "✦",
                                "界面布局对齐 cc-switch",
                                "侧栏应用列表滚动区 + 全局页面贴底 + 底栏应用/设置",
                                pal,
                            ))
                            .child(welcome_row(
                                "✦",
                                "供应商卡片增强",
                                "左拖拽手柄 + 右侧额度列 + 健康芯片 + 模式色边框",
                                pal,
                            ))
                            .child(welcome_row(
                                "✦",
                                "切换引擎稳定",
                                "保序补丁改写、首写备份、冲突检测",
                                pal,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::primary_button("whatsnew-close", "开始使用").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.whats_new_visible = false;
                                        cx.notify();
                                    }),
                                ),
                            ),
                    ),
            )
    }

    /// 写入前冲突确认浮层（切换目标被占用/修改；确认强制执行 / 取消）。
    fn render_pending_switch_confirm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (tool, provider, note) = self
            .pending_switch
            .clone()
            .expect("pending_switch 非空");
        div()
            .id("switch-confirm-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.6))
            .child(
                div()
                    .id("switch-confirm")
                    .w(px(480.))
                    .flex_col()
                    .gap(px(14.))
                    .p(px(22.))
                    .rounded(px(16.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.3))
                    .shadow_lg()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("切换前冲突确认"),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme::text())
                            .child(format!(
                                "将切换到「{}」（{}）。\n{}",
                                provider.name,
                                tool.label(),
                                note
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::cinnabar())
                            .child("⚠ 目标配置文件已被手动修改/占用，继续切换将覆盖这些更改。"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("switch-cancel", 0usize), "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.pending_switch = None;
                                        this.status = "已取消切换".into();
                                        this.status_tone = StatusTone::Neutral;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::primary_button(("switch-confirm", 0usize), "强制切换")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some((tool, provider, _)) = this.pending_switch.clone()
                                        {
                                            this.pending_switch = None;
                                            this.apply_switch(tool, &provider, cx);
                                        }
                                    })),
                            ),
                    ),
            )
    }

    /// 深链导入确认浮层（脱敏摘要 + 确认/取消；风险提示）。
    fn render_deeplink_confirm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (_, summary) = self
            .deeplink_pending
            .clone()
            .expect("deeplink_pending 非空");
        div()
            .id("deeplink-confirm-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.6))
            .child(
                div()
                    .id("deeplink-confirm")
                    .w(px(460.))
                    .flex_col()
                    .gap(px(14.))
                    .p(px(22.))
                    .rounded(px(16.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.3))
                    .shadow_lg()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("深链导入确认"),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme::text())
                            .child(summary),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::cinnabar())
                            .child("⚠ 导入会写入本机配置文件，请确认来源可信。"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("deeplink-cancel", 0usize), "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.deeplink_pending = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::primary_button(("deeplink-confirm", 0usize), "确认导入")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some((action, _)) = this.deeplink_pending.clone() {
                                            this.execute_deeplink(&action, cx);
                                        }
                                    })),
                            ),
                    ),
            )
    }




    /// 扫描全部 CLI 工具版本（异步）。
    fn scan_tools(&mut self, cx: &mut Context<Self>) {
        self.tools_scanning = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = ns_tools::detect_all();
            this.update(cx, |this, cx| {
                this.tools_result = result;
                this.tools_scanning = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 安装/升级 CLI 工具（异步执行 npm 命令）。
    fn install_cli(&mut self, tool: ns_tools::Tool, cx: &mut Context<Self>) {
        self.tool_msg = format!("正在安装 {} …", tool.name());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = ns_tools::install_tool(tool);
            this.update(cx, |this, cx| {
                this.tool_msg = match result {
                    Ok(msg) => format!("已安装 {}：{}", tool.name(), msg.trim()),
                    Err(e) => format!("安装 {} 失败：{e}", tool.name()),
                };
                this.scan_tools(cx);
            })
            .ok();
        })
        .detach();
    }

    /// 卸载 CLI 工具（异步执行 npm 命令）。
    fn uninstall_cli(&mut self, tool: ns_tools::Tool, cx: &mut Context<Self>) {
        self.tool_msg = format!("正在卸载 {} …", tool.name());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = ns_tools::uninstall_tool(tool);
            this.update(cx, |this, cx| {
                this.tool_msg = match result {
                    Ok(msg) => format!("已卸载 {}：{}", tool.name(), msg.trim()),
                    Err(e) => format!("卸载 {} 失败：{e}", tool.name()),
                };
                this.scan_tools(cx);
            })
            .ok();
        })
        .detach();
    }

    /// 渲染 M11 工具管理页：版本探测、安装/升级、重复安装诊断。
    fn render_tools_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let scanning = self.tools_scanning;
        div()
            .id("tools-scroll")
            .flex_1()
            .h_full()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("工具管理"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_faint())
                                    .child("CLI 工具版本探测与安装"),
                            ),
                    )
                    .children([
                        widgets::ghost_button(("tools-scan", 0usize), "扫描").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.scan_tools(cx);
                            }),
                        ),
                        widgets::ghost_button(("tools-upgrade-all", 0usize), "全部升级").on_click(
                            cx.listener(|this, _, _, cx| {
                                let installed: Vec<ns_tools::Tool> = this
                                    .tools_result
                                    .iter()
                                    .filter(|(_, i)| i.is_some())
                                    .map(|(t, _)| *t)
                                    .collect();
                                if installed.is_empty() {
                                    this.tool_msg = "没有已安装的工具可升级".into();
                                    cx.notify();
                                    return;
                                }
                                this.tool_msg =
                                    format!("正在升级 {} 个工具…", installed.len());
                                cx.notify();
                                for t in installed {
                                    this.install_cli(t, cx);
                                }
                            }),
                        ),
                    ])
            )
            .child(if !self.tool_msg.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(theme::jade())
                    .child(self.tool_msg.clone())
            } else {
                div()
            })
            .child(if scanning {
                div()
                    .text_size(px(12.))
                    .text_color(theme::text_dim())
                    .child("扫描中…")
            } else {
                div()
                    .flex_col()
                    .gap(px(8.))
                    .children(ns_tools::Tool::ALL.iter().map(|tool| {
                        let inst = self
                            .tools_result
                            .iter()
                            .find(|(t, _)| t == tool)
                            .and_then(|(_, i)| i.clone());
                        let dup = ns_tools::duplicate_installations(*tool);
                        let name = tool.name();
                        let bin = tool.bin();
                        div()
                            .id(("tool-row", *tool as usize))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(8.))
                            .bg(theme::panel_a(0.5))
                            .border_1()
                            .border_color(theme::stroke_a(0.1))
                            .child(
                                div()
                                    .flex_1()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(theme::text())
                                            .child(format!("{name} ({bin})")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(if dup.len() > 1 {
                                                theme::DANGER
                                            } else {
                                                theme::TEXT_FAINT
                                            }))
                                            .child(match (&inst, dup.len()) {
                                                (Some(i), n) if n > 1 => format!(
                                                    "{} · 重复安装 {} 处",
                                                    i.version.clone().unwrap_or_default(),
                                                    n
                                                ),
                                                (Some(i), _) => format!(
                                                    "{} · {}",
                                                    i.version.clone().unwrap_or_default(),
                                                    i.path
                                                ),
                                                (None, n) if n > 1 => {
                                                    format!("重复安装 {} 处", n)
                                                }
                                                (None, _) => "未安装".to_string(),
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        widgets::ghost_button(
                                            ("tool-install", *tool as usize),
                                            if inst.is_some() { "升级" } else { "安装" },
                                        )
                                        .on_click({
                                            let t = *tool;
                                            cx.listener(move |this, _, _, cx| {
                                                this.install_cli(t, cx);
                                            })
                                        }),
                                    )
                                    .when(inst.is_some(), |el| {
                                        el.child(
                                            widgets::danger_button(
                                                ("tool-uninstall", *tool as usize),
                                                "卸载",
                                            )
                                            .on_click({
                                                let t = *tool;
                                                cx.listener(move |this, _, _, cx| {
                                                    this.uninstall_cli(t, cx);
                                                })
                                            }),
                                        )
                                    }),
                            )
                    }))
            })
    }

    /// 收集当前 MCP/Skills/Prompts 状态为项目快照 extras（JSON 载体）。
    ///
    /// key：`mcp` / `skills` / `prompts`，值为 `{tool_id: [...]}`。
    fn project_extras(
        &self,
        _cx: &mut Context<Self>,
    ) -> std::collections::BTreeMap<String, serde_json::Value> {
        use std::collections::BTreeMap;
        let mut extras: BTreeMap<String, serde_json::Value> = BTreeMap::new();

        // MCP：遍历支持 MCP 的工具，读服务器列表。
        let mcp_tools = [
            ns_presets::Tool::Claude,
            ns_presets::Tool::Codex,
            ns_presets::Tool::Gemini,
            ns_presets::Tool::Opencode,
            ns_presets::Tool::GrokBuild,
        ];
        let mcp_paths = ns_mcp::tools::McpPaths::from_env();
        let mut mcp_map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for t in mcp_tools {
            if let Ok(servers) = ns_mcp::manager::read_tool(t, &mcp_paths) {
                if !servers.is_empty() {
                    mcp_map.insert(
                        t.id_str().to_string(),
                        serde_json::to_value(&servers).unwrap_or(serde_json::Value::Null),
                    );
                }
            }
        }
        if !mcp_map.is_empty() {
            extras.insert("mcp".into(), serde_json::to_value(&mcp_map).unwrap_or_default());
        }

        // Skills：遍历全部工具读技能（含共享目录设置）。
        let skill_paths = ns_skills::model::SkillPaths::from_env();
        let mut skills_map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for t in ns_presets::Tool::ALL {
            if let Ok(skills) = ns_skills::manager::read_tool(t, &skill_paths) {
                if !skills.is_empty() {
                    skills_map.insert(
                        t.id_str().to_string(),
                        serde_json::to_value(&skills).unwrap_or(serde_json::Value::Null),
                    );
                }
            }
        }
        if !skills_map.is_empty() {
            extras.insert("skills".into(), serde_json::to_value(&skills_map).unwrap_or_default());
        }

        // Prompts：读各工具提示库条目。
        let prompt_paths = ns_prompts::model::PromptPaths::from_env();
        let mut prompts_map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        if let Ok(store) = ns_prompts::store::PromptStore::open_at(prompt_paths.home.clone()) {
            for t in ns_presets::Tool::ALL {
                let entries = store.entries_for(t);
                if !entries.is_empty() {
                    prompts_map.insert(
                        t.id_str().to_string(),
                        serde_json::to_value(&entries).unwrap_or(serde_json::Value::Null),
                    );
                }
            }
        }
        if !prompts_map.is_empty() {
            extras.insert("prompts".into(), serde_json::to_value(&prompts_map).unwrap_or_default());
        }

        // 工作区快照：OpenClaw openclaw.json + Hermes config.yaml（全文备份/恢复）。
        if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
            let files: [( &str, std::path::PathBuf); 2] = [
                ("openclaw", home.join(".openclaw").join("openclaw.json")),
                ("hermes", home.join(".hermes").join("config.yaml")),
            ];
            let mut ws: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            for (key, path) in files {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    ws.insert(key.to_string(), serde_json::Value::String(text));
                }
            }
            if !ws.is_empty() {
                extras.insert("workspace".into(), serde_json::to_value(&ws).unwrap_or_default());
            }
        }
        extras
    }

    /// 恢复项目快照 extras 到 MCP/Skills/Prompts（覆盖对应工具当前配置）。
    fn apply_project_extras(
        &mut self,
        extras: &std::collections::BTreeMap<String, serde_json::Value>,
        cx: &mut Context<Self>,
    ) {


        // MCP 恢复。
        if let Some(v) = extras.get("mcp").and_then(|v| v.as_object()) {
            let paths = ns_mcp::tools::McpPaths::from_env();
            for (tid, servers_v) in v {
                let Some(tool) = ns_presets::Tool::ALL.iter().find(|t| t.id_str() == tid) else {
                    continue;
                };
                let Some(servers) = serde_json::from_value::<Vec<ns_mcp::model::McpServer>>(
                    servers_v.clone(),
                )
                .ok()
                else {
                    continue;
                };
                let names: Vec<String> = servers.iter().map(|s| s.name.clone()).collect();
                // 先移除当前全部（按已读列表），再写回快照列表。
                if let Ok(current) = ns_mcp::manager::read_tool(*tool, &paths) {
                    let cur: Vec<String> = current.iter().map(|s| s.name.clone()).collect();
                    let _ = ns_mcp::manager::write_tool(*tool, &paths, &cur, &[]);
                }
                let _ = ns_mcp::manager::write_tool(*tool, &paths, &names, &servers);
            }
        }

        // Skills 恢复（写回各工具技能目录）。
        if let Some(v) = extras.get("skills").and_then(|v| v.as_object()) {
            let paths = ns_skills::model::SkillPaths::from_env();
            for (tid, skills_v) in v {
                let Some(tool) = ns_presets::Tool::ALL.iter().find(|t| t.id_str() == tid) else {
                    continue;
                };
                let Some(skills) = serde_json::from_value::<Vec<ns_skills::model::Skill>>(
                    skills_v.clone(),
                )
                .ok()
                else {
                    continue;
                };
                // 逐技能写回（复用 manager 单技能写入）。
                for sk in skills {
                    let _ = ns_skills::manager::write_tool(
                        *tool,
                        &paths,
                        &sk,
                        ns_skills::model::SyncStrategy::Auto,
                    );
                }
            }
        }

        // Prompts 恢复（写回提示库）。
        if let Some(v) = extras.get("prompts").and_then(|v| v.as_object()) {
            let prompt_paths = ns_prompts::model::PromptPaths::from_env();
            if let Ok(mut store) =
                ns_prompts::store::PromptStore::open_at(prompt_paths.home.clone())
            {
                for (tid, entries_v) in v {
                    let Some(tool) = ns_presets::Tool::ALL.iter().find(|t| t.id_str() == tid)
                    else {
                        continue;
                    };
                    let Some(entries) =
                        serde_json::from_value::<Vec<ns_prompts::model::PromptEntry>>(
                            entries_v.clone(),
                        )
                        .ok()
                    else {
                        continue;
                    };
                    for e in entries {
                        let _ = store.save_entry(*tool, e);
                    }
                }
            }
        }

        // 工作区恢复：写回 OpenClaw openclaw.json + Hermes config.yaml。
        if let Some(v) = extras.get("workspace").and_then(|v| v.as_object()) {
            if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
                let files: [(&str, std::path::PathBuf); 2] = [
                    ("openclaw", home.join(".openclaw").join("openclaw.json")),
                    ("hermes", home.join(".hermes").join("config.yaml")),
                ];
                for (key, path) in files {
                    if let Some(text) = v.get(key).and_then(|x| x.as_str()) {
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(&path, text);
                    }
                }
            }
        }
        self.status = "已恢复项目扩展状态（MCP/Skills/Prompts/工作区）".into();

        cx.notify();
    }

    /// 渲染 M11 项目快照页：保存当前状态、整组切换、自动回存。
    fn render_projects_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let projects: Vec<String> = self
            .store
            .read()
            .map(|s| s.list_projects())
            .unwrap_or_default();
        let active: Option<String> = self
            .store
            .read()
            .ok()
            .and_then(|s| s.active_project_for(self.active_tool));
        div()
            .id("projects-scroll")
            .flex_1()
            .h_full()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("项目快照"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child("保存当前供应商 + 聚合为一项目，一键整组切换"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().w(px(220.)).child(self.project_name.clone()))
                    .child(
                        widgets::ghost_button("project-save", "保存当前为项目").on_click(
                            cx.listener(|this, _, _, cx| {
                                let name = this.project_name.read(cx).text();
                                if name.is_empty() {
                                    this.status = "请输入项目名".into();
                                    cx.notify();
                                    return;
                                }
                                let extras = this.project_extras(cx);
                                let ok = {
                                    let mut s = this.store.write().expect("store lock");
                                    s.save_project(&name, ns_store::Store::scope_for(this.active_tool), &extras).is_ok()
                                };
                                this.status = if ok {
                                    format!("已保存项目 {name}")
                                } else {
                                    "保存项目失败".into()
                                };
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(8.))
                    .children(projects.into_iter().enumerate().map(|(i, name)| {
                        let n = name.clone();
                        div()
                            .id(("project-row", i))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(8.))
                            .bg(theme::panel_a(0.5))
                            .border_1()
                            .border_color(theme::stroke_a(0.1))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.))
                                    .text_color(theme::text())
                                    .child(name.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        widgets::ghost_button(
                                            gpui::ElementId::from((
                                                "project-current-badge",
                                                i,
                                            )),
                                            "当前",
                                        )
                                        .when(active.as_deref() != Some(name.as_str()), |b| {
                                            b.hidden()
                                        }),
                                    ),
                            )
                            .child(
                                widgets::ghost_button(("project-apply", i), "应用").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let current_extras = this.project_extras(cx);
                                        let target_extras = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.apply_project(&n, ns_store::Store::scope_for(this.active_tool), &current_extras).ok()
                                        };
                                        match target_extras {
                                            Some(ext) => {
                                                this.apply_project_extras(&ext, cx);
                                            }
                                            None => {
                                                this.status = format!("应用项目 {n} 失败");
                                                cx.notify();
                                            }
                                        }
                                    }),
                                ),
                            )
                            .child(
                                widgets::ghost_button(("project-del", i), "删除").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let _ = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.delete_project(&name)
                                        };
                                        this.status = format!("已删除项目 {name}");
                                        cx.notify();
                                    }),
                                ),
                            )
                    }))
            )
    }


    /// 后台轮询 OAuth device-code（参考 cc-switch `auth_poll_for_account`）。
    fn start_oauth_poll(&mut self, cx: &mut Context<Self>) {
        let Some((provider, session, account_id, account_name)) = self.oauth_login.clone() else {
            return;
        };
        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let mut session = session;
            loop {
                match session.poll() {
                    Ok(Some(token)) => {
                        let login_result = {
                            let mut s = store.write().expect("store lock");
                            s.login_account(&account_id, &account_name, &provider, &token)
                        };
                        this.update(cx, |this, cx| {
                            this.oauth_login = None;
                            this.status = match login_result {
                                Ok(()) => format!(
                                    "已登录 {account_name}（OAuth 成功，token 摘要 {}）",
                                    &token[..token.len().min(8)]
                                ),
                                Err(e) => format!("登录失败: {e}"),
                            };
                            cx.notify();
                        })
                        .ok();
                        break;
                    }
                    Ok(None) => {
                        cx.background_executor()
                            .timer(std::time::Duration::from_secs(3))
                            .await;
                    }
                    Err(e) => {
                        this.update(cx, |this, cx| {
                            this.oauth_login = None;
                            this.status = format!("OAuth 失败: {e}");
                            cx.notify();
                        })
                        .ok();
                        break;
                    }
                }
                let _ = &provider;
            }
        })
        .detach();
    }
    /// 渲染 M11 托管账号页（Beta：Copilot / ChatGPT / xAI 订阅作为供应商）。
    fn render_accounts_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = crate::i18n::Lang::parse(
            &self
                .store
                .read()
                .map(|s| s.config().settings.language.clone())
                .unwrap_or_else(|_| "zh-cn".into()),
        );
        let accounts: Vec<(String, bool)> = {
            let s = self.store.read().expect("store lock");
            [
                ("copilot", "GitHub Copilot"),
                ("chatgpt", "ChatGPT"),
                ("xai", "xAI"),
            ]
            .iter()
            .map(|(id, _name)| {
                let logged = s
                    .config()
                    .accounts
                    .get(*id)
                    .map(|a| a.logged_in)
                    .unwrap_or(false);
                ((*id).to_string(), logged)
            })
            .collect()
        };
        div()
            .id("accounts-scroll")
            .flex_1()
            .h_full()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child(crate::i18n::t(lang, "auth.title")),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child(crate::i18n::t(lang, "auth.subtitle")),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme::text())
                            .child("CLI 工具凭据"),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(8.))
                            .children(crate::auth::statuses().into_iter().map(|s| {
                                let tool = s.tool.to_string();
                                let name = s.name.to_string();
                                let hint = s.token_hint.clone();
                                let path = s.path.display().to_string();
                                div()
                                    .id(gpui::ElementId::from(gpui::SharedString::from(
                                        format!("cred-row-{}", tool),
                                    )))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .p(px(10.))
                                    .rounded(px(8.))
                                    .bg(theme::panel_a(0.5))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.1))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex_col()
                                            .gap(px(2.))
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(theme::text())
                                                    .child(name.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(if s.logged_in {
                                                        theme::ACCENT
                                                    } else {
                                                        theme::TEXT_FAINT
                                                    }))
                                                    .child(if s.logged_in {
                                                        format!("已授权 · {hint}")
                                                    } else {
                                                        "未授权（登录 CLI 生成凭据）".to_string()
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(theme::text_faint())
                                                    .child(path),
                                            )
                                            .child(if let Some(e) = s.error.clone() {
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(theme::danger())
                                                    .child(format!("读取失败: {e}"))
                                            } else {
                                                div()
                                            }),
                                    )
                                    .child(
                                        widgets::ghost_button(
                                            gpui::ElementId::from(gpui::SharedString::from(
                                                format!("cred-clear-{tool}"),
                                            )),
                                            "清除",
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            match crate::auth::clear(&tool) {
                                                Ok(true) => {
                                                    this.status =
                                                        format!("已清除 {name} 凭据");
                                                }
                                                Ok(false) => {
                                                    this.status =
                                                        format!("{name} 无凭据文件");
                                                }
                                                Err(e) => {
                                                    this.status = format!("清除失败: {e}");
                                                }
                                            }
                                            cx.notify();
                                        })),
                                    )
                            })),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(8.))
                    .children(accounts.into_iter().enumerate().map(|(i, (id, logged))| {
                        let name = match id.as_str() {
                            "copilot" => "GitHub Copilot",
                            "chatgpt" => "ChatGPT",
                            _ => "xAI",
                        };
                        let id2 = id.clone();
                        div()
                            .id(("account-row", i))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(8.))
                            .bg(theme::panel_a(0.5))
                            .border_1()
                            .border_color(theme::stroke_a(0.1))
                            .child(
                                div()
                                    .flex_1()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(theme::text())
                                            .child(format!("{name} ({id})")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(if logged {
                                                theme::ACCENT
                                            } else {
                                                theme::TEXT_FAINT
                                            }))
                                            .child(if logged {
                                                "已登录 · 订阅作为供应商"
                                            } else {
                                                "未登录"
                                            }),
                                    ),
                            )
                            .child(if logged {
                                widgets::ghost_button(("account-logout", i), "登出").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let _ = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.logout_account(&id2)
                                        };
                                        this.status = format!("已登出 {name}");
                                        cx.notify();
                                    }),
                                )
                            } else {
                                widgets::ghost_button(("account-login", i), "登录").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        // 真实 OAuth device-code 流程（参考 cc-switch）。
                                        let provider = match id2.as_str() {
                                            "xai" => "xai",
                                            "copilot" => "copilot",
                                            _ => "codex", // chatgpt → Codex OAuth
                                        };
                                        match crate::auth::start_device_flow(provider) {
                                            Ok(session) => {
                                                let user_code = session.user_code.clone();
                                                let uri = session.verification_uri.clone();
                                                let device_code = session.device_code.clone();
                                                let expires_in = session.expires_in;
                                                let interval = session.interval;
                                                this.oauth_login = Some((
                                                    provider.to_string(),
                                                    session,
                                                    id2.clone(),
                                                    name.to_string(),
                                                ));
                                                this.status = format!(
                                                    "请在浏览器打开 {uri} 输入代码 {user_code}（{expires_in} 秒内有效，每 {interval}s 轮询，设备码 {device_code}）"
                                                );
                                                this.start_oauth_poll(cx);
                                            }
                                            Err(e) => {
                                                this.status = format!("OAuth 启动失败: {e}");
                                            }
                                        }
                                        cx.notify();
                                    }),
                                )
                            })
                    }))
            )
            .child(if let Some((provider, session, _, _)) = &self.oauth_login {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::accent_a(0.08))
                    .border_1()
                    .border_color(theme::stroke_a(0.32))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text())
                            .child(format!(
                                "OAuth 登录中（{}）：浏览器输入代码 {}",
                                if provider == "xai" { "xAI" } else { "Codex" },
                                session.user_code
                            )),
                    )
            } else {
                div()
            })
    }

    /// 渲染工具配置页：OpenClaw 工作区指令 + Hermes 记忆。
    fn render_toolconfig_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let files = crate::toolconfig::files();
        div()
            .id("toolconfig-scroll")
            .flex_1()
            .h_full()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("工具配置"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child("OpenClaw 工作区指令与 Hermes 持久记忆（文件不存在时保存即创建）"),
                    ),
            )
            .child({
                // OpenClaw / Hermes 配置健康横幅。
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
                let oc_json = home
                    .as_ref()
                    .map(|h| h.join(".openclaw").join("openclaw.json"));
                let hermes_yaml = home
                    .as_ref()
                    .map(|h| h.join(".hermes").join("config.yaml"));
                let oc_ok = oc_json.as_ref().is_some_and(|p| p.exists());
                let oc_valid = oc_json
                    .as_ref()
                    .filter(|p| p.exists())
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .is_some_and(|t| serde_json::from_str::<serde_json::Value>(&t).is_ok());
                let hm_ok = hermes_yaml.as_ref().is_some_and(|p| p.exists());
                let tone = if oc_ok && oc_valid {
                    theme::jade()
                } else if oc_ok {
                    theme::cinnabar()
                } else {
                    theme::text_faint()
                };
                let text = match (oc_ok, oc_valid, hm_ok) {
                    (true, true, true) => "OpenClaw 配置有效 · Hermes 配置存在".to_string(),
                    (true, true, false) => "OpenClaw 配置有效 · Hermes 未初始化".to_string(),
                    (true, false, _) => "OpenClaw openclaw.json 存在但 JSON 解析失败（请检查）".to_string(),
                    (false, _, true) => "OpenClaw 未初始化 · Hermes 配置存在".to_string(),
                    (false, _, false) => "OpenClaw / Hermes 均未初始化（编辑保存即创建）".to_string(),
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .h(px(30.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.4))
                    .border_1()
                    .border_color(theme::stroke_a(0.1))
                    .child(
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(tone),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_dim())
                            .child(text),
                    )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        widgets::ghost_button("oc-defaults-open", "代理默认值").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.open_openclaw_defaults(cx);
                            }),
                        ),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(theme::text_faint())
                            .child("agents.defaults · 主模型/工作区/超时/并发"),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(8.))
                    .children(files.into_iter().map(|f| {
                        let id = f.id.to_string();
                        let label = f.label.to_string();
                        let hint = f.hint.to_string();
                        let path = f.path.display().to_string();
                        let exists = crate::toolconfig::exists(&id);
                        div()
                            .id(gpui::ElementId::from(gpui::SharedString::from(
                                format!("toolcfg-{id}"),
                            )))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(8.))
                            .bg(theme::panel_a(0.5))
                            .border_1()
                            .border_color(theme::stroke_a(0.1))
                            .child(
                                div()
                                    .flex_1()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(theme::text())
                                            .child(label),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::text_faint())
                                            .child(hint),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(if exists {
                                                theme::ACCENT
                                            } else {
                                                theme::TEXT_FAINT
                                            }))
                                            .child(if exists {
                                                format!("{path} · 已存在")
                                            } else {
                                                format!("{path} · 未初始化")
                                            }),
                                    ),
                            )
                            .child(
                                widgets::ghost_button(
                                    gpui::ElementId::from(gpui::SharedString::from(
                                        format!("toolcfg-edit-{id}"),
                                    )),
                                    "编辑",
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_toolconfig(&id, cx);
                                })),
                            )
                    })),
            )
            .child({
                // OpenClaw 每日记忆区。
                let today = crate::toolconfig::today_daily_memory_filename();
                let today_exists = crate::toolconfig::daily_memory_dir()
                    .map(|d| d.join(&today).exists())
                    .unwrap_or(false);
                let files = crate::toolconfig::list_daily_memory();
                div()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child("OpenClaw 每日记忆"),
                            )
                            .child(
                                widgets::primary_button(
                                    gpui::ElementId::from(gpui::SharedString::from(
                                        "dm-new-today",
                                    )),
                                    if today_exists { "编辑今日笔记" } else { "新建今日笔记" },
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_daily_memory(
                                        &crate::toolconfig::today_daily_memory_filename(),
                                        cx,
                                    );
                                })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child(format!(
                                "~/.openclaw/workspace/memory/{today}{}",
                                if today_exists { " · 已存在" } else { " · 今日尚未创建" }
                            )),
                    )
                    .child(if files.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("暂无每日记忆（新建今日笔记后出现在此）")
                            .into_any_element()
                    } else {
                        div()
                            .flex_col()
                            .gap(px(6.))
                            .children(files.into_iter().map(|f| {
                                let name = f.filename.clone();
                                let date = f.date.clone();
                                let size = f.size_bytes;
                                let modified = f.modified_at;
                                let mtime = if modified > 0 {
                                    let secs = modified / 1000;
                                    let days = secs.div_euclid(86400) as i64;
                                    let rem = secs.rem_euclid(86400);
                                    let (y, m, d) = crate::toolconfig::civil_from_days(days);
                                    let hh = rem / 3600;
                                    let mm = (rem % 3600) / 60;
                                    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}")
                                } else {
                                    "—".to_string()
                                };
                                div()
                                    .id(gpui::ElementId::from(gpui::SharedString::from(
                                        format!("dm-{name}"),
                                    )))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .p(px(10.))
                                    .rounded(px(8.))
                                    .bg(theme::panel_a(0.5))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.1))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex_col()
                                            .gap(px(2.))
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(theme::text())
                                                    .child(date.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(theme::text_faint())
                                                    .child(format!("{size} 字节 · 修改于 {mtime}")),
                                            ),
                                    )
                                    .child(
                                        widgets::ghost_button(
                                            gpui::ElementId::from(gpui::SharedString::from(
                                                format!("dm-edit-{name}"),
                                            )),
                                            "编辑",
                                        )
                                        .on_click(cx.listener({
                                            let n = name.clone();
                                            move |this, _, _, cx| {
                                                this.open_daily_memory(&n, cx);
                                            }
                                        })),
                                    )
                                    .child(
                                        widgets::ghost_button(
                                            gpui::ElementId::from(gpui::SharedString::from(
                                                format!("dm-del-{name}"),
                                            )),
                                            "删除",
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.daily_memory_confirm_delete = Some(name.clone());
                                            cx.notify();
                                        })),
                                    )
                            }))
                            .into_any_element()
                    })
            })
    }

    /// 打开工具配置编辑器（预填内容）。
    fn open_toolconfig(&mut self, id: &str, cx: &mut Context<Self>) {
        let content = match crate::toolconfig::read_file(id) {
            Ok(Some(c)) => c,
            Ok(None) => String::new(),
            Err(e) => {
                self.status = format!("读取失败: {e}");
                cx.notify();
                return;
            }
        };
        let editor = cx.new(|cx| {
            let mut ta = crate::text_area::TextArea::new("", cx);
            ta.set_text(&content);
            ta
        });
        self.toolconfig_id = Some(id.to_string());
        self.toolconfig_editor = Some(editor);
        self.toolconfig_open = true;
        self.toolconfig_status = String::new();
        cx.notify();
    }

    /// 保存工具配置文件。
    fn save_toolconfig(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.toolconfig_id.clone() else {
            return;
        };
        let content = self
            .toolconfig_editor
            .as_ref()
            .map(|e| e.read(cx).text())
            .unwrap_or_default();
        match crate::toolconfig::write_file(&id, &content) {
            Ok(()) => {
                self.toolconfig_status = format!("已保存 {id}");
                self.status = format!("已保存配置文件 {id}");
            }
            Err(e) => {
                self.toolconfig_status = format!("保存失败: {e}");
            }
        }
        cx.notify();
    }

    /// 打开每日记忆编辑器（预填内容；不存在则空白）。
    fn open_daily_memory(&mut self, filename: &str, cx: &mut Context<Self>) {
        if !crate::toolconfig::is_daily_memory_filename(filename) {
            self.status = format!("无效文件名: {filename}");
            self.status_tone = StatusTone::Error;
            cx.notify();
            return;
        }
        let content = match crate::toolconfig::read_daily_memory(filename) {
            Ok(Some(c)) => c,
            Ok(None) => String::new(),
            Err(e) => {
                self.status = format!("读取失败: {e}");
                self.status_tone = StatusTone::Error;
                cx.notify();
                return;
            }
        };
        let editor = cx.new(|cx| {
            let mut ta = crate::text_area::TextArea::new("", cx);
            ta.set_text(&content);
            ta
        });
        self.daily_memory_editing = Some(filename.to_string());
        self.daily_memory_editor = Some(editor);
        self.daily_memory_status = String::new();
        cx.notify();
    }

    /// 保存每日记忆文件。
    fn save_daily_memory(&mut self, cx: &mut Context<Self>) {
        let Some(filename) = self.daily_memory_editing.clone() else {
            return;
        };
        let content = self
            .daily_memory_editor
            .as_ref()
            .map(|e| e.read(cx).text())
            .unwrap_or_default();
        match crate::toolconfig::write_daily_memory(&filename, &content) {
            Ok(()) => {
                self.daily_memory_status = format!("已保存 {filename}");
                self.status = format!("已保存每日记忆 {filename}");
                self.status_tone = StatusTone::Success;
                self.daily_memory_files = crate::toolconfig::list_daily_memory();
            }
            Err(e) => {
                self.daily_memory_status = format!("保存失败: {e}");
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    /// 渲染每日记忆编辑器浮层。
    fn render_daily_memory_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(filename) = self.daily_memory_editing.clone() else {
            return div().into_any_element();
        };
        let editor = self
            .daily_memory_editor
            .clone()
            .unwrap_or_else(|| cx.new(|inner| crate::text_area::TextArea::new("", inner)));
        div()
            .id("daily-memory-editor-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x000000, 0.5))
            .child(
                div()
                    .id("daily-memory-editor-panel")
                    .w(px(640.))
                    .max_h(px(520.))
                    .flex_col()
                    .rounded(px(12.))
                    .bg(theme::bg_1())
                    .border_1()
                    .border_color(theme::stroke_a(0.2))
                    .p(px(16.))
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child(format!("每日记忆 · {filename}")),
                            )
                            .child(
                                widgets::ghost_button("dm-editor-close", "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.daily_memory_editing = None;
                                        this.daily_memory_editor = None;
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(280.))
                            .rounded(px(8.))
                            .bg(theme::alpha(0x000000, 0.25))
                            .border_1()
                            .border_color(theme::stroke_a(0.14))
                            .child(editor),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(if self.daily_memory_status.is_empty() {
                                        theme::TEXT_FAINT
                                    } else {
                                        theme::ACCENT
                                    }))
                                    .child(if self.daily_memory_status.is_empty() {
                                        "Markdown 文本（保存即写盘）".to_string()
                                    } else {
                                        self.daily_memory_status.clone()
                                    }),
                            )
                            .child(
                                widgets::ghost_button("dm-editor-save", "保存").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.save_daily_memory(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// 渲染每日记忆删除确认浮层。
    fn render_daily_memory_confirm_delete(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(filename) = self.daily_memory_confirm_delete.clone() else {
            return div().into_any_element();
        };
        div()
            .id("dm-delete-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x000000, 0.55))
            .child(
                div()
                    .w(px(360.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::danger_a(0.4))
                    .shadow_lg()
                    .p(px(18.))
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::danger())
                            .child("删除每日记忆"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text())
                            .child(format!("确定删除「{filename}」？此操作不可撤销。")),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("dm-del-cancel", "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.daily_memory_confirm_delete = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::danger_button("dm-del-confirm", "确认删除")
                                    .on_click(cx.listener(move |this, _, _, _cx| {
                                        let result =
                                            crate::toolconfig::delete_daily_memory(&filename);
                                        this.daily_memory_confirm_delete = None;
                                        let ok = result.is_ok();
                                        this.status = match result {
                                            Ok(()) => format!("已删除每日记忆 {filename}"),
                                            Err(e) => format!("删除失败: {e}"),
                                        };
                                        this.status_tone = if ok {
                                            StatusTone::Success
                                        } else {
                                            StatusTone::Error
                                        };
                                        if ok {
                                            this.daily_memory_files =
                                                crate::toolconfig::list_daily_memory();
                                        }
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// 打开 OpenClaw `agents.defaults` 结构化编辑器（预填当前值）。
    fn open_openclaw_defaults(&mut self, cx: &mut Context<Self>) {
        self.oc_defaults = crate::toolconfig::OpenClawAgentsDefaults::from_openclaw_json();
        let d = self.oc_defaults.clone();
        let primary = d.model.as_ref().map(|m| m.primary.clone()).unwrap_or_default();
        if let Some(e) = self.oc_defaults_inputs.primary.as_ref() {
            e.update(cx, |f, _| f.set_text(&primary));
        }
        if let Some(e) = self.oc_defaults_inputs.workspace.as_ref() {
            e.update(cx, |f, _| f.set_text(d.workspace.as_deref().unwrap_or_default()));
        }
        if let Some(e) = self.oc_defaults_inputs.timeout.as_ref() {
            e.update(cx, |f, _| f.set_text(&d.timeout_seconds.map(|v| v.to_string()).unwrap_or_default()));
        }
        if let Some(e) = self.oc_defaults_inputs.context_tokens.as_ref() {
            e.update(cx, |f, _| f.set_text(&d.context_tokens.map(|v| v.to_string()).unwrap_or_default()));
        }
        if let Some(e) = self.oc_defaults_inputs.max_concurrent.as_ref() {
            e.update(cx, |f, _| f.set_text(&d.max_concurrent.map(|v| v.to_string()).unwrap_or_default()));
        }
        self.openclaw_defaults_open = true;
        cx.notify();
    }

    /// 保存 OpenClaw `agents.defaults`。
    fn save_openclaw_defaults(&mut self, cx: &mut Context<Self>) {
        let text = |e: &Option<gpui::Entity<crate::text_field::TextField>>| {
            e.as_ref()
                .map(|x| x.read(cx).text().trim().to_string())
                .unwrap_or_default()
        };
        let primary = text(&self.oc_defaults_inputs.primary);
        let workspace = text(&self.oc_defaults_inputs.workspace);
        let timeout = text(&self.oc_defaults_inputs.timeout);
        let ctx = text(&self.oc_defaults_inputs.context_tokens);
        let maxc = text(&self.oc_defaults_inputs.max_concurrent);
        let parse_u64 = |s: &str| -> Option<u64> {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<u64>().ok()
            }
        };
        // 从当前 oc_defaults 继承未知字段（extra）。
        let mut d = self.oc_defaults.clone();
        d.model = if primary.is_empty() {
            None
        } else {
            Some(crate::toolconfig::OpenClawDefaultModel {
                primary,
                fallbacks: d.model.as_ref().and_then(|m| m.fallbacks.clone()),
            })
        };
        d.workspace = if workspace.is_empty() { None } else { Some(workspace) };
        d.timeout_seconds = parse_u64(&timeout);
        d.context_tokens = parse_u64(&ctx);
        d.max_concurrent = parse_u64(&maxc);
        match d.save_to_openclaw_json() {
            Ok(()) => {
                self.oc_defaults_status = "已保存 agents.defaults".to_string();
                self.status = "已保存 OpenClaw agents.defaults".to_string();
                self.status_tone = StatusTone::Success;
                self.oc_defaults = d;
            }
            Err(e) => {
                self.oc_defaults_status = format!("保存失败: {e}");
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    /// 渲染 OpenClaw `agents.defaults` 结构化编辑器浮层。
    fn render_openclaw_defaults_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.openclaw_defaults_open {
            return div().into_any_element();
        }
        let exists = crate::toolconfig::openclaw_agents_defaults_exists();
        let field = |label: String, e: gpui::Entity<crate::text_field::TextField>| {
            div()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::text_dim())
                        .child(label),
                )
                .child(e)
        };
        let primary = self.oc_defaults_inputs.primary.clone().unwrap_or_else(|| cx.new(|cx| crate::text_field::TextField::new("", cx)));
        let workspace = self.oc_defaults_inputs.workspace.clone().unwrap_or_else(|| cx.new(|cx| crate::text_field::TextField::new("", cx)));
        let timeout = self.oc_defaults_inputs.timeout.clone().unwrap_or_else(|| cx.new(|cx| crate::text_field::TextField::new("", cx)));
        let context_tokens = self.oc_defaults_inputs.context_tokens.clone().unwrap_or_else(|| cx.new(|cx| crate::text_field::TextField::new("", cx)));
        let max_concurrent = self.oc_defaults_inputs.max_concurrent.clone().unwrap_or_else(|| cx.new(|cx| crate::text_field::TextField::new("", cx)));
        div()
            .id("oc-defaults-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x000000, 0.5))
            .child(
                div()
                    .id("oc-defaults-panel")
                    .w(px(520.))
                    .max_h(px(560.))
                    .flex_col()
                    .rounded(px(12.))
                    .bg(theme::bg_1())
                    .border_1()
                    .border_color(theme::stroke_a(0.2))
                    .p(px(16.))
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(theme::text())
                                            .child("OpenClaw 代理默认值"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(theme::text_faint())
                                            .child(if exists {
                                                "openclaw.json · agents.defaults（已有配置，保存覆盖已知字段、保留未知字段）"
                                            } else {
                                                "openclaw.json · agents.defaults（保存即创建）"
                                            }),
                                    ),
                            )
                            .child(
                                widgets::ghost_button("oc-defaults-close", "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.openclaw_defaults_open = false;
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(10.))
                                    .child(field("主模型（model.primary）".to_string(), primary))
                                    .child(field("工作区（workspace）".to_string(), workspace))
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(10.))
                                            .child(
                                                field("超时（timeoutSeconds）".to_string(), timeout).flex_1(),
                                            )
                                            .child(
                                                field(
                                                    "上下文 token（contextTokens）".to_string(),
                                                    context_tokens,
                                                )
                                                .flex_1(),
                                            ),
                                    )
                                    .child(field("最大并发（maxConcurrent）".to_string(), max_concurrent)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(if self.oc_defaults_status.is_empty() {
                                        theme::TEXT_FAINT
                                    } else {
                                        theme::ACCENT
                                    }))
                                    .child(if self.oc_defaults_status.is_empty() {
                                        "保存写回 openclaw.json（保留未知字段）".to_string()
                                    } else {
                                        self.oc_defaults_status.clone()
                                    }),
                            )
                            .child(
                                widgets::primary_button("oc-defaults-save", "保存").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.save_openclaw_defaults(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// 渲染工具配置编辑器浮层。

    /// 渲染工具配置编辑器浮层。

    fn render_toolconfig_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(id) = self.toolconfig_id.clone() else {
            return div().into_any_element();
        };
        let label = crate::toolconfig::file_by_id(&id)
            .map(|f| f.label.to_string())
            .unwrap_or_default();
        let editor = self
            .toolconfig_editor
            .clone()
            .unwrap_or_else(|| cx.new(|inner| crate::text_area::TextArea::new("", inner)));
        div()
            .id("toolconfig-editor-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x000000, 0.5))
            .child(
                div()
                    .id("toolconfig-editor-panel")
                    .w(px(640.))
                    .max_h(px(520.))
                    .flex_col()
                    .rounded(px(12.))
                    .bg(theme::bg_1())
                    .border_1()
                    .border_color(theme::stroke_a(0.2))
                    .p(px(16.))
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child(format!("编辑 · {label}")),
                            )
                            .child(
                                widgets::ghost_button("toolcfg-editor-close", "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.toolconfig_open = false;
                                        this.toolconfig_editor = None;
                                        this.toolconfig_id = None;
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(280.))
                            .rounded(px(8.))
                            .bg(theme::alpha(0x000000, 0.25))
                            .border_1()
                            .border_color(theme::stroke_a(0.14))
                            .child(editor),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(if self.toolconfig_status.is_empty() {
                                        theme::TEXT_FAINT
                                    } else {
                                        theme::ACCENT
                                    }))
                                    .child(if self.toolconfig_status.is_empty() {
                                        "Markdown 文本（保存即写盘）".to_string()
                                    } else {
                                        self.toolconfig_status.clone()
                                    }),
                            )
                            .child(
                                widgets::ghost_button("toolcfg-editor-save", "保存").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.save_toolconfig(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// 渲染 M12 设置页：自动更新、深链导入、版本。
    fn render_settings_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = crate::i18n::Lang::parse(
            &self
                .store
                .read()
                .map(|s| s.config().settings.language.clone())
                .unwrap_or_else(|_| "zh-cn".into()),
        );
        let status = self.update_status.clone();
        let backups = self.store.read().map(|s| s.list_backups()).unwrap_or_default();
        let manual_backups = self
            .store
            .read()
            .map(|s| s.list_manual_backups())
            .unwrap_or_default();
        // 环境冲突告警：路由模式下多工具共用同一监听端口。
        let conflicts: Vec<String> = {
            let s = self.store.read().expect("store lock");
            let tools = [
                ns_presets::Tool::Claude,
                ns_presets::Tool::Codex,
                ns_presets::Tool::Gemini,
                ns_presets::Tool::GrokBuild,
            ];
            let mut by_port: std::collections::BTreeMap<u16, Vec<&str>> =
                std::collections::BTreeMap::new();
            for t in tools {
                let cfg = s.config().settings.routing_for(t);
                if cfg.enabled && cfg.port != 0 {
                    by_port.entry(cfg.port).or_default().push(t.id_str());
                }
            }
            by_port
                .into_iter()
                .filter(|(_, ts)| ts.len() > 1)
                .map(|(port, ts)| format!("端口 {port} 被 {} 共用", ts.join("、")))
                .collect()
        };
        div()
            .id("settings-scroll")
            .flex_1()
            .h_full()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child(crate::i18n::t(lang, "settings.title")),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child("自动更新、深链导入"),
                    ),
            )
            .child(self.render_settings_section_tabs(cx))
            .when(
                self.settings_section == SettingsSection::AppConfig,
                |el| el.child(self.render_settings_appconfig(cx)),
            )
            .when(
                self.settings_section == SettingsSection::Routing,
                |el| el.child(self.render_settings_routing(cx)),
            )
            .when(
                self.settings_section == SettingsSection::Network,
                |el| el.child(self.render_settings_network(cx)),
            )
            .when(self.settings_section == SettingsSection::General, |el| {
                el.child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.3))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child(crate::i18n::t(lang, "settings.theme")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .children(
                                ["dark", "light", "system"].into_iter().map(|mode| {
                                    let cur = self
                                        .store
                                        .read()
                                        .map(|s| s.config().settings.theme.clone())
                                        .unwrap_or_else(|_| "dark".into());
                                    let label = match mode {
                                        "light" => crate::i18n::t(lang, "theme.light"),
                                        "system" => crate::i18n::t(lang, "theme.system"),
                                        _ => crate::i18n::t(lang, "theme.dark"),
                                    };
                                    widgets::ghost_button(
                                        gpui::ElementId::from(gpui::SharedString::from(
                                            format!("theme-{mode}"),
                                        )),
                                        label,
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let _ = {
                                            let mut s =
                                                this.store.write().expect("store lock");
                                            s.set_theme(mode)
                                        };
                                        ns_ui::theme::set_active_mode(
                                            ns_ui::theme::ThemeMode::parse(mode),
                                        );
                                        this.status =
                                            format!("主题已切换为 {label}");
                                        cx.notify();
                                    }))
                                    .map(|b| {
                                        if cur == mode {
                                            b.bg(theme::accent_a(0.18))
                                                .border_1()
                                                .border_color(theme::accent_a(0.5))
                                        } else {
                                            b
                                        }
                                    })
                                }),
                            ),
                    ),
                )
            })
            .when(self.settings_section == SettingsSection::General, |el| {
                el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.5))
                    .border_1()
                    .border_color(theme::stroke_a(0.1))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child(if crate::autostart::is_enabled() {
                                crate::i18n::t(lang, "settings.autostart.on")
                            } else {
                                crate::i18n::t(lang, "settings.autostart.off")
                            }),
                    )
                    .child(
                        widgets::ghost_button(
                            "autostart-toggle",
                            if crate::autostart::is_enabled() {
                                crate::i18n::t(lang, "settings.autostart.disable")
                            } else {
                                crate::i18n::t(lang, "settings.autostart.enable")
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = if crate::autostart::is_enabled() {
                                crate::autostart::disable()
                            } else {
                                crate::autostart::enable()
                            };
                            this.status = match result {
                                Ok(()) => {
                                    if crate::autostart::is_enabled() {
                                        "已开启开机自启".into()
                                    } else {
                                        "已关闭开机自启".into()
                                    }
                                }
                                Err(e) => format!("自启失败: {e}"),
                            };
                            cx.notify();
                        })),
                    ),
                )
            })
            .when(self.settings_section == SettingsSection::General, |el| {
                el.child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.3))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child(crate::i18n::t(lang, "settings.language")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .children(
                                ["zh-cn", "zh-tw", "en", "ja"]
                                    .into_iter()
                                    .map(|code| {
                                        let cur = self
                                            .store
                                            .read()
                                            .map(|s| s.config().settings.language.clone())
                                            .unwrap_or_else(|_| "zh-cn".into());
                                        let lang = crate::i18n::Lang::parse(code);
                                        let label = lang.label();
                                        widgets::ghost_button(
                                            gpui::ElementId::from(gpui::SharedString::from(
                                                format!("lang-{code}"),
                                            )),
                                            label,
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            let _ = {
                                                let mut s = this
                                                    .store
                                                    .write()
                                                    .expect("store lock");
                                                s.set_language(code)
                                            };
                                            this.status =
                                                format!("语言已切换为 {label}");
                                            cx.notify();
                                        }))
                                        .map(|b| {
                                            if cur == code {
                                                b.bg(theme::accent_a(0.18))
                                                    .border_1()
                                                    .border_color(theme::alpha(
                                                        theme::ACCENT,
                                                        0.5,
                                                    ))
                                            } else {
                                                b
                                            }
                                        })
                                    }),
                            ),
                    ),
                )
            })
            .when(self.settings_section == SettingsSection::General, |el| {
                el.child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.3))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child("小工具"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_dim())
                                    .child("跳过初次确认（CLAUDE_CODE_SKIP_VERIFY_AUTH）"),
                            )
                            .child(
                                widgets::ghost_button(
                                    "gadget-skip",
                                    if crate::gadgets::skip_verify_enabled() {
                                        "开启"
                                    } else {
                                        "关闭"
                                    },
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let next = !crate::gadgets::skip_verify_enabled();
                                    this.status = match crate::gadgets::set_skip_verify(next) {
                                        Ok(()) => format!("跳过初次确认：{}", if next { "开启" } else { "关闭" }),
                                        Err(e) => format!("写入失败: {e}"),
                                    };
                                    cx.notify();
                                })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_dim())
                                    .child("隐藏署名（CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC）"),
                            )
                            .child(
                                widgets::ghost_button(
                                    "gadget-signature",
                                    if crate::gadgets::hide_signature_enabled() {
                                        "开启"
                                    } else {
                                        "关闭"
                                    },
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let next = !crate::gadgets::hide_signature_enabled();
                                    this.status = match crate::gadgets::set_hide_signature(next) {
                                        Ok(()) => format!("隐藏署名：{}", if next { "开启" } else { "关闭" }),
                                        Err(e) => format!("写入失败: {e}"),
                                    };
                                    cx.notify();
                                })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_dim())
                                    .child("VS Code 联动（code CLI）"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(if crate::gadgets::vscode_available() {
                                        theme::JADE
                                    } else {
                                        theme::TEXT_FAINT
                                    }))
                                    .child(if crate::gadgets::vscode_available() {
                                        "可用"
                                    } else {
                                        "未安装"
                                    }),
                            ),
                    ),
                )
            })
            .when(self.settings_section == SettingsSection::General, |el| {
                el.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child("顶部项目切换器"),
                    )
                    .child(
                        widgets::ghost_button(
                            "gadget-project-switcher",
                            if self
                                .store
                                .read()
                                .map(|s| s.config().settings.project_switcher)
                                .unwrap_or(false)
                            {
                                "开启"
                            } else {
                                "关闭"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            let next = !this
                                .store
                                .read()
                                .map(|s| s.config().settings.project_switcher)
                                .unwrap_or(false);
                            let r = {
                                let mut s = this.store.write().expect("store lock");
                                s.set_project_switcher(next)
                            };
                            this.status = match r {
                                Ok(()) => format!(
                                    "顶部项目切换器：{}",
                                    if next { "开启" } else { "关闭" }
                                ),
                                Err(e) => format!("写入失败: {e}"),
                            };
                            cx.notify();
                        })),
                    ),
                )
            })
            .when(self.settings_section == SettingsSection::General, |el| {
                el.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_dim())
                                    .child("共享 Skills 目录"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(theme::text_faint())
                                    .child("所有工具共用 ~/.agents/skills"),
                            ),
                    )
                    .child(
                        widgets::ghost_button(
                            "gadget-shared-skills",
                            if self
                                .store
                                .read()
                                .map(|s| s.config().settings.shared_skills)
                                .unwrap_or(false)
                            {
                                "开启"
                            } else {
                                "关闭"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            let next = !this
                                .store
                                .read()
                                .map(|s| s.config().settings.shared_skills)
                                .unwrap_or(false);
                            let r = {
                                let mut s = this.store.write().expect("store lock");
                                s.set_shared_skills(next)
                            };
                            this.status = match r {
                                Ok(()) => format!(
                                    "共享 Skills 目录：{}",
                                    if next { "开启" } else { "关闭" }
                                ),
                                Err(e) => format!("写入失败: {e}"),
                            };
                            this.skills.shared = next;
                            cx.notify();
                        })),
                    ),
                )
            })
            .when(self.settings_section == SettingsSection::General, |el| {
                el.child(
                div()
                    .flex_col()
                    .gap(px(8.))
                    .child(widgets::section_label("侧栏工具"))
                    .children({
                        let store = self.store.read().expect("store lock");
                        let items: Vec<(Tool, bool)> = store
                            .tool_order()
                            .into_iter()
                            .map(|t| (t, store.tool_visible(t)))
                            .collect();
                        items.into_iter().enumerate().map(|(i, (tool, visible))| {
                            div()
                                .id(("app-vis-row", tool as usize))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .p(px(8.))
                                .rounded(px(6.))
                                .bg(theme::panel_a(0.4))
                                .child(
                                    div()
                                        .w(px(18.))
                                        .text_size(px(12.))
                                        .text_color(theme::text_faint())
                                        .child(tool.glyph()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(theme::text())
                                        .child(tool.label()),
                                )
                                .child(
                                    widgets::ghost_button(
                                        ("app-vis-toggle", tool as usize),
                                        if visible { "隐藏" } else { "显示" },
                                    )
                                    .on_click({
                                        let t = tool;
                                        cx.listener(move |this, _, _, cx| {
                                            let cur = this
                                                .store
                                                .read()
                                                .map(|s| s.tool_visible(t))
                                                .unwrap_or(true);
                                            let r = {
                                                let mut s =
                                                    this.store.write().expect("store lock");
                                                s.set_tool_visible(t, !cur)
                                            };
                                            this.status = match r {
                                                Ok(()) => format!(
                                                    "{} 已{}",
                                                    t.label(),
                                                    if !cur { "显示" } else { "隐藏" }
                                                ),
                                                Err(e) => format!("写入失败: {e}"),
                                            };
                                            cx.notify();
                                        })
                                    }),
                                )
                                .child(
                                    widgets::ghost_button(
                                        ("app-vis-up", tool as usize),
                                        "↑",
                                    )
                                    .on_click({
                                        let t = tool;
                                        cx.listener(move |this, _, _, cx| {
                                            let r = {
                                                let mut s =
                                                    this.store.write().expect("store lock");
                                                s.move_tool(t, i as isize - 1)
                                            };
                                            this.status = match r {
                                                Ok(()) => "顺序已更新".into(),
                                                Err(e) => format!("写入失败: {e}"),
                                            };
                                            cx.notify();
                                        })
                                    }),
                                )
                                .child(
                                    widgets::ghost_button(
                                        ("app-vis-down", tool as usize),
                                        "↓",
                                    )
                                    .on_click({
                                        let t = tool;
                                        cx.listener(move |this, _, _, cx| {
                                            let r = {
                                                let mut s =
                                                    this.store.write().expect("store lock");
                                                s.move_tool(t, i as isize + 2)
                                            };
                                            this.status = match r {
                                                Ok(()) => "顺序已更新".into(),
                                                Err(e) => format!("写入失败: {e}"),
                                            };
                                            cx.notify();
                                        })
                                    }),
                                )
                        })
                    }),
                )
            })
            .when(self.settings_section == SettingsSection::About, |el| {
                el.child(
                 div()
                     .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.5))
                    .border_1()
                    .border_color(theme::stroke_a(0.1))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child(format!(
                                "版本 {}",
                                crate::update::CURRENT_VERSION
                            )),
                    )
                    .child(
                        widgets::ghost_button("update-check", "检查更新").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.update_status = Some(Ok("检查中…".into()));
                                cx.notify();
                                cx.spawn(async move |this, cx| {
                                    let result = crate::update::check_update();
                                    this.update(cx, |this, cx| {
                                        this.update_status = Some(match result {
                                            Some(v) => Ok(format!("发现新版本 {v}")),
                                            None => Ok("已是最新".into()),
                                        });
                                        cx.notify();
                                    })
                                    .ok();
                                })
                                .detach();
                            }),
                        ),
                    )
                    .child(
                        widgets::ghost_button("update-download", "下载并安装").on_click(
                            cx.listener(|this, _, _, cx| {
                                let tag = this
                                    .update_status
                                    .as_ref()
                                    .and_then(|r| r.as_ref().ok())
                                    .and_then(|s| {
                                        s.strip_prefix("发现新版本 ")
                                            .map(|v| v.to_string())
                                    });
                                let Some(tag) = tag else {
                                    this.update_status = Some(Err("请先检查更新".into()));
                                    cx.notify();
                                    return;
                                };
                                this.update_status =
                                    Some(Ok(format!("正在下载 {tag} …")));
                                cx.notify();
                                cx.spawn(async move |this, cx| {
                                    let r = crate::update::download_and_install(&tag);
                                    this.update(cx, |this, cx| {
                                        this.update_status = Some(match r {
                                            Ok(m) => Ok(m),
                                            Err(e) => Err(e),
                                        });
                                        cx.notify();
                                    })
                                    .ok();
                                })
                                .detach();
                            }),
                        ),
                    )
                )
            })
            .when(self.settings_section == SettingsSection::Data, |el| {
                el.child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.3))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("深链导入"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child(
                                "nsswitch://import-provider?preset=claude-88api&endpoint=...&key=...&model=...",
                            ),
                    ),
                )
            })
            .when(self.settings_section == SettingsSection::Data, |el| {
                el.child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.3))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child("备份与环境"),
                            )
                            .child(
                                widgets::ghost_button("backup-now", "立即备份").on_click(
                                    cx.listener(|this, _, _, cx| this.create_manual_backup(cx)),
                                ),
                            )
                            .child(
                                widgets::ghost_button("backup-open-dir", "打开备份目录").on_click(
                                    cx.listener(|this, _, _, cx| this.open_backups_folder(cx)),
                                ),
                            ),
                    )
                    .child(if conflicts.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("无环境冲突")
                    } else {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::danger())
                            .child(conflicts.join("；"))
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child(format!("首写备份（{}）", backups.len())),
                    )
                    .child(if backups.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("无首写备份（首次切换供应商时自动生成）")
                    } else {
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .children(backups.into_iter().enumerate().map(|(i, b)| {
                                let bp = b.path.clone();
                                let label = b.name.clone();
                                div()
                                    .id(("backup-row", i))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(11.))
                                            .text_color(theme::text_dim())
                                            .child(format!("{} · {} B · {}", b.name, b.size, b.source)),
                                    )
                                    .child(
                                        widgets::ghost_button(("backup-restore", i), "恢复").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.backup_restore_pending = Some((
                                                    BackupRestoreKind::Live,
                                                    bp.clone(),
                                                    label.clone(),
                                                ));
                                                cx.notify();
                                            }),
                                        ),
                                    )
                            }))
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child(format!("手动备份（{}）", manual_backups.len())),
                    )
                    .child(if manual_backups.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("暂无手动备份（点「立即备份」保存当前配置）")
                    } else {
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .children(manual_backups.into_iter().enumerate().map(|(i, b)| {
                                let bp = b.path.clone();
                                let label = b.name.clone();
                                div()
                                    .id(("manual-backup-row", i))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(11.))
                                            .text_color(theme::text_dim())
                                            .child(format!("{} · {} B", b.name, b.size)),
                                    )
                                    .child(
                                        widgets::ghost_button(("manual-backup-restore", i), "恢复").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.backup_restore_pending = Some((
                                                    BackupRestoreKind::Manual,
                                                    bp.clone(),
                                                    label.clone(),
                                                ));
                                                cx.notify();
                                            }),
                                        ),
                                    )
                            }))
                    })
                    .when(!self.backup_status.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::accent())
                                .child(self.backup_status.clone()),
                        )
                    })
                )
            })
            .when(self.settings_section == SettingsSection::Data, |el| {
                el.child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.3))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("云同步（WebDAV / S3）"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                widgets::pill(
                                    "WebDAV",
                                    if self.sync_provider != "s3" { theme::jade() } else { theme::text_faint() },
                                ).id("sync-provider-webdav").cursor_pointer().on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.sync_provider = "webdav".into();
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::pill(
                                    "S3",
                                    if self.sync_provider == "s3" { theme::jade() } else { theme::text_faint() },
                                ).id("sync-provider-s3").cursor_pointer().on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.sync_provider = "s3".into();
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                    .child(div().w_full().child(self.sync_url.clone()))
                    .child(div().w_full().child(self.sync_user.clone()))
                    .child(div().w_full().child(self.sync_pass.clone()))
                    .when(self.sync_provider == "s3", |el| {
                        el.child(div().w_full().child(self.sync_bucket.clone()))
                    })
                    .child(div().w_full().child(self.sync_interval.clone()))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("sync-save", "保存配置").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        let url = this.sync_url.read(cx).text();
                                        let user = this.sync_user.read(cx).text();
                                        let pass = this.sync_pass.read(cx).text();
                                        let bucket = this.sync_bucket.read(cx).text();
                                        let interval = this
                                            .sync_interval
                                            .read(cx)
                                            .text()
                                            .trim()
                                            .parse::<u32>()
                                            .unwrap_or(0);
                                        let enabled = !url.is_empty();
                                        let provider = this.sync_provider.clone();
                                        let ok = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.set_sync_config(ns_store::SyncConfig {
                                                provider,
                                                url,
                                                username: user,
                                                password: pass,
                                                bucket,
                                                enabled,
                                                auto_sync_minutes: interval,
                                            })
                                            .is_ok()
                                        };
                                        this.sync_status = if ok {
                                            "已保存云同步配置".into()
                                        } else {
                                            "保存失败".into()
                                        };
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::ghost_button("sync-push", "推送").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        let r = {
                                            let s = this.store.read().expect("store lock");
                                            s.sync_push()
                                        };
                                        this.sync_status = match r {
                                            Ok(()) => "已推送 config.json".into(),
                                            Err(e) => format!("推送失败: {e}"),
                                        };
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::ghost_button("sync-pull", "拉取").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        let r = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.sync_pull()
                                        };
                                        this.sync_status = match r {
                                            Ok(()) => "已拉取并应用 config.json".into(),
                                            Err(e) => format!("拉取失败: {e}"),
                                        };
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_dim())
                            .child(self.sync_status.clone()),
                    ),
                )
            })
            .when(self.settings_section == SettingsSection::Data, |el| {
                let pricing = self
                    .store
                    .read()
                    .map(|s| s.config().settings.custom_pricing.clone())
                    .unwrap_or_default();
                el.child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.3))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("自定义定价"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("覆盖内置价格表（USD/1M tokens）。用量页按此计算花费。"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(div().flex_1().child(self.price_model.clone()))
                            .child(div().w(px(110.)).child(self.price_input.clone()))
                            .child(div().w(px(110.)).child(self.price_output.clone()))
                            .child(div().w(px(130.)).child(self.price_cache.clone()))
                            .child(
                                widgets::primary_button("price-add", "添加").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.add_custom_price(cx);
                                    }),
                                ),
                            )
                            .child(
                                widgets::ghost_button("price-import-models", "导入 models.dev")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_models_dev_pricing(cx);
                                    })),
                            ),
                    )
                    .child(if pricing.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("无自定义定价")
                    } else {
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .children(pricing.into_iter().enumerate().map(|(i, (k, v))| {
                                let key = k.clone();
                                let desc = format!(
                                    "{}：in {} / out {} / cache {}",
                                    k,
                                    v.get("input_per_mtok").and_then(|x| x.as_f64()).map(|x| format!("{x}")).unwrap_or_else(|| "?".into()),
                                    v.get("output_per_mtok").and_then(|x| x.as_f64()).map(|x| format!("{x}")).unwrap_or_else(|| "?".into()),
                                    v.get("cache_read_per_mtok").and_then(|x| x.as_f64()).map(|x| format!("{x}")).unwrap_or_else(|| "—".into()),
                                );
                                div()
                                    .id(("price-row", i))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(11.))
                                            .text_color(theme::text_dim())
                                            .child(desc),
                                    )
                                    .child(
                                        widgets::ghost_button(("price-del", i), "删除").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                let ok = {
                                                    let mut s = this.store.write().expect("store lock");
                                                    s.remove_custom_price(&key).is_ok()
                                                };
                                                this.pricing_status = if ok {
                                                    "已删除自定义定价".into()
                                                } else {
                                                    "删除失败".into()
                                                };
                                                cx.notify();
                                            }),
                                        ),
                                    )
                            }))
                    })
                    .child(if !self.pricing_status.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_dim())
                            .child(self.pricing_status.clone())
                    } else {
                        div()
                    })
                )
            })
            .child(if let Some(res) = status {
                let (color, text) = match res {
                    Ok(t) => (theme::ACCENT, t),
                    Err(e) => (theme::DANGER, e),
                };
                div()
                    .text_size(px(12.))
                    .text_color(rgb(color))
                    .child(text)
            } else {
                div()
            })
    }

    /// 设置页分区导航（对齐 cc-switch SettingsDirectory）。
    fn render_settings_section_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .children(SettingsSection::ALL.into_iter().enumerate().map(|(i, section)| {
                let selected = self.settings_section == section;
                div()
                    .id(("settings-section", i))
                    .px(px(12.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .bg(if selected {
                        theme::accent_a(0.18)
                    } else {
                        theme::surface_a(0.4)
                    })
                    .border_1()
                    .border_color(if selected {
                        theme::accent_a(0.5)
                    } else {
                        theme::stroke_a(0.1)
                    })
                    .text_color(if selected {
                        theme::accent()
                    } else {
                        theme::text_dim()
                    })
                    .text_size(px(12.))
                    .child(section.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings_section = section;
                        cx.notify();
                    }))
            }))
    }

    /// 应用配置分区：每个工具的配置目录与关键能力概览。
    fn render_settings_appconfig(&self, cx: &mut Context<Self>) -> impl IntoElement {

        div()
            .flex_col()
            .gap(px(8.))
            .child(if self.config_dir_status.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(theme::text_faint())
                    .child("配置目录可自定义（覆盖默认 ~/.<tool>；留空恢复默认）")
                    .into_any_element()
            } else {
                div()
                    .text_size(px(11.))
                    .text_color(rgb(if self.config_dir_status.contains("失败") {
                        theme::DANGER
                    } else {
                        theme::ACCENT
                    }))
                    .child(self.config_dir_status.clone())
                    .into_any_element()
            })
            .children(Tool::ALL.iter().map(|&tool| {
                let dir = self.tool_config_dir(tool)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "—".to_string());
                let exists = self.tool_config_dir(tool)
                    .map(|p| p.exists())
                    .unwrap_or(false);
                div()
                    .id(("appcfg-row", tool as usize))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.5))
                    .border_1()
                    .border_color(theme::stroke_a(0.1))
                    .child(
                        div()
                            .w(px(24.))
                            .text_size(px(13.))
                            .text_color(theme::accent())
                            .child(tool.glyph()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(theme::text())
                                    .child(tool.label()),
                            )
                            .child(
                                div()
                                    .font_family(theme::MONO_FONT)
                                    .text_size(px(10.))
                                    .text_color(rgb(if exists {
                                        theme::JADE
                                    } else {
                                        theme::TEXT_FAINT
                                    }))
                                    .child(if exists {
                                        format!("{dir} · 已初始化")
                                    } else {
                                        format!("{dir} · 未初始化")
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child({
                                        // 配置目录覆盖输入（预填当前覆盖值，否则空）。
                                        let key = tool.id_str().to_string();
                                        let entity = self
                                            .config_dir_inputs
                                            .get(&key)
                                            .cloned()
                                            .unwrap_or_else(|| {
                                                let cur = self
                                                    .store
                                                    .read()
                                                    .ok()
                                                    .and_then(|s| {
                                                        s.config()
                                                            .settings
                                                            .config_dir_for(tool)
                                                            .map(str::to_string)
                                                    })
                                                    .unwrap_or_default();
                                                let e = cx.new(|cx| {
                                                    crate::text_field::TextField::new(
                                                        "自定义配置目录（留空=默认）",
                                                        cx,
                                                    )
                                                });
                                                e.update(cx, |f, _| f.set_text(&cur));
                                                e
                                            });
                                        entity
                                    })
                                    .child(
                                        widgets::ghost_button(
                                            gpui::ElementId::from(("appcfg-save", tool as usize)),
                                            "保存目录",
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.save_config_dir(tool, cx);
                                        })),
                                    )
                                    .child(
                                        widgets::ghost_button(
                                            gpui::ElementId::from(("appcfg-open", tool as usize)),
                                            "打开配置目录",
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            let dir = this.tool_config_dir(tool);
                                            if let Some(dir) = dir {
                                                #[cfg(target_os = "macos")]
                                                let _ = std::process::Command::new("open").arg(&dir).spawn();
                                                #[cfg(target_os = "linux")]
                                                let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                                                #[cfg(target_os = "windows")]
                                                let _ = std::process::Command::new("cmd")
                                                    .args(["/C", "start", "", &dir.display().to_string()])
                                                    .spawn();
                                                this.status = format!("已打开 {}", dir.display());
                                                cx.notify();
                                            }
                                        })),
                                    ),
                            ),
                    )
            }))
    }

    /// 路由分区：监听地址/端口汇总 + 每工具故障转移设置。
    fn render_settings_routing(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let routable = [Tool::Claude, Tool::Codex, Tool::Gemini, Tool::GrokBuild];
        let config = self.store.read().expect("store lock").config().clone();
        div()
            .flex_col()
            .gap(px(8.))
            .children(routable.into_iter().enumerate().map(|(i, tool)| {
                let rc = config.settings.routing_for(tool);
                let running = self.routing.is_running(tool);
                div()
                    .id(("settings-route-row", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.5))
                    .border_1()
                    .border_color(theme::stroke_a(0.1))
                    .child(
                        div()
                            .flex_1()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(theme::text())
                                    .child(tool.label()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(if running {
                                        theme::JADE
                                    } else {
                                        theme::TEXT_FAINT
                                    }))
                                    .child(if running {
                                        format!(
                                            "运行中 · {}:{} · 故障转移 {}",
                                            rc.listen_addr,
                                            rc.port,
                                            if rc.failover { "开启" } else { "关闭" }
                                        )
                                    } else {
                                        format!(
                                            "未运行 · 监听 {}:{}",
                                            rc.listen_addr, rc.port
                                        )
                                    }),
                            ),
                    )
                    .child(widgets::pill(
                        if rc.enabled { "路由中" } else { "直连" },
                        if rc.enabled {
                            theme::jade()
                        } else {
                            theme::text_dim()
                        },
                    ))
            }))
    }

    /// 网络分区：连接状态概览 + 全局代理占位。
    fn render_settings_network(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // 每工具路由连接状态（监听地址/端口/运行）。
        let tools: Vec<Tool> = Tool::ALL
            .iter()
            .copied()
            .filter(|t| t.supports_aggregation())
            .collect();
        div()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.5))
                    .border_1()
                    .border_color(theme::stroke_a(0.1))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child("全局出站代理"),
                    )
                    .child(match self.store.read().ok().and_then(|s| s.config().settings.global_proxy().map(|p| p.to_string())) {
                        Some(_) => widgets::pill("已配置", theme::jade()),
                        None => widgets::pill("未配置", theme::text_dim()),
                    }),
            )
            .child(div().w_full().child(self.global_proxy_url.clone()))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        widgets::ghost_button("proxy-save", "保存").on_click(
                            cx.listener(|this, _, _, cx| this.save_global_proxy(cx)),
                        ),
                    )
                    .child(
                        widgets::ghost_button("proxy-test", "测试连接").on_click(
                            cx.listener(|this, _, _, cx| this.test_global_proxy(cx)),
                        ),
                    )
                    .child(
                        widgets::ghost_button("proxy-scan", "扫描本地").on_click(
                            cx.listener(|this, _, _, cx| this.scan_proxies(cx)),
                        ),
                    ),
            )
            .when(!self.proxy_scan.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .children(self.proxy_scan.iter().cloned().enumerate().map(|(i, (url, _proto, _port))| {
                            div()
                                .id(("proxy-scan-item", i))
                                .cursor_pointer()
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(6.))
                                .bg(theme::surface_a(0.35))
                                .border_1()
                                .border_color(theme::stroke_a(0.12))
                                .text_size(px(11.))
                                .text_color(theme::text_dim())
                                .hover(|s| s.bg(theme::stroke_a(0.08)))
                                .child(url.clone())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.global_proxy_url.update(cx, |f, _cx| f.set_text(&url));
                                    cx.notify();
                                }))
                        }))
                )
            })
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::text_faint())
                    .child("ns-switch 自身出站请求（用量/GitHub/models.dev/更新检查）经此代理；本地路由代理（127.0.0.1）不受影响。"),
            )
            .when(!self.proxy_status.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(match self.status_tone {
                            StatusTone::Success => theme::jade(),
                            StatusTone::Error => theme::danger(),
                            StatusTone::Neutral => theme::text_dim(),
                        })
                        .child(self.proxy_status.clone()),
                )
            })
            .when(
                self.proxy_test.is_some(),
                |el| el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::text_dim())
                        .child(match &self.proxy_test {
                            Some((true, ms, _)) => format!("✓ 经代理可达（{ms}ms）"),
                            Some((false, _, Some(e))) => format!("✗ 不可达：{e}"),
                            _ => String::new(),
                        }),
                ),
            )
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child("连接状态"),
            )
            .children(tools.into_iter().enumerate().map(|(i, tool)| {
                let rc = self
                    .store
                    .read()
                    .map(|s| s.config().settings.routing_for(tool))
                    .unwrap_or_default();
                let running = self.routing.is_running(tool);
                let port_text = if running {
                    self.routing
                        .port_of(tool)
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| rc.port.to_string())
                } else {
                    rc.port.to_string()
                };
                div()
                    .id(("settings-net-row", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.35))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.5))
                            .text_color(theme::text())
                            .child(format!(
                                "{} · 监听 {}:{}",
                                tool.label(),
                                rc.listen_addr,
                                port_text
                            )),
                    )
                    .child(if running {
                        widgets::pill("运行中", theme::jade())
                    } else {
                        widgets::pill("未运行", theme::text_dim())
                    })
            }))
    }

    /// 渲染 M10 会话页：列表 / 搜索 / 分组 / 批量删除。
    fn render_sessions_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.session_search.read(cx).text();
        let filtered = ns_sessions::filter_sessions(&self.sessions, &query);
        let grouped = ns_sessions::group_by_project(&filtered);
        let total = filtered.len();
        let delete_confirm = self.session_delete.clone();
        div()
            .id("sessions-scroll")
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("会话管理"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_faint())
                                    .child(format!(
                                        "{total} 个会话 · {} · 按项目分组",
                                        match self.session_tool {
                                            ns_sessions::SessionTool::Claude => "Claude Code",
                                            ns_sessions::SessionTool::Codex => "Codex",
                                            ns_sessions::SessionTool::Gemini => "Gemini CLI",
                                            ns_sessions::SessionTool::OpenCode => "OpenCode",
                                            ns_sessions::SessionTool::Pi => "Pi",
                                            ns_sessions::SessionTool::Hermes => "Hermes",
                                            ns_sessions::SessionTool::OpenClaw => "OpenClaw",
                                        }
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .children(
                                [
                                    (ns_sessions::SessionTool::Claude, "Claude"),
                                    (ns_sessions::SessionTool::Codex, "Codex"),
                                    (ns_sessions::SessionTool::Gemini, "Gemini"),
                                    (ns_sessions::SessionTool::OpenCode, "OpenCode"),
                                    (ns_sessions::SessionTool::Pi, "Pi"),
                                    (ns_sessions::SessionTool::Hermes, "Hermes"),
                                    (ns_sessions::SessionTool::OpenClaw, "OpenClaw"),
                                ]
                                .into_iter()
                                .enumerate()
                                .map(|(i, (tool, label))| {
                                    let selected = self.session_tool == tool;
                                    let mut b = widgets::ghost_button(
                                        gpui::ElementId::from(("session-tool", i)),
                                        label,
                                    );
                                    if selected {
                                        b = b
                                            .bg(theme::accent_a(0.18))
                                            .border_1()
                                            .border_color(theme::accent_a(0.5));
                                    }
                                    b.on_click(cx.listener(move |this, _, _, cx| {
                                        this.session_tool = tool;
                                        this.session_viewing = None;
                                        this.load_sessions(cx);
                                    }))
                                }),
                            )
                            .child(widgets::ghost_button(("sessions-refresh", 0usize), "刷新").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.load_sessions(cx);
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .w(px(280.))
                    .child(self.session_search.clone()),
            )
            .child(if self.sessions_loading {
                div()
                    .text_size(px(12.))
                    .text_color(theme::text_dim())
                    .child("加载会话中…")
            } else if grouped.is_empty() {
                let icon = match self.session_tool {
                    ns_sessions::SessionTool::Claude => "☰",
                    ns_sessions::SessionTool::Codex => "⌬",
                    ns_sessions::SessionTool::Gemini => "✦",
                    ns_sessions::SessionTool::OpenCode => "⬡",
                    ns_sessions::SessionTool::Pi => "π",
                    ns_sessions::SessionTool::Hermes => "⚡",
                    ns_sessions::SessionTool::OpenClaw => "❖",
                };
                let tool_name = match self.session_tool {
                    ns_sessions::SessionTool::Claude => "Claude",
                    ns_sessions::SessionTool::Codex => "Codex",
                    ns_sessions::SessionTool::Gemini => "Gemini",
                    ns_sessions::SessionTool::OpenCode => "OpenCode",
                    ns_sessions::SessionTool::Pi => "Pi",
                    ns_sessions::SessionTool::Hermes => "Hermes",
                    ns_sessions::SessionTool::OpenClaw => "OpenClaw",
                };
                div()
                    .flex_col()
                    .items_center()
                    .gap(px(10.))
                    .p(px(24.))
                    .child(
                        div()
                            .size(px(48.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme::accent_a(0.08))
                            .border_1()
                            .border_color(theme::stroke_a(0.14))
                            .text_size(px(20.))
                            .text_color(theme::accent())
                            .child(icon),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(format!("还没有 {} 的会话", tool_name)),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child(match tool_name {
                                "Claude" => "点击「刷新」扫描 ~/.claude/projects 目录",
                                "Codex" => "点击「刷新」扫描 ~/.codex/sessions 目录",
                                "Gemini" => "点击「刷新」扫描 ~/.gemini/tmp/**/chats 目录",
                                "OpenCode" => "点击「刷新」扫描 ~/.local/share/opencode 数据库",
                                "Pi" => "点击「刷新」扫描 ~/.pi/agent/sessions 目录",
                                "Hermes" => "点击「刷新」扫描 ~/.hermes/state.db 数据库",
                                "OpenClaw" => "点击「刷新」调用 openclaw sessions 列出会话",
                                _ => "点击「刷新」扫描工具会话目录",
                            }),
                    )
            } else {
                div()
                    .flex_col()
                    .gap(px(10.))
                    .children(grouped.into_iter().enumerate().map(|(gi, (project, sess))| {
                        div()
                            .id(("session-group", gi))
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text_dim())
                                    .child(format!("{} · {} 个", project, sess.len())),
                            )
                            .children(sess.into_iter().enumerate().map(|(si, s)| {
                                let path = s.path.clone();
                                div()
                                    .id(("session-row", gi * 1000 + si))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .p(px(10.))
                                    .rounded(px(8.))
                                    .bg(theme::panel_a(0.5))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.1))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::stroke_a(0.05)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_session_view(path.clone(), cx);
                                    }))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex_col()
                                            .gap(px(2.))
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(theme::text())
                                                    .child(if s.title.is_empty() {
                                                        format!(
                                                            "{} · {} 消息",
                                                            s.id.chars().take(8).collect::<String>(),
                                                            s.message_count
                                                        )
                                                    } else {
                                                        format!(
                                                            "{} · {} 消息",
                                                            s.title,
                                                            s.message_count
                                                        )
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(theme::text_faint())
                                                    .child(s.last_model.clone().unwrap_or_default()),
                                            ),
                                    )
                                    .child(
                                        widgets::ghost_button(
                                            ("session-del", gi * 1000 + si),
                                            "删除",
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.session_delete = Some(s.path.clone());
                                            cx.notify();
                                        }))
                                    )
                            }))
                    }))
            })
            .child(if let Some(path) = delete_confirm {
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme::alpha(0x000000, 0.55))
                    .child(
                        div()
                            .w(px(360.))
                            .p(px(16.))
                            .rounded(px(12.))
                            .bg(theme::panel())
                            .border_1()
                            .border_color(theme::danger_a(0.4))
                            .flex_col()
                            .gap(px(12.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child("删除会话？"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_dim())
                                    .child(format!(
                                        "该操作不可撤销：{}",
                                        std::path::Path::new(&path)
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or(&path)
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(8.))
                                    .child(widgets::ghost_button("session-del-cancel", "取消").on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.session_delete = None;
                                            cx.notify();
                                        }),
                                    ))
                                    .child(widgets::danger_button("session-del-confirm", "确认删除").on_click(
                                        cx.listener(move |this, _, _, cx| {
                                            if let Some(p) = this.session_delete.clone() {
                                                let _ = ns_sessions::delete_session(
                                                    std::path::Path::new(&p),
                                                );
                                                this.session_delete = None;
                                                this.load_sessions(cx);
                                            }
                                        }),
                                    )),
                            ),
                    )
            } else {
                div()
            })
    }
    /// 渲染路由页：每工具一个开关 + 状态。
    fn render_routing_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let routable = [Tool::Claude, Tool::Codex, Tool::Gemini, Tool::GrokBuild];
        let (config, current) = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config().clone();
            let cur = cfg.current.clone();
            (cfg, cur)
        };
        div()
            .id("routing-scroll")
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("本地路由"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::text_faint())
                                    .child("每工具独立代理：live 配置指向 127.0.0.1，真实 key 留在 config.json"),
                            ),
                    )
                    .child(
                        widgets::pill(
                            format!("{} 个工具可路由", routable.len()),
                            theme::accent(),
                        ),
                    ),
            )
            .children(routable.into_iter().enumerate().map(|(i, tool)| {
                let tool_key = tool.id_str().to_string();
                let rc = config.settings.routing_for(tool);
                let running = self.routing.is_running(tool);
                let current_id = current.get(tool).cloned();
                let provider_name = current_id
                    .as_ref()
                    .and_then(|id| config.providers.get(id))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "未选择供应商".into());
                let status = self
                    .routing_status
                    .get(&tool_key)
                    .cloned()
                    .unwrap_or_default();
                let port_text = if running {
                    self.routing
                        .port_of(tool)
                        .map(|p| p.to_string())
                        .unwrap_or_default()
                } else {
                    rc.port.to_string()
                };

                div()
                    .flex_1()
                    .flex_col()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(theme::stroke_a(0.12))
                    .bg(theme::bg_1().opacity(0.55))
                    .p(px(16.))
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .text_color(theme::accent())
                                            .child(tool.glyph()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme::text())
                                            .child(tool.label()),
                                    ),
                            )
                            .child(self.render_routing_switch(tool, rc.enabled, cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("当前供应商"))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_dim())
                                    .child(provider_name),
                            )
                            .child(if running {
                                widgets::status_dot(theme::jade(), 7.)
                            } else {
                                widgets::status_dot(theme::text_dim(), 7.)
                            })
                            .child(div().text_size(px(11.)).text_color(theme::text_faint()).child(
                                if running {
                                    format!("代理运行中 · 127.0.0.1:{port_text}")
                                } else {
                                    "代理未运行".to_string()
                                },
                            )),
                    )
                    .child(if !status.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child(status)
                    } else {
                        div()
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("监听端口"))
                            .child(
                                div()
                                    .id(("routing-port", i))
                                    .flex()
                                    .items_center()
                                    .h(px(28.))
                                    .w(px(80.))
                                    .px(px(8.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.16))
                                    .bg(theme::surface_a(0.5))
                                    .text_color(theme::text())
                                    .text_size(px(12.))
                                    .child(port_text),
                            )
                            .child(
                                widgets::ghost_button(
                                    ("routing-toggle", i),
                                    if rc.enabled { "停用路由" } else { "启用路由" },
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_routing(tool, cx);
                                })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("监听地址"))
                            .child(
                                widgets::ghost_button(
                                    ("routing-addr", i),
                                    rc.listen_addr.clone(),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let mut store = this.store.write().expect("store lock");
                                    let mut rc = store.config().settings.routing_for(tool);
                                    rc.listen_addr = if rc.listen_addr == "127.0.0.1" {
                                        "0.0.0.0".to_string()
                                    } else {
                                        "127.0.0.1".to_string()
                                    };
                                    let new_addr = rc.listen_addr.clone();
                                    let r = store.set_routing(tool, rc);
                                    this.status = match r {
                                        Ok(()) => format!(
                                            "{} 监听地址: {}",
                                            tool.label(),
                                            if new_addr == "127.0.0.1" {
                                                "127.0.0.1（本机）"
                                            } else {
                                                "0.0.0.0（所有接口）"
                                            }
                                        ),
                                        Err(e) => format!("保存失败: {e}"),
                                    };
                                    cx.notify();
                                })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("故障转移"))
                            .child(
                                widgets::ghost_button(
                                    ("routing-failover", i),
                                    if rc.failover { "开启" } else { "关闭" },
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let mut store = this.store.write().expect("store lock");
                                    let mut rc = store.config().settings.routing_for(tool);
                                    rc.failover = !rc.failover;
                                    let new_failover = rc.failover;
                                    let r = store.set_routing(tool, rc);
                                    this.status = match r {
                                        Ok(()) => format!(
                                            "{} 故障转移: {}",
                                            tool.label(),
                                            if new_failover { "开启" } else { "关闭" }
                                        ),
                                        Err(e) => format!("保存失败: {e}"),
                                    };
                                    cx.notify();
                                })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("熔断阈值"))
                            .child(
                                widgets::ghost_button(("breaker-th-down", i), "−").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let mut store = this.store.write().expect("store lock");
                                        let mut rc = store.config().settings.routing_for(tool);
                                        rc.breaker_threshold = rc.breaker_threshold.saturating_sub(1).max(1);
                                        let v = rc.breaker_threshold;
                                        let r = store.set_routing(tool, rc);
                                        this.status = match r {
                                            Ok(()) => format!("{} 熔断阈值: {v}", tool.label()),
                                            Err(e) => format!("保存失败: {e}"),
                                        };
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                div()
                                    .min_w(px(24.))
                                    .text_size(px(12.))
                                    .text_color(theme::text())
                                    .child(rc.breaker_threshold.to_string()),
                            )
                            .child(
                                widgets::ghost_button(("breaker-th-up", i), "+").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let mut store = this.store.write().expect("store lock");
                                        let mut rc = store.config().settings.routing_for(tool);
                                        rc.breaker_threshold = (rc.breaker_threshold + 1).min(20);
                                        let v = rc.breaker_threshold;
                                        let r = store.set_routing(tool, rc);
                                        this.status = match r {
                                            Ok(()) => format!("{} 熔断阈值: {v}", tool.label()),
                                            Err(e) => format!("保存失败: {e}"),
                                        };
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("冷却(秒)"))
                            .child(
                                widgets::ghost_button(("breaker-cd-down", i), "−5").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let mut store = this.store.write().expect("store lock");
                                        let mut rc = store.config().settings.routing_for(tool);
                                        rc.breaker_cooldown = rc.breaker_cooldown.saturating_sub(5).max(1);
                                        let v = rc.breaker_cooldown;
                                        let r = store.set_routing(tool, rc);
                                        this.status = match r {
                                            Ok(()) => format!("{} 冷却: {v}s", tool.label()),
                                            Err(e) => format!("保存失败: {e}"),
                                        };
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                div()
                                    .min_w(px(40.))
                                    .text_size(px(12.))
                                    .text_color(theme::text())
                                    .child(rc.breaker_cooldown.to_string()),
                            )
                            .child(
                                widgets::ghost_button(("breaker-cd-up", i), "+5").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let mut store = this.store.write().expect("store lock");
                                        let mut rc = store.config().settings.routing_for(tool);
                                        rc.breaker_cooldown = (rc.breaker_cooldown + 5).min(300);
                                        let v = rc.breaker_cooldown;
                                        let r = store.set_routing(tool, rc);
                                        this.status = match r {
                                            Ok(()) => format!("{} 冷却: {v}s", tool.label()),
                                            Err(e) => format!("保存失败: {e}"),
                                        };
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
            }))
            .child(self.render_breaker_section(cx))
            .child(self.render_aggregation_section(cx))
            .child(self.render_request_log_section(cx))
    }


    /// 渲染熔断/故障转移状态分区（路由页内）。
    fn render_breaker_section(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        const ROUTABLE: [Tool; 4] = [Tool::Claude, Tool::Codex, Tool::Gemini, Tool::GrokBuild];
        div()
            .id("breaker-section")
            .flex_col()
            .gap(px(12.))
            .mt(px(8.))
            .child(
                div()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("熔断 / 故障转移状态"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("每工具各上游熔断器状态（未运行时无记录）"),
                    ),
            )
            .children(ROUTABLE.into_iter().enumerate().map(|(i, tool)| {
                let running = self.routing.is_running(tool);
                let states = self.routing.breaker_states(tool);
                div()
                    .id(("breaker-row", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::panel_a(0.35))
                    .border_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .w(px(120.))
                            .text_size(px(12.5))
                            .text_color(theme::text())
                            .child(tool.label()),
                    )
                    .child(if !running {
                        widgets::pill("未运行", theme::text_dim())
                    } else if states.is_empty() {
                        widgets::pill("无上游记录", theme::text_dim())
                    } else {
                        div()
                            .flex()
                            .gap(px(6.))
                            .children(states.into_iter().map(|(idx, st)| {
                                let (label, color) = match st {
                                    ns_proxy::CircuitState::Closed => {
                                        ("关闭", theme::jade())
                                    }
                                    ns_proxy::CircuitState::Open => {
                                        ("熔断", theme::danger())
                                    }
                                    ns_proxy::CircuitState::HalfOpen => {
                                        ("半开", theme::cinnabar())
                                    }
                                };
                                widgets::pill(&format!("上游{idx} {label}"), color)
                            }))
                    })
            }))
    }
    /// 渲染聚合模式分区（路由页内）。
    fn render_aggregation_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        const AGG_TOOLS: [Tool; 2] = [Tool::Claude, Tool::Codex];
        let (config, _current) = {
            let store = self.store.read().expect("store lock");
            (store.config().clone(), store.config().current.clone())
        };
        let agg_tool = self.agg_tool;
        let agg = config.aggregation_for(agg_tool);
        // 该工具全部供应商（候选）。
        let all_providers: Vec<ns_presets::Provider> = config
            .providers_for(agg_tool)
            .into_iter()
            .cloned()
            .collect();
        // 已聚合供应商（保序）。
        let agg_providers: Vec<ns_presets::Provider> = agg
            .provider_ids
            .iter()
            .filter_map(|id| config.providers.get(id).cloned())
            .collect();

        div()
            .id("aggregation-section")
            .flex_col()
            .gap(px(12.))
            .mt(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("聚合模式"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::text_faint())
                                    .child("多供应商模型合并、选默认、按模型路由"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .children(AGG_TOOLS.iter().enumerate().map(|(i, &t)| {
                                let selected = t == agg_tool;
                                div()
                                    .id(("agg-tool", i))
                                    .px(px(12.))
                                    .h(px(30.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(8.))
                                    .cursor_pointer()
                                    .bg(if selected {
                                        theme::accent_a(0.18)
                                    } else {
                                        theme::surface_a(0.4)
                                    })
                                    .border_1()
                                    .border_color(if selected {
                                        theme::accent_a(0.5)
                                    } else {
                                        theme::stroke_a(0.1)
                                    })
                                    .text_color(if selected {
                                        theme::accent()
                                    } else {
                                        theme::text_dim()
                                    })
                                    .text_size(px(12.))
                                    .child(t.label())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.agg_tool = t;
                                        cx.notify();
                                    }))
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex_col()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(theme::stroke_a(0.12))
                    .bg(theme::bg_1().opacity(0.55))
                    .p(px(16.))
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme::text())
                                            .child(agg_tool.label()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::text_faint())
                                            .child(if agg.enabled {
                                                "聚合已启用"
                                            } else {
                                                "聚合未启用"
                                            }),
                                    ),
                            )
                            .child(
                                widgets::ghost_button(
                                    ("agg-toggle", 0usize),
                                    if agg.enabled { "停用聚合" } else { "启用聚合" },
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_aggregation(agg_tool, cx);
                                })),
                            ),
                    )
                    .child(if agg.enabled {
                        // 已聚合供应商行。
                        div()
                            .flex_col()
                            .gap(px(8.))
                            .child(widgets::section_label("已聚合供应商（点击设默认）"))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(8.))
                                    .children(agg_providers.iter().enumerate().map(|(i, p)| {
                                        let is_default = agg.default_id.as_deref() == Some(p.id.as_str());
                                        let pid = p.id.clone();
                                        div()
                                            .id(("agg-chip", i))
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .px(px(10.))
                                            .h(px(30.))
                                            .rounded(px(8.))
                                            .cursor_pointer()
                                            .bg(theme::alpha(
                                                if is_default { theme::ACCENT } else { 0x261d12 },
                                                if is_default { 0.22 } else { 0.4 },
                                            ))
                                            .border_1()
                                            .border_color(if is_default {
                                                theme::accent_a(0.5)
                                            } else {
                                                theme::stroke_a(0.1)
                                            })
                                            .on_click({
                                                let pid = pid.clone();
                                                cx.listener(move |this, _, _, cx| {
                                                    this.agg_set_default(agg_tool, &pid, cx);
                                                })
                                            })
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(rgb(if is_default {
                                                        theme::ACCENT
                                                    } else {
                                                        theme::TEXT
                                                    }))
                                                    .child(p.name.clone()),
                                            )
                                            .child(
                                                widgets::danger_button(("agg-remove", i), "×")
                                                    .on_click({
                                                        let pid = pid.clone();
                                                        cx.listener(move |this, _, _, cx| {
                                                            this.agg_remove_provider(agg_tool, &pid, cx);
                                                        })
                                                    }),
                                            )
                                    })),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(widgets::section_label("添加供应商"))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .gap(px(8.))
                                            .children(all_providers.iter().enumerate().filter_map(|(i, p)| {
                                                if agg.provider_ids.contains(&p.id) {
                                                    return None;
                                                }
                                                let pid = p.id.clone();
                                                Some(
                                                    widgets::ghost_button(("agg-add", i), &p.name)
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.agg_add_provider(agg_tool, &pid, cx);
                                                        })),
                                                )
                                            })),
                                    ),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("启用后可将多个供应商加入聚合，并选一个默认。")
                            .into_any_element()
                    }),
            )
    }

    /// 渲染请求日志分区（路由页内）。
    fn render_request_log_section(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.routing.request_log.snapshot();
        let (total, ok) = entries
            .iter()
            .fold((0usize, 0usize), |(t, o), e| (t + 1, o + usize::from(e.ok)));
        div()
            .id("request-log-section")
            .flex_col()
            .gap(px(12.))
            .mt(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("请求日志"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::text_faint())
                                    .child("代理转发的最近请求（工具 / 请求模型 / 实际模型 / 状态 / 耗时）"),
                            ),
                    )
                    .child(widgets::pill(format!("{total} 条 · {ok} 成功"), theme::accent())),
            )
            .child(if entries.is_empty() {
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(120.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_faint())
                            .child("暂无请求。启用路由并发出请求后，这里会显示转发记录。"),
                    )
            } else {
                div()
                    .flex_col()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(theme::stroke_a(0.12))
                    .bg(theme::bg_1().opacity(0.55))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .px(px(12.))
                            .py(px(8.))
                            .bg(theme::bg_2().opacity(0.6))
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child(div().w(px(90.)).child("工具"))
                            .child(div().flex_1().child("请求模型 → 实际模型"))
                            .child(div().w(px(70.)).child("状态"))
                            .child(div().w(px(70.)).child("耗时")),
                    )
                    .children(entries.iter().take(200).enumerate().map(|(i, e)| {
                        let tool_label = ns_presets::Tool::ALL
                            .iter()
                            .find(|t| t.id_str() == e.tool)
                            .map(|t| t.label().to_string())
                            .unwrap_or_else(|| e.tool.clone());
                        div()
                            .id(("reqlog-row", i))
                            .flex()
                            .items_center()
                            .px(px(12.))
                            .py(px(6.))
                            .border_b_1()
                            .border_color(theme::stroke_a(0.06))
                            .text_size(px(11.))
                            .child(div().w(px(90.)).text_color(theme::text_dim()).child(tool_label))
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(theme::text_dim())
                                    .child(format!(
                                        "{} → {}",
                                        if e.requested_model.is_empty() {
                                            "-"
                                        } else {
                                            &e.requested_model
                                        },
                                        if e.actual_model.is_empty() {
                                            "-"
                                        } else {
                                            &e.actual_model
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .w(px(70.))
                                    .text_color(if e.ok {
                                        theme::jade()
                                    } else {
                                        theme::danger()
                                    })
                                    .child(e.status.to_string()),
                            )
                            .child(
                                div()
                                    .w(px(70.))
                                    .text_color(theme::text_faint())
                                    .child(format!("{}ms", e.elapsed_ms)),
                            )
                    }))
            })
    }

    /// 切换某工具聚合开关。启用时若无供应商，自动加入该工具当前/首个供应商。
    fn toggle_aggregation(&mut self, tool: Tool, cx: &mut Context<Self>) {
        {
            let mut store = self.store.write().expect("store lock");
            let mut cfg = store.config().aggregation_for(tool);
            cfg.enabled = !cfg.enabled;
            if cfg.enabled && cfg.provider_ids.is_empty() {
                // 自动加入：当前供应商优先，否则该工具首个供应商。
                let current = store.config().current.get(tool).cloned();
                let first = current.or_else(|| {
                    store
                        .config()
                        .providers_for(tool)
                        .first()
                        .map(|p| p.id.clone())
                });
                if let Some(id) = first {
                    cfg.provider_ids.push(id.clone());
                    cfg.default_id = Some(id);
                }
            }
            match store.set_aggregation(tool, cfg) {
                Ok(_) => {
                    self.status = format!(
                        "聚合已{}",
                        if store.config().aggregation_for(tool).enabled {
                            "启用"
                        } else {
                            "停用"
                        }
                    )
                }
                Err(e) => self.status = format!("聚合切换失败: {e}"),
            }
        }
        self.agg_apply_live(tool, cx);
        cx.notify();
    }

    /// 把某供应商加入聚合列表。
    fn agg_add_provider(&mut self, tool: Tool, id: &str, cx: &mut Context<Self>) {
        let enabled = {
            let mut store = self.store.write().expect("store lock");
            let mut cfg = store.config().aggregation_for(tool);
            if cfg.provider_ids.contains(&id.to_string()) {
                return;
            }
            cfg.provider_ids.push(id.to_string());
            if cfg.default_id.is_none() {
                cfg.default_id = Some(id.to_string());
            }
            let enabled = cfg.enabled;
            match store.set_aggregation(tool, cfg) {
                Ok(_) => self.status = format!("已加入聚合: {id}"),
                Err(e) => self.status = format!("加入失败: {e}"),
            }
            enabled
        };
        if enabled {
            self.agg_apply_live(tool, cx);
        }
        cx.notify();
    }

    /// 从聚合列表移除供应商。默认供应商不可移除（保留至少一个）。
    fn agg_remove_provider(&mut self, tool: Tool, id: &str, cx: &mut Context<Self>) {
        let enabled = {
            let mut store = self.store.write().expect("store lock");
            let mut cfg = store.config().aggregation_for(tool);
            if cfg.default_id.as_deref() == Some(id) {
                self.status = format!("默认供应商不可移除（先切换默认）");
                cx.notify();
                return;
            }
            if !cfg.provider_ids.contains(&id.to_string()) {
                return;
            }
            cfg.provider_ids.retain(|p| p != id);
            let enabled = cfg.enabled;
            match store.set_aggregation(tool, cfg) {
                Ok(_) => self.status = format!("已从聚合移除: {id}"),
                Err(e) => self.status = format!("移除失败: {e}"),
            }
            enabled
        };
        if enabled {
            self.agg_apply_live(tool, cx);
        }
        cx.notify();
    }

    /// 设置默认供应商。
    /// 设置默认供应商，并把聚合配置写回客户端（聚合模式下 live 指向默认）。
    fn agg_set_default(&mut self, tool: Tool, id: &str, cx: &mut Context<Self>) {
        {
            let mut store = self.store.write().expect("store lock");
            let mut cfg = store.config().aggregation_for(tool);
            if !cfg.provider_ids.contains(&id.to_string()) {
                self.status = format!("{id} 不在聚合列表");
                cx.notify();
                return;
            }
            cfg.default_id = Some(id.to_string());
            if let Err(e) = store.set_aggregation(tool, cfg) {
                self.status = format!("设置失败: {e}");
                cx.notify();
                return;
            }
        }
        self.agg_apply_live(tool, cx);
        if !self.status.is_empty() {
            let s = self.status.clone();
            self.status = format!("默认供应商已设为 {id}；{s}");
        }
        cx.notify();
    }

    /// 渲染 M7 统一 MCP 面板。
    fn render_mcp_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mcp_tools = crate::mcp::MCP_TOOLS;
        let tool = self.mcp.tool;
        let servers = self.mcp.servers.clone();
        let status = self.mcp.status.clone();
        let write_msg = self.mcp.write_msg.clone();
        let confirm_delete = self.mcp.confirm_delete.clone();

        div()
            .id("mcp-scroll")
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("统一 MCP 面板"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::text_faint())
                                    .child("一处管理各工具的 MCP 服务器，可跨工具同步"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::ghost_button(("mcp-refresh", 0usize), "刷新").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.mcp.load(cx);
                                }),
                            ))
                            .child(
                                widgets::ghost_button(("mcp-import", 0usize), "从其他工具导入").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        // 导入来源：取另一个支持 MCP 的工具（首个与当前不同者）。
                                        let source = crate::mcp::MCP_TOOLS
                                            .iter()
                                            .copied()
                                            .find(|&t| t != this.mcp.tool)
                                            .unwrap_or(this.mcp.tool);
                                        this.mcp.import_from(source, cx);
                                    }),
                                ),
                            )
                            .child(
                                widgets::ghost_button(("mcp-scope", 0usize), "项目作用域")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mcp.open_scope(cx);
                                    })),
                            )
                            .child(
                                widgets::ghost_button(("mcp-enable-all", 0usize), "全部启用")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mcp.set_all_enabled(true, cx);
                                    })),
                            )
                            .child(
                                widgets::ghost_button(("mcp-disable-all", 0usize), "全部禁用")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mcp.set_all_enabled(false, cx);
                                    })),
                            )
                            .child(
                                widgets::primary_button(("mcp-add", 0usize), "添加服务器").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.open_add(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .children(mcp_tools.iter().enumerate().map(|(i, &t)| {
                        let selected = t == tool;
                        div()
                            .id(("mcp-tool", i))
                            .px(px(12.))
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .bg(if selected {
                                theme::accent_a(0.18)
                            } else {
                                theme::surface_a(0.4)
                            })
                            .border_1()
                            .border_color(if selected {
                                theme::accent_a(0.5)
                            } else {
                                theme::stroke_a(0.1)
                            })
                            .text_color(if selected {
                                theme::accent()
                            } else {
                                theme::text_dim()
                            })
                            .text_size(px(12.))
                            .child(t.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.mcp.set_tool(t, cx);
                            }))
                    })),
            )
            .child(if !status.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(theme::cinnabar())
                    .child(status)
            } else {
                div()
            })
            .child(if !write_msg.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(theme::jade())
                    .child(write_msg)
            } else {
                div()
            })
            .child(if servers.is_empty() {
                div()
                    .flex_col()
                    .items_center()
                    .gap(px(10.))
                    .p(px(24.))
                    .child(
                        div()
                            .size(px(48.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme::accent_a(0.08))
                            .border_1()
                            .border_color(theme::stroke_a(0.14))
                            .text_size(px(20.))
                            .text_color(theme::accent())
                            .child("⛁"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(format!("{} 暂无 MCP 服务器", tool.label())),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("点击右上角「添加服务器」配置 MCP 连接"),
                    )
            } else {
                div()
                    .flex_col()
                    .gap(px(8.))
                    .children(servers.iter().enumerate().map(|(i, s)| {
                        let s = s.clone();
                        let sname = s.name.clone();
                        div()
                            .id(("mcp-server", i))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(theme::stroke_a(0.12))
                            .bg(theme::bg_1().opacity(0.55))
                            .px(px(14.))
                            .h(px(52.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme::text())
                                            .child(s.name.clone()),
                                    )
                                    .child(widgets::pill(
                                        transport_label(s.transport),
                                        theme::accent(),
                                    ))
                                    .child(if s.enabled {
                                        widgets::pill("启用", theme::jade())
                                    } else {
                                        widgets::pill("停用", theme::text_dim())
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        widgets::ghost_button(("mcp-toggle", i), if s.enabled {
                                            "停用"
                                        } else {
                                            "启用"
                                        })
                                        .on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.mcp.toggle_enabled(&sn, cx);
                                            })
                                        }),
                                    )
                                    .child(
                                        widgets::ghost_button(("mcp-sync", i), "同步到…").on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.mcp.open_sync_targets(&sn, cx);
                                            })
                                        }),
                                    )
                                    .child(
                                        widgets::ghost_button(("mcp-edit", i), "编辑").on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.mcp.open_edit(&sn, cx);
                                            })
                                        }),
                                    )
                                    .child(
                                        widgets::danger_button(("mcp-del", i), "删除").on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.mcp.request_delete(&sn, cx);
                                            })
                                        }),
                                    ),
                            )
                    }))})
            .child(if self.mcp.dialog_open {
                self.render_mcp_dialog(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.mcp.scope_open {
                self.render_mcp_scope_dialog(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if let Some(name) = confirm_delete {
                self.render_mcp_confirm_delete(&name, cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.mcp.sync_targets_open.is_some() {
                self.render_mcp_sync_targets(cx).into_any_element()
            } else {
                div().into_any_element()
            })
    }

    /// MCP 新增/编辑对话框。
    fn render_mcp_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_edit = self.mcp.editing_name.is_some();
        let form = &self.mcp.form;
        let name = form.name.clone();
        let command = form.command.clone();
        let args = form.args.clone();
        let url = form.url.clone();
        let is_http = matches!(form.transport, McpTransport::Http | McpTransport::Sse | McpTransport::Ws);

        div()
            .id("mcp-dialog-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("mcp-dialog")
                    .w(px(460.))
                    .flex_col()
                    .gap(px(12.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.2))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child(if is_edit { "编辑 MCP 服务器" } else { "添加 MCP 服务器" }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(self.mcp_transport_pill(McpTransport::Stdio, cx))
                            .child(self.mcp_transport_pill(McpTransport::Http, cx))
                            .child(self.mcp_transport_pill(McpTransport::Sse, cx))
                            .child(self.mcp_transport_pill(McpTransport::Ws, cx)),
                    )
                    .child(self.mcp_field("服务器名", name))
                    .child(if is_http {
                        self.mcp_field("端点 URL", url).into_any_element()
                    } else {
                        div()
                            .flex_col()
                            .gap(px(8.))
                            .child(self.mcp_field("启动命令", command))
                            .child(self.mcp_field("参数（空格分隔）", args))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_col()
                            .gap(px(6.))
                            .child(widgets::section_label("环境变量（每行 key=value）"))
                            .child(self.mcp_field("", form.env_raw.clone())),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(6.))
                            .child(widgets::section_label("请求头（每行 key: value，仅远程传输有效）"))
                            .child(self.mcp_field("", form.headers_raw.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("启用"))
                            .child(
                                div()
                                    .id("mcp-form-enabled")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mcp.form.enabled = !this.mcp.form.enabled;
                                        cx.notify();
                                    }))
                                    .child(if self.mcp.form.enabled {
                                        widgets::pill("启用", theme::jade())
                                    } else {
                                        widgets::pill("停用", theme::text_dim())
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("mcp-cancel", 0usize), "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.dialog_open = false;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::primary_button(("mcp-save", 0usize), "保存").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.save(cx);
                                    }),
                                ),
                            ),
                    ),
            )
    }

    /// MCP 项目作用域对话框（选择项目目录，读取 `.mcp.json`）。
    fn render_mcp_scope_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let path_input = self.mcp.scope_path.clone();
        let result = self.mcp.scope_result.clone();
        div()
            .id("mcp-scope-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x000000, 0.55))
            .child(
                div()
                    .w(px(480.))
                    .rounded(px(12.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.25))
                    .shadow_lg()
                    .p(px(20.))
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("项目作用域 MCP"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child("输入项目目录路径，读取该项目的 `.mcp.json` 项目作用域 MCP 服务器。"),
                    )
                    .child(path_input)
                    .child(
                        div()
                            .min_h(px(32.))
                            .text_size(px(11.5))
                            .text_color(theme::text_dim())
                            .child(if result.is_empty() {
                                "尚未读取".to_string()
                            } else {
                                result
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("mcp-scope-choose", "选择目录").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.choose_scope_dir(cx);
                                    }),
                                ),
                            )
                            .child(
                                widgets::ghost_button("mcp-scope-cancel", "关闭").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.scope_open = false;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::primary_button("mcp-scope-read", "读取").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.read_scope(cx);
                                    }),
                                ),
                            ),
                    ),
            )
    }

    /// MCP 传输类型选择药丸。
    fn mcp_transport_pill(&self, transport: McpTransport, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.mcp.form.transport == transport;
        let label = match transport {
            McpTransport::Stdio => "本地进程",
            McpTransport::Http => "HTTP 远程",
            McpTransport::Sse => "SSE",
            McpTransport::Ws => "WebSocket",
        };
        div()
            .id((gpui::ElementId::from("mcp-transport"), transport_label(transport)))
            .px(px(12.))
            .h(px(28.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .cursor_pointer()
            .bg(if selected {
                theme::accent_a(0.18)
            } else {
                theme::surface_a(0.4)
            })
            .border_1()
            .border_color(if selected {
                theme::accent_a(0.5)
            } else {
                theme::stroke_a(0.1)
            })
            .text_color(if selected {
                theme::accent()
            } else {
                theme::text_dim()
            })
            .text_size(px(12.))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.mcp.form.transport = transport;
                cx.notify();
            }))
    }

    /// MCP 表单字段（标签 + TextField）。
    fn mcp_field(&self, label: &'static str, field: gpui::Entity<crate::text_field::TextField>) -> impl IntoElement {
        div()
            .flex_col()
            .gap(px(6.))
            .child(widgets::section_label(label))
            .child(
                div()
                    .h(px(32.))
                    .px(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme::stroke_a(0.16))
                    .bg(theme::surface_a(0.5))
                    .child(field),
            )
    }

    /// MCP 删除确认浮层。
    fn render_mcp_confirm_delete(&self, name: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let name = name.to_string();
        div()
            .id("mcp-delete-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("mcp-delete")
                    .w(px(380.))
                    .flex_col()
                    .gap(px(14.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::danger_a(0.4))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("删除 MCP 服务器"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child(format!("确认删除服务器 “{name}”？此操作将从 {} 配置移除。", self.mcp.tool.label())),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("mcp-del-cancel", 0usize), "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.confirm_delete = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::danger_button(("mcp-del-confirm", 0usize), "删除").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.confirm_delete(cx);
                                    }),
                                ),
                            ),
                    ),
            )
    }

    /// MCP 同步目标选择浮层（勾选目标工具后同步，默认全选）。
    fn render_mcp_sync_targets(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(name) = self.mcp.sync_targets_open.clone() else {
            return div().into_any_element();
        };
        let current = self.mcp.tool;
        let targets: Vec<(Tool, bool)> = crate::mcp::MCP_TOOLS
            .iter()
            .copied()
            .filter(|t| *t != current)
            .map(|t| {
                let on = self
                    .mcp
                    .sync_target_selected
                    .get(t.id_str())
                    .copied()
                    .unwrap_or(false);
                (t, on)
            })
            .collect();
        div()
            .id("mcp-sync-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("mcp-sync-targets")
                    .w(px(420.))
                    .flex_col()
                    .gap(px(12.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.3))
                    .shadow_lg()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("同步 MCP 服务器"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child(format!(
                                "将「{name}」从 {} 同步到所选工具（只改目标工具的 mcpServers/mcp_servers）：",
                                current.label()
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("mcp-sync-all", "全选").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.set_all_sync_targets(true, cx);
                                    }),
                                ),
                            )
                            .child(
                                widgets::ghost_button("mcp-sync-none", "全不选").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.set_all_sync_targets(false, cx);
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(6.))
                            .children(targets.into_iter().enumerate().map(|(i, (t, on))| {
                                div()
                                    .id(("mcp-sync-target", i))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .cursor_pointer()
                                    .child(
                                        widgets::pill(
                                            if on { "☑" } else { "☐" },
                                            if on { theme::jade() } else { theme::text_faint() },
                                        )
                                        .id(("mcp-sync-check", i)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(theme::text())
                                            .child(t.label()),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.mcp.toggle_sync_target(t, cx);
                                    }))
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("mcp-sync-cancel", "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.sync_targets_open = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::primary_button("mcp-sync-confirm", "同步").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.mcp.confirm_sync_targets(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// 渲染 M7 统一 Skills 面板。
    fn render_skills_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        const SKILL_TOOLS: [Tool; 3] = [Tool::Claude, Tool::Codex, Tool::Gemini];
        let tool = self.skills.tool;
        let skills = self.skills.skills.clone();
        let status = self.skills.status.clone();
        let write_msg = self.skills.write_msg.clone();
        let confirm_delete = self.skills.confirm_delete.clone();

        div()
            .id("skills-scroll")
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("统一 Skills 面板"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::text_faint())
                                    .child("一处管理各工具的 Skills，可跨工具同步"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::ghost_button(("skills-refresh", 0usize), "刷新").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.skills.shared = this
                                        .store
                                        .read()
                                        .map(|s| s.config().settings.shared_skills)
                                        .unwrap_or(false);
                                    this.skills.load(cx);
                                }),
                            ))
                            .child(
                                widgets::ghost_button(("skills-check-updates", 0usize), "检查更新")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.check_updates(cx);
                                    })),
                            )
                            .child(
                                widgets::ghost_button(("skills-update-all", 0usize), "全部更新")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.update_all(cx);
                                    })),
                            )
                            .child(
                                widgets::ghost_button(("skills-install", 0usize), "安装")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.open_install(cx);
                                    })),
                            )
                            .child(
                                widgets::ghost_button(("skills-enable-all", 0usize), "全部启用")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.set_all_enabled(true, cx);
                                    })),
                            )
                            .child(
                                widgets::ghost_button(("skills-disable-all", 0usize), "全部禁用")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.set_all_enabled(false, cx);
                                    })),
                            )
                            .child(
                                widgets::primary_button(("skills-add", 0usize), "添加技能").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.skills.open_add(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .children(SKILL_TOOLS.iter().enumerate().map(|(i, &t)| {
                        let selected = t == tool;
                        div()
                            .id(("skills-tool", i))
                            .px(px(12.))
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .bg(if selected {
                                theme::accent_a(0.18)
                            } else {
                                theme::surface_a(0.4)
                            })
                            .border_1()
                            .border_color(if selected {
                                theme::accent_a(0.5)
                            } else {
                                theme::stroke_a(0.1)
                            })
                            .text_color(if selected {
                                theme::accent()
                            } else {
                                theme::text_dim()
                            })
                            .text_size(px(12.))
                            .child(t.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.skills.set_tool(t, cx);
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .children(
                        [
                            (crate::skills::SkillsView::Installed, "已安装"),
                            (crate::skills::SkillsView::Discover, "发现"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (view, label))| {
                            let selected = self.skills.view == view;
                            widgets::ghost_button(
                                gpui::ElementId::from(("skills-view", i)),
                                label,
                            )
                            .when(selected, |b| {
                                b.bg(theme::accent_a(0.18))
                                    .border_1()
                                    .border_color(theme::accent_a(0.5))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.skills.view = view;
                                cx.notify();
                            }))
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("同步策略"),
                    )
                    .children(
                        [
                            (ns_skills::model::SyncStrategy::Auto, "自动"),
                            (ns_skills::model::SyncStrategy::Symlink, "符号链接"),
                            (ns_skills::model::SyncStrategy::Copy, "复制"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (strat, label))| {
                            let cur = self.skills.strategy;
                            widgets::ghost_button(
                                gpui::ElementId::from(("skills-strategy", i)),
                                label,
                            )
                            .when(strat == cur, |b| {
                                b.bg(theme::accent_a(0.18))
                                    .border_1()
                                    .border_color(theme::accent_a(0.5))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.skills.strategy = strat;
                                cx.notify();
                            }))
                        }),
                    ),
            )
            .child(if !status.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(theme::cinnabar())
                    .child(status)
            } else {
                div()
            })
            .child(if !write_msg.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(theme::jade())
                    .child(write_msg)
            } else {
                div()
            })
            .child(if self.skills.view == crate::skills::SkillsView::Discover {
                let q = self.skills.discover_query.clone();
                let status = self.skills.discover_status.clone();
                let results = self.skills.discover_results.clone();
                div()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .child(q.clone()),
                            )
                            .child(
                                widgets::ghost_button("skills-discover-search", "搜索").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.skills.search_discover(cx);
                                    }),
                                ),
                            ),
                    )
                    .child(if !status.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child(status)
                    } else {
                        div()
                    })
                    .children(results.into_iter().enumerate().map(|(i, (name, desc))| {
                        let repo_url = format!("https://github.com/{name}");
                        div()
                            .id(("discover-row", i))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(8.))
                            .bg(theme::panel_a(0.35))
                            .border_1()
                            .border_color(theme::stroke_a(0.08))
                            .child(
                                div()
                                    .flex_1()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .text_color(theme::text())
                                            .child(name),
                                    )
                                    .child(if desc.is_empty() {
                                        div()
                                    } else {
                                        div()
                                            .text_size(px(10.5))
                                            .text_color(theme::text_faint())
                                            .child(desc)
                                    }),
                            )
                            .child(
                                widgets::ghost_button(
                                    ("discover-install", i),
                                    "从 GitHub 安装",
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let targets = [
                                        ns_presets::Tool::Claude,
                                        ns_presets::Tool::Codex,
                                        ns_presets::Tool::Gemini,
                                    ];
                                    let paths = {
                                        let mut p = ns_skills::model::SkillPaths::from_env();
                                        p.shared = this.skills.shared;
                                        p
                                    };
                                    match ns_skills::manager::install_from_github(
                                        &repo_url,
                                        "",
                                        &paths,
                                        &targets,
                                    ) {
                                        Ok(name) => {
                                            this.skills.write_msg =
                                                format!("已安装技能 {name}");
                                            this.skills.load(cx);
                                        }
                                        Err(e) => {
                                            this.skills.write_msg =
                                                format!("安装失败: {e}")
                                        }
                                    }
                                    cx.notify();
                                }))
                            )
                    }))
            } else if skills.is_empty() {
                div()
                    .flex_col()
                    .items_center()
                    .gap(px(10.))
                    .p(px(24.))
                    .child(
                        div()
                            .size(px(48.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme::accent_a(0.08))
                            .border_1()
                            .border_color(theme::stroke_a(0.14))
                            .text_size(px(20.))
                            .text_color(theme::accent())
                            .child("✦"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(format!("{} 暂无已安装技能", self.skills.tool.label())),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("点击「添加技能」安装新的 Skill 或切换到「发现」标签浏览"),
                    )
            } else {
                div()
                    .flex_col()
                    .gap(px(8.))
                    .children(skills.iter().enumerate().map(|(i, s)| {
                        let s = s.clone();
                        let sname = s.name.clone();
                        div()
                            .id(("skill-row", i))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(theme::stroke_a(0.12))
                            .bg(theme::bg_1().opacity(0.55))
                            .px(px(14.))
                            .h(px(52.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(theme::text())
                                                    .child(s.name.clone()),
                                            )
                                            .child(if !s.enabled {
                                                widgets::pill("已禁用", theme::cinnabar())
                                            } else {
                                                div()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::text_faint())
                                            .child(if s.description.is_empty() {
                                                "—".to_string()
                                            } else {
                                                s.description.clone()
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        widgets::ghost_button(
                                            ("skill-toggle", i),
                                            if s.enabled { "禁用" } else { "启用" },
                                        )
                                        .on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.skills.toggle_enabled(&sn, cx);
                                            })
                                        }),
                                    )
                                    .child(
                                        widgets::ghost_button(("skill-sync", i), "同步").on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                let others: Vec<Tool> = SKILL_TOOLS
                                                    .iter()
                                                    .copied()
                                                    .filter(|&t| t != this.skills.tool)
                                                    .collect();
                                                this.skills.sync_to(&sn, &others, cx);
                                            })
                                        }),
                                    )
                                    .child(
                                        widgets::ghost_button(("skill-edit", i), "编辑").on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.skills.open_edit(&sn, cx);
                                            })
                                        }),
                                    )
                                    .child(
                                        widgets::danger_button(("skill-del", i), "删除").on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.skills.request_delete(&sn, cx);
                                            })
                                        }),
                                    ),
                            )
                    }))})
            .child(if self.skills.dialog_open {
                self.render_skill_dialog(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.skills.install_open {
                self.render_skill_install_dialog(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.skills.confirm_close {
                self.render_skill_confirm_close(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if let Some(name) = confirm_delete {
                self.render_skill_confirm_delete(&name, cx).into_any_element()
            } else {
                div().into_any_element()
            })
    }

    /// Skills 新增/编辑对话框。
    fn render_skill_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_edit = self.skills.editing_name.is_some();
        let form = &self.skills.form;
        let name = form.name.clone();
        let description = form.description.clone();
        let body = form.body.clone();

        div()
            .id("skill-dialog-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("skill-dialog")
                    .w(px(480.))
                    .flex_col()
                    .gap(px(12.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.2))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child(if is_edit { "编辑 Skill" } else { "添加 Skill" }),
                    )
                    .child(self.skill_field("技能名", name))
                    .child(self.skill_field("描述", description))
                    .child(self.skill_field("允许工具（逗号分隔）", form.allowed_tools.clone()))
                    .child(
                        div()
                            .flex_col()
                            .gap(px(6.))
                            .child(widgets::section_label("正文（Markdown）"))
                            .child(
                                div()
                                    .h(px(140.))
                                    .px(px(8.))
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.16))
                                    .bg(theme::surface_a(0.5))
                                    .child(body),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("启用"))
                            .child(
                                div()
                                    .id("skill-form-enabled")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.toggle_form_enabled(cx);
                                    }))
                                    .child(if self.skills.form.enabled {
                                        widgets::pill("启用", theme::jade())
                                    } else {
                                        widgets::pill("禁用", theme::cinnabar())
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("skill-cancel", 0usize), "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.skills.request_close(cx);
                                    }),
                                ),
                            )
                            .child(
                                widgets::primary_button(("skill-save", 0usize), "保存").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.skills.save(cx);
                                    }),
                                ),
                            ),
                    ),
            )
    }

    /// Skills 从 GitHub 安装对话框。
    fn render_skill_install_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let url = self.skills.install_url.clone();
        div()
            .id("skill-install-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("skill-install")
                    .w(px(440.))
                    .flex_col()
                    .gap(px(12.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.2))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .children(
                                [
                                    (
                                        crate::skills::SkillInstallSource::Github,
                                        "GitHub 仓库",
                                    ),
                                    (
                                        crate::skills::SkillInstallSource::LocalDir,
                                        "本地目录",
                                    ),
                                ]
                                .into_iter()
                                .enumerate()
                                .map(|(i, (src, label))| {
                                    let selected = self.skills.install_source == src;
                                    widgets::ghost_button(
                                        gpui::ElementId::from(("skill-install-src", i)),
                                        label,
                                    )
                                    .when(selected, |b| {
                                        b.bg(theme::accent_a(0.18))
                                            .border_1()
                                            .border_color(theme::accent_a(0.5))
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.skills.install_source = src;
                                        cx.notify();
                                    }))
                                }))
                    )
                    .child(if self.skills.install_source
                        == crate::skills::SkillInstallSource::Github
                    {
                        self.skill_field("仓库 URL", url).into_any_element()
                    } else {
                        self.skill_field(
                            "本地目录",
                            self.skills.install_local_dir.clone(),
                        )
                        .into_any_element()
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child(if self.skills.install_source
                                == crate::skills::SkillInstallSource::Github
                            {
                                "仓库需含 skills/<name>/SKILL.md（或仓库根 SKILL.md）"
                            } else {
                                "目录需含 SKILL.md（或 <name>.md）"
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("skill-install-cancel", 0usize), "取消")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.install_open = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                widgets::primary_button(("skill-install-go", 0usize), "安装")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        match this.skills.install_source {
                                            crate::skills::SkillInstallSource::Github => {
                                                this.skills.install_github(cx)
                                            }
                                            crate::skills::SkillInstallSource::LocalDir => {
                                                this.skills.install_local(cx)
                                            }
                                        }
                                    })),
                            ),
                    ),
            )
    }

    /// Skills 表单字段（标签 + TextField）。
    fn skill_field(
        &self,
        label: &'static str,
        field: gpui::Entity<crate::text_field::TextField>,
    ) -> impl IntoElement {
        div()
            .flex_col()
            .gap(px(6.))
            .child(widgets::section_label(label))
            .child(
                div()
                    .h(px(32.))
                    .px(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme::stroke_a(0.16))
                    .bg(theme::surface_a(0.5))
                    .child(field),
            )
    }

    /// Skills 删除确认浮层。
    fn render_skill_confirm_delete(&self, name: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let name = name.to_string();
        div()
            .id("skill-delete-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("skill-delete")
                    .w(px(380.))
                    .flex_col()
                    .gap(px(14.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::danger_a(0.4))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("删除 Skill"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child(format!(
                                "确认删除技能 “{name}”？此操作将从 {} 移除。",
                                self.skills.tool.label()
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("skill-del-cancel", 0usize), "取消")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.confirm_delete = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                widgets::danger_button(("skill-del-confirm", 0usize), "删除")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.confirm_delete(cx);
                                    })),
                            ),
                    ),
            )
    }

    /// Skills 表单未保存更改确认浮层（继续编辑 / 放弃更改）。
    fn render_skill_confirm_close(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("skill-close-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("skill-close")
                    .w(px(380.))
                    .flex_col()
                    .gap(px(14.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.3))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("未保存的更改"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child("此技能有未保存的更改。确定要放弃吗？"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("skill-close-continue", 0usize), "继续编辑")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.confirm_close = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                widgets::danger_button(("skill-close-discard", 0usize), "放弃更改")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.confirm_close = false;
                                        this.skills.dialog_open = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    /// 渲染 M7 统一 Prompts 面板。
    fn render_prompts_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        const PROMPT_TOOLS: [Tool; 5] = [
            Tool::Claude,
            Tool::ClaudeDesktop,
            Tool::Codex,
            Tool::Gemini,
            Tool::Pi,
        ];
        let tool = self.prompts.tool;
        let entries = self.prompts.entries.clone();
        let status = self.prompts.status.clone();
        let write_msg = self.prompts.write_msg.clone();
        let confirm_delete = self.prompts.confirm_delete.clone();

        div()
            .id("prompts-scroll")
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("统一提示词面板"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::text_faint())
                                    .child("管理各工具提示词，写回提示文件（先回存既有内容）"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::ghost_button(("prompts-refresh", 0usize), "刷新").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.prompts.load(cx);
                                }),
                            ))
                            .child(
                                widgets::ghost_button(("prompts-writeback", 0usize), "写回文件")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.prompts.write_back(cx);
                                    })),
                            )
                            .child(
                                widgets::primary_button(("prompts-add", 0usize), "添加条目").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.prompts.open_add(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .children(PROMPT_TOOLS.iter().enumerate().map(|(i, &t)| {
                        let selected = t == tool;
                        div()
                            .id(("prompts-tool", i))
                            .px(px(12.))
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .bg(if selected {
                                theme::accent_a(0.18)
                            } else {
                                theme::surface_a(0.4)
                            })
                            .border_1()
                            .border_color(if selected {
                                theme::accent_a(0.5)
                            } else {
                                theme::stroke_a(0.1)
                            })
                            .text_color(if selected {
                                theme::accent()
                            } else {
                                theme::text_dim()
                            })
                            .text_size(px(12.))
                            .child(t.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.prompts.set_tool(t, cx);
                            }))
                    })),
            )
            .child(if !status.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(theme::cinnabar())
                    .child(status)
            } else {
                div()
            })
            .child(if !write_msg.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(theme::jade())
                    .child(write_msg)
            } else {
                div()
            })
            .child(if entries.is_empty() {
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .size(px(48.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme::accent_a(0.08))
                            .border_1()
                            .border_color(theme::stroke_a(0.14))
                            .text_size(px(20.))
                            .text_color(theme::accent())
                            .child("✎"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(format!("{} 暂无提示词条目", tool.label())),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::text_faint())
                            .child("点击右上角「添加条目」新建提示词，写回时合并到工具提示文件"),
                    )
            } else {
                div()
                    .flex_col()
                    .gap(px(8.))
                    .children(entries.iter().enumerate().map(|(i, e)| {
                        let e = e.clone();
                        let ename = e.name.clone();
                        div()
                            .id(("prompt-row", i))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(theme::stroke_a(0.12))
                            .bg(theme::bg_1().opacity(0.55))
                            .px(px(14.))
                            .h(px(52.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme::text())
                                            .child(e.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::text_faint())
                                            .child(if e.body.lines().count() > 1 {
                                                format!("{} 行", e.body.lines().count())
                                            } else {
                                                "单行".to_string()
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .id(("prompt-enabled", i))
                                            .cursor_pointer()
                                            .on_click({
                                                let en = ename.clone();
                                                cx.listener(move |this, _, _, cx| {
                                                    this.prompts.toggle_enabled(&en, cx);
                                                })
                                            })
                                            .child(if e.enabled {
                                                widgets::pill("启用", theme::jade())
                                            } else {
                                                widgets::pill("停用", theme::text_dim())
                                            }),
                                    )
                                    .child(
                                        widgets::ghost_button(("prompt-edit", i), "编辑").on_click({
                                            let en = ename.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.prompts.open_edit(&en, cx);
                                            })
                                        }),
                                    )
                                    .child(
                                        widgets::danger_button(("prompt-del", i), "删除").on_click({
                                            let en = ename.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.prompts.request_delete(&en, cx);
                                            })
                                        }),
                                    ),
                            )
                    }))})
            .child(if self.prompts.dialog_open {
                self.render_prompt_dialog(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if let Some(name) = confirm_delete {
                self.render_prompt_confirm_delete(&name, cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.prompts.confirm_close {
                self.render_prompt_confirm_close(cx).into_any_element()
            } else {
                div().into_any_element()
            })
    }

    /// Prompts 新增/编辑对话框。
    fn render_prompt_dialog(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(form) = &self.prompts.form else {
            return div().into_any_element();
        };
        let is_edit = self.prompts.editing_name.is_some();
        let name = form.name.clone();
        let body = form.body.clone();

        div()
            .id("prompt-dialog-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("prompt-dialog")
                    .w(px(520.))
                    .flex_col()
                    .gap(px(12.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.2))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child(if is_edit { "编辑提示词条目" } else { "添加提示词条目" }),
                    )
                    .child(self.skill_field("条目名", name))
                    .child(
                        div()
                            .flex_col()
                            .gap(px(6.))
                            .child(widgets::section_label("正文（Markdown）"))
                            .child(
                                div()
                                    .px(px(8.))
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(theme::stroke_a(0.16))
                                    .bg(theme::surface_a(0.5))
                                    .child(body),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label("启用"))
                            .child(
                                div()
                                    .id("prompt-form-enabled")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(f) = &mut this.prompts.form {
                                            f.enabled = !f.enabled;
                                        }
                                        cx.notify();
                                    }))
                                    .child(if form.enabled {
                                        widgets::pill("启用", theme::jade())
                                    } else {
                                        widgets::pill("停用", theme::text_dim())
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("prompt-cancel", 0usize), "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.prompts.request_close(cx);
                                    }),
                                ),
                            )
                            .child(
                                widgets::primary_button(("prompt-save", 0usize), "保存").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.prompts.save(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Prompts 删除确认浮层。
    fn render_prompt_confirm_delete(&self, name: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let name = name.to_string();
        div()
            .id("prompt-delete-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("prompt-delete")
                    .w(px(380.))
                    .flex_col()
                    .gap(px(14.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::danger_a(0.4))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("删除提示词条目"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child(format!(
                                "确认删除条目 “{name}”？此操作仅从库移除，不影响已写回的文件。"
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("prompt-del-cancel", 0usize), "取消")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.prompts.confirm_delete = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                widgets::danger_button(("prompt-del-confirm", 0usize), "删除")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.prompts.confirm_delete(cx);
                                    })),
                            ),
                    ),
            )
    }

    /// Prompts 未保存更改确认浮层（关闭对话框前）。
    fn render_prompt_confirm_close(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("prompt-close-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.55))
            .child(
                div()
                    .id("prompt-close")
                    .w(px(380.))
                    .flex_col()
                    .gap(px(14.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::danger_a(0.5))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child("未保存的更改"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_dim())
                            .child("此条目有未保存的更改。关闭将丢弃这些更改。"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button(("prompt-close-back", 0usize), "继续编辑")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.prompts.confirm_close = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                widgets::danger_button(("prompt-close-discard", 0usize), "丢弃并关闭")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.prompts.confirm_close = false;
                                        this.prompts.dialog_open = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    /// 路由开关指示（启用/禁用徽章）。
    fn render_routing_switch(
        &self,
        _tool: Tool,
        enabled: bool,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if enabled {
            widgets::pill("已启用", theme::jade())
        } else {
            widgets::pill("未启用", theme::text_dim())
        }
    }

    /// 通用供应商页：跨应用共享配置列表。
    fn render_universal_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let universals = {
            let store = self.store.read().expect("store lock");
            store.config().universal.values().cloned().collect::<Vec<_>>()
        };
        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(42.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::stroke_a(0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("通用供应商"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(theme::text_faint())
                                    .child(format!("{} 个", universals.len())),
                            )
                            .child(
                                widgets::ghost_button("uni-add", "＋ 添加通用供应商")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.universal_open = true;
                                        this.universal_edit = None;
                                        this.universal_form = crate::universal::UniversalForm::default();
                                        this.universal_inputs
                                            .name
                                            .update(cx, |f, _| f.set_text(""));
                                        this.universal_inputs
                                            .provider_type
                                            .update(cx, |f, _| f.set_text("newapi"));
                                        this.universal_inputs
                                            .base_url
                                            .update(cx, |f, _| f.set_text(""));
                                        this.universal_inputs
                                            .api_key
                                            .update(cx, |f, _| f.set_text(""));
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("uni-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(16.))
                    .flex_col()
                    .gap(px(10.))
                    .children(if universals.is_empty() {
                        vec![
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .py(px(40.))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(theme::text_faint())
                                        .child("尚无通用供应商，点击右上角添加"),
                                )
                                .into_any_element(),
                        ]
                    } else {
                        universals
                            .iter()
                            .map(|u| self.render_universal_card(u.clone(), cx).into_any_element())
                            .collect()
                    }),
            )
    }

    /// 通用供应商卡片。
    fn render_universal_card(
        &self,
        u: ns_presets::UniversalProvider,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let app_count = [u.apps.claude, u.apps.codex, u.apps.gemini]
            .iter()
            .filter(|b| **b)
            .count();
        let uid = u.id.clone();
        div()
            .rounded(px(12.))
            .p(px(14.))
            .bg(theme::bg_1().opacity(0.72))
            .border_1()
            .border_color(theme::stroke_a(0.12))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(34.))
                            .rounded(px(9.))
                            .bg(theme::accent_a(0.14))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(15.))
                            .text_color(theme::accent())
                            .child("◈"),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(u.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(theme::text_faint())
                                    .child(format!("{} · {}", u.provider_type, u.base_url)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(widgets::pill(
                        format!("{app_count}/3 应用"),
                        theme::jade(),
                    ))
                    .child({
                        let edit_id = uid.clone();
                        widgets::ghost_button(
                            gpui::ElementId::from(gpui::SharedString::from(format!(
                                "uni-edit-{}",
                                uid
                            ))),
                            "编辑",
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let id = edit_id.clone();
                            this.open_universal_edit(&id, cx);
                        }))
                    })
                    .child({
                        let sync_id = uid.clone();
                        widgets::ghost_button(
                            gpui::ElementId::from(gpui::SharedString::from(format!(
                                "uni-sync-{}",
                                uid
                            ))),
                            "同步到应用",
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let id = sync_id.clone();
                            let result = {
                                let mut store = this.store.write().expect("store lock");
                                store.sync_universal_children(&id)
                            };
                            this.status = match result {
                                Ok(()) => format!("已同步通用供应商子配置"),
                                Err(e) => format!("同步失败: {e}"),
                            };
                            cx.notify();
                        }))
                    })
                    .child({
                        let del_id = uid.clone();
                        widgets::ghost_button(
                            gpui::ElementId::from(gpui::SharedString::from(format!(
                                "uni-del-{}",
                                uid
                            ))),
                            "删除",
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let id = del_id.clone();
                            this.universal_confirm_delete = Some(id);
                            cx.notify();
                        }))
                    }),
            )
    }

    /// 打开通用供应商编辑对话框（预填）。
    fn open_universal_edit(&mut self, id: &str, cx: &mut Context<Self>) {
        let u = self
            .store
            .read()
            .expect("store lock")
            .config()
            .universal
            .get(id)
            .cloned();
        let Some(u) = u else {
            self.status = "通用供应商不存在".into();
            cx.notify();
            return;
        };
        self.universal_edit = Some(id.to_string());
        self.universal_form = crate::universal::UniversalForm {
            claude: u.apps.claude,
            codex: u.apps.codex,
            gemini: u.apps.gemini,
        };
        self.universal_inputs
            .name
            .update(cx, |f, _| f.set_text(&u.name));
        self.universal_inputs
            .provider_type
            .update(cx, |f, _| f.set_text(&u.provider_type));
        self.universal_inputs
            .base_url
            .update(cx, |f, _| f.set_text(&u.base_url));
        self.universal_inputs
            .api_key
            .update(cx, |f, _| f.set_text(&u.api_key));
        self.universal_inputs
            .claude_model
            .update(cx, |f, _| {
                f.set_text(
                    u.models
                        .claude
                        .as_ref()
                        .and_then(|m| m.model.clone())
                        .unwrap_or_default()
                        .as_str(),
                )
            });
        self.universal_inputs
            .codex_model
            .update(cx, |f, _| {
                f.set_text(
                    u.models
                        .codex
                        .as_ref()
                        .and_then(|m| m.model.clone())
                        .unwrap_or_default()
                        .as_str(),
                )
            });
        self.universal_inputs
            .gemini_model
            .update(cx, |f, _| {
                f.set_text(
                    u.models
                        .gemini
                        .as_ref()
                        .and_then(|m| m.model.clone())
                        .unwrap_or_default()
                        .as_str(),
                )
            });
        self.universal_open = true;
        cx.notify();
    }


    /// 渲染应用管理页：受管 CLI 工具应用清单（能力/当前供应商/配置路径）。
    fn render_apps_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (providers, current) = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            (cfg.clone(), cfg.current.clone())
        };
        div()
            .id("apps-scroll")
            .flex_1()
            .h_full()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme::text())
                                    .child("应用"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::text_faint())
                                    .child("受管 AI 编程助手 CLI 应用清单"),
                            ),
                    )
                    .child(
                        widgets::pill(
                            format!("{} 个应用", Tool::ALL.len()),
                            theme::accent(),
                        ),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(8.))
                    .children(Tool::ALL.iter().map(|&tool| {
                        let cur = current.get(tool).and_then(|id| providers.providers.get(id));
                        let prov_count = providers.providers_for(tool).len();
                        let route_on = providers.settings.routing_for(tool).enabled;
                        div()
                            .id(("app-card", tool as usize))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .p(px(12.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(theme::stroke_a(0.12))
                            .bg(theme::bg_1().opacity(0.55))
                            .child(
                                div()
                                    .w(px(30.))
                                    .h(px(30.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(8.))
                                    .bg(theme::accent_a(0.14))
                                    .text_size(px(14.))
                                    .text_color(theme::accent())
                                    .child(tool.glyph()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme::text())
                                            .child(tool.label()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::text_faint())
                                            .child(format!(
                                                "{} · {} 个供应商 · {}",
                                                tool.id_str(),
                                                prov_count,
                                                if cur.is_some() {
                                                    format!("当前：{}", cur.unwrap().name)
                                                } else {
                                                    "未设置".to_string()
                                                }
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(if tool.is_switch() {
                                        widgets::pill("切换", theme::jade())
                                    } else {
                                        widgets::pill("共存", theme::accent())
                                    })
                                    .child(if tool.supports_routing() {
                                        widgets::pill(
                                            if route_on { "路由中" } else { "路由" },
                                            if route_on {
                                                theme::jade()
                                            } else {
                                                theme::text_dim()
                                            },
                                        )
                                    } else {
                                        div()
                                    })
                                    .child(if tool.supports_aggregation() {
                                        widgets::pill("聚合", theme::color(theme::ACCENT_2))
                                    } else {
                                        div()
                                    }),
                            )
                            .child(
                                widgets::ghost_button(("app-open", tool as usize), "查看供应商")
                                    .on_click({
                                        let t = tool;
                                        cx.listener(move |this, _, _, cx| {
                                            this.select_tool(t, cx);
                                            this.select_page(Page::Providers, cx);
                                        })
                                    }),
                            )
                    })),
            )
    }


    /// 切换某工具的路由模式：启用 → 启动代理 + 写 live 配置；停用 → 停止代理。
    fn toggle_routing(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let (enabled, provider) = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            let enabled = cfg.settings.routing_for(tool).enabled;
            let provider = cfg
                .current
                .get(tool)
                .and_then(|id| cfg.providers.get(id).cloned());
            (enabled, provider)
        };
        if enabled {
            self.disable_routing(tool, cx);
        } else {
            self.enable_routing(tool, provider, cx);
        }
        self.refresh_tray_menu(cx);
    }

    fn enable_routing(
        &mut self,
        tool: Tool,
        provider: Option<ns_presets::Provider>,
        cx: &mut Context<Self>,
    ) {
        // 官方供应商路由边界：官方默认不走代理（直连更优）。
        // 例外：Codex OpenAI 官方（认证经代理格式转换，需代理）。
        if !matches!(provider.as_ref().map(|p| p.category), Some(Some(ns_presets::ProviderCategory::Official))) {
            // 非官方或未知类别：继续。
        } else if let Some(p) = &provider {
            let is_codex_openai = tool == Tool::Codex && p.id == "codex-official";
            if !is_codex_openai {
                self.routing_status.insert(
                    tool.id_str().to_string(),
                    "官方供应商默认直连，不走代理".to_string(),
                );
                cx.notify();
                return;
            }
        }
        // 聚合模式：多上游 + 按模型路由；否则单上游。
        let (upstreams, model_routes, status_note) = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            let agg = cfg.aggregation_for(tool);
            if agg.enabled && !agg.provider_ids.is_empty() {
                let mut refs = Vec::new();
                for id in &agg.provider_ids {
                    if let Some(p) = cfg.providers.get(id) {
                        refs.push(p);
                    }
                }
                let upstreams = crate::routing::provider_upstreams(tool, &refs);
                if upstreams.is_empty() {
                    (Vec::new(), None, "聚合供应商缺少可路由的上游配置".to_string())
                } else {
                    let routes = crate::routing::aggregation_model_routes(&refs);
                    (upstreams, Some(routes), String::new())
                }
            } else {
                match provider.as_ref().and_then(|p| crate::routing::provider_upstream(tool, p)) {
                    Some(u) => (vec![u], None, String::new()),
                    None => (Vec::new(), None, "供应商缺少可路由的上游配置".to_string()),
                }
            }
        };
        if upstreams.is_empty() {
            self.routing_status
                .insert(tool.id_str().to_string(), status_note);
            cx.notify();
            return;
        }
        let (listen_addr, failover_enabled, breaker_threshold, breaker_cooldown, rectify_enabled, rectify_thinking, rectify_image) = {
            let s = self.store.read().expect("store lock");
            let rc = s.config().settings.routing_for(tool);
            (
                rc.listen_addr.clone(),
                rc.failover,
                rc.breaker_threshold,
                rc.breaker_cooldown,
                rc.rectify_enabled,
                rc.rectify_thinking,
                rc.rectify_image,
            )
        };
        let tool = tool;
        let mut rt = std::mem::take(&mut self.routing);
        cx.spawn(async move |this, cx| {
            let port = match rt
                .ensure_running_with(
                    tool,
                    upstreams,
                    model_routes,
                    &listen_addr,
                    failover_enabled,
                    breaker_threshold,
                    breaker_cooldown,
                    rectify_enabled,
                    rectify_thinking,
                    rectify_image,
                )
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.routing = rt;
                        this.routing_status
                            .insert(tool.id_str().to_string(), format!("启动失败: {e}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            this.update(cx, |this, cx| {
                this.routing = rt;
                // 写路由 live 配置（指向 127.0.0.1:<port> + 占位 key）。
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
                let backup = this.store.read().expect("store lock").backups_dir();
                let cdirs = this.config_dirs();
                match home.as_deref().map(|h| {
                    crate::switch_engine::switch_to_routing_in_dirs(tool, port, &backup, h, &cdirs)
                }) {
                    Some(Ok(outcome)) => {
                        let mut store = this.store.write().expect("store lock");
                        let mut rc = store.config().settings.routing_for(tool);
                        rc.enabled = true;
                        rc.port = port;
                        let save = store.set_routing(tool, rc);
                        if let Err(e) = save {
                            this.routing_status
                                .insert(tool.id_str().to_string(), format!("保存失败: {e}"));
                            cx.notify();
                            return;
                        }
                        this.routing_status.insert(
                            tool.id_str().to_string(),
                            format!("已启用 · 写入 {} 个文件", outcome.written),
                        );
                    }
                    Some(Err(e)) => {
                        this.routing_status
                            .insert(tool.id_str().to_string(), format!("写入失败: {e}"));
                    }
                    None => {
                        this.routing_status
                            .insert(tool.id_str().to_string(), "无法定位 HOME".into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn disable_routing(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.routing.stop(tool);
        let mut store = self.store.write().expect("store lock");
        let mut rc = store.config().settings.routing_for(tool);
        rc.enabled = false;
        let _ = store.set_routing(tool, rc);
        drop(store);
        self.routing_status
            .insert(tool.id_str().to_string(), "已停用".into());
        cx.notify();

    }

    /// 退出时把路由模式写回的占位配置还原为真实供应商直连配置。
    ///
    /// 遍历开启了路由的工具：读当前供应商 → `switch_to_in` 写回真实
    /// live 配置（与未开路由时一致）。失败不影响退出（仅记录状态）。
    fn write_back_routing_direct(&mut self) {
        let (routed, home, backup) = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            let routed: Vec<(Tool, Option<ns_presets::Provider>)> = Tool::ALL
                .into_iter()
                .filter(|t| cfg.settings.routing_for(*t).enabled)
                .map(|t| {
                    let p = cfg
                        .current
                        .get(t)
                        .and_then(|id| cfg.providers.get(id).cloned());
                    (t, p)
                })
                .collect();
            (
                routed,
                std::env::var_os("HOME").map(std::path::PathBuf::from),
                store.backups_dir(),
            )
        };
        let Some(home) = home else {
            return;
        };
        for (tool, provider) in routed {
            let Some(provider) = provider else {
                continue;
            };
            // 写回直连（与 switch_to_in 一致）。
            let cdirs = self.config_dirs();
            let _ = crate::switch_engine::switch_to_in_dirs(tool, &provider, &backup, &home, &cdirs);
            let mut store = self.store.write().expect("store lock");
            let mut rc = store.config().settings.routing_for(tool);
            rc.enabled = false;
            let _ = store.set_routing(tool, rc);
        }
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .child(
                div()
                    .size(px(64.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme::accent_a(0.08))
                    .border_1()
                    .border_color(theme::stroke_a(0.14))
                    .child(
                        div()
                            .text_size(px(26.))
                            .text_color(theme::accent())
                            .child(self.active_tool.glyph()),
                    ),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(theme::text())
                    .child(format!("还没有 {} 的供应商", self.active_tool.label())),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::text_faint())
                    .child("点击右上角「添加供应商」从预设库选择"),
            )
            .child(
                widgets::ghost_button("empty-add", "＋ 添加供应商")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_add_dialog(cx);
                    })),
            )
    }

    fn render_provider_card(
        &self,
        i: usize,
        p: ns_presets::Provider,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let is_current = self
            .store
            .read()
            .expect("store lock")
            .config()
            .current
            .get(self.active_tool)
            .map(|id| id == &p.id)
            .unwrap_or(false);
        let menu_open = self.open_menu.as_deref() == Some(p.id.as_str());
        // 复制后的高亮反馈（新副本短暂描边）。
        let copied = self.copied_provider.as_deref() == Some(p.id.as_str());
        let _ = copied;

        let p_id = p.id.clone();
        let api_fmt = p
            .meta
            .as_ref()
            .and_then(|m| m.api_format.as_deref())
            .unwrap_or("anthropic")
            .to_string();
        let show_fmt = !matches!(api_fmt.as_str(), "" | "anthropic");
        let fmt_label = match api_fmt.as_str() {
            "openai_chat" => "OpenAI Chat".to_string(),
            "openai_responses" => "OpenAI Resp".to_string(),
            "gemini_native" => "Gemini".to_string(),
            _ => api_fmt.clone(),
        };
        let subtitle = p
            .notes
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .or_else(|| {
                p.website_url.as_deref().and_then(|u| {
                    u.trim()
                        .trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .split('/')
                        .next()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                })
            })
            .unwrap_or_else(|| p.id.clone());
        // 模型名（卡片副标题第二行，对齐 cc-switch 显示当前模型）。
        let model = provider_model(&p);
        let is_official = provider_is_official(&p);
        let icon_bg = p
            .icon_color
            .as_deref()
            .and_then(parse_hex_color)
            .map(|c| gpui::rgb(c).into())
            .unwrap_or_else(|| theme::accent());
        // 是否在故障转移队列（健康芯片显示依据）。
        let in_queue = p.in_failover_queue;
        // 模式色边框：当前那张用强调色；复制后的副本用玉色高亮描边。
        let (card_bg, card_border) = if copied {
            (theme::surface_alt_a(0.6), theme::jade_a(0.55))
        } else if is_current {
            (theme::surface_alt_a(0.6), theme::accent_a(0.45))
        } else {
            (theme::surface_a(0.4), theme::stroke_a(0.1))
        };
        div()
            .id(("provider-card", i))
            .min_h(px(58.))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(12.))
            .relative()
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .hover(|style| {
                style
                    .border_color(theme::stroke_a(0.24))
                    .bg(theme::surface_alt_a(0.5))
            })
            // 左拖拽手柄（对齐 cc-switch dragHandle；整卡可拖，手柄为视觉指示）
            .child(
                div()
                    .w(px(16.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme::text_faint())
                    .text_size(px(12.))
                    .child("⠿"),
            )
            // 图标
            .child(
                div()
                    .size(px(36.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(icon_bg)
                    .border_1()
                    .border_color(theme::stroke_a(0.18))
                    .text_color(theme::dark_text())
                    .text_size(px(15.))
                    .child(icon_glyph(
                        p.icon.as_deref().unwrap_or(""),
                        p.name.chars().next().unwrap_or('?'),
                    )),
            )
            // 名称/芯片 + 副标题
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(theme::text())
                                    .child(p.name.clone()),
                            )
                            .child(if is_current {
                                widgets::pill("当前", theme::accent())
                            } else {
                                div()
                            })
                            .child({
                                let store = self.store.read().expect("store lock");
                                let routing = store.config().settings.routing_for(self.active_tool);
                                let agg = store.config().aggregation_for(self.active_tool);
                                let mode_pill = if routing.enabled {
                                    Some(("路由", theme::jade()))
                                } else if agg.enabled {
                                    Some(("聚合", theme::color(theme::ACCENT_2)))
                                } else {
                                    Some(("直连", theme::text_faint()))
                                };
                                match mode_pill {
                                    Some((label, tone)) => widgets::pill(label, tone),
                                    None => div(),
                                }
                            })
                            .child(if show_fmt {
                                widgets::pill(fmt_label, theme::jade())
                            } else {
                                div()
                            })
                            .child(if in_queue {
                                widgets::pill("故障转移", theme::color(theme::ACCENT_2))
                            } else {
                                div()
                            })
                            .child(if is_coding_plan(&p.name) {
                                widgets::pill("Coding Plan", theme::cinnabar())
                            } else {
                                div()
                            })
                            .child({
                                let health = self.health.get(&p_id);
                                match health {
                                    Some((true, _)) => {
                                        widgets::pill("在线", theme::jade())
                                    }
                                    Some((false, _)) => {
                                        widgets::pill("离线", theme::danger())
                                    }
                                    None => div(),
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(theme::text_faint())
                            .child(subtitle),
                    )
                    .child(if let Some(m) = model {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(theme::text_dim())
                                    .child(format!("模型 {m}")),
                            )
                            .child(if is_official {
                                widgets::pill("官方", theme::jade())
                            } else {
                                div()
                            })
                    } else {
                        div()
                    }),
            )
            // 右侧额度列（固定宽，对齐 cc-switch QuotaLines；仅用量查询启用时显示）。
            .child({
                let tool_key = self.active_tool.id_str().to_string();
                let usage_enabled = p
                    .meta
                    .as_ref()
                    .map(|m| m.usage_enabled)
                    .unwrap_or(true);
                let usage = if usage_enabled {
                    self.usage.get(&tool_key).cloned()
                } else {
                    None
                };
                match usage {
                    Some(u) if u.remaining.is_some() || u.total.is_some() => {
                        let mut line = String::new();
                        let unit = if u.unit.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", u.unit)
                        };
                        let currency = u.currency.as_deref().unwrap_or("");
                        if let Some(total) = u.total {
                            line.push_str(&format!("{total}{unit}{currency}"));
                            if let Some(remaining) = u.remaining {
                                line.push_str(&format!(" · 剩 {remaining}{unit}{currency}"));
                            }
                        } else if let Some(remaining) = u.remaining {
                            line.push_str(&format!("剩 {remaining}{unit}{currency}"));
                        }
                        div()
                            .w(px(150.))
                            .flex_col()
                            .items_end()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(theme::text_dim())
                                    .child(line),
                            )
                            .child(if let (Some(total), Some(used)) = (u.total, u.used) {
                                if total > 0.0 {
                                    let pct = (used / total).clamp(0.0, 1.0);
                                    let color = if pct >= 0.9 {
                                        theme::danger()
                                    } else if pct >= 0.7 {
                                        theme::cinnabar()
                                    } else {
                                        theme::jade()
                                    };
                                    div()
                                        .w(px(110.))
                                        .h(px(4.))
                                        .rounded_full()
                                        .bg(theme::stroke_a(0.12))
                                        .child(
                                            div()
                                                .h_full()
                                                .w(px((110. * pct) as f32))
                                                .rounded_full()
                                                .bg(color),
                                        )
                                } else {
                                    div()
                                }
                            } else {
                                div()
                            })
                            .child(if let Some(reset_at) = u.reset_at {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs() as i64)
                                    .unwrap_or(0);
                                if reset_at > now {
                                    let secs = reset_at - now;
                                    let h = secs / 3600;
                                    let m = (secs % 3600) / 60;
                                    div()
                                        .text_size(px(9.5))
                                        .text_color(theme::text_faint())
                                        .child(format!("⏱ {h}h{m:02}m"))
                                } else {
                                    div()
                                }
                            } else {
                                div()
                            })
                            .into_any_element()
                    }
                    _ => div().into_any_element(),
                }
            })
            // 主操作位 + 更多
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(if is_current {
                        // 当前供应商：状态文本（cc-switch 卡片右侧状态）。
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(widgets::status_dot(theme::jade(), 7.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::jade())
                                    .child("使用中"),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .id(("card-switch", i))
                            .cursor_pointer()
                            .h(px(30.))
                            .px(px(12.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .text_size(px(11.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::dark_text())
                            .bg(theme::gold_gradient())
                            .child("切换")
                            .on_click(cx.listener({
                                let switch_id = p_id.clone();
                                move |this, _, _, cx| {
                                    this.switch_to_provider(&switch_id, cx);
                                }
                            }))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .id(("card-menu", i))
                            .cursor_pointer()
                            .size(px(30.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(theme::text_dim())
                            .text_size(px(14.))
                            .hover(|s| s.bg(theme::stroke_a(0.1)))
                            .child("⋯")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_menu = if menu_open { None } else { Some(p_id.clone()) };
                                cx.notify();
                            })),
                    ),
            )
            .child(if menu_open {
                self.render_card_menu(i, p.clone(), cx)
            } else {
                div()
            })
    }

    fn render_card_menu(&self, i: usize, p: ns_presets::Provider, cx: &mut Context<Self>) -> Div {
        let has_site = p.website_url.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false);
        let mut col = div()
            .absolute()
            .top(px(64.))
            .right(px(12.))
            .w(px(190.))
            .rounded(px(10.))
            .py(px(6.))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::stroke_a(0.2))
            .shadow_lg()
            .flex_col()
            .child(self.menu_item_action("测试连接", ("menu-test", i), p.id.clone(), cx, |this, id, cx| {
                this.test_provider_connectivity(&id, cx);
            }))
            .child(self.menu_item_action("编辑", ("menu-edit", i), p.id.clone(), cx, |this, id, cx| {
                this.open_edit_dialog(&id, cx);
            }));
        if has_site {
            col = col.child(self.menu_item_action("打开官网", ("menu-site", i), p.id.clone(), cx, |this, id, cx| {
                this.open_provider_website(&id, cx);
            }));
        }
        col.child(self.menu_item_action("终端打开配置目录", ("menu-term", i), p.id.clone(), cx, |this, id, cx| {
            this.open_tool_config_terminal(&id, cx);
        }))
        .child(self.menu_item_action("复制供应商", ("menu-dup", i), p.id.clone(), cx, |this, id, cx| {
            this.duplicate_provider(&id, cx);
        }))
        .child(self.menu_item_action("配置用量", ("menu-usage", i), p.id.clone(), cx, |this, id, cx| {
            this.configure_usage(&id, cx);
        }))
        .child(self.menu_item_action(
            if p.meta.as_ref().map(|m| m.usage_enabled).unwrap_or(true) {
                "停用用量查询"
            } else {
                "启用用量查询"
            },
            ("menu-usage-toggle", i),
            p.id.clone(),
            cx,
            |this, id, cx| {
                this.toggle_usage_enabled(&id, cx);
            },
        ))
        .child(self.menu_item_action("导出此供应商", ("menu-exp", i), p.id.clone(), cx, |this, id, cx| {
            this.export_single_provider(&id, cx);
        }))
        .child(
            div()
                .h(px(1.))
                .my(px(4.))
                .bg(theme::stroke_a(0.12)),
        )
        .child(self.menu_item_danger_action(("menu-del", i), p.id.clone(), cx))
    }

    /// 顶部项目切换菜单（B10：默认隐藏，设置开启后显示）。
    fn render_project_menu(&self, cx: &mut Context<Self>) -> Div {
        let projects = self
            .store
            .read()
            .map(|s| s.list_projects())
            .unwrap_or_default();
        div()
            .absolute()
            .top(px(52.))
            .left(px(14.))
            .w(px(220.))
            .rounded(px(10.))
            .py(px(6.))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::stroke_a(0.2))
            .shadow_lg()
            .flex_col()
            .child(if projects.is_empty() {
                div()
                    .px(px(12.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .text_color(theme::text_faint())
                    .child("暂无项目（项目页可保存）")
                    .into_any_element()
            } else {
                div()
                    .flex_col()
                    .children(
                        projects
                            .iter()
                            .enumerate()
                            .map(|(i, name)| {
                                let name = name.clone();
                                div()
                                    .id(("project-item", i))
                                    .px(px(12.))
                                    .h(px(30.))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .text_size(px(12.))
                                    .text_color(theme::text_dim())
                                    .hover(|s| s.bg(theme::stroke_a(0.08)))
                                    .child(name.clone())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_menu = None;
                                        let current_extras = this.project_extras(cx);
                                        let r = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.apply_project(&name, ns_store::Store::scope_for(this.active_tool), &current_extras)
                                        };
                                        match r {
                                            Ok(ext) => {
                                                this.apply_project_extras(&ext, cx);
                                            }
                                            Err(e) => {
                                                this.status = format!("切换失败: {e}");
                                                cx.notify();
                                            }
                                        }
                                    }))
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .into_any_element()
            })
    }
    fn menu_item_action<F>(
        &self,
        label: &str,
        element_id: impl Into<gpui::ElementId>,
        id: String,
        cx: &mut Context<Self>,
        action: F,
    ) -> impl IntoElement
    where
        F: Fn(&mut Self, String, &mut Context<Self>) + 'static,
    {
        let label = label.to_string();
        div()
            .id(element_id)
            .px(px(12.))
            .h(px(30.))
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(theme::text_dim())
            .hover(|s| s.bg(theme::stroke_a(0.08)))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_menu = None;
                action(this, id.clone(), cx);
            }))
    }

    fn menu_item_danger_action(
        &self,
        element_id: impl Into<gpui::ElementId>,
        id: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(element_id)
            .px(px(12.))
            .h(px(30.))
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(theme::danger())
            .hover(|s| s.bg(theme::danger_a(0.12)))
            .child("删除")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_menu = None;
                this.confirm_delete = Some(id.clone());
                cx.notify();
            }))
    }

    /// 从供应商 settings_config 提取可测试的端点 URL。
    fn provider_endpoint(p: &ns_presets::Provider) -> Option<String> {
        fn find(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
            match value {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        if let Some(s) = v.as_str() {
                            let s = s.trim();
                            if !s.is_empty()
                                && keys
                                    .iter()
                                    .any(|k2| k.to_lowercase().contains(&k2.to_lowercase()))
                            {
                                return Some(s.to_string());
                            }
                        }
                    }
                    map.values().find_map(|v| find(v, keys))
                }
                _ => None,
            }
        }
        let url = find(
            &p.settings_config,
            &[
                "base_url", "endpoint", "baseurl", "url", "host", "server", "api_base",
            ],
        )?;
        let url = url.trim().trim_end_matches('/').to_string();
        if url.starts_with("http://") || url.starts_with("https://") {
            Some(url)
        } else if url.contains('.') || url.starts_with("localhost") || url.starts_with("127.") {
            Some(format!("https://{url}"))
        } else {
            None
        }
    }

    fn get_provider(&self, id: &str) -> Option<ns_presets::Provider> {
        self.store
            .read()
            .expect("store lock")
            .config()
            .providers
            .get(id)
            .cloned()
    }

    fn test_provider_connectivity(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.get_provider(id) else { return; };
        let Some(url) = Self::provider_endpoint(&p) else {
            self.status = format!("测试连接失败（{}）：未找到可用的端点 URL", p.name);
            cx.notify();
            return;
        };
        let name = p.name.clone();
        let pid = id.to_string();
        self.status = format!("正在测试连接：{name} …");
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let out = std::process::Command::new("curl")
                        .args([
                            "-sS",
                            "-o",
                            "/dev/null",
                            "-m",
                            "8",
                            "-w",
                            "%{http_code} %{time_total}",
                            "-I",
                            &url,
                        ])
                        .output();
                    match out {
                        Ok(o) => {
                            let raw = String::from_utf8_lossy(&o.stdout);
                            let mut parts = raw.split_whitespace();
                            let code = parts.next().unwrap_or_default().to_string();
                            let secs = parts
                                .next()
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);
                            let latency = format!("{} ms", (secs * 1000.0).round() as u64);
                            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                            if (o.status.success() || !code.is_empty()) && code != "000" {
                                Ok(format!("HTTP {code} · {latency} · {url}"))
                            } else if err.is_empty() {
                                Err(format!("无法连接 · {latency} · {url}"))
                            } else {
                                Err(format!("{err} · {url}"))
                            }
                        }
                        Err(e) => Err(format!("执行 curl 失败: {e}")),
                    }
                })
                .await;
            let (ok, msg) = match result {
                Ok(ok) => (true, format!("连通性测试通过（{name}）：{ok}")),
                Err(e) => (false, format!("连通性测试失败（{name}）：{e}")),
            };
            this.update(cx, |this, cx| {
                this.health.insert(pid, (ok, msg.clone()));
                this.status = msg;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_provider_website(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.get_provider(id) else { return; };
        let Some(url) = p.website_url.as_ref().filter(|s| !s.trim().is_empty()) else {
            self.status = format!("{} 未配置官网地址", p.name);
            cx.notify();
            return;
        };
        #[cfg(target_os = "macos")]
        let r = std::process::Command::new("open").arg(url).spawn();
        #[cfg(target_os = "linux")]
        let r = std::process::Command::new("xdg-open").arg(url).spawn();
        #[cfg(target_os = "windows")]
        let r = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let r: std::io::Result<std::process::Child> =
            Err(std::io::Error::other("unsupported platform"));
        match r {
            Ok(_) => self.status = format!("已打开官网：{}", p.name),
            Err(e) => self.status = format!("打开官网失败：{e}"),
        }
        cx.notify();
    }

    /// 当前配置目录覆盖表（key = 工具 id_str，value = 绝对路径）。
    fn config_dirs(&self) -> crate::switch_engine::ConfigDirs {
        self.store
            .read()
            .ok()
            .map(|s| s.config().settings.config_dirs.clone())
            .unwrap_or_default()
    }

    fn tool_config_dir(&self, tool: ns_presets::Tool) -> Option<std::path::PathBuf> {
        // 用户覆盖优先（对齐 cc-switch DirectorySettings）。
        let override_dir = self
            .store
            .read()
            .ok()
            .and_then(|s| s.config().settings.config_dir_for(tool).map(str::to_string));
        if let Some(d) = override_dir {
            return Some(std::path::PathBuf::from(d));
        }
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
        Some(tool.default_config_dir(&home))
    }

    fn open_tool_config_terminal(&mut self, _id: &str, cx: &mut Context<Self>) {
        let Some(dir) = self.tool_config_dir(self.active_tool) else {
            self.status = "无法定位 HOME 目录".into();
            cx.notify();
            return;
        };
        let target = if dir.exists() {
            dir.clone()
        } else if let Some(p) = dir.parent().filter(|p| p.exists()) {
            p.to_path_buf()
        } else {
            std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or(dir)
        };
        #[cfg(target_os = "macos")]
        let r = std::process::Command::new("open")
            .args(["-a", "Terminal"])
            .arg(&target)
            .spawn();
        #[cfg(target_os = "linux")]
        let r = std::process::Command::new("x-terminal-emulator")
            .arg(format!("--working-directory={}", target.display()))
            .spawn()
            .or_else(|_| std::process::Command::new("xdg-open").arg(&target).spawn());
        #[cfg(target_os = "windows")]
        let r = std::process::Command::new("cmd")
            .args([
                "/C",
                "start",
                "cmd",
                "/K",
                &format!("cd /d {}", target.display()),
            ])
            .spawn();
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let r: std::io::Result<std::process::Child> =
            Err(std::io::Error::other("unsupported platform"));
        match r {
            Ok(_) => self.status = format!("已在终端打开：{}", target.display()),
            Err(e) => self.status = format!("打开终端失败：{e}"),
        }
        cx.notify();
    }

    /// 立即备份 `config.json` 到 `~/.ns-switch/backups/manual/`。
    fn create_manual_backup(&mut self, cx: &mut Context<Self>) {
        let r = self.store.read().expect("store lock").manual_backup();
        match r {
            Ok(p) => {
                self.backup_status = format!("已备份配置 → {}", p.display());
                self.status = self.backup_status.clone();
                self.status_tone = StatusTone::Success;
            }
            Err(e) => {
                self.backup_status = format!("备份失败：{e}");
                self.status = self.backup_status.clone();
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    /// 在系统文件管理器中打开备份根目录。
    fn open_backups_folder(&mut self, cx: &mut Context<Self>) {
        let dir = self.store.read().expect("store lock").backups_root();
        let _ = std::fs::create_dir_all(&dir);
        match open_folder(&dir) {
            Ok(()) => {
                self.backup_status = format!("已打开备份目录：{}", dir.display());
                self.status = self.backup_status.clone();
                self.status_tone = StatusTone::Neutral;
            }
            Err(e) => {
                self.backup_status = format!("打开备份目录失败：{e}");
                self.status = self.backup_status.clone();
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    /// 执行待确认的备份恢复（首写备份写回原文件 / 手动备份覆盖 config.json）。
    fn confirm_backup_restore(&mut self, cx: &mut Context<Self>) {
        let Some((kind, path, name)) = self.backup_restore_pending.take() else {
            return;
        };
        let result = match kind {
            BackupRestoreKind::Live => self
                .store
                .read()
                .expect("store lock")
                .restore_backup(&path)
                .map(|_| String::new()),
            BackupRestoreKind::Manual => self
                .store
                .write()
                .expect("store lock")
                .restore_manual_backup(&path)
                .map(|_| "（配置已重新载入）".to_string()),
        };
        match result {
            Ok(suffix) => {
                self.backup_status = format!("已从{}恢复：{name}{suffix}", kind.label());
                self.status = self.backup_status.clone();
                self.status_tone = StatusTone::Success;
            }
            Err(e) => {
                self.backup_status = format!("恢复失败：{e}");
                self.status = self.backup_status.clone();
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    /// 备份恢复确认浮层（恢复会覆盖现有文件，先确认）。
    fn render_backup_restore_confirm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (kind, _, name) = self
            .backup_restore_pending
            .clone()
            .expect("backup_restore_pending 非空");
        let (title, detail) = match kind {
            BackupRestoreKind::Live => (
                "恢复首写备份",
                "将把备份内容写回对应的工具客户端配置文件，当前 live 配置会被覆盖。",
            ),
            BackupRestoreKind::Manual => (
                "恢复手动备份",
                "将用该备份覆盖 ~/.ns-switch/config.json（供应商/设置/路由），当前配置会先存到 backups/pre-restore/。",
            ),
        };
        div()
            .id("backup-restore-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x000000).opacity(0.6))
            .child(
                div()
                    .id("backup-restore-confirm")
                    .w(px(460.))
                    .flex_col()
                    .gap(px(14.))
                    .p(px(22.))
                    .rounded(px(16.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.3))
                    .shadow_lg()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme::text())
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme::text())
                            .child(format!("{detail}\n备份：{name}")),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::cinnabar())
                            .child("⚠ 恢复后建议重启对应工具（Codex/Gemini/Grok/Claude 桌面版需重启）。"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("backup-restore-cancel", "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.backup_restore_pending = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::primary_button("backup-restore-go", "确认恢复").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.confirm_backup_restore(cx);
                                    }),
                                ),
                            ),
                    ),
            )
    }

    fn duplicate_provider(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.get_provider(id) else { return; };
        let new_id = format!("{}-copy-{}", p.id, now_ts_suffix());
        let mut dup = p.clone();
        dup.id = new_id.clone();
        dup.name = format!("{}（副本）", p.name);
        dup.created_at = Some(now_ts());
        let name = dup.name.clone();
        let res = self.store.write().expect("store lock").add_provider(dup);
        match res {
            Ok(()) => {
                self.status = format!("已复制供应商：{name}");
                self.status_tone = StatusTone::Success;
                // 新副本短暂高亮（复制后反馈定位）。
                self.copied_provider = Some(new_id);
            }
            Err(e) => {
                self.status = format!("复制供应商失败：{e}");
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
        // 复制高亮短暂展示后自动清除（反馈定位，不长期驻留）。
        let copied = self.copied_provider.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            this.update(cx, |this, cx| {
                if this.copied_provider == copied {
                    this.copied_provider = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn export_single_provider(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.get_provider(id) else { return; };
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let dir = home
            .unwrap_or_else(|| std::env::temp_dir())
            .join(".ns-switch")
            .join("exports");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.status = format!("导出失败：{e}");
            cx.notify();
            return;
        }
        let safe: String = p
            .id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{safe}.json"));
        let json = match serde_json::to_string_pretty(&serde_json::json!({ "providers": [p] })) {
            Ok(j) => j,
            Err(e) => {
                self.status = format!("导出失败：{e}");
                cx.notify();
                return;
            }
        };
        match std::fs::write(&path, json) {
            Ok(()) => {
                self.status = format!("已导出供应商到：{}", path.display());
                self.status_tone = StatusTone::Success;
            }
            Err(e) => {
                self.status = format!("导出失败：{e}");
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    /// 配置供应商用量脚本路径（直接打开编辑对话框并预填用量脚本字段）。
    /// 切换某供应商的用量查询启用/停用（直接写 meta，无需打开对话框）。
    fn toggle_usage_enabled(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.get_provider(id) else { return; };
        let mut provider = p;
        let mut meta = provider.meta.take().unwrap_or_default();
        meta.usage_enabled = !meta.usage_enabled;
        provider.meta = Some(meta);
        let name = provider.name.clone();
        let enabled = provider.meta.as_ref().map(|m| m.usage_enabled).unwrap_or(true);
        let result = {
            let mut store = self.store.write().expect("store lock");
            store.update_provider(provider)
        };
        match result {
            Ok(()) => {
                self.status = format!(
                    "{} 用量查询: {}",
                    name,
                    if enabled { "启用" } else { "停用" }
                );
                self.status_tone = StatusTone::Success;
            }
            Err(e) => {
                self.status = format!("保存失败: {e}");
                self.status_tone = StatusTone::Error;
            }
        }
        cx.notify();
    }

    fn configure_usage(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.get_provider(id) else { return; };
        self.open_edit_dialog(id, cx);
        // 预填用量脚本路径到对话框字段。
        let usage_script = p
            .meta
            .as_ref()
            .and_then(|m| m.usage_script.clone())
            .unwrap_or_default();
        self.dialog_inputs
            .usage_script
            .update(cx, |f, _| f.set_text(&usage_script));
        self.status = format!("配置用量脚本：{}", p.name);
        cx.notify();
    }

    /// 打开「添加供应商」对话框：清空上次编辑残留的表单与预设选择。
    fn open_add_dialog(&mut self, cx: &mut Context<Self>) {
        self.add_open = true;
        self.edit_id = None;
        self.dialog_preset = None;
        self.dialog_fields = dialog::DialogFields::default();
        let name = self.dialog_inputs.name.clone();
        let endpoint = self.dialog_inputs.endpoint.clone();
        let key = self.dialog_inputs.key.clone();
        let model = self.dialog_inputs.model.clone();
        let usage = self.dialog_inputs.usage_script.clone();
        let icon = self.dialog_inputs.icon.clone();
        let icon_color = self.dialog_inputs.icon_color.clone();
        let search = self.dialog_inputs.search.clone();
        for field in [&name, &endpoint, &key, &model, &usage, &icon, &icon_color, &search] {
            field.update(cx, |f, _| f.set_text(""));
        }
        cx.notify();
    }

    fn open_edit_dialog(&mut self, id: &str, cx: &mut Context<Self>) {
        let provider = {
            let store = self.store.read().expect("store lock");
            store.config().providers.get(id).cloned()
        };
        let Some(p) = provider else {
            return;
        };
        // 找到所属预设（按 id 前缀匹配），用于表单构建。
        let preset = ns_presets::presets_for(self.active_tool)
            .find(|pr| p.id.starts_with(&format!("{}-", pr.id)))
            .or_else(|| {
                ns_presets::search_presets("", None)
                    .into_iter()
                    .find(|pr| p.id.starts_with(&format!("{}-", pr.id)))
            });
        let Some(preset) = preset else {
            self.status = "未找到该供应商对应的预设，无法编辑".into();
            cx.notify();
            return;
        };
        self.edit_id = Some(id.to_string());
        self.dialog_preset = Some(preset.id.to_string());
        self.dialog_fields = dialog::DialogFields::from_provider(&p);
        // 预填输入框（先克隆，避免同时借用 self 字段）。
        let name = self.dialog_fields.name.clone();
        self.dialog_inputs
            .name
            .update(cx, |f, _| f.set_text(&name));
        let endpoint = self.dialog_fields.endpoint.clone();
        let key = self.dialog_fields.key.clone();
        let model = self.dialog_fields.model.clone();
        self.dialog_inputs
            .endpoint
            .update(cx, |f, _| f.set_text(&endpoint));
        self.dialog_inputs
            .key
            .update(cx, |f, _| f.set_text(&key));
        self.dialog_inputs
            .model
            .update(cx, |f, _| f.set_text(&model));
        let usage_script = self.dialog_fields.usage_script.clone();
        self.dialog_inputs
            .usage_script
            .update(cx, |f, _| f.set_text(&usage_script));
        let icon = self.dialog_fields.icon.clone();
        self.dialog_inputs
            .icon
            .update(cx, |f, _| f.set_text(&icon));
        let icon_color = self.dialog_fields.icon_color.clone();
        self.dialog_inputs
            .icon_color
            .update(cx, |f, _| f.set_text(&icon_color));
        self.dialog_inputs
            .search
            .update(cx, |f, _| f.set_text(""));
        cx.notify();
    }

    fn render_statusbar(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let lang = crate::i18n::Lang::parse(
            &self
                .store
                .read()
                .map(|s| s.config().settings.language.clone())
                .unwrap_or_else(|_| "zh-cn".into()),
        );
        div()
            .h(px(30.))
            .px(px(12.))
            .flex()
            .items_center()
            .justify_between()
            .bg(theme::bg_1().opacity(0.78))
            .border_t_1()
            .border_color(theme::stroke_a(0.1))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(widgets::status_dot(self.status_tone.color(), 7.))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(self.status_tone.color())
                            .child(self.status.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(theme::text_faint())
                            .child(format!(
                                "{}: {}",
                                crate::i18n::t(lang, "status.current"),
                                self.active_tool.label()
                            )),
                    )
                    .child(self.render_shortcut_hint())
                    .child(widgets::section_label("v0.1.0")),
            )
    }
    
    fn render_shortcut_hint(&self) -> Div {
        // 每页操作提示（界面内可发现性）；全局键在所有页适用。
        let page_hint = match self.page {
            Page::Providers => "⌘F 搜索供应商 · ⌘R 刷新",
            Page::Routing => "路由开关/故障转移每工具独立",
            Page::Mcp => "MCP 跨工具同步 · Esc 关闭对话框",
            Page::Skills => "安装/同步 · Esc 关闭对话框",
            Page::Prompts => "写回合并 · Esc 关闭对话框",
            Page::Usage => "用量趋势/热力图",
            Page::Tools => "安装/升级/全部升级",
            Page::Projects => "保存/应用/删除项目快照",
            Page::Sessions => "列表/搜索/阅读视图",
            Page::Accounts => "托管账号 Beta",
            Page::Apps => "应用配置/重复安装诊断",
            Page::Universal => "工具配置全文编辑",
            Page::ToolConfig => "配置文件编辑",
            Page::Settings => "通用/应用配置/路由/网络/数据/关于",
        };
        let hint = format!("{page_hint} · ⌘1-9 切换页面 · ⌘S 保存 · Esc 关闭");
        div()
            .text_size(px(9.))
            .text_color(theme::text_faint().opacity(0.6))
            .child(hint)
    }

    fn render_add_dialog(&self, cx: &mut Context<Self>) -> Div {
        let selected = self
            .dialog_preset
            .as_deref()
            .and_then(|id| {
                ns_presets::search_presets("", None)
                    .into_iter()
                    .find(|p| p.id == id)
            });
        dialog::add_dialog(
            self.active_tool,
            selected,
            self.edit_id.is_some(),
            &self.dialog_fields,
            &self.dialog_inputs,
            cx,
        )
    }

    /// 对话框未保存更改确认浮层（关闭前提醒）。
    fn render_dialog_confirm_close(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.dialog_dirty {
            return div().into_any_element();
        }
        div()
            .id("dialog-close-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x000000, 0.55))
            .child(
                div()
                    .w(px(360.))
                    .rounded(px(12.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::stroke_a(0.25))
                    .shadow_lg()
                    .p(px(20.))
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("未保存的更改"),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme::text_dim())
                            .child("对话框中有未保存的修改。确定要放弃这些更改并关闭吗？"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("dlg-guard-cancel", "继续编辑")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dialog_dirty = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                widgets::danger_button("dlg-guard-discard", "放弃更改")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dialog_dirty = false;
                                        this.add_open = false;
                                        this.edit_id = None;
                                        this.dialog_preset = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_confirm_delete(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(id) = self.confirm_delete.clone() else {
            return div().into_any_element();
        };
        let provider = self
            .store
            .read()
            .expect("store lock")
            .config()
            .providers
            .get(&id)
            .cloned();
        let Some(provider) = provider else {
            return div().into_any_element();
        };
        // 判断是否当前激活：任意工具当前指向该 id。
        let is_current = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            ns_presets::Tool::ALL
                .iter()
                .any(|t| cfg.current.get(*t) == Some(&id))
        };
        let name = provider.name.clone();

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x000000, 0.55))
            .child(
                div()
                    .w(px(380.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::danger_a(0.4))
                    .shadow_lg()
                    .p(px(18.))
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::danger())
                            .child("删除供应商"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text())
                            .child(format!("确定删除「{name}」？此操作不可撤销。")),
                    )
                    .child(if is_current {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::danger())
                            .child("该供应商正在被使用，请先切换到其他供应商再删除。")
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("del-cancel", "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.confirm_delete = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(if is_current {
                                widgets::primary_button("del-close", "知道了").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.confirm_delete = None;
                                        cx.notify();
                                    }),
                                )
                                .into_any_element()
                            } else {
                                widgets::danger_button("del-confirm", "确认删除")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let result = {
                                            let mut store = this.store.write().expect("store lock");
                                            store.remove_provider(&id)
                                        };
                                        this.confirm_delete = None;
                                        let ok = result.is_ok();
                                        this.status = match result {
                                            Ok(()) => format!("已删除供应商 {name}"),
                                            Err(e) => format!("删除失败: {e}"),
                                        };
                                        this.status_tone = if ok { StatusTone::Success } else { StatusTone::Error };
                                        if ok {
                                            this.refresh_tray_menu(cx);
                                        }
                                    }))
                                    .into_any_element()
                            }),
                    ),
            )
            .into_any_element()
    }

    /// 通用供应商删除确认浮层。
    fn render_universal_confirm_delete(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(id) = self.universal_confirm_delete.clone() else {
            return div().into_any_element();
        };
        let u = self
            .store
            .read()
            .expect("store lock")
            .config()
            .universal
            .get(&id)
            .cloned();
        let Some(u) = u else {
            return div().into_any_element();
        };
        let name = u.name.clone();
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::alpha(0x000000, 0.55))
            .child(
                div()
                    .w(px(380.))
                    .rounded(px(14.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::danger_a(0.4))
                    .shadow_lg()
                    .p(px(18.))
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::danger())
                            .child("删除通用供应商"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text())
                            .child(format!(
                                "确定删除「{name}」？其 Claude/Codex/Gemini 子供应商也会一并删除。"
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                widgets::ghost_button("uni-del-cancel", "取消").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.universal_confirm_delete = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                widgets::danger_button("uni-del-confirm", "确认删除").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let result = {
                                            let mut store =
                                                this.store.write().expect("store lock");
                                            store.remove_universal(&id)
                                        };
                                        this.universal_confirm_delete = None;
                                        this.status = match result {
                                            Ok(()) => format!("已删除通用供应商 {name}"),
                                            Err(e) => format!("删除失败: {e}"),
                                        };
                                        cx.notify();
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }
}

/// 当前时间戳（秒）。
pub fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 供应商 id 后缀（时间戳）。
pub fn now_ts_suffix() -> String {
    format!("{}", now_ts())
}

/// 解析 `#RRGGBB`/`RRGGBB` 为 0xRRGGBB。
fn parse_hex_color(s: &str) -> Option<u32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

/// MCP 传输类型短标签（用于药丸/徽章与元素 id）。
fn transport_label(transport: McpTransport) -> &'static str {
    match transport {
        McpTransport::Stdio => "stdio",
        McpTransport::Http => "http",
        McpTransport::Sse => "sse",
        McpTransport::Ws => "ws",
    }
}

impl Render for SwitchApp {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {

        // 消费托盘事件（切换/打开窗口/退出）。
        self.poll_tray(window, cx);
        let mode = theme::ThemeMode::parse(
            &self
                .store
                .read()
                .map(|s| s.config().settings.theme.clone())
                .unwrap_or_else(|_| "dark".into()),
        );
        // 仅在配置值变化时更新全局主题（避免 System 模式下每帧探测）。
        if ns_ui::theme::active_mode() != mode {
            ns_ui::theme::set_active_mode(mode);
        }
        let pal = ns_ui::theme::active_palette();
        let content: Stateful<Div> = div()
            .id("page-content")
            .flex_1()
            .h_full()
            .child(match self.page {
                Page::Providers => self.render_provider_page(cx).into_any_element(),
                Page::Routing => self.render_routing_page(cx).into_any_element(),
                Page::Mcp => self.render_mcp_page(cx).into_any_element(),
                Page::Skills => self.render_skills_page(cx).into_any_element(),
                Page::Prompts => self.render_prompts_page(cx).into_any_element(),
                Page::Usage => self.render_usage_page(cx).into_any_element(),
                Page::Tools => self.render_tools_page(cx).into_any_element(),
                Page::Projects => self.render_projects_page(cx).into_any_element(),
                Page::Accounts => self.render_accounts_page(cx).into_any_element(),
                Page::Sessions => {
                    if self.session_viewing.is_some() {
                        self.render_session_view(cx).into_any_element()
                    } else {
                        self.render_sessions_page(cx).into_any_element()
                    }
                }
                Page::Universal => self.render_universal_page(cx).into_any_element(),
                Page::ToolConfig => self.render_toolconfig_page(cx).into_any_element(),
                Page::Settings => self.render_settings_page(cx).into_any_element(),
                Page::Apps => self.render_apps_page(cx).into_any_element(),
            });

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(theme::app_background_for(mode))
            .font_family(theme::UI_FONT)
            .text_color(rgb(pal.text))
            .track_focus(&self.focus_handle)
            .key_context("App")
            .on_action(cx.listener(|this, _: &QuitApp, _, cx| {
                this.write_back_routing_direct();
                this.routing.stop_all();
                cx.quit();
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar_visible = !this.sidebar_visible;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &RefreshProviders, _, cx| this.refresh(cx)))
            .on_action(cx.listener(|this, _: &TogglePage0, _, cx| this.goto_page(0, cx)))
            .on_action(cx.listener(|this, _: &TogglePage1, _, cx| this.goto_page(1, cx)))
            .on_action(cx.listener(|this, _: &TogglePage2, _, cx| this.goto_page(2, cx)))
            .on_action(cx.listener(|this, _: &TogglePage3, _, cx| this.goto_page(3, cx)))
            .on_action(cx.listener(|this, _: &TogglePage4, _, cx| this.goto_page(4, cx)))
            .on_action(cx.listener(|this, _: &TogglePage5, _, cx| this.goto_page(5, cx)))
            .on_action(cx.listener(|this, _: &TogglePage6, _, cx| this.goto_page(6, cx)))
            .on_action(cx.listener(|this, _: &TogglePage7, _, cx| this.goto_page(7, cx)))
            .on_action(cx.listener(|this, _: &TogglePage8, _, cx| this.goto_page(8, cx)))
            .on_action(cx.listener(|this, _: &EscapeDialog, _, cx| this.escape_dialog(cx)))
            .on_action(cx.listener(|this, _: &SaveCurrent, _, cx| this.save_current(cx)))
            .on_action(cx.listener(|this, _: &FocusProviderSearch, window, cx| {
                let handle = this.provider_search.read(cx).focus_handle();
                window.focus(&handle);
            }))
            .child(self.render_topbar(cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .min_h_0()
                    .child(if self.sidebar_visible {
                        self.render_sidebar(cx).into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(content),
            )
            .child(self.render_statusbar(cx))
            .child(if self.add_open {
                self.render_add_dialog(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(self.render_confirm_delete(cx))
            .child(self.render_dialog_confirm_close(cx))
            .child(if self.open_menu.as_deref() == Some("project") {
                self.render_project_menu(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.universal_open {
                crate::universal::universal_dialog(
                    self.universal_edit.is_some(),
                    &self.universal_form,
                    &self.universal_inputs,
                    cx,
                )
                .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(self.render_universal_confirm_delete(cx))
            .child(if self.toolconfig_open {
                self.render_toolconfig_editor(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.daily_memory_editing.is_some() {
                self.render_daily_memory_editor(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.daily_memory_confirm_delete.is_some() {
                self.render_daily_memory_confirm_delete(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.openclaw_defaults_open {
                self.render_openclaw_defaults_editor(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.welcome_visible {
                self.render_welcome(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.whats_new_visible {
                self.render_whats_new(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.pending_switch.is_some() {
                self.render_pending_switch_confirm(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.backup_restore_pending.is_some() {
                self.render_backup_restore_confirm(cx).into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if self.deeplink_pending.is_some() {
                self.render_deeplink_confirm(cx).into_any_element()
            } else {
                div().into_any_element()
            })
}
}

/// 用系统文件管理器打开目录（跨平台）。
fn open_folder(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ());
    #[cfg(target_os = "linux")]
    let r = std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ());
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ());
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let r: std::io::Result<()> = Err(std::io::Error::other("unsupported platform"));
    r
}

/// 从存储快照构建托盘菜单条目（每工具列出供应商，标记当前）。
fn tray_entries(
    store: &RwLock<Store>,
    usage: &std::collections::BTreeMap<String, ns_usage::Usage>,
) -> Vec<tray::ToolMenuEntry> {
    let cfg = store.read().expect("store lock").config().clone();
    let mut entries = Vec::new();
    for tool in Tool::ALL {
        let providers = cfg.providers_for(tool);
        let current_id = cfg.current.get(tool).cloned();
        let routing = cfg.settings.routing_for(tool).enabled;
        let providers = providers
            .into_iter()
            .map(|p| {
                let is_current = current_id.as_deref() == Some(p.id.as_str());
                (p.id.clone(), p.name.clone(), is_current)
            })
            .collect();
        // 用量摘要（当前供应商的配额/剩余/重置倒计时；仅用量查询启用时显示）。
        let usage_enabled = current_id
            .as_ref()
            .and_then(|id| cfg.providers.get(id))
            .and_then(|p| p.meta.as_ref())
            .map(|m| m.usage_enabled)
            .unwrap_or(true);
        let usage_note = if usage_enabled {
            usage.get(tool.id_str()).and_then(|u| {
            let unit = if u.unit.is_empty() {
                String::new()
            } else {
                format!(" {}", u.unit)
            };
            let currency = u.currency.as_deref().unwrap_or("");
            let mut parts = Vec::new();
            if let Some(remaining) = u.remaining {
                parts.push(format!("剩 {remaining}{unit}{currency}"));
            } else if let Some(total) = u.total {
                parts.push(format!("总量 {total}{unit}{currency}"));
            }
            if let Some(reset_at) = u.reset_at {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                if reset_at > now {
                    let secs = reset_at - now;
                    let h = secs / 3600;
                    let m = (secs % 3600) / 60;
                    parts.push(format!("⏱ {h}h{m:02}m"));
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" · "))
            }
        })
        } else {
            None
        };
        entries.push(tray::ToolMenuEntry {
            tool,
            routing,
            providers,
            usage_note,
        });
    }
    entries
}

/// 欢迎页引导行（序号 + 标题 + 描述）。
fn welcome_row(
    num: &'static str,
    title: &'static str,
    desc: &'static str,
    _pal: ns_ui::theme::Palette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .rounded(px(12.))
        .border_1()
        .border_color(theme::stroke_a(0.14))
        .bg(theme::bg_1().opacity(0.5))
        .px(px(14.))
        .py(px(10.))
        .child(
            div()
                .size(px(26.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme::accent_a(0.2))
                .text_size(px(12.))
                .text_color(theme::accent())
                .child(num),
        )
        .child(
            div()
                .flex_col()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::text_faint())
                        .child(desc),
                ),
        )
}

/// 供应商是否为 Coding Plan / Token Plan（按名称关键词派生，不新增字段）。
fn is_coding_plan(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("coding plan") || n.contains("token plan") || n.contains("coding-plan")
        || n.contains("token-plan")
}

/// 从供应商 settings_config 提取当前模型名（用于卡片副标题）。
/// 扫描顶层与 `env`/`auth`/`config`/`model` 键，取第一个非空字符串模型。
fn provider_model(p: &ns_presets::Provider) -> Option<String> {
    fn find_model(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let kl = k.to_lowercase();
                    if kl == "model"
                        || kl == "default_model"
                        || kl == "defaultmodel"
                        || kl == "model_name"
                        || kl.contains("modelname")
                    {
                        if let Some(s) = v.as_str() {
                            let t = s.trim();
                            if !t.is_empty() {
                                return Some(t.to_string());
                            }
                        }
                        if let Some(s) = find_model(v) {
                            return Some(s);
                        }
                    } else if let Some(s) = find_model(v) {
                        // 只在嵌套对象（env/auth/config/model 容器）内继续搜索。
                        return Some(s);
                    }
                }
                None
            }
            _ => None,
        }
    }
    find_model(&p.settings_config)
}

/// 判断供应商是否为官方（category == official 或无自定义端点/key）。
fn provider_is_official(p: &ns_presets::Provider) -> bool {
    if matches!(
        p.category,
        Some(ns_presets::ProviderCategory::Official)
    ) {
        return true;
    }
    // 无自定义端点且无 key 的 Claude/Codex/Gemini 官方账号视为官方。
    let cfg = &p.settings_config;
    let text = serde_json::to_string(cfg).unwrap_or_default().to_lowercase();
    let has_endpoint = text.contains("base_url")
        || text.contains("baseurl")
        || text.contains("endpoint")
        || text.contains("anthropic_base_url");
    let has_key = text.contains("api_key")
        || text.contains("apikey")
        || text.contains("auth_token")
        || text.contains("openai_api_key")
        || text.contains("gemini_api_key");
    !has_endpoint && !has_key
}

/// 品牌图标 id → 显示字形（图标字符映射；未知 id 回退首字母）。
fn icon_glyph(icon_id: &str, fallback: char) -> String {
    let g = match icon_id {
        "anthropic" => '❋',
        "openai" => '⬡',
        "google" => '◆',
        "grok" => '✦',
        "copilot" => '⌥',
        "api" => '⌁',
        "router" => '⛶',
        "cloud" => '☁',
        "code" => '⌘',
        "sub" => '⧉',
        "flow" => '≋',
        "local" => '⬢',
        "mini" => '▣',
        "kimi" => '✳',
        "pi" => 'π',
        "hermes" => '⚡',
        "claw" => '✪',
        "omp" => '◉',
        _ => fallback,
    };
    g.to_string()
}

#[cfg(test)]
mod tests {
    use super::{cache_hit_rate, format_tokens, icon_glyph, is_coding_plan};

    #[test]
    fn coding_plan_detection() {
        assert!(is_coding_plan("火山 Coding Plan"));
        assert!(is_coding_plan("Tencent Token Plan"));
        assert!(is_coding_plan("compshare-coding-plan"));
        assert!(!is_coding_plan("88API"));
        assert!(!is_coding_plan("OpenRouter"));
        assert!(!is_coding_plan("OpenRouter"));
    }

    #[test]
    fn icon_glyph_maps_brands() {
        assert_eq!(icon_glyph("anthropic", '?'), "❋");
        assert_eq!(icon_glyph("openai", '?'), "⬡");
        assert_eq!(icon_glyph("unknown", 'X'), "X");
        assert_eq!(icon_glyph("", 'A'), "A");
    }

    #[test]
    fn format_tokens_scales() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(2_000_000), "2.0M");
        assert_eq!(format_tokens(26_274_810), "26.3M");
    }

    #[test]
    fn cache_hit_rate_counts_cache_in_prompt() {
        assert_eq!(cache_hit_rate(0, 0), 0.0);
        assert_eq!(cache_hit_rate(0, 100), 0.0);
        assert!((cache_hit_rate(100, 100) - 50.0).abs() < f64::EPSILON);
        assert!((cache_hit_rate(900, 100) - 90.0).abs() < f64::EPSILON);
        assert!((cache_hit_rate(250, 750) - 25.0).abs() < f64::EPSILON);
    }
}

