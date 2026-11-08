//! 主界面：顶栏、侧栏（工具列表）、供应商页、状态栏。

use std::sync::{Arc, RwLock};

use gpui::{
    Context, Div, FocusHandle, KeyBinding, Render, Stateful, div, prelude::*, px, rgb,
    actions,
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
            .bg(theme::color(theme::PANEL_SOLID))
            .border_1()
            .border_color(theme::alpha(theme::STROKE, 0.4))
            .shadow_lg()
            .text_size(px(11.))
            .text_color(rgb(theme::TEXT))
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


    fn label(self) -> &'static str {
        match self {
            Page::Providers => "供应商",
            Page::Routing => "路由",
            Page::Mcp => "MCP",
            Page::Skills => "Skills",
            Page::Prompts => "提示词",
            Page::Usage => "用量",
            Page::Tools => "工具",
            Page::Projects => "项目",
            Page::Sessions => "会话",
            Page::Accounts => "账号",
            Page::Apps => "应用",
            Page::Universal => "通用",
            Page::ToolConfig => "工具配置",
            Page::Settings => "设置",
        }
    }
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


/// 应用状态。
pub struct SwitchApp {
    pub(crate) store: Arc<RwLock<Store>>,
    pub(crate) active_tool: Tool,
    pub(crate) page: Page,
    pub(crate) sidebar_visible: bool,
    pub(crate) status: String,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) open_menu: Option<String>,
    pub(crate) add_open: bool,
    pub(crate) dialog_preset: Option<String>,
    /// 正在编辑的供应商 id（None = 无）。
    pub(crate) edit_id: Option<String>,
    /// 待确认删除的供应商 id（None = 无）。
    pub(crate) confirm_delete: Option<String>,

    pub(crate) dialog_fields: dialog::DialogFields,
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
    /// M10 会话列表。
    pub(crate) sessions: Vec<ns_sessions::SessionMeta>,
    /// 会话页搜索输入。
    pub(crate) session_search: gpui::Entity<crate::text_field::TextField>,
    /// 待确认删除的会话路径（None = 无）。
    pub(crate) session_delete: Option<String>,
    /// 会话加载中。
    pub(crate) sessions_loading: bool,
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
    /// 工具扫描中。
    pub(crate) tools_scanning: bool,
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
}

impl SwitchApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store = Arc::new(RwLock::new(
            Store::open().unwrap_or_else(|e| {
                eprintln!("打开存储失败，使用默认: {e}");
                Store::open_at(std::env::temp_dir().join("ns-switch-fallback")).unwrap_or_else(|_| {
                    // 极端情况：临时目录也不可写；用内存态（不落盘）。
                    Store::open_at(std::env::temp_dir()).expect("临时目录不可写")
                })
            }),
        ));
        let (tray_tx, tray_rx) = std::sync::mpsc::channel();
        crate::tray::register_event_handler(tray_tx);
        let tray = Self::build_tray(store.clone());
        let tray_rx = std::sync::Arc::new(std::sync::Mutex::new(tray_rx));
        let window_handle = std::sync::Arc::new(std::sync::Mutex::new(None));
        let app = Self {
            store,
            active_tool: Tool::Claude,
            page: Page::Providers,
            sidebar_visible: true,
            status: "就绪".into(),
            focus_handle: cx.focus_handle(),
            open_menu: None,
            add_open: false,
            dialog_preset: None,
            edit_id: None,
            confirm_delete: None,

            dialog_fields: dialog::DialogFields::default(),
            dialog_inputs: dialog::DialogInputs::new(cx),
            provider_search: cx.new(|cx| crate::text_field::TextField::new("搜索供应商…", cx)),
            drag_provider: None,
            drop_target: None,
            routing: crate::routing::RoutingRuntime::new(),
            routing_status: Default::default(),
            tray,
            tray_rx: tray_rx.clone(),

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
            sessions: Vec::new(),
            session_search: cx.new(|cx| crate::text_field::TextField::new("搜索会话…", cx)),
            session_delete: None,
            sessions_loading: false,
            session_viewing: None,
            session_rounds: Vec::new(),
            session_meta: None,
            session_collapsed: Vec::new(),
            session_filter: cx.new(|cx| crate::text_field::TextField::new("会话内搜索…", cx)),
            tools_result: Vec::new(),
            tools_scanning: false,
            project_name: cx.new(|cx| crate::text_field::TextField::new("项目名…", cx)),
            update_status: None,
            sync_url: cx.new(|cx| crate::text_field::TextField::new("WebDAV URL…", cx)),
            sync_user: cx.new(|cx| crate::text_field::TextField::new("用户名…", cx)),
            sync_pass: cx.new(|cx| crate::text_field::TextField::new("密码…", cx)),
            sync_status: String::new(),
            universal_open: false,
            universal_edit: None,
            universal_confirm_delete: None,
            universal_form: crate::universal::UniversalForm::default(),
            universal_inputs: crate::universal::UniversalInputs::new(cx),
            toolconfig_open: false,
            toolconfig_id: None,
            toolconfig_editor: None,
            toolconfig_status: String::new(),
        };
        // 深链导入：启动时处理 `nsswitch://`（env 或命令行参数）。
        let deeplink = std::env::var("NS_SWITCH_DEEPLINK")
            .ok()
            .or_else(|| std::env::args().nth(1).filter(|a| a.starts_with("nsswitch://")));
        if let Some(url) = deeplink {
            if let Some(action) = crate::deeplink::parse(&url) {
                app.apply_deeplink(&action);
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
        app
    }

    /// 从存储快照构建托盘（含初始菜单）。
    fn build_tray(store: Arc<RwLock<Store>>) -> Option<std::sync::Arc<tray::Tray>> {
        let entries = tray_entries(&store);
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
            let entries = tray_entries(&self.store);
            if let Err(e) = tray.rebuild_menu(&entries) {
                eprintln!("托盘菜单重建失败: {e}");
            }
        }
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        // 从存储重新加载（M4 起真正读写；当前为占位，保持就绪）
        self.status = "就绪".into();
        cx.notify();
    }

    fn select_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.active_tool = tool;
        self.page = Page::Providers;
        cx.notify();
    }

    fn select_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        cx.notify();
    }

    fn render_topbar(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(48.))
            .px(px(14.))
            .flex()
            .items_center()
            .justify_between()
            .bg(theme::color(theme::BG_1).opacity(0.78))
            .border_b_1()
            .border_color(theme::alpha(theme::STROKE, 0.1))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(widgets::brand_mark(26.))
                    .child(
                        div()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::BOLD)

                                    .text_color(rgb(theme::TEXT))
                                    .child("商量切换器"),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(rgb(theme::TEXT_FAINT))
                                    .child("ns-switch · 一键切换 API 供应商"),
                            ),
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
                                    .on_click(_cx.listener(|this, _, _, cx| {
                                        this.open_menu = Some("project".to_string());
                                        cx.notify();
                                    })),
                            ),
                    )
                },
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(widgets::section_label("工具"))
                    .child(
                        widgets::ghost_button("top-add", "＋ 添加供应商")
                            .on_click(_cx.listener(|this, _, _, cx| {
                                this.status = "添加供应商（M3 实现）".into();
                                cx.notify();
                            })),
                    ),
            )
    }

    fn render_sidebar(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let lang = crate::i18n::Lang::parse(
            &self
                .store
                .read()
                .map(|s| s.config().settings.language.clone())
                .unwrap_or_else(|_| "zh-cn".into()),
        );
        div()
            .w(px(212.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::color(theme::BG_1).opacity(0.56))
            .border_r_1()
            .border_color(theme::alpha(theme::STROKE, 0.1))
            .child(
                div()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(widgets::section_label("AI 工具"))
                    .child(
                        div()
                            .text_size(px(9.5))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child(format!("{} 个", Tool::ALL.len())),
                    ),
            )
            .child(
                div()
                    .id("tool-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .py(px(6.))
                    .px(px(6.))
                    .flex()
                    .flex_col()
                    .children(Tool::ALL.into_iter().enumerate().map(|(i, tool)| {
                        let active = tool == self.active_tool;
                        div()
                            .id(("tool-item", i))
                            .h(px(28.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(6.))
                            .bg(if active {
                                theme::alpha(theme::ACCENT, 0.12)
                            } else {
                                theme::alpha(0xffffff, 0.0)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::alpha(theme::STROKE, 0.07)))
                            .on_click(_cx.listener(move |this, _, _, cx| {
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
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(px(11.5))
                                    .text_color(if active {
                                        rgb(theme::TEXT)
                                    } else {
                                        rgb(theme::TEXT_DIM)
                                    })
                                    .child(tool.label()),
                            )
                            .child(if tool.is_switch() {
                                widgets::pill("切换", theme::color(theme::JADE))
                            } else {
                                widgets::pill("共存", theme::color(theme::ACCENT))
                            })
                    })),
            )
            .child(
                div()
                    .h(px(40.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(widgets::section_label("页面"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .children(Page::ALL.into_iter().enumerate().map(|(i, page)| {
                                let active = page == self.page;
                                div()
                                    .id(("page-item", i))
                                    .h(px(24.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.))
                                    .bg(if active {
                                        theme::alpha(theme::ACCENT, 0.12)
                                    } else {
                                        theme::alpha(0xffffff, 0.0)
                                    })
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::alpha(theme::STROKE, 0.07)))
                                    .on_click(_cx.listener(move |this, _, _, cx| {
                                        this.select_page(page, cx);
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(if active {
                                                rgb(theme::ACCENT)
                                            } else {
                                                rgb(theme::TEXT_DIM)
                                            })
                                            .child(crate::i18n::t(
                                                lang,
                                                page_i18n_key(page),
                                            )),
                                    )
                            })),
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
            .child(
                div()
                    .h(px(42.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme::TEXT))
                                    .child(self.active_tool.label()),
                            )
                            .child(widgets::pill(
                                if self.active_tool.is_switch() { "切换" } else { "共存" },
                                theme::color(theme::JADE),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .w(px(200.))
                                    .h(px(30.))
                                    .px(px(10.))
                                    .rounded(px(8.))
                                    .bg(theme::alpha(0x000000, 0.25))
                                    .border_1()
                                    .border_color(theme::alpha(theme::STROKE, 0.14))
                                    .flex()
                                    .items_center()
                                    .child(self.provider_search.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(theme::TEXT_FAINT))
                                    .child(format!("{} 个供应商", providers.len())),
                            )
                            .child(
                                widgets::ghost_button("page-export", "导出")
                                    .on_click(cx.listener(Self::export_providers)),
                            )
                            .child(
                                widgets::ghost_button("page-import", "导入")
                                    .on_click(cx.listener(Self::import_providers)),
                            )
                            .child(
                                widgets::ghost_button("page-add", "＋ 添加供应商")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.add_open = true;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
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
                    .children(if providers.is_empty() {
                        vec![self.render_empty_state(cx).into_any_element()]
                    } else {
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
                            .collect()
                    }),
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
                this.update(cx, |this, cx| {
                    let result = match json {
                        Ok(text) => this
                            .store
                            .write()
                            .expect("store lock")
                            .import_providers(tool, &text),
                        Err(e) => Err(anyhow::anyhow!("读取失败: {e}")),
                    };
                    this.status = match result {
                        Ok(n) => format!("已导入 {n} 个供应商"),
                        Err(e) => format!("导入失败: {e}"),
                    };
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
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
        // 写入前冲突检测：目标 live 文件被手动改动/他方占用 → 告警。
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let conflicts = home
            .as_deref()
            .map(|h| {
                crate::switch_engine::check_conflicts(tool, provider, current_provider, h)
            })
            .unwrap_or_default();
        let conflict_note = if conflicts.is_empty() {
            String::new()
        } else {
            let first = &conflicts[0];
            format!(
                " ⚠ {} 处配置被占用/修改（{}：{}）",
                conflicts.len(),
                first.path.display(),
                first.reason
            )
        };
        let backup_dir = self.store.read().expect("store lock").backups_dir();
        match crate::switch_engine::switch_to(tool, provider, &backup_dir) {
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
                        format!(
                            "已切换到 {}（写入 {} 个文件{}）{}{}",
                            provider.name,
                            outcome.written,
                            if outcome.noop { "，无内容变化" } else { "" },
                            restart_hint,
                            conflict_note
                        )
                    }
                    Err(e) => format!("切换失败: {e}"),
                };
            }
            Err(e) => {
                self.status = format!("切换失败: {e}");
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
        let script = {
            let store = self.store.read().expect("store lock");
            let cfg = store.config();
            cfg.current
                .get(tool)
                .and_then(|id| cfg.providers.get(id))
                .and_then(|p| p.meta.as_ref())
                .and_then(|m| m.usage_script.clone())
        };
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

    /// 扫描 Claude Code 会话日志，统计按模型请求/token。
    fn refresh_session_stats(&mut self, cx: &mut Context<Self>) {
        self.session_scanning = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let (stats, daily, details) = match home.as_deref() {
                Some(h) => (
                    ns_usage::scan_claude_sessions(h, 0),
                    ns_usage::scan_claude_daily(h, 30),
                    ns_usage::scan_claude_details(h),
                ),
                None => (Vec::new(), Vec::new(), Vec::new()),
            };
            this.update(cx, |this, cx| {
                this.session_stats = stats;
                this.usage_daily = daily;
                this.usage_details = details;
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
                                    .text_color(rgb(theme::TEXT))
                                    .child("用量统计"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(theme::TEXT_FAINT))
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
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.6))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.12))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme::TEXT))
                                    .child(format!("{} · {}", tool.label(), provider_name)),
                            )
                            .child(
                                widgets::ghost_button(("usage-query", i), "查询").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.query_usage(tool, cx);
                                    }),
                                ),
                            ),
                    )
                    .child(match (u, is_loading) {
                        (_, true) => div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_DIM))
                            .child("查询中…"),
                        (Some(u), false) => {
                            let mut line = String::new();
                            if let Some(remaining) = u.remaining {
                                let unit = if u.unit.is_empty() {
                                    String::new()
                                } else {
                                    format!(" {}", u.unit)
                                };
                                let currency = u.currency.as_deref().unwrap_or("");
                                line.push_str(&format!(
                                    "剩余 {remaining}{unit}{currency}"
                                ));
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
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(theme::TEXT_DIM))
                                .child(line)
                                .child(if u.source.is_empty() {
                                    div()
                                } else {
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(theme::TEXT_FAINT))
                                        .child(format!("来源: {}", u.source))
                                })
                        }
                        (None, false) => div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("点击“查询”获取用量"),
                    })
            }))
            .child(
                div()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(14.))
                    .rounded(px(10.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.6))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.12))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme::TEXT))
                                    .child("会话日志统计（Claude Code）"),
                            )
                            .child(
                                widgets::ghost_button(("usage-session-refresh", 0usize), "扫描")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_session_stats(cx);
                                    })),
                            ),
                    )
                    .child(if self.session_scanning {
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_DIM))
                            .child("扫描中…")
                    } else if self.session_stats.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("点击“扫描”统计会话请求")
                    } else {
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .children(self.session_stats.iter().enumerate().map(|(i, s)| {
                                div()
                                    .id(("usage-session-row", i))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .text_size(px(12.))
                                    .text_color(rgb(theme::TEXT_DIM))
                                    .child(
                                        div()
                                            .font_family(theme::MONO_FONT)
                                            .text_color(rgb(theme::ACCENT))
                                            .child(s.model.clone()),
                                    )
                                    .child(format!(
                                        "{} 请求 · {} 输入 · {} 输出 · 缓存读 {}",
                                        s.requests, s.input_tokens, s.output_tokens, s.cache_read_tokens
                                    ))
                                    .child(
                                        div()
                                            .font_family(theme::MONO_FONT)
                                            .text_color(rgb(theme::JADE))
                                            .child(match s.cost_usd {
                                                Some(c) => format!("≈ ${:.4}", c),
                                                None => "价格未知".to_string(),
                                            })
                                    )
                            }))
                    })
            )
            .child(self.render_usage_charts(cx))
    }

    /// 渲染 M9 趋势图 + 热力图 + 请求明细。
    fn render_usage_charts(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let daily = self.usage_daily.clone();
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
                div()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("近 30 日请求趋势"),
                    )
                    .child(if daily.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
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
                                    .bg(theme::alpha(theme::ACCENT, 0.55))
                                    .child(
                                        div()
                                            .text_size(px(9.))
                                            .text_color(rgb(theme::TEXT_FAINT))
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
                            .text_color(rgb(theme::TEXT))
                            .child("近 30 日热力图"),
                    )
                    .child(if daily.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
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
                                    .bg(theme::alpha(theme::ACCENT, alpha))
                                    .border_1()
                                    .border_color(theme::alpha(theme::STROKE, 0.08))
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
                            .text_color(rgb(theme::TEXT))
                            .child("请求明细"),
                    )
                    .child(if details.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
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
                                    .text_color(rgb(theme::TEXT_DIM))
                                    .child(
                                        div()
                                            .font_family(theme::MONO_FONT)
                                            .text_color(rgb(theme::ACCENT))
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


    /// 加载 Claude Code 会话列表（异步扫描）。
    fn load_sessions(&mut self, cx: &mut Context<Self>) {
        self.sessions_loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let sessions = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .as_deref()
                .map(|h| ns_sessions::list_claude_sessions(h))
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
        cx.spawn(async move |this, cx| {
            let meta = ns_sessions::SessionMeta {
                id: std::path::Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("session")
                    .to_string(),
                project: String::new(),
                path: path.clone(),
                mtime: 0,
                message_count: 0,
                last_model: None,
                title: String::new(),
            };
            let msgs = ns_sessions::parse_session(std::path::Path::new(&path));
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
                                            .text_color(rgb(theme::TEXT))
                                            .child(format!("会话 {}", session_id)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(theme::TEXT_FAINT))
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
                                        widgets::ghost_button(("sess-recover", 0usize), "恢复命令").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                let cmd = ns_sessions::recover_command(&session_id);
                                                this.status = cmd.clone();
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
                                div()
                                    .id(("round", i))
                                    .flex_col()
                                    .gap(px(4.))
                                    .p(px(10.))
                                    .rounded(px(8.))
                                    .bg(theme::alpha(theme::PANEL_SOLID, 0.4))
                                    .border_1()
                                    .border_color(theme::alpha(theme::STROKE, 0.08))
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
                                                    .text_color(rgb(theme::TEXT))
                                                    .child(format!("用户：{}", r.user_text)),
                                            )
                                            .children(r.assistant.iter().enumerate().map(|(_, m)| {
                                                div()
                                                    .flex_col()
                                                    .gap(px(2.))
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .text_color(rgb(theme::ACCENT))
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
                                                            .text_color(rgb(theme::TEXT_DIM))
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
                    .border_color(theme::alpha(theme::STROKE, 0.1))
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
                                    .hover(|s| s.bg(theme::alpha(theme::STROKE, 0.06)))
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

    /// 应用深链动作（启动时）。
    fn apply_deeplink(&self, action: &crate::deeplink::DeepLinkAction) {
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
                        endpoint: endpoint.clone(),
                        key: key.clone(),
                        model: model.clone(),
                        usage_script: String::new(),
                    };
                    let provider = crate::dialog::build_provider(preset, &fields);
                    let id = provider.id.clone();
                    {
                        let mut s = self.store.write().expect("store lock");
                        let _ = s.add_provider(provider);
                        let _ = s.set_current(ns_presets::Tool::Claude, id);
                    }
                }
            }
            crate::deeplink::DeepLinkAction::SwitchProvider { tool, provider_id } => {
                if let Some(t) = ns_presets::Tool::ALL.iter().find(|t| t.id_str() == tool) {
                    let _ = self
                        .store
                        .write()
                        .expect("store lock")
                        .set_current(*t, provider_id.clone());
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
            _ => {}
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

    /// 切到第 idx 页（快捷键 cmd-1..6）。
    fn goto_page(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(p) = Page::ALL.get(idx) {
            self.page = *p;
            cx.notify();
        }
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
                                    .text_color(rgb(theme::TEXT))
                                    .child("工具管理"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(theme::TEXT_FAINT))
                                    .child("CLI 工具版本探测与安装"),
                            ),
                    )
                    .child(
                        widgets::ghost_button(("tools-scan", 0usize), "扫描").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.scan_tools(cx);
                            }),
                        ),
                    ),
            )
            .child(if scanning {
                div()
                    .text_size(px(12.))
                    .text_color(rgb(theme::TEXT_DIM))
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
                            .bg(theme::alpha(theme::PANEL_SOLID, 0.5))
                            .border_1()
                            .border_color(theme::alpha(theme::STROKE, 0.1))
                            .child(
                                div()
                                    .flex_1()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(rgb(theme::TEXT))
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
                                    .text_size(px(11.))
                                    .text_color(rgb(theme::TEXT_FAINT))
                                    .child(tool.install_command()),
                            )
                    }))
            })
    }

    /// 渲染 M11 项目快照页：保存当前状态、整组切换、自动回存。
    fn render_projects_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let projects: Vec<String> = self
            .store
            .read()
            .map(|s| s.list_projects())
            .unwrap_or_default();
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
                            .text_color(rgb(theme::TEXT))
                            .child("项目快照"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
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
                                let ok = {
                                    let mut s = this.store.write().expect("store lock");
                                    s.save_project(&name).is_ok()
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
                            .bg(theme::alpha(theme::PANEL_SOLID, 0.5))
                            .border_1()
                            .border_color(theme::alpha(theme::STROKE, 0.1))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.))
                                    .text_color(rgb(theme::TEXT))
                                    .child(name.clone()),
                            )
                            .child(
                                widgets::ghost_button(("project-apply", i), "应用").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        let ok = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.apply_project(&n).is_ok()
                                        };
                                        this.status = if ok {
                                            format!("已切换到项目 {n}")
                                        } else {
                                            format!("应用项目 {n} 失败")
                                        };
                                        cx.notify();
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

    /// 渲染 M11 托管账号页（Beta：Copilot / ChatGPT / xAI 订阅作为供应商）。
    fn render_accounts_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .text_color(rgb(theme::TEXT))
                            .child("授权中心"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("CLI 工具凭据与托管订阅（Claude Desktop / Codex / Copilot / ChatGPT / xAI）"),
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
                            .text_color(rgb(theme::TEXT))
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
                                    .bg(theme::alpha(theme::PANEL_SOLID, 0.5))
                                    .border_1()
                                    .border_color(theme::alpha(theme::STROKE, 0.1))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex_col()
                                            .gap(px(2.))
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(rgb(theme::TEXT))
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
                                                    .text_color(rgb(theme::TEXT_FAINT))
                                                    .child(path),
                                            )
                                            .child(if let Some(e) = s.error.clone() {
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(theme::DANGER))
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
                            .bg(theme::alpha(theme::PANEL_SOLID, 0.5))
                            .border_1()
                            .border_color(theme::alpha(theme::STROKE, 0.1))
                            .child(
                                div()
                                    .flex_1()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(rgb(theme::TEXT))
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
                                        let _ = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.login_account(&id2, name)
                                        };
                                        this.status = format!("已登录 {name}（Beta：OAuth 占位）");
                                        cx.notify();
                                    }),
                                )
                            })
                    }))
            )
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
                            .text_color(rgb(theme::TEXT))
                            .child("工具配置"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("OpenClaw 工作区指令与 Hermes 持久记忆（文件不存在时保存即创建）"),
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
                            .bg(theme::alpha(theme::PANEL_SOLID, 0.5))
                            .border_1()
                            .border_color(theme::alpha(theme::STROKE, 0.1))
                            .child(
                                div()
                                    .flex_1()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(rgb(theme::TEXT))
                                            .child(label),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(theme::TEXT_FAINT))
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
                    .bg(theme::color(theme::BG_1))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.2))
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
                                    .text_color(rgb(theme::TEXT))
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
                            .border_color(theme::alpha(theme::STROKE, 0.14))
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
        let status = self.update_status.clone();
        let backups = self.store.read().map(|s| s.list_backups()).unwrap_or_default();
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
                            .text_color(rgb(theme::TEXT))
                            .child("设置"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("自动更新、深链导入"),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.3))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(theme::TEXT))
                            .child("主题"),
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
                                        "light" => "浅色",
                                        "system" => "跟随系统",
                                        _ => "深色",
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
                                        this.status =
                                            format!("主题已切换为 {label}");
                                        cx.notify();
                                    }))
                                    .map(|b| {
                                        if cur == mode {
                                            b.bg(theme::alpha(theme::ACCENT, 0.18))
                                                .border_1()
                                                .border_color(theme::alpha(theme::ACCENT, 0.5))
                                        } else {
                                            b
                                        }
                                    })
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.5))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.1))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(rgb(theme::TEXT))
                            .child(if crate::autostart::is_enabled() {
                                "开机自启：已开启"
                            } else {
                                "开机自启：关闭"
                            }),
                    )
                    .child(
                        widgets::ghost_button(
                            "autostart-toggle",
                            if crate::autostart::is_enabled() {
                                "关闭自启"
                            } else {
                                "开启自启"
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
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.3))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(theme::TEXT))
                            .child("语言"),
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
                                                b.bg(theme::alpha(theme::ACCENT, 0.18))
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
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.3))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(theme::TEXT))
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
                                    .text_color(rgb(theme::TEXT_DIM))
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
                                    .text_color(rgb(theme::TEXT_DIM))
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
                                    .text_color(rgb(theme::TEXT_DIM))
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
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_DIM))
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
             .child(
                 div()
                     .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.5))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.1))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(rgb(theme::TEXT))
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
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.3))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("深链导入"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_DIM))
                            .child(
                                "nsswitch://import-provider?preset=claude-88api&endpoint=...&key=...&model=...",
                            ),
                    ),
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.3))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("备份与环境"),
                    )
                    .child(if conflicts.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("无环境冲突")
                    } else {
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme::DANGER))
                            .child(conflicts.join("；"))
                    })
                    .child(if backups.is_empty() {
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("无首写备份")
                    } else {
                        div()
                            .flex_col()
                            .gap(px(4.))
                            .children(backups.into_iter().enumerate().map(|(i, b)| {
                                let bp = b.path.clone();
                                div()
                                    .id(("backup-row", i))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(11.))
                                            .text_color(rgb(theme::TEXT_DIM))
                                            .child(format!("{} · {} B · {}", b.name, b.size, b.source)),
                                    )
                                    .child(
                                        widgets::ghost_button(("backup-restore", i), "恢复").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                let ok = {
                                                    let s = this.store.read().expect("store lock");
                                                    s.restore_backup(&bp).is_ok()
                                                };
                                                this.status = if ok {
                                                    format!("已恢复 {bp}")
                                                } else {
                                                    "恢复备份失败".into()
                                                };
                                                cx.notify();
                                            }),
                                        ),
                                    )
                            }))
                    })
            )
            .child(
                div()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(theme::alpha(theme::PANEL_SOLID, 0.3))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("云同步（WebDAV）"),
                    )
                    .child(div().w_full().child(self.sync_url.clone()))
                    .child(div().w_full().child(self.sync_user.clone()))
                    .child(div().w_full().child(self.sync_pass.clone()))
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
                                        let enabled = !url.is_empty();
                                        let ok = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.set_sync_config(ns_store::SyncConfig {
                                                provider: "webdav".into(),
                                                url,
                                                username: user,
                                                password: pass,
                                                bucket: String::new(),
                                                enabled,
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
                            .text_color(rgb(theme::TEXT_DIM))
                            .child(self.sync_status.clone()),
                    ),
            )
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
                                    .text_color(rgb(theme::TEXT))
                                    .child("会话管理"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(theme::TEXT_FAINT))
                                    .child(format!("{total} 个会话 · 按项目分组")),
                            ),
                    )
                    .child(
                        widgets::ghost_button(("sessions-refresh", 0usize), "刷新").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.load_sessions(cx);
                            }),
                        ),
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
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("加载会话中…")
            } else if grouped.is_empty() {
                div()
                    .text_size(px(12.))
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child("未找到会话（点击“刷新”扫描 ~/.claude/projects）")
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
                                    .text_color(rgb(theme::TEXT_DIM))
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
                                    .bg(theme::alpha(theme::PANEL_SOLID, 0.5))
                                    .border_1()
                                    .border_color(theme::alpha(theme::STROKE, 0.1))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::alpha(theme::STROKE, 0.05)))
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
                                                    .text_color(rgb(theme::TEXT))
                                                    .child(format!(
                                                        "{} · {} 消息",
                                                        s.id.chars().take(8).collect::<String>(),
                                                        s.message_count
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(theme::TEXT_FAINT))
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
                            .bg(theme::color(theme::PANEL_SOLID))
                            .border_1()
                            .border_color(theme::alpha(theme::DANGER, 0.4))
                            .flex_col()
                            .gap(px(12.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme::TEXT))
                                    .child("删除会话？"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(theme::TEXT_DIM))
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
                                    .text_color(rgb(theme::TEXT))
                                    .child("本地路由"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(theme::TEXT_FAINT))
                                    .child("每工具独立代理：live 配置指向 127.0.0.1，真实 key 留在 config.json"),
                            ),
                    )
                    .child(
                        widgets::pill(
                            format!("{} 个工具可路由", routable.len()),
                            theme::color(theme::ACCENT),
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
                    .border_color(theme::alpha(theme::STROKE, 0.12))
                    .bg(theme::color(theme::BG_1).opacity(0.55))
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
                                            .text_color(rgb(theme::ACCENT))
                                            .child(tool.glyph()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(theme::TEXT))
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
                                    .text_color(rgb(theme::TEXT_DIM))
                                    .child(provider_name),
                            )
                            .child(if running {
                                widgets::status_dot(theme::color(theme::JADE), 7.)
                            } else {
                                widgets::status_dot(theme::color(theme::TEXT_DIM), 7.)
                            })
                            .child(div().text_size(px(11.)).text_color(rgb(theme::TEXT_FAINT)).child(
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
                            .text_color(rgb(theme::TEXT_FAINT))
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
                                    .border_color(theme::alpha(theme::STROKE, 0.16))
                                    .bg(theme::alpha(0x261d12, 0.5))
                                    .text_color(rgb(theme::TEXT))
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
            }))
            .child(self.render_aggregation_section(cx))
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
                                    .text_color(rgb(theme::TEXT))
                                    .child("聚合模式"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(theme::TEXT_FAINT))
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
                                        theme::alpha(theme::ACCENT, 0.18)
                                    } else {
                                        theme::alpha(0x261d12, 0.4)
                                    })
                                    .border_1()
                                    .border_color(if selected {
                                        theme::alpha(theme::ACCENT, 0.5)
                                    } else {
                                        theme::alpha(theme::STROKE, 0.1)
                                    })
                                    .text_color(if selected {
                                        rgb(theme::ACCENT)
                                    } else {
                                        rgb(theme::TEXT_DIM)
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
                    .border_color(theme::alpha(theme::STROKE, 0.12))
                    .bg(theme::color(theme::BG_1).opacity(0.55))
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
                                            .text_color(rgb(theme::TEXT))
                                            .child(agg_tool.label()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(theme::TEXT_FAINT))
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
                                                theme::alpha(theme::ACCENT, 0.5)
                                            } else {
                                                theme::alpha(theme::STROKE, 0.1)
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
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("启用后可将多个供应商加入聚合，并选一个默认。")
                            .into_any_element()
                    }),
            )
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
        const MCP_TOOLS: [Tool; 3] = [Tool::Claude, Tool::Codex, Tool::Gemini];
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
                                    .text_color(rgb(theme::TEXT))
                                    .child("统一 MCP 面板"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(theme::TEXT_FAINT))
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
                    .children(MCP_TOOLS.iter().enumerate().map(|(i, &t)| {
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
                                theme::alpha(theme::ACCENT, 0.18)
                            } else {
                                theme::alpha(0x261d12, 0.4)
                            })
                            .border_1()
                            .border_color(if selected {
                                theme::alpha(theme::ACCENT, 0.5)
                            } else {
                                theme::alpha(theme::STROKE, 0.1)
                            })
                            .text_color(if selected {
                                rgb(theme::ACCENT)
                            } else {
                                rgb(theme::TEXT_DIM)
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
                    .text_color(rgb(theme::CINNABAR))
                    .child(status)
            } else {
                div()
            })
            .child(if !write_msg.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(rgb(theme::JADE))
                    .child(write_msg)
            } else {
                div()
            })
            .child(if servers.is_empty() {
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(160.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("该工具暂无 MCP 服务器，点击“添加服务器”"),
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
                            .border_color(theme::alpha(theme::STROKE, 0.12))
                            .bg(theme::color(theme::BG_1).opacity(0.55))
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
                                            .text_color(rgb(theme::TEXT))
                                            .child(s.name.clone()),
                                    )
                                    .child(widgets::pill(
                                        transport_label(s.transport),
                                        theme::color(theme::ACCENT),
                                    ))
                                    .child(if s.enabled {
                                        widgets::pill("启用", theme::color(theme::JADE))
                                    } else {
                                        widgets::pill("停用", theme::color(theme::TEXT_DIM))
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
                                        widgets::ghost_button(("mcp-sync", i), "同步").on_click({
                                            let sn = sname.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                let others: Vec<Tool> = MCP_TOOLS
                                                    .iter()
                                                    .copied()
                                                    .filter(|&t| t != this.mcp.tool)
                                                    .collect();
                                                this.mcp.sync_to(&sn, &others, cx);
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
            .child(if let Some(name) = confirm_delete {
                self.render_mcp_confirm_delete(&name, cx).into_any_element()
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.2))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme::TEXT))
                            .child(if is_edit { "编辑 MCP 服务器" } else { "添加 MCP 服务器" }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(self.mcp_transport_pill(McpTransport::Stdio, cx))
                            .child(self.mcp_transport_pill(McpTransport::Http, cx)),
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
                                        widgets::pill("启用", theme::color(theme::JADE))
                                    } else {
                                        widgets::pill("停用", theme::color(theme::TEXT_DIM))
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
                theme::alpha(theme::ACCENT, 0.18)
            } else {
                theme::alpha(0x261d12, 0.4)
            })
            .border_1()
            .border_color(if selected {
                theme::alpha(theme::ACCENT, 0.5)
            } else {
                theme::alpha(theme::STROKE, 0.1)
            })
            .text_color(if selected {
                rgb(theme::ACCENT)
            } else {
                rgb(theme::TEXT_DIM)
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
                    .border_color(theme::alpha(theme::STROKE, 0.16))
                    .bg(theme::alpha(0x261d12, 0.5))
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::DANGER, 0.4))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("删除 MCP 服务器"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_DIM))
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
                                    .text_color(rgb(theme::TEXT))
                                    .child("统一 Skills 面板"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(theme::TEXT_FAINT))
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
                                    this.skills.load(cx);
                                }),
                            ))
                            .child(
                                widgets::ghost_button(("skills-install", 0usize), "安装")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.skills.open_install(cx);
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
                                theme::alpha(theme::ACCENT, 0.18)
                            } else {
                                theme::alpha(0x261d12, 0.4)
                            })
                            .border_1()
                            .border_color(if selected {
                                theme::alpha(theme::ACCENT, 0.5)
                            } else {
                                theme::alpha(theme::STROKE, 0.1)
                            })
                            .text_color(if selected {
                                rgb(theme::ACCENT)
                            } else {
                                rgb(theme::TEXT_DIM)
                            })
                            .text_size(px(12.))
                            .child(t.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.skills.set_tool(t, cx);
                            }))
                    })),
            )
            .child(if !status.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(rgb(theme::CINNABAR))
                    .child(status)
            } else {
                div()
            })
            .child(if !write_msg.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(rgb(theme::JADE))
                    .child(write_msg)
            } else {
                div()
            })
            .child(if skills.is_empty() {
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(160.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("该工具暂无 Skills，点击“添加技能”"),
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
                            .border_color(theme::alpha(theme::STROKE, 0.12))
                            .bg(theme::color(theme::BG_1).opacity(0.55))
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
                                            .text_color(rgb(theme::TEXT))
                                            .child(s.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(theme::TEXT_FAINT))
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.2))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme::TEXT))
                            .child(if is_edit { "编辑 Skill" } else { "添加 Skill" }),
                    )
                    .child(self.skill_field("技能名", name))
                    .child(self.skill_field("描述", description))
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
                                    .border_color(theme::alpha(theme::STROKE, 0.16))
                                    .bg(theme::alpha(0x261d12, 0.5))
                                    .child(body),
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
                                        this.skills.dialog_open = false;
                                        cx.notify();
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.2))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("从 GitHub 安装 Skill"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("仓库需含 skills/<name>/SKILL.md（或仓库根 SKILL.md）"),
                    )
                    .child(self.skill_field("仓库 URL", url))
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
                                        this.skills.install_github(cx);
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
                    .border_color(theme::alpha(theme::STROKE, 0.16))
                    .bg(theme::alpha(0x261d12, 0.5))
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::DANGER, 0.4))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("删除 Skill"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_DIM))
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

    /// 渲染 M7 统一 Prompts 面板。
    fn render_prompts_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        const PROMPT_TOOLS: [Tool; 4] = [
            Tool::Claude,
            Tool::ClaudeDesktop,
            Tool::Codex,
            Tool::Gemini,
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
                                    .text_color(rgb(theme::TEXT))
                                    .child("统一提示词面板"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(theme::TEXT_FAINT))
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
                                theme::alpha(theme::ACCENT, 0.18)
                            } else {
                                theme::alpha(0x261d12, 0.4)
                            })
                            .border_1()
                            .border_color(if selected {
                                theme::alpha(theme::ACCENT, 0.5)
                            } else {
                                theme::alpha(theme::STROKE, 0.1)
                            })
                            .text_color(if selected {
                                rgb(theme::ACCENT)
                            } else {
                                rgb(theme::TEXT_DIM)
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
                    .text_color(rgb(theme::CINNABAR))
                    .child(status)
            } else {
                div()
            })
            .child(if !write_msg.is_empty() {
                div()
                    .text_size(px(11.))
                    .text_color(rgb(theme::JADE))
                    .child(write_msg)
            } else {
                div()
            })
            .child(if entries.is_empty() {
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(160.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("该工具暂无提示词条目，点击“添加条目”"),
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
                            .border_color(theme::alpha(theme::STROKE, 0.12))
                            .bg(theme::color(theme::BG_1).opacity(0.55))
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
                                            .text_color(rgb(theme::TEXT))
                                            .child(e.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(theme::TEXT_FAINT))
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
                                                widgets::pill("启用", theme::color(theme::JADE))
                                            } else {
                                                widgets::pill("停用", theme::color(theme::TEXT_DIM))
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.2))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme::TEXT))
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
                                    .border_color(theme::alpha(theme::STROKE, 0.16))
                                    .bg(theme::alpha(0x261d12, 0.5))
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
                                        widgets::pill("启用", theme::color(theme::JADE))
                                    } else {
                                        widgets::pill("停用", theme::color(theme::TEXT_DIM))
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
                                        this.prompts.dialog_open = false;
                                        cx.notify();
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::DANGER, 0.4))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("删除提示词条目"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_DIM))
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

    /// 路由开关指示（启用/禁用徽章）。
    fn render_routing_switch(
        &self,
        _tool: Tool,
        enabled: bool,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if enabled {
            widgets::pill("已启用", theme::color(theme::JADE))
        } else {
            widgets::pill("未启用", theme::color(theme::TEXT_DIM))
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
                    .border_color(theme::alpha(theme::STROKE, 0.08))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::TEXT))
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
                                    .text_color(rgb(theme::TEXT_FAINT))
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
                                        .text_color(rgb(theme::TEXT_FAINT))
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
            .bg(theme::color(theme::BG_1).opacity(0.72))
            .border_1()
            .border_color(theme::alpha(theme::STROKE, 0.12))
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
                            .bg(theme::alpha(theme::ACCENT, 0.14))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(15.))
                            .text_color(rgb(theme::ACCENT))
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
                                    .text_color(rgb(theme::TEXT))
                                    .child(u.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(theme::TEXT_FAINT))
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
                        theme::color(theme::JADE),
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

    /// 占位页（未实现里程碑）。
    fn render_placeholder_page(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex_col()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::TEXT))
                            .child(self.page.label()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child("该功能将在对应里程碑实现"),
                    ),
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
        let listen_addr = self
            .store
            .read()
            .map(|s| s.config().settings.routing_for(tool).listen_addr.clone())
            .unwrap_or_else(|_| "127.0.0.1".into());
        let tool = tool;
        let mut rt = std::mem::take(&mut self.routing);
        cx.spawn(async move |this, cx| {
            let port =
                match rt.ensure_running_with(tool, upstreams, model_routes, &listen_addr).await {
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
                match home.as_deref().map(|h| {
                    crate::switch_engine::switch_to_routing_in(tool, port, &backup, h)
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
            let _ = crate::switch_engine::switch_to_in(tool, &provider, &backup, &home);
            let mut store = self.store.write().expect("store lock");
            let mut rc = store.config().settings.routing_for(tool);
            rc.enabled = false;
            let _ = store.set_routing(tool, rc);
        }
    }

    fn render_empty_state(&self, _cx: &mut Context<Self>) -> Div {
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
                    .bg(theme::alpha(theme::ACCENT, 0.08))
                    .border_1()
                    .border_color(theme::alpha(theme::STROKE, 0.14))
                    .child(
                        div()
                            .text_size(px(26.))
                            .text_color(rgb(theme::ACCENT))
                            .child(self.active_tool.glyph()),
                    ),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(rgb(theme::TEXT))
                    .child(format!("还没有 {} 的供应商", self.active_tool.label())),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child("点击右上角「添加供应商」从预设库选择"),
            )
            .child(
                widgets::ghost_button("empty-add", "＋ 添加供应商")
                    .on_click(_cx.listener(|this, _, _, cx| {
                        this.add_open = true;
                        cx.notify();
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

        let p_id = p.id.clone();
        div()
            .id(("provider-card", i))
            .h(px(72.))

            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(12.))
            .rounded(px(12.))
            .relative()
            .bg(if is_current {
                theme::alpha(0x2c2314, 0.6)
            } else {
                theme::alpha(0x261d12, 0.4)
            })
            .border_1()
            .border_color(if is_current {
                theme::alpha(theme::ACCENT, 0.45)
            } else {
                theme::alpha(theme::STROKE, 0.1)
            })
            .hover(|style| {
                style
                    .border_color(theme::alpha(theme::STROKE, 0.24))
                    .bg(theme::alpha(0x2c2314, 0.5))
            })
            .child(
                div()
                    .size(px(38.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme::gold_gradient())
                    .text_color(rgb(theme::DARK_TEXT))
                    .text_size(px(16.))
                    .child(p.name.chars().next().unwrap_or('?').to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(rgb(theme::TEXT))
                                    .child(p.name.clone()),
                            )
                            .child(if is_current {
                                widgets::pill("当前", theme::color(theme::ACCENT))
                            } else {
                                div()
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child(p.id.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
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
                            .text_color(rgb(theme::DARK_TEXT))
                            .bg(theme::gold_gradient())
                            .child("切换")
                            .on_click(cx.listener({
                                let switch_id = p_id.clone();
                                move |this, _, _, cx| {
                                    this.switch_to_provider(&switch_id, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .id(("card-menu", i))
                            .cursor_pointer()
                            .size(px(30.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(theme::TEXT_DIM))
                            .text_size(px(14.))
                            .hover(|s| s.bg(theme::alpha(theme::STROKE, 0.1)))
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
        div()
            .absolute()
            .top(px(64.))
            .right(px(12.))
            .w(px(180.))
            .rounded(px(10.))
            .py(px(6.))
            .bg(theme::color(theme::PANEL_SOLID))
            .border_1()
            .border_color(theme::alpha(theme::STROKE, 0.2))
            .shadow_lg()
            .flex_col()
            .child(self.menu_item("编辑", i, p.id.clone(), cx))
            .child(self.menu_item("复制", i, p.id.clone(), cx))
            .child(self.menu_item("导出", i, p.id.clone(), cx))
            .child(
                div()
                    .h(px(1.))
                    .my(px(4.))
                    .bg(theme::alpha(theme::STROKE, 0.12)),
            )
            .child(self.menu_item_danger("删除", i, p.id.clone(), cx))
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
            .bg(theme::color(theme::PANEL_SOLID))
            .border_1()
            .border_color(theme::alpha(theme::STROKE, 0.2))
            .shadow_lg()
            .flex_col()
            .child(if projects.is_empty() {
                div()
                    .px(px(12.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .text_color(rgb(theme::TEXT_FAINT))
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
                                    .text_color(rgb(theme::TEXT_DIM))
                                    .hover(|s| s.bg(theme::alpha(theme::STROKE, 0.08)))
                                    .child(name.clone())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_menu = None;
                                        let r = {
                                            let mut s = this.store.write().expect("store lock");
                                            s.apply_project(&name)
                                        };
                                        this.status = match r {
                                            Ok(()) => format!("已切换项目 {name}"),
                                            Err(e) => format!("切换失败: {e}"),
                                        };
                                        cx.notify();
                                    }))
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .into_any_element()
            })
    }
    fn menu_item(&self, label: &str, i: usize, id: String, cx: &mut Context<Self>) -> impl IntoElement {
        let label = label.to_string();
        let is_edit = label == "编辑";
        let id2 = id.clone();
        div()
            .id(("menu-item", i))
            .px(px(12.))
            .h(px(30.))
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(rgb(theme::TEXT_DIM))
            .hover(|s| s.bg(theme::alpha(theme::STROKE, 0.08)))
            .child(label.clone())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_menu = None;
                if is_edit {
                    this.open_edit_dialog(&id2, cx);
                } else {
                    this.status = format!("{label} {id2}（M3 实现）");
                    cx.notify();
                }
            }))
    }

    fn menu_item_danger(&self, label: &str, i: usize, id: String, cx: &mut Context<Self>) -> impl IntoElement {
        let label = label.to_string();
        div()
            .id(("menu-item-danger", i))
            .px(px(12.))
            .h(px(30.))
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(rgb(theme::DANGER))
            .hover(|s| s.bg(theme::alpha(theme::DANGER, 0.12)))
            .child(label.clone())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_menu = None;
                this.confirm_delete = Some(id.clone());
                cx.notify();
            }))
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
        self.dialog_inputs
            .search
            .update(cx, |f, _| f.set_text(""));
        cx.notify();
    }

    fn render_statusbar(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(30.))
            .px(px(12.))
            .flex()
            .items_center()
            .justify_between()
            .bg(theme::color(theme::BG_1).opacity(0.78))
            .border_t_1()
            .border_color(theme::alpha(theme::STROKE, 0.1))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(widgets::status_dot(theme::color(theme::JADE), 7.))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(theme::TEXT_DIM))
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
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child(format!("当前: {}", self.active_tool.label())),
                    )
                    .child(widgets::section_label("v0.1.0")),
            )
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::DANGER, 0.4))
                    .shadow_lg()
                    .p(px(18.))
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::DANGER))
                            .child("删除供应商"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT))
                            .child(format!("确定删除「{name}」？此操作不可撤销。")),
                    )
                    .child(if is_current {
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme::DANGER))
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
                                        if ok {
                                            this.refresh_tray_menu(cx);
                                        }
                                        cx.notify();
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
                    .bg(theme::color(theme::PANEL_SOLID))
                    .border_1()
                    .border_color(theme::alpha(theme::DANGER, 0.4))
                    .shadow_lg()
                    .p(px(18.))
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::DANGER))
                            .child("删除通用供应商"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::TEXT))
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
        let pal = theme::palette(mode);
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
                Page::Apps => self.render_placeholder_page(cx).into_any_element(),
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
}
}

/// 从存储快照构建托盘菜单条目（每工具列出供应商，标记当前）。
fn tray_entries(store: &RwLock<Store>) -> Vec<tray::ToolMenuEntry> {
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
        entries.push(tray::ToolMenuEntry {
            tool,
            routing,
            providers,
        });
    }
    entries
}
