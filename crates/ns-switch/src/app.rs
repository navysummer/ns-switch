//! 主界面：顶栏、侧栏（工具列表）、供应商页、状态栏。

use std::sync::{Arc, RwLock};

use gpui::{
    Context, Div, FocusHandle, KeyBinding, Render, Stateful, div, prelude::*, px, rgb,
    actions,
};
use ns_presets::Tool;
use ns_store::Store;
use ns_ui::{theme, widgets};

actions!(
    ns_switch,
    [
        QuitApp,
        ToggleSidebar,
        RefreshProviders,
    ]
);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-q", QuitApp, None),
        KeyBinding::new("ctrl-q", QuitApp, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("ctrl-b", ToggleSidebar, None),
        KeyBinding::new("cmd-r", RefreshProviders, None),
        KeyBinding::new("ctrl-r", RefreshProviders, None),
    ]
}

/// 界面页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Providers,
    Routing,
    Mcp,
    Skills,
    Prompts,
    Usage,
    Sessions,
    Apps,
    Settings,
}

impl Page {
    const ALL: [Page; 9] = [
        Page::Providers,
        Page::Routing,
        Page::Mcp,
        Page::Skills,
        Page::Prompts,
        Page::Usage,
        Page::Sessions,
        Page::Apps,
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
            Page::Sessions => "会话",
            Page::Apps => "应用",
            Page::Settings => "设置",
        }
    }
}

/// 应用状态。
pub struct SwitchApp {
    store: Arc<RwLock<Store>>,
    active_tool: Tool,
    page: Page,
    sidebar_visible: bool,
    status: String,
    focus_handle: FocusHandle,
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
        Self {
            store,
            active_tool: Tool::Claude,
            page: Page::Providers,
            sidebar_visible: true,
            status: "就绪".into(),
            focus_handle: cx.focus_handle(),
        }
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
                                            .child(page.label()),
                                    )
                            })),
                    ),
            )
    }

    fn render_provider_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let providers = {
            let store = self.store.read().expect("store lock");
            let providers = store.config().providers_for(self.active_tool);
            providers.into_iter().map(|p| p.clone()).collect::<Vec<_>>()
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
                            .text_size(px(10.5))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child(format!("{} 个供应商", providers.len())),
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
                    .children(if providers.is_empty() {
                        vec![self.render_empty_state(cx)]
                    } else {
                        providers
                            .iter()
                            .enumerate()
                            .map(|(i, p)| self.render_provider_card(i, p.clone(), cx))
                            .collect()
                    }),
            )
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
                        this.status = "添加供应商（M3 实现）".into();
                        cx.notify();
                    })),
            )
    }

    fn render_provider_card(&self, _i: usize, _p: ns_presets::Provider, _cx: &mut Context<Self>) -> Div {
        div()
            .h(px(72.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(12.))
            .rounded(px(12.))
            .bg(theme::alpha(0x261d12, 0.4))
            .border_1()
            .border_color(theme::alpha(theme::STROKE, 0.1))
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
                    .child(_p.name.chars().next().unwrap_or('?').to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(rgb(theme::TEXT))
                            .child(_p.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child(_p.id.clone()),
                    ),
            )
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
}

impl Render for SwitchApp {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content: Stateful<Div> = div()
            .id("provider-content")
            .flex_1()
            .h_full()
            .child(self.render_provider_page(cx));

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(theme::app_background())
            .font_family(theme::UI_FONT)
            .text_color(rgb(theme::TEXT))
            .track_focus(&self.focus_handle)
            .key_context("App")
            .on_action(cx.listener(|_this, _: &QuitApp, _, cx| cx.quit()))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar_visible = !this.sidebar_visible;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &RefreshProviders, _, cx| this.refresh(cx)))
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
    }
}
