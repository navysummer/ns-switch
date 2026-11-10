//! 多行文本编辑器（Markdown 正文用）。
//!
//! 与 [`crate::text_field::TextField`] 同源的轻量多行变体：支持硬换行
//! （Enter）、跨行光标移动（Up/Down）、按逻辑行渲染（不软换行，超宽裁剪）。
//! 文本编辑逻辑与 TextField 一致（grapheme 边界、剪贴板、选区）。

use std::ops::Range;

use gpui::{
    App, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, InspectorElementId, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    Render, SharedString, Style, TextRun, UTF16Selection, Window, actions, div, fill, font,
    prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use ns_ui::theme;

actions!(
    text_area,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Enter,
    ]
);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, Some("TextArea")),
        KeyBinding::new("delete", Delete, Some("TextArea")),
        KeyBinding::new("left", Left, Some("TextArea")),
        KeyBinding::new("right", Right, Some("TextArea")),
        KeyBinding::new("up", Up, Some("TextArea")),
        KeyBinding::new("down", Down, Some("TextArea")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextArea")),
        KeyBinding::new("shift-right", SelectRight, Some("TextArea")),
        KeyBinding::new("shift-up", SelectUp, Some("TextArea")),
        KeyBinding::new("shift-down", SelectDown, Some("TextArea")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextArea")),
        KeyBinding::new("ctrl-a", SelectAll, Some("TextArea")),
        KeyBinding::new("home", Home, Some("TextArea")),
        KeyBinding::new("end", End, Some("TextArea")),
        KeyBinding::new("enter", Enter, Some("TextArea")),
        KeyBinding::new("cmd-v", Paste, Some("TextArea")),
        KeyBinding::new("ctrl-v", Paste, Some("TextArea")),
        KeyBinding::new("cmd-c", Copy, Some("TextArea")),
        KeyBinding::new("ctrl-c", Copy, Some("TextArea")),
        KeyBinding::new("cmd-x", Cut, Some("TextArea")),
        KeyBinding::new("ctrl-x", Cut, Some("TextArea")),
    ]
}

pub struct TextArea {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_bounds: Option<gpui::Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextArea {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,

            last_bounds: None,
            is_selecting: false,
        }
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }

    /// 设置文本并重置选区。
    pub fn set_text(&mut self, text: &str) {
        self.content = text.to_string().into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
    }

    // ---- 光标/选区移动 ----

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        let cur = self.cursor_offset();
        let (row, col) = self.row_col_of(cur);
        if row > 0 {
            let target = self.offset_of(row - 1, col);
            self.move_to(target, cx);
        } else {
            self.move_to(0, cx);
        }
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        let cur = self.cursor_offset();
        let (row, col) = self.row_col_of(cur);
        if row + 1 < self.logical_rows() {
            let target = self.offset_of(row + 1, col);
            self.move_to(target, cx);
        } else {
            self.move_to(self.content.len(), cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let cur = self.cursor_offset();
        let (row, col) = self.row_col_of(cur);
        if row > 0 {
            self.select_to(self.offset_of(row - 1, col), cx);
        } else {
            self.select_to(0, cx);
        }
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let cur = self.cursor_offset();
        let (row, col) = self.row_col_of(cur);
        if row + 1 < self.logical_rows() {
            self.select_to(self.offset_of(row + 1, col), cx);
        } else {
            self.select_to(self.content.len(), cx);
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let cur = self.cursor_offset();
        let (row, _) = self.row_col_of(cur);
        self.move_to(self.line_start_of(row), cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let cur = self.cursor_offset();
        let (row, _) = self.row_col_of(cur);
        self.move_to(self.line_end_of(row), cx);
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    // ---- 鼠标 ----

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        window.focus(&self.focus_handle);
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    // ---- 逻辑行 ----

    /// 逻辑行数（以 `\n` 分割；空内容为 1 行）。
    fn logical_rows(&self) -> usize {
        self.content.split('\n').count().max(1)
    }

    /// 内容偏移 → (行号, 列内的字符索引)。
    fn row_col_of(&self, offset: usize) -> (usize, usize) {
        let content = &self.content;
        let clamped = offset.min(content.len());
        let before = &content[..clamped];
        let row = before.bytes().filter(|&b| b == b'\n').count();
        let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = content[line_start..clamped].chars().count();
        (row, col)
    }

    /// 行号 → 该行起始字节偏移。
    fn line_start_of(&self, row: usize) -> usize {
        let mut current = 0;
        for (i, part) in self.content.split('\n').enumerate() {
            if i == row {
                return current;
            }
            current += part.len() + 1;
        }
        self.content.len()
    }

    /// 行号 → 该行结束字节偏移（不含 `\n`）。
    fn line_end_of(&self, row: usize) -> usize {
        let mut current = 0;
        for (i, part) in self.content.split('\n').enumerate() {
            let end = current + part.len();
            if i == row {
                return end;
            }
            current = end + 1;
        }
        self.content.len()
    }

    /// (行号, 列字符索引) → 内容字节偏移。
    fn offset_of(&self, row: usize, col: usize) -> usize {
        let start = self.line_start_of(row);
        let end = self.line_end_of(row);
        let line = &self.content[start..end];
        let mut count = 0;
        for (byte_idx, _) in line.char_indices() {
            if count == col {
                return start + byte_idx;
            }
            count += 1;
        }
        end
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let Some(bounds) = self.last_bounds.as_ref() else {
            return self.content.len();
        };
        // 按 y 确定逻辑行。
        let line_height = 20.0_f32;
        let y = position.y - bounds.top();
        let row = ((f32::from(y) / line_height).floor() as i64).max(0) as usize;
        let row = row.min(self.logical_rows().saturating_sub(1));
        let start = self.line_start_of(row);
        let end = self.line_end_of(row);
        let line = &self.content[start..end];
        if line.is_empty() {
            return start;
        }
        if position.x <= bounds.left() {
            return start;
        }
        if position.x >= bounds.right() {
            return end;
        }
        // 近似：按字符宽度均分（够用于点击定位）。
        let width = f32::from(bounds.right() - bounds.left()).max(1.0);
        let char_count = line.chars().count().max(1);
        let idx = ((f32::from(position.x - bounds.left()) / width) * char_count as f32) as usize;
        let mut count = 0;
        for (byte_idx, _) in line.char_indices() {
            if count == idx {
                return start + byte_idx;
            }
            count += 1;
        }
        end
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

impl EntityInputHandler for TextArea {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        // 多行：保留 `\n`。
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.selection_reversed = false;
        self.marked_range.take();
        // 普通 ASCII 键入走此路径；不 notify 可能不触发重绘。
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .map(|range| range.start + range.start..range.end + range.start)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        bounds: gpui::Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::Bounds<Pixels>> {
        // 近似：返回整行 bounds（多行组件用行定位）。
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.index_for_mouse_position(point);
        Some(self.offset_to_utf16(offset))
    }
}

struct TextAreaElement {
    input: Entity<TextArea>,
}

struct PrepaintState {
    /// 每逻辑行：字节范围 + shaped line。
    lines: Vec<(Range<usize>, gpui::ShapedLine)>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextAreaElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextAreaElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = px(160.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let placeholder = input.placeholder.clone();

        let (display_text, text_color) = if content.is_empty() {
            (placeholder, theme::text_faint())
        } else {
            (content, theme::text())
        };

        let font = font(theme::UI_FONT);
        let font_size = px(13.5);
        let line_height = px(20.0);

        // 逐逻辑行 shape（超宽裁剪）。
        let mut lines = Vec::new();
        let mut line_start = 0;
        for part in display_text.split('\n') {
            let line_run = TextRun {
                len: part.len(),
                font: font.clone(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window
                .text_system()
                .shape_line(part.to_string().into(), font_size, &[line_run], None);
            let end = line_start + part.len();
            lines.push((line_start..end, line));
            line_start = end + 1;
        }
        if lines.is_empty() {
            let line = window
                .text_system()
                .shape_line("".into(), font_size, &[TextRun {
                    len: 0,
                    font: font.clone(),
                    color: text_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }], None);
            lines.push((0..0, line));
        }

        // 光标/选区位置（按逻辑行计算 y）。
        let (row, _) = input.row_col_of(cursor);
        let cursor_y = bounds.top() + px(row as f32 * 20.0);
        let cursor_x: Pixels = lines
            .get(row)
            .map(|(range, line)| {
                let within = cursor.saturating_sub(range.start).min(range.len());
                line.x_for_index(within.min(line.text.len()))
            })
            .unwrap_or(Pixels::ZERO);

        let (selection, cursor_quad) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    gpui::Bounds::new(
                        Point::new(bounds.left() + cursor_x, cursor_y),
                        size(px(1.5), line_height),
                    ),
                    theme::color(theme::ACCENT_3),
                )),
            )
        } else {
            (
                Some(fill(
                    gpui::Bounds::from_corners(
                        Point::new(bounds.left(), cursor_y),
                        Point::new(bounds.right(), cursor_y + line_height),
                    ),
                    theme::accent_a(0.22),
                )),
                None,
            )
        };

        PrepaintState {
            lines,
            cursor: cursor_quad,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line_height = px(20.0);
        for (i, (_range, line)) in prepaint.lines.iter().enumerate() {
            let origin = Point::new(bounds.left(), bounds.top() + px(i as f32 * 20.0));
            // 裁剪超宽行。
            line.paint(origin, line_height, window, cx).ok();
        }

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_bounds = Some(bounds);
        });
    }
}

impl Focusable for TextArea {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextArea {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        div()
            .h(px(160.))
            .w_full()
            .px(px(11.))
            .py(px(8.))
            .rounded(px(10.))
            .bg(theme::alpha(0xffebc8, 0.05))
            .border_1()
            .border_color(if focused {
                theme::accent_a(0.75)
            } else {
                theme::stroke_a(0.16)
            })
            .key_context("TextArea")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .text_color(theme::text())
            .font_family(theme::UI_FONT)
            .text_size(px(13.5))
            .child(TextAreaElement { input: cx.entity() })
    }
}
