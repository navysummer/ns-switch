//! 通用 GPUI 组件：品牌徽标、按钮、胶囊徽章、状态点（移植自 ns-embed-ide widgets.rs）。

use gpui::{
    Div, ElementId, FontWeight, Hsla, SharedString, Stateful, div, prelude::*, px, rgb,
};

use crate::theme;

pub fn brand_mark(size: f32) -> Div {
    div()
        .size(px(size))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::gold_gradient())
        .border_1()
        .border_color(theme::alpha(0xffe6b8, 0.55))
        .text_color(theme::dark_text())
        .font_family(theme::SERIF_FONT)
        .font_weight(FontWeight::BOLD)
        .text_size(px(size * 0.42))
        .child("切")
}

pub fn primary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    let label: SharedString = label.into();
    div()
        .id(id)
        .h(px(34.))
        .px(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .bg(theme::gold_gradient())
        .border_1()
        .border_color(theme::alpha(0xffe6b8, 0.35))
        .text_color(theme::dark_text())
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(13.))
        .cursor_pointer()
        .hover(|style| style.opacity(0.92))
        .active(|style| style.opacity(0.85))
        .child(label)
}

pub fn danger_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    let label: SharedString = label.into();
    div()
        .id(id)
        .h(px(34.))
        .px(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .bg(theme::danger())
        .border_1()
        .border_color(theme::danger_a(0.55))
        .text_color(rgb(0xffffff))
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(13.))
        .cursor_pointer()
        .hover(|style| style.opacity(0.92))
        .active(|style| style.opacity(0.85))
        .child(label)
}

pub fn ghost_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    let label: SharedString = label.into();
    div()
        .id(id)
        .h(px(34.))
        .px(px(13.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(10.))
        .bg(theme::surface_a(0.55))
        .border_1()
        .border_color(theme::stroke_a(0.14))
        .text_color(theme::text_dim())
        .text_size(px(12.5))
        .cursor_pointer()
        .hover(|style| {
            style
                .text_color(theme::text())
                .border_color(theme::stroke_a(0.34))
                .bg(theme::stroke_a(0.08))
        })
        .child(label)
}

pub fn pill(label: impl Into<SharedString>, tone: Hsla) -> Div {
    let label: SharedString = label.into();
    div()
        .h(px(22.))
        .px(px(9.))
        .flex()
        .items_center()
        .rounded_full()
        .bg(tone.opacity(0.14))
        .border_1()
        .border_color(tone.opacity(0.35))
        .text_color(tone)
        .text_size(px(11.))
        .child(label)
}

pub fn section_label(label: impl Into<SharedString>) -> Div {
    let label: SharedString = label.into();
    div()
        .text_color(theme::text_faint())
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .child(label)
}

pub fn status_dot(color: Hsla, size: f32) -> Div {
    div()
        .size(px(size))
        .rounded_full()
        .bg(color)
        .shadow(vec![gpui::BoxShadow {
            color: color.opacity(0.55),
            offset: gpui::point(px(0.), px(0.)),
            blur_radius: px(8.),
            spread_radius: px(1.),
        }])
}
