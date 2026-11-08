//! 添加/编辑供应商对话框：预设搜索 + 关键字段表单 + 配置预览。

use gpui::{Context, Div, div, prelude::*, px, rgb};
use ns_presets::{Preset, Provider, ProviderMeta, search_presets};
use ns_ui::{theme, widgets};

/// 对话框字段输入。
#[derive(Debug, Clone, Default)]
pub struct DialogFields {
    pub endpoint: String,
    pub key: String,
    pub model: String,
    pub usage_script: String,

}

impl DialogFields {
    /// 从预设构建字段（endpoint 留空由预设/用户填）。
    pub fn from_preset(preset: &Preset) -> Self {
        let mut f = Self::default();
        f.model = preset_default_model(preset);
        f
    }

    /// 从已有供应商读取可编辑字段（编辑对话框预填）。
    /// 扫描 `settings_config` 顶层与嵌套对象（`env`/`auth`/`config`），
    /// 按关键字启发式恢复 endpoint/key/model。
    pub fn from_provider(p: &Provider) -> Self {
        fn find(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
            match value {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        if let Some(s) = v.as_str() {
                            if !s.is_empty()
                                && keys.iter().any(|k2| {
                                    k.to_lowercase().contains(&k2.to_lowercase())
                                })
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
        let cfg = &p.settings_config;
        Self {
            endpoint: find(cfg, &["endpoint", "base_url", "baseUrl", "url", "BASE_URL"]).unwrap_or_default(),
            key: find(
                cfg,
                &[
                    "api_key", "apiKey", "key", "token", "auth_token", "AUTH_TOKEN", "API_KEY",
                    "api_token",
                ],
            )
            .unwrap_or_default(),
            model: find(cfg, &["model", "default_model", "defaultModel", "MODEL"]).unwrap_or_default(),
            usage_script: p
                .meta
                .as_ref()
                .and_then(|m| m.usage_script.clone())
                .unwrap_or_default(),
        }
    }
}
/// 对话框输入实体（真实可编辑文本域）。
pub struct DialogInputs {
    pub search: gpui::Entity<crate::text_field::TextField>,
    pub endpoint: gpui::Entity<crate::text_field::TextField>,
    pub key: gpui::Entity<crate::text_field::TextField>,
    pub model: gpui::Entity<crate::text_field::TextField>,
    pub usage_script: gpui::Entity<crate::text_field::TextField>,
}

impl DialogInputs {
    pub fn new(
        cx: &mut gpui::Context<crate::app::SwitchApp>,
    ) -> Self {
        Self {
            search: cx.new(|cx| crate::text_field::TextField::new("搜索预设…", cx)),
            endpoint: cx.new(|cx| crate::text_field::TextField::new("https://…", cx)),
            key: cx.new(|cx| crate::text_field::TextField::new("sk-…", cx)),
            model: cx.new(|cx| crate::text_field::TextField::new("模型名", cx)),
            usage_script: cx.new(|cx| crate::text_field::TextField::new("/path/to/usage.js", cx)),
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
                .w(px(560.))
                .max_h(px(620.))
                .rounded(px(14.))
                .bg(theme::color(theme::PANEL_SOLID))
                .border_1()
                .border_color(theme::alpha(theme::STROKE, 0.25))
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

fn dialog_header(
    tool: ns_presets::Tool,
    editing: bool,
    _cx: &mut Context<crate::app::SwitchApp>,
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
        .border_color(theme::alpha(theme::STROKE, 0.12))
        .child(
            div()
                .text_size(px(14.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme::TEXT))
                .child(title),
        )
        .child(widgets::ghost_button("dlg-close", "✕"))
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
                .border_color(theme::alpha(theme::STROKE, 0.14))
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
        .hover(|s| s.bg(theme::alpha(theme::STROKE, 0.08)))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.dialog_preset = Some(pid.clone());
            this.dialog_fields = DialogFields::from_preset(preset);
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
                .text_color(rgb(theme::DARK_TEXT))
                .text_size(px(11.))
                .child(preset.name.chars().next().unwrap_or('?').to_string()),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(12.))
                .text_color(rgb(theme::TEXT))
                .child(preset.name),
        )
        .child(
            div()
                .text_size(px(9.5))
                .text_color(rgb(theme::TEXT_FAINT))
                .child(preset.aliases.first().copied().unwrap_or("")),
        )
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
    let mut live_fields = fields.clone();
    live_fields.endpoint = endpoint.clone();
    live_fields.key = key.clone();
    live_fields.model = model.clone();
    live_fields.usage_script = inputs.usage_script.read(cx).text();
    let save_label = if editing { "保存修改" } else { "添加并保存" };

    div()
        .flex_1()
        .flex()
        .gap(px(0.))
        .child(
            div()
                .w(px(300.))
                .p(px(16.))
                .flex_col()
                .gap(px(10.))
                .child(field_input("接口地址", &inputs.endpoint, cx))
                .child(field_input("API 密钥", &inputs.key, cx))

                .child(field_input("模型", &inputs.model, cx))
                .child(field_input("用量脚本路径", &inputs.usage_script, cx)),
            )
        .child(
            div()
                .flex_1()
                .border_l_1()
                .border_color(theme::alpha(theme::STROKE, 0.12))
                .p(px(16.))
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(theme::TEXT_FAINT))
                        .child("配置预览"),
                )
                .child(
                    div()
                        .font_family(theme::MONO_FONT)
                        .text_size(px(10.))
                        .text_color(rgb(theme::TEXT_DIM))
                        .child(preview_text(preset, &live_fields)),
                ),
        )
        .child(
            div()
                .px(px(16.))
                .py(px(12.))
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(widgets::ghost_button("dlg-cancel", "取消"))
                .child(
                    widgets::primary_button("dlg-save", save_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let save_fields = live_fields.clone();
                            crate::dialog::save_provider(this, preset, &save_fields, cx);
                        })),
                ),
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
                .text_color(rgb(theme::TEXT_FAINT))
                .child(label),
        )
        .child(
            div()
                .h(px(32.))
                .px(px(10.))
                .rounded(px(8.))
                .bg(theme::alpha(0x000000, 0.25))
                .border_1()
                .border_color(theme::alpha(theme::STROKE, 0.14))
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
        name: preset.name.to_string(),
        settings_config: (preset.build)(&fields.endpoint, &fields.key, &fields.model),
        website_url: preset.website_url.map(str::to_string),
        category: Some(preset.category),
        created_at: Some(crate::app::now_ts()),
        sort_index: None,
        notes: None,
        meta: {
            let has_fmt = preset.api_format != "anthropic";
            let routes = preset.desktop_routes.map(|(s, o, h)| {
                vec![s.to_string(), o.to_string(), h.to_string()]
            });
            if !has_fmt && routes.is_none() {
                None
            } else {
                Some(ProviderMeta {
                    api_format: has_fmt.then(|| preset.api_format.to_string()),
                    models: routes,
                    ..Default::default()
                })
            }
        },
        icon: Some(preset.icon.to_string()),
        icon_color: Some(preset.icon_color.to_string()),
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
    let editing = app.edit_id.clone();
    let mut provider = build_provider(preset, fields);
    if !fields.usage_script.is_empty() {
        let mut meta = provider.meta.clone().unwrap_or_default();
        meta.usage_script = Some(fields.usage_script.clone());
        provider.meta = Some(meta);
    }
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
            // 合并用量脚本路径（保留原 meta 其他字段）。
            if !fields.usage_script.is_empty() {
                let mut meta = updated.meta.take().unwrap_or_default();
                meta.usage_script = Some(fields.usage_script.clone());
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
            endpoint: "https://api.88api.com".into(),
            key: "sk-test".into(),
            model: "claude-sonnet-4-5".into(),
            usage_script: String::new(),
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
