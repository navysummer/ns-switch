//! 添加/编辑供应商对话框：预设搜索 + 关键字段表单 + 配置预览。

use gpui::{Context, Div, div, prelude::*, px};
use ns_presets::{Preset, Provider, ProviderMeta, search_presets};
use ns_ui::{theme, widgets};
use crate::app::StatusTone;

/// 对话框字段输入。
#[derive(Debug, Clone, Default)]
pub struct DialogFields {
    /// 供应商显示名（可自定义；空 = 用预设名）。
    pub name: String,
    pub endpoint: String,
    pub key: String,
    pub model: String,
    pub usage_script: String,
    /// API 格式：anthropic / openai_chat / openai_responses / gemini_native
    pub api_format: String,
    /// 图标（预设/供应商图标 id；自定义可覆盖）。
    pub icon: String,
    /// 图标颜色（十六进制 `#RRGGBB`；空 = 使用主题强调色）。
    pub icon_color: String,
    /// 是否启用用量查询（false = 卡片不显示配额脚注/不自动查询）。
    pub usage_enabled: bool,
}

impl DialogFields {
    /// 从预设构建字段（endpoint 留空由预设/用户填）。
    pub fn from_preset(preset: &Preset) -> Self {
        let mut f = Self::default();
        f.name = preset.name.to_string();
        f.model = preset_default_model(preset);
        f.api_format = preset.api_format.to_string();
        f.icon = preset.icon.to_string();
        f.icon_color = preset.icon_color.to_string();
        f.usage_enabled = true;
        f
    }

    /// 从已有供应商读取可编辑字段（编辑对话框预填）。
    /// 扫描 `settings_config` 顶层与嵌套对象（`env`/`auth`/`config`），
    /// 先按精确键名匹配，再退回关键字包含匹配（排除 `model_provider`
    /// 这类语义不同但名字相近的键），避免把协议选择器误当成模型名。
    pub fn from_provider(p: &Provider) -> Self {
        fn exact(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
            match value {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        if let Some(s) = v.as_str() {
                            if !s.is_empty()
                                && keys.iter().any(|k2| k.eq_ignore_ascii_case(k2))
                            {
                                return Some(s.to_string());
                            }
                        }
                    }
                    map.values().find_map(|v| exact(v, keys))
                }
                _ => None,
            }
        }
        fn fuzzy(
            value: &serde_json::Value,
            keys: &[&str],
            exclude: &[&str],
        ) -> Option<String> {
            match value {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        let lk = k.to_lowercase();
                        if let Some(s) = v.as_str() {
                            if !s.is_empty()
                                && !exclude.iter().any(|e| lk.contains(e))
                                && keys.iter().any(|k2| lk.contains(&k2.to_lowercase()))
                            {
                                return Some(s.to_string());
                            }
                        }
                    }
                    map.values().find_map(|v| fuzzy(v, keys, exclude))
                }
                _ => None,
            }
        }
        fn find(
            value: &serde_json::Value,
            exact_keys: &[&str],
            fuzzy_keys: &[&str],
            exclude: &[&str],
        ) -> String {
            exact(value, exact_keys)
                .or_else(|| fuzzy(value, fuzzy_keys, exclude))
                .unwrap_or_default()
        }
        let cfg = &p.settings_config;
        Self {
            name: p.name.clone(),
            endpoint: find(
                cfg,
                &[
                    "base_url",
                    "baseUrl",
                    "ANTHROPIC_BASE_URL",
                    "GEMINI_BASE_URL",
                    "OPENAI_BASE_URL",
                    "endpoint",
                    "url",
                ],
                &["endpoint", "base_url", "baseurl", "url"],
                &[],
            ),
            key: find(
                cfg,
                &[
                    "ANTHROPIC_AUTH_TOKEN",
                    "ANTHROPIC_API_KEY",
                    "OPENAI_API_KEY",
                    "GEMINI_API_KEY",
                    "GOOGLE_API_KEY",
                    "experimental_bearer_token",
                    "api_key",
                    "apiKey",
                    "auth_token",
                    "token",
                ],
                &["api_key", "apikey", "auth_token", "token", "key"],
                &["header"],
            ),
            model: find(
                cfg,
                &[
                    "model",
                    "default_model",
                    "defaultModel",
                    "ANTHROPIC_MODEL",
                    "GEMINI_MODEL",
                ],
                &["model"],
                &["provider", "providers", "catalog", "models", "effort"],
            ),
            usage_script: p
                .meta
                .as_ref()
                .and_then(|m| m.usage_script.clone())
                .unwrap_or_default(),
            api_format: p
                .meta
                .as_ref()
                .and_then(|m| m.api_format.clone())
                .unwrap_or_else(|| "anthropic".into()),
            icon: p.icon.clone().unwrap_or_default(),
            icon_color: p.icon_color.clone().unwrap_or_default(),
            usage_enabled: p
                .meta
                .as_ref()
                .map(|m| m.usage_enabled)
                .unwrap_or(true),
        }
    }
}
/// 对话框输入实体（真实可编辑文本域）。
pub struct DialogInputs {
    pub search: gpui::Entity<crate::text_field::TextField>,
    pub name: gpui::Entity<crate::text_field::TextField>,
    pub endpoint: gpui::Entity<crate::text_field::TextField>,
    pub key: gpui::Entity<crate::text_field::TextField>,
    pub model: gpui::Entity<crate::text_field::TextField>,
    pub usage_script: gpui::Entity<crate::text_field::TextField>,
    pub icon: gpui::Entity<crate::text_field::TextField>,
    pub icon_color: gpui::Entity<crate::text_field::TextField>,
}

impl DialogInputs {
    pub fn new(
        cx: &mut gpui::Context<crate::app::SwitchApp>,
    ) -> Self {
        Self {
            search: cx.new(|cx| crate::text_field::TextField::new("搜索预设…", cx)),
            name: cx.new(|cx| crate::text_field::TextField::new("供应商名称", cx)),
            endpoint: cx.new(|cx| crate::text_field::TextField::new("https://…", cx)),
            key: cx.new(|cx| crate::text_field::TextField::new("sk-…", cx)),
            model: cx.new(|cx| crate::text_field::TextField::new("模型名", cx)),
            usage_script: cx.new(|cx| crate::text_field::TextField::new("/path/to/usage.js", cx)),
            icon: cx.new(|cx| crate::text_field::TextField::new("图标 id（如 openai）", cx)),
            icon_color: cx.new(|cx| crate::text_field::TextField::new("#RRGGBB（空=主题色）", cx)),
        }
    }
}
/// 预设默认模型（无则空）。
fn preset_default_model(_preset: &Preset) -> String {
    String::new()
}

/// 添加供应商对话框主体。
pub fn add_dialog(
    tool: ns_presets::Tool,
    selected: Option<&'static Preset>,
    editing: bool,
    fields: &DialogFields,
    inputs: &DialogInputs,
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
                .w(px(620.))
                .max_h(px(680.))
                .rounded(px(14.))
                .bg(theme::panel())
                .border_1()
                .border_color(theme::stroke_a(0.25))
                .shadow_lg()
                .flex_col()
                .child(dialog_header(tool, editing, cx))
                .child(search_box(inputs, cx))
                .child(if let Some(preset) = selected {
                    form_and_preview(preset, fields, inputs, editing, cx).into_any_element()
                } else {
                    preset_grid(tool, inputs, cx).into_any_element()
                }),
        )
}

/// 关闭对话框：有未保存修改时先弹守卫浮层（✕ / 取消 / Esc 共用）。
fn request_close(
    app: &mut crate::app::SwitchApp,
    cx: &mut Context<crate::app::SwitchApp>,
) {
    if app.dialog_is_dirty(cx) {
        app.dialog_dirty = true;
        cx.notify();
        return;
    }
    app.add_open = false;
    app.edit_id = None;
    app.dialog_preset = None;
    cx.notify();
}

fn dialog_header(
    tool: ns_presets::Tool,
    editing: bool,
    cx: &mut Context<crate::app::SwitchApp>,
) -> Div {
    let title = if editing {
        format!("编辑供应商 · {}", tool.label())
    } else {
        format!("添加供应商 · {}", tool.label())
    };
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
                .text_size(px(14.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(title),
        )
        .child(widgets::ghost_button("dlg-close", "✕").on_click(cx.listener(
            |this, _, _, cx| request_close(this, cx),
        )))
}

fn search_box(inputs: &DialogInputs, _cx: &mut Context<crate::app::SwitchApp>) -> Div {
    div()
        .px(px(16.))
        .py(px(12.))
        .child(
            div()
                .h(px(34.))
                .px(px(10.))
                .rounded(px(8.))
                .bg(theme::alpha(0x000000, 0.25))
                .border_1()
                .border_color(theme::stroke_a(0.14))
                .flex()
                .items_center()
                .child(inputs.search.clone()),
        )
}

fn preset_grid(
    tool: ns_presets::Tool,
    inputs: &DialogInputs,
    cx: &mut Context<crate::app::SwitchApp>,
) -> impl IntoElement {
    let query = inputs.search.read(cx).text();
    let presets = search_presets(&query, Some(tool));
    let mut rows = Vec::new();
    for p in presets.into_iter().take(20) {
        rows.push(preset_row(p, cx).into_any_element());
    }
    div()
        .id("preset-grid")
        .flex_1()
        .max_h(px(500.))
        .min_h(px(0.))
        .overflow_y_scroll()
        .p(px(16.))
        .flex_col()
        .gap(px(6.))
        .children(rows)
}

fn preset_row(preset: &'static Preset, cx: &mut Context<crate::app::SwitchApp>) -> impl IntoElement {
    let pid = preset.id.to_string();
    div()
        .id(gpui::SharedString::from(pid.clone()))
        .h(px(40.))
        .px(px(10.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .gap(px(10.))
        .cursor_pointer()
        .hover(|s| s.bg(theme::stroke_a(0.08)))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.dialog_preset = Some(pid.clone());
            this.dialog_fields = DialogFields::from_preset(preset);
            // 选择预设后回填预设自带字段；接口地址/密钥保持用户当前输入（同预设可只改 key）。
            let fields = this.dialog_fields.clone();
            this.dialog_inputs
                .name
                .update(cx, |f, _| f.set_text(&fields.name));
            this.dialog_inputs
                .model
                .update(cx, |f, _| f.set_text(&fields.model));
            this.dialog_inputs
                .icon
                .update(cx, |f, _| f.set_text(&fields.icon));
            this.dialog_inputs
                .icon_color
                .update(cx, |f, _| f.set_text(&fields.icon_color));
            cx.notify();
        }))
        .child(
            div()
                .size(px(24.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme::gold_gradient())
                .text_color(theme::dark_text())
                .text_size(px(11.))
                .child(preset.name.chars().next().unwrap_or('?').to_string()),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(12.))
                .text_color(theme::text())
                .child(preset.name),
        )
        .child(
            div()
                .text_size(px(9.5))
                .text_color(theme::text_faint())
                .child(preset.aliases.first().copied().unwrap_or("")),
        )
}


/// 图标选择器：品牌字形网格（点选写入图标 id）。
fn icon_picker(
    icon: &gpui::Entity<crate::text_field::TextField>,
    cx: &mut Context<crate::app::SwitchApp>,
) -> Div {
    let current = icon.read(cx).text();
    div()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(theme::text_faint())
                .child("选择图标"),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(5.))
                .children(ns_presets::ICONS.iter().enumerate().map(
                    |(i, (id, label, glyph))| {
                        let id = *id;
                        let selected = current == id;
                        div()
                            .id(("dlg-icon", i))
                            .size(px(28.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .bg(if selected {
                                theme::accent().opacity(0.28)
                            } else {
                                theme::panel_a(0.4)
                            })
                            .border_1()
                            .border_color(if selected {
                                theme::accent()
                            } else {
                                theme::stroke_a(0.15)
                            })
                            .text_color(if selected {
                                theme::accent()
                            } else {
                                theme::text_dim()
                            })
                            .text_size(px(13.))
                            .child(glyph.to_string())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.dialog_inputs
                                    .icon
                                    .update(cx, |f, _| f.set_text(id));
                                this.status = format!("图标：{label}");
                                this.status_tone = StatusTone::Neutral;
                                cx.notify();
                            }))
                    },
                )),
        )
}

/// 颜色选择器：预设色板（点选写入十六进制颜色）。
fn color_picker(
    color: &gpui::Entity<crate::text_field::TextField>,
    cx: &mut Context<crate::app::SwitchApp>,
) -> Div {
    let current = color.read(cx).text().to_lowercase();
    div()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(theme::text_faint())
                .child("选择颜色"),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(5.))
                .children(ns_presets::ICON_COLORS.iter().enumerate().map(
                    |(i, (hex, name))| {
                        let hex = *hex;
                        let name = *name;
                        let rgb = crate::app::parse_hex_color(hex).unwrap_or(0xc9a25f);
                        let selected = current == hex.to_lowercase();
                        div()
                            .id(("dlg-color", i))
                            .size(px(22.))
                            .rounded_full()
                            .cursor_pointer()
                            .bg(theme::color(rgb))
                            .border_1()
                            .border_color(if selected {
                                theme::text()
                            } else {
                                theme::stroke_a(0.2)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.dialog_inputs
                                    .icon_color
                                    .update(cx, |f, _| f.set_text(hex));
                                this.status = format!("图标颜色：{name}");
                                this.status_tone = StatusTone::Neutral;
                                cx.notify();
                            }))
                    },
                )),
        )
}

impl DialogFields {
    /// 用当前输入实体覆盖文本字段（Cmd-S 等非渲染入口使用）。
    ///
    /// `api_format`/`usage_enabled` 等开关字段仍取自 `self`（由界面直接写）。
    pub fn with_inputs(&self, inputs: &DialogInputs, cx: &gpui::App) -> Self {
        let mut f = self.clone();
        f.name = inputs.name.read(cx).text();
        f.endpoint = inputs.endpoint.read(cx).text();
        f.key = inputs.key.read(cx).text();
        f.model = inputs.model.read(cx).text();
        f.usage_script = inputs.usage_script.read(cx).text();
        f.icon = inputs.icon.read(cx).text();
        f.icon_color = inputs.icon_color.read(cx).text();
        f
    }
}

fn form_and_preview(
    preset: &'static Preset,
    fields: &DialogFields,
    inputs: &DialogInputs,
    editing: bool,
    cx: &mut Context<crate::app::SwitchApp>,
) -> Div {
    let endpoint = inputs.endpoint.read(cx).text();
    let key = inputs.key.read(cx).text();
    let model = inputs.model.read(cx).text();
    let name = inputs.name.read(cx).text();
    let mut live_fields = fields.clone();
    live_fields.name = name.clone();
    live_fields.endpoint = endpoint.clone();
    live_fields.key = key.clone();
    live_fields.model = model.clone();
    live_fields.usage_script = inputs.usage_script.read(cx).text();
    live_fields.icon = inputs.icon.read(cx).text();
    live_fields.icon_color = inputs.icon_color.read(cx).text();
    if live_fields.api_format.is_empty() {
        live_fields.api_format = preset.api_format.to_string();
    }
    let save_label = if editing { "保存修改" } else { "添加并保存" };

    div()
        .flex_1()
        .flex_col()
        .min_h(px(0.))
        .child(
            div()
                .flex_1()
                .flex()
                .min_h(px(0.))
                .gap(px(0.))
                .child(
                    div()
                        .id("dlg-form")
                        .w(px(300.))
                        .max_h(px(500.))
                        .p(px(16.))
                        .flex_col()
                        .gap(px(10.))
                        .overflow_y_scroll()
                        .child(field_input("供应商名称", &inputs.name, cx))
                .child(field_input("接口地址", &inputs.endpoint, cx))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .child(
                            widgets::ghost_button("dlg-test-endpoint", "测试连接")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let url = this.dialog_inputs.endpoint.read(cx).text().trim().to_string();
                                    if url.is_empty() {
                                        this.status = "请先填写接口地址".into();
                                        cx.notify();
                                        return;
                                    }
                                    let name = "对话框端点".to_string();
                                    this.status = format!("正在测试连接：{name} …");
                                    cx.notify();
                                    cx.spawn(async move |this, cx| {
                                        let test_url = url.clone();
                                        let result = cx
                                            .background_executor()
                                            .spawn(async move {
                                                std::process::Command::new("curl")
                                                    .args(["-sS", "-o", "/dev/null", "-m", "8", "-w", "%{http_code}", "-I", &test_url])
                                                    .output()
                                            })
                                            .await;
                                        let msg = match result {
                                            Ok(o) => {
                                                let code = String::from_utf8_lossy(&o.stdout).trim().to_string();
                                                if o.status.success() || (!code.is_empty() && code != "000") {
                                                    format!("HTTP {code} · {url}")
                                                } else {
                                                    format!("无法连接 · {url}")
                                                }
                                            }
                                            Err(e) => format!("执行 curl 失败: {e}"),
                                        };
                                        this.update(cx, |this, cx| {
                                            this.status = format!("连通性测试：{msg}");
                                            cx.notify();
                                        }).ok();
                                    }).detach();
                                })),
                        ),
                )
                .child(field_input("API 密钥", &inputs.key, cx))
                .child(field_input("模型", &inputs.model, cx))
                .child(api_format_selector(&live_fields.api_format, cx))
                .child(field_input("用量脚本路径", &inputs.usage_script, cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(widgets::section_label("启用用量查询"))
                        .child(
                            div()
                                .id("dlg-usage-enabled")
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dialog_fields.usage_enabled =
                                        !this.dialog_fields.usage_enabled;
                                    cx.notify();
                                }))
                                .child(if live_fields.usage_enabled {
                                    widgets::pill("启用", theme::jade())
                                } else {
                                    widgets::pill("停用", theme::text_dim())
                                }),
                        ),
                )
                .child(field_input("图标", &inputs.icon, cx))
                .child(icon_picker(&inputs.icon, cx))
                .child(field_input("图标颜色", &inputs.icon_color, cx))
                .child(color_picker(&inputs.icon_color, cx)),
                )
                .child(
                    div()
                        .id("dlg-preview")
                        .flex_1()
                        .max_h(px(500.))
                        .min_w(px(0.))
                        .border_l_1()
                        .border_color(theme::stroke_a(0.12))
                        .p(px(16.))
                        .flex_col()
                        .gap(px(8.))
                        .overflow_y_scroll()
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(theme::text_faint())
                                .child("配置预览（切换后写入客户端的关键字段）"),
                        )
                        .child(
                            div()
                                .font_family(theme::MONO_FONT)
                                .text_size(px(10.))
                                .text_color(theme::text_dim())
                                .child(preview_text(preset, &live_fields)),
                        ),
                ),
        )
        .child(
            div()
                .px(px(16.))
                .py(px(12.))
                .flex()
                .justify_end()
                .gap(px(8.))
                .border_t_1()
                .border_color(theme::stroke_a(0.12))
                .child(widgets::ghost_button("dlg-cancel", "取消").on_click(cx.listener(
                    |this, _, _, cx| request_close(this, cx),
                )))
                .child(
                    widgets::primary_button("dlg-save", save_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let save_fields = live_fields.clone();
                            crate::dialog::save_provider(this, preset, &save_fields, cx);
                        })),
                ),
        )
}

fn api_format_selector(current: &str, cx: &mut Context<crate::app::SwitchApp>) -> Div {
    let formats = [
        ("anthropic", "Anthropic"),
        ("openai_chat", "OpenAI Chat"),
        ("openai_responses", "OpenAI Resp"),
        ("gemini_native", "Gemini"),
    ];
    let cur = if current.is_empty() { "anthropic" } else { current };
    div()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(10.))
                .text_color(theme::text_faint())
                .child("上游 API 格式（路由/聚合使用）"),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .children(formats.into_iter().enumerate().map(|(i, (id, label))| {
                    let selected = cur == id;
                    let id_owned = id.to_string();
                    let mut b = div()
                        .id(("fmt", i))
                        .h(px(26.))
                        .px(px(8.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .text_size(px(10.5))
                        .child(label.to_string());
                    b = if selected {
                        b.bg(theme::accent_a(0.18))
                            .border_1()
                            .border_color(theme::accent_a(0.5))
                            .text_color(theme::text())
                    } else {
                        b.bg(theme::surface_a(0.35))
                            .border_1()
                            .border_color(theme::stroke_a(0.12))
                            .text_color(theme::text_dim())
                            .hover(|s| s.bg(theme::stroke_a(0.08)))
                    };
                    b.on_click(cx.listener(move |this, _, _, cx| {
                        this.dialog_fields.api_format = id_owned.clone();
                        cx.notify();
                    }))
                })),
        )
}

fn field_input(
    label: &str,
    input: &gpui::Entity<crate::text_field::TextField>,
    _cx: &mut Context<crate::app::SwitchApp>,
) -> Div {
    let label = label.to_string();
    div()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(10.))
                .text_color(theme::text_faint())
                .child(label),
        )
        .child(
            div()
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

/// 预览文本：用预设 build 生成 settingsConfig，再显示 JSON。
fn preview_text(preset: &Preset, fields: &DialogFields) -> String {
    let cfg = (preset.build)(&fields.endpoint, &fields.key, &fields.model);
    serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| String::new())
}

/// 由表单构建 Provider（保存时调用）。
pub fn build_provider(
    preset: &'static Preset,
    fields: &DialogFields,
) -> Provider {
    Provider {
        id: format!("{}-{}", preset.id, crate::app::now_ts_suffix()),
        name: if fields.name.trim().is_empty() {
            preset.name.to_string()
        } else {
            fields.name.trim().to_string()
        },
        settings_config: (preset.build)(&fields.endpoint, &fields.key, &fields.model),
        website_url: preset.website_url.map(str::to_string),
        category: Some(preset.category),
        created_at: Some(crate::app::now_ts()),
        sort_index: None,
        notes: None,
        meta: {
            let fmt = if fields.api_format.is_empty() {
                preset.api_format
            } else {
                fields.api_format.as_str()
            };
            let has_fmt = fmt != "anthropic";
            let routes = preset.desktop_routes.map(|(s, o, h)| {
                vec![s.to_string(), o.to_string(), h.to_string()]
            });
            if !has_fmt && routes.is_none() {
                None
            } else {
                Some(ProviderMeta {
                    api_format: has_fmt.then(|| fmt.to_string()),
                    models: routes,
                    usage_enabled: fields.usage_enabled,
                    ..Default::default()
                })
            }
        },
        icon: if fields.icon.trim().is_empty() {
            Some(preset.icon.to_string())
        } else {
            Some(fields.icon.trim().to_string())
        },
        icon_color: if fields.icon_color.trim().is_empty() {
            Some(preset.icon_color.to_string())
        } else {
            Some(fields.icon_color.trim().to_string())
        },
        in_failover_queue: false,
    }
}

/// 保存：构建 Provider 写入存储（新增或编辑），并关闭对话框。
pub fn save_provider(
    app: &mut crate::app::SwitchApp,
    preset: &'static Preset,
    fields: &DialogFields,
    cx: &mut gpui::Context<crate::app::SwitchApp>,
) {
    // 校验：官方/开源官方走 env 内置配置，无需 endpoint；本机服务无需密钥；
    // 其余供应商必须填接口地址，云服务/聚合/第三方/自定义必须填密钥。
    let is_official = matches!(
        preset.category,
        ns_presets::ProviderCategory::Official | ns_presets::ProviderCategory::CnOfficial
    );
    let is_local = preset.category == ns_presets::ProviderCategory::Local;
    if !is_official && fields.endpoint.trim().is_empty() {
        app.status = "请填写接口地址".into();
        app.status_tone = StatusTone::Error;
        cx.notify();
        return;
    }
    if !is_local && !is_official && fields.key.trim().is_empty() {
        app.status = "请填写 API 密钥".into();
        app.status_tone = StatusTone::Error;
        cx.notify();
        return;
    }
    let editing = app.edit_id.clone();
    let provider = build_provider(preset, fields);
    let name = provider.name.clone();
    let id = provider.id.clone();
    let result = {
        let mut store = app.store.write().expect("store lock");
        if let Some(edit_id) = editing.as_deref() {
            // 编辑：保留原 id/创建时间，只更新关键字段。
            let mut updated = store
                .config()
                .providers
                .get(edit_id)
                .cloned()
                .unwrap_or(provider.clone());
            updated.settings_config = provider.settings_config.clone();
            updated.name = provider.name.clone();
            updated.icon = provider.icon.clone();
            updated.icon_color = provider.icon_color.clone();
            // 合并用量脚本路径、API 格式与用量开关（保留原 meta 其他字段）。
            {
                let mut meta = updated.meta.take().unwrap_or_default();
                meta.usage_script = if fields.usage_script.trim().is_empty() {
                    None
                } else {
                    Some(fields.usage_script.trim().to_string())
                };
                meta.api_format = if fields.api_format.trim().is_empty() {
                    None
                } else {
                    Some(fields.api_format.trim().to_string())
                };
                meta.usage_enabled = fields.usage_enabled;
                updated.meta = Some(meta);
            }
            store.update_provider(updated)
        } else {
            store.add_provider(provider)
        }
    };
    if let Err(e) = result {
        app.status = format!("保存失败: {e}");
        cx.notify();
        return;
    }
    app.add_open = false;
    app.edit_id = None;
    app.dialog_preset = None;
    app.status = if editing.is_some() {
        format!("已更新供应商 {name}（{id}）")
    } else {
        format!("已添加供应商 {name}（{id}）")
    };
    app.status_tone = StatusTone::Success;
    app.refresh_tray_menu(cx);
    cx.notify();
}

#[cfg(test)]
mod tests {
    use super::*;
    use ns_presets::Tool;

    #[test]
    fn build_provider_from_preset_roundtrips() {
        let preset = ns_presets::presets_for(Tool::Claude)
            .find(|p| p.id == "claude-88api")
            .expect("88api preset");
        let fields = DialogFields {
            name: preset.name.to_string(),
            endpoint: "https://api.88api.com".into(),
            key: "sk-test".into(),
            model: "claude-sonnet-4-5".into(),
            usage_script: String::new(),
            api_format: "anthropic".into(),
            icon: preset.icon.to_string(),
            icon_color: preset.icon_color.to_string(),
            usage_enabled: true,
        };
        let provider = build_provider(preset, &fields);
        assert!(provider.id.starts_with("claude-88api-"));
        assert_eq!(provider.name, preset.name);
        assert_eq!(provider.icon.as_deref(), Some(preset.icon));
        // settings_config 含关键字段
        let cfg = &provider.settings_config;
        assert!(cfg.get("env").is_some());
    }

    #[test]
    fn fields_from_provider_prefers_exact_model_key() {
        // Codex 的 config 同时有 `model_provider = "custom"` 与 `model = "gpt-5"`；
        // 旧实现按「包含 model」匹配会先取到 "custom"。
        let provider = ns_presets::Provider::with_id(
            "codex-custom-1".into(),
            "My Codex".into(),
            serde_json::json!({
                "auth": { "OPENAI_API_KEY": "sk-codex" },
                "config": {
                    "model_provider": "custom",
                    "model": "gpt-5",
                    "model_providers": {
                        "custom": { "base_url": "https://api.example.com", "wire_api": "chat" }
                    }
                }
            }),
        );
        let f = DialogFields::from_provider(&provider);
        assert_eq!(f.name, "My Codex");
        assert_eq!(f.model, "gpt-5");
        assert_eq!(f.endpoint, "https://api.example.com");
        assert_eq!(f.key, "sk-codex");
    }

    #[test]
    fn fields_from_provider_reads_settings() {
        let mut provider = ns_presets::Provider::with_id(
            "claude-88api-12345".into(),
            "88api".into(),
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.88api.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test",
                    "ANTHROPIC_MODEL": "claude-sonnet-4-5",
                }
            }),
        );
        provider.category = Some(ns_presets::ProviderCategory::Aggregator);
        let f = DialogFields::from_provider(&provider);
        assert_eq!(f.endpoint, "https://api.88api.com");
        assert_eq!(f.key, "sk-test");
        assert_eq!(f.model, "claude-sonnet-4-5");
    }
}
