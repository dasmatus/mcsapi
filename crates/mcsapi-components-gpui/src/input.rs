//! The editable text behind [`Input`](crate::Input) and [`Textarea`](crate::Textarea).
//!
//! Adapted from GPUI's `input` example (Copyright 2022 - 2025 Zed Industries,
//! Inc., Apache-2.0), extended with multiple lines, masking, and theme colors.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div,
    fill, point, prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::Tokens;

actions!(
    mcsapi_text_input,
    [
        /// Deletes the selection or the grapheme before the cursor.
        Backspace,
        /// Deletes the selection or the grapheme after the cursor.
        Delete,
        /// Moves the cursor one grapheme left.
        Left,
        /// Moves the cursor one grapheme right.
        Right,
        /// Moves the cursor up one line.
        Up,
        /// Moves the cursor down one line.
        Down,
        /// Extends the selection one grapheme left.
        SelectLeft,
        /// Extends the selection one grapheme right.
        SelectRight,
        /// Selects all text.
        SelectAll,
        /// Moves the cursor to the start of the line.
        Home,
        /// Moves the cursor to the end of the line.
        End,
        /// Inserts a line break in a multi-line field.
        Newline,
        /// Pastes from the clipboard.
        Paste,
        /// Cuts the selection to the clipboard.
        Cut,
        /// Copies the selection to the clipboard.
        Copy,
    ]
);

const CONTEXT: &str = "McsapiTextInput";

/// Binds the usual editing keys for text fields. Call once at startup.
pub fn bind_text_input_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("secondary-a", SelectAll, context),
        KeyBinding::new("secondary-v", Paste, context),
        KeyBinding::new("secondary-c", Copy, context),
        KeyBinding::new("secondary-x", Cut, context),
    ]);
}

/// Editable text with a cursor, selection, clipboard, and IME support.
///
/// Create one per field with `cx.new(|cx| TextInput::new(cx))` and draw it with
/// [`Input`](crate::Input) or [`Textarea`](crate::Textarea). Read the text
/// with [`TextInput::text`]; observe the entity to hear about edits.
pub struct TextInput {
    focus_handle: FocusHandle,
    content: String,
    placeholder: SharedString,
    masked: bool,
    multiline: bool,
    disabled: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    lines: Vec<(usize, ShapedLine)>,
    last_bounds: Option<Bounds<Pixels>>,
    line_height: Pixels,
    is_selecting: bool,
}

impl TextInput {
    /// An empty single-line field.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: String::new(),
            placeholder: SharedString::default(),
            masked: false,
            multiline: false,
            disabled: false,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            lines: Vec::new(),
            last_bounds: None,
            line_height: px(20.0),
            is_selecting: false,
        }
    }

    /// Sets the text shown while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Shows every character as a dot, for passwords.
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// Lets Enter insert line breaks.
    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    /// Masks or reveals the text, for a "show password" control.
    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        self.masked = masked;
        cx.notify();
    }

    /// Whether the text is shown as dots.
    pub fn is_masked(&self) -> bool {
        self.masked
    }

    /// Makes the field read-only and unfocusable while `true`.
    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    /// Whether the field is read-only.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Starts with `text` and the cursor at its end.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self
    }

    /// The current text.
    pub fn text(&self) -> &str {
        &self.content
    }

    /// Replaces the text and puts the cursor at its end.
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn vertical(&mut self, down: bool, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        let Some(line) = self.line_index(cursor) else {
            return;
        };
        let target = if down {
            line + 1
        } else if let Some(line) = line.checked_sub(1) {
            line
        } else {
            return self.move_to(0, cx);
        };
        let Some((start, shaped)) = self.lines.get(target) else {
            return self.move_to(self.content.len(), cx);
        };
        let (from_start, from_shaped) = &self.lines[line];
        let x = from_shaped.x_for_index(self.to_display(cursor) - self.to_display(*from_start));
        let index =
            self.offset_from_display(self.to_display(*start) + shaped.closest_index_for_x(x));
        self.move_to(index, cx);
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(false, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(true, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        let start = self.content[..cursor].rfind('\n').map_or(0, |i| i + 1);
        self.move_to(start, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        let end = self.content[cursor..]
            .find('\n')
            .map_or(self.content.len(), |i| cursor + i);
        self.move_to(end, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        let index = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = if self.multiline {
                text
            } else {
                text.replace('\n', " ")
            };
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() && !self.masked {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() && !self.masked {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    /// The shaped line holding content offset `offset`.
    fn line_index(&self, offset: usize) -> Option<usize> {
        let display = self.to_display(offset);
        self.lines
            .iter()
            .rposition(|(start, _)| self.to_display(*start) <= display)
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        if self.lines.is_empty() || position.y < bounds.top() {
            return 0;
        }
        let row = ((position.y - bounds.top()) / self.line_height).floor() as usize;
        let Some((start, line)) = self.lines.get(row) else {
            return self.content.len();
        };
        self.offset_from_display(
            self.to_display(*start) + line.closest_index_for_x(position.x - bounds.left()),
        )
    }

    /// The text as drawn: dots for a masked field.
    fn display_text(&self) -> String {
        if self.masked {
            "•".repeat(self.content.chars().count())
        } else {
            self.content.clone()
        }
    }

    /// Maps a content byte offset to the drawn text.
    fn to_display(&self, offset: usize) -> usize {
        if self.masked {
            self.content[..offset].chars().count() * '•'.len_utf8()
        } else {
            offset
        }
    }

    /// Maps a drawn-text byte offset back to the content.
    fn offset_from_display(&self, offset: usize) -> usize {
        if self.masked {
            let chars = offset / '•'.len_utf8();
            self.content
                .char_indices()
                .nth(chars)
                .map_or(self.content.len(), |(index, _)| index)
        } else {
            offset
        }
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

    /// Lines the field shows at least, from its text.
    pub(crate) fn line_count(&self) -> usize {
        self.content.split('\n').count()
    }

    /// Puts keyboard focus in the field, with the cursor at the end of the
    /// text, unless it is disabled.
    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.disabled {
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    pub(crate) fn move_cursor(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.move_to(offset, cx);
    }

    pub(crate) fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.content.replace_range(range.clone(), new_text);
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.selection_reversed = false;
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.content.replace_range(range.clone(), new_text);
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let row = self.line_index(range.start)?;
        let (start, line) = self.lines.get(row)?;
        let base = self.to_display(*start);
        let top = bounds.top() + self.line_height * row as f32;
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(self.to_display(range.start) - base),
                top,
            ),
            point(
                bounds.left() + line.x_for_index(self.to_display(range.end).saturating_sub(base)),
                top + self.line_height,
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let index = self.index_for_mouse_position(point);
        Some(self.offset_to_utf16(index))
    }
}

/// Paints the lines, selection, and cursor of a [`TextInput`].
struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    lines: Vec<(usize, ShapedLine)>,
    quads: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
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
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let lines = self.input.read(cx).line_count();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * lines as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let tokens = Tokens::get(cx);
        let input = self.input.read(cx);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let placeholder = input.content.is_empty();
        let text = if placeholder {
            input.placeholder.to_string()
        } else {
            input.display_text()
        };
        let color = if placeholder {
            tokens.muted_foreground
        } else {
            style.color
        };
        let marked = input
            .marked_range
            .as_ref()
            .map(|range| input.to_display(range.start)..input.to_display(range.end));

        let mut lines = Vec::new();
        let mut start = 0;
        for line_text in text.split('\n') {
            let end = start + line_text.len();
            let run = TextRun {
                len: line_text.len(),
                font: style.font(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs = match &marked {
                Some(marked) if marked.start < end && marked.end > start => {
                    let a = marked.start.max(start) - start;
                    let b = marked.end.min(end) - start;
                    [
                        TextRun {
                            len: a,
                            ..run.clone()
                        },
                        TextRun {
                            len: b - a,
                            underline: Some(UnderlineStyle {
                                color: Some(color),
                                thickness: px(1.0),
                                wavy: false,
                            }),
                            ..run.clone()
                        },
                        TextRun {
                            len: line_text.len() - b,
                            ..run
                        },
                    ]
                    .into_iter()
                    .filter(|run| run.len > 0)
                    .collect()
                }
                _ => vec![run],
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(line_text.to_owned()),
                font_size,
                &runs,
                None,
            );
            // Line starts are kept as content offsets.
            let content_start = if placeholder {
                0
            } else {
                input.offset_from_display(start)
            };
            lines.push((content_start, shaped));
            start = end + 1;
        }

        let selection = input.to_display(input.selected_range.start)
            ..input.to_display(input.selected_range.end);
        let cursor_at = input.to_display(input.cursor_offset());
        let mut quads = Vec::new();
        let mut cursor = None;
        for (row, (line_start, shaped)) in lines.iter().enumerate() {
            let line_start = if placeholder {
                0
            } else {
                input.to_display(*line_start)
            };
            let line_end = line_start + shaped.text.len();
            let top = bounds.top() + line_height * row as f32;
            if !selection.is_empty() && selection.start <= line_end && selection.end >= line_start {
                let a = selection.start.max(line_start) - line_start;
                let b = selection.end.min(line_end) - line_start;
                let mut right = bounds.left() + shaped.x_for_index(b);
                if selection.end > line_end {
                    // Show the selected line break.
                    right += px(6.0);
                }
                quads.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + shaped.x_for_index(a), top),
                        point(right, top + line_height),
                    ),
                    tokens.primary.opacity(0.35),
                ));
            }
            let on_line = cursor_at >= line_start && cursor_at <= line_end;
            if selection.is_empty() && cursor.is_none() && (placeholder || on_line) {
                let x = if placeholder {
                    px(0.0)
                } else {
                    shaped.x_for_index(cursor_at - line_start)
                };
                cursor = Some(fill(
                    Bounds::new(point(bounds.left() + x, top), size(px(1.5), line_height)),
                    tokens.foreground,
                ));
            }
        }
        PrepaintState {
            lines,
            quads,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
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
        for quad in prepaint.quads.drain(..) {
            window.paint_quad(quad);
        }
        let line_height = window.line_height();
        for (row, (_, line)) in prepaint.lines.iter().enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * row as f32);
            line.paint(origin, line_height, TextAlign::Left, None, window, cx)
                .ok();
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        let lines = std::mem::take(&mut prepaint.lines);
        let placeholder = self.input.read(cx).content.is_empty();
        self.input.update(cx, |input, _| {
            input.lines = if placeholder { Vec::new() } else { lines };
            input.last_bounds = Some(bounds);
            input.line_height = line_height;
        });
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let element = div()
            .flex()
            .w_full()
            .line_height(px(20.0))
            .text_size(px(14.0))
            .text_color(Tokens::get(cx).foreground)
            .child(TextElement { input: cx.entity() });
        if self.disabled {
            return element;
        }
        element
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
    }
}
