//! 通用供应商管理页：跨应用共享配置（一份 baseUrl/apiKey 同步到 Claude/Codex/Gemini）。
//!
//! 与 cc-switch `UniversalProvider` 语义一致：`save_universal` 投影子供应商到
//! `providers`（`universal-claude-<id>` 等），各应用切换时直接写 live。

use gpui::{Context, Div, div, prelude::*, px};
use ns_presets::{UniversalApps, UniversalModels};
use ns_ui::{theme, widgets};

/// 通用供应商表单字段（对话框输入）。
#[derive(Debug, Clone, Default)]
pub struct UniversalForm {
    pub claude: bool,
    pub codex: bool,
    pub gemini: bool,
}

/// 通用供应商对话框输入实体。
pub struct UniversalInputs {
    pub name: gpui::Entity<crate::text_field::TextField>,
    pub provider_type: gpui::Entity<crate::text_field::TextField>,
    pub base_url: gpui::Entity<crate::text_field::TextField>,
    pub api_key: gpui::Entity<crate::text_field::TextField>,
    pub claude_model: gpui::Entity<crate::text_field::TextField>,
    pub codex_model: gpui::Entity<crate::text_field::TextField>,
    pub gemini_model: gpui::Entity<crate::text_field::TextField>,
}

impl UniversalInputs {
    pub fn new(cx: &mut Context<crate::app::SwitchApp>) -> Self {
        Self {
            name: cx.new(|cx| crate::text_field::TextField::new("NewAPI", cx)),
            provider_type: cx.new(|cx| crate::text_field::TextField::new("newapi", cx)),
            base_url: cx.new(|cx| crate::text_field::TextField::new("https://api.example.com", cx)),
            api_key: cx.new(|cx| crate::text_field::TextField::new("sk-…", cx)),
            claude_model: cx.new(|cx| crate::text_field::TextField::new("claude-sonnet-5", cx)),
            codex_model: cx.new(|cx| crate::text_field::TextField::new("gpt-5.6-sol", cx)),
            gemini_model: cx.new(|cx| crate::text_field::TextField::new("gemini-3.6-flash", cx)),
        }
    }
}

/// 通用供应商对话框。
pub fn universal_dialog(
    editing: bool,
    form: &UniversalForm,
    inputs: &UniversalInputs,
    cx: &mut Context<crate::app::SwitchApp>,
) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::alpha(0x000000, 0.55))
        .child(
            div()
                .w(px(560.))
                .max_h(px(640.))
                .rounded(px(14.))
                .bg(theme::panel())
                .border_1()
                .border_color(theme::stroke_a(0.25))
                .shadow_lg()
                .flex_col()
                .child(
                    div()
                        .h(px(44.))
                        .px(px(16.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(theme::stroke_a(0.12))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(theme::text())
                                .child(if editing {
                                    "编辑通用供应商"
                                } else {
                                    "添加通用供应商"
                                }),
                        )
                        .child(widgets::ghost_button("uni-dlg-close", "✕").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.universal_open = false;
                                cx.notify();
                            }),
                        )),
                )
                .child(
                    div()
                        .id("uni-dlg-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .p(px(16.))
                        .flex_col()
                        .gap(px(10.))
                        .child(field("名称", &inputs.name))
                        .child(field("类型标识", &inputs.provider_type))
                        .child(field("Base URL", &inputs.base_url))
                        .child(field("API Key", &inputs.api_key))
                        .child(apps_row(form, cx))
                        .child(div().h(px(1.)).bg(theme::stroke_a(0.1)))
                        .child(field("Claude 模型", &inputs.claude_model))
                        .child(field("Codex 模型", &inputs.codex_model))
                        .child(field("Gemini 模型", &inputs.gemini_model)),
                )
                .child(
                    div()
                        .h(px(52.))
                        .px(px(16.))
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(10.))
                        .border_t_1()
                        .border_color(theme::stroke_a(0.1))
                        .child(widgets::ghost_button("uni-dlg-cancel", "取消").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.universal_open = false;
                                cx.notify();
                            }),
                        ))
                        .child(
                            widgets::ghost_button("uni-dlg-save", "保存")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let _ = save_universal_from_dialog(this, cx);
                                })),
                        )
                        .child(
                            widgets::primary_button("uni-dlg-save-sync", "保存并同步")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let saved_id = save_universal_from_dialog(this, cx);
                                    if let Some(id) = saved_id {
                                        let result = {
                                            let mut store =
                                                this.store.write().expect("store lock");
                                            store.sync_universal_children(&id)
                                        };
                                        match result {
                                            Ok(()) => {
                                                this.status = format!("已保存并同步通用供应商");
                                            }
                                            Err(e) => {
                                                this.status = format!("同步失败: {e}");
                                            }
                                        }
                                        cx.notify();
                                    }
                                })),
                        ),
                ),
        )
}

/// 应用启用勾选行。
fn apps_row(form: &UniversalForm, cx: &mut Context<crate::app::SwitchApp>) -> Div {
    let apps = [("claude", "Claude", form.claude), ("codex", "Codex", form.codex), ("gemini", "Gemini", form.gemini)];
    div()
        .flex()
        .items_center()
        .gap(px(14.))
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::text_dim())
                .child("同步到"),
        )
        .children(apps.into_iter().map(|(id, label, checked)| {
            let id_static: &'static str = match id {
                "claude" => "claude",
                "codex" => "codex",
                _ => "gemini",
            };
            let app_id = gpui::ElementId::from(gpui::SharedString::from(id_static));
            div()
                .id(app_id)
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    match id_static {
                        "claude" => this.universal_form.claude = !this.universal_form.claude,
                        "codex" => this.universal_form.codex = !this.universal_form.codex,
                        _ => this.universal_form.gemini = !this.universal_form.gemini,
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .size(px(14.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(theme::stroke_a(0.4))
                        .bg(if checked {
                            theme::accent()
                        } else {
                            theme::alpha(0xffffff, 0.02)
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(if checked {
                            div().text_size(px(9.)).text_color(theme::dark_text()).child("✓")
                        } else {
                            div()
                        }),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::text())
                        .child(label),
                )
        }))
}

/// 字段行。
fn field(
    label: impl Into<gpui::SharedString>,
    input: &gpui::Entity<crate::text_field::TextField>,
) -> Div {
    let label: gpui::SharedString = label.into();
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(
            div()
                .w(px(110.))
                .flex_none()
                .text_size(px(11.))
                .text_color(theme::text_dim())
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .h(px(32.))
                .px(px(10.))
                .rounded(px(8.))
                .bg(theme::alpha(0x000000, 0.25))
                .border_1()
                .border_color(theme::stroke_a(0.14))
                .flex()
                .items_center()
                .child(input.clone()),
        )
}

/// 从对话框输入保存通用供应商。返回保存成功的 id（保存失败/空输入返回 None）。
fn save_universal_from_dialog(
    this: &mut crate::app::SwitchApp,
    cx: &mut Context<crate::app::SwitchApp>,
) -> Option<String> {
    let name = this.universal_inputs.name.read(cx).text().trim().to_string();
    if name.is_empty() {
        this.status = "通用供应商名称不能为空".into();
        cx.notify();
        return None;
    }
    let base_url = this.universal_inputs.base_url.read(cx).text().trim().to_string();
    if base_url.is_empty() {
        this.status = "Base URL 不能为空".into();
        cx.notify();
        return None;
    }
    let api_key = this.universal_inputs.api_key.read(cx).text().trim().to_string();
    let provider_type = this.universal_inputs.provider_type.read(cx).text().trim().to_string();
    let provider_type = if provider_type.is_empty() { "newapi".into() } else { provider_type };
    let claude_model = this.universal_inputs.claude_model.read(cx).text().trim().to_string();
    let codex_model = this.universal_inputs.codex_model.read(cx).text().trim().to_string();
    let gemini_model = this.universal_inputs.gemini_model.read(cx).text().trim().to_string();

    let id = this.universal_edit.clone().unwrap_or_else(|| {
        let slug: String = name
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect();
        if slug.is_empty() { "gateway".into() } else { slug }
    });

    let u = ns_presets::UniversalProvider {
        id: id.clone(),
        name,
        provider_type,
        apps: UniversalApps {
            claude: this.universal_form.claude,
            codex: this.universal_form.codex,
            gemini: this.universal_form.gemini,
        },
        base_url,
        api_key,
        models: UniversalModels {
            claude: Some(ns_presets::UniversalClaudeModels {
                model: Some(claude_model),
                ..Default::default()
            }),
            codex: Some(ns_presets::UniversalCodexModels {
                model: Some(codex_model),
                ..Default::default()
            }),
            gemini: Some(ns_presets::UniversalGeminiModels {
                model: Some(gemini_model),
            }),
        },
        website_url: None,
        icon: Some("newapi".into()),
        icon_color: Some("#00A67E".into()),
        created_at: this.universal_edit.as_ref().and_then(|_| {
            this.store.read().expect("store lock").config().universal.get(&id).and_then(|u| u.created_at)
        }).or_else(|| Some(crate::app::now_ts())),
    };

    let result = {
        let mut store = this.store.write().expect("store lock");
        store.save_universal(u)
    };
    match result {
        Ok(()) => {
            this.status = format!("已保存通用供应商 {id}");
            this.universal_open = false;
            this.universal_edit = None;
            cx.notify();
            Some(id)
        }
        Err(e) => {
            this.status = format!("保存失败: {e}");
            cx.notify();
            None
        }
    }
}
