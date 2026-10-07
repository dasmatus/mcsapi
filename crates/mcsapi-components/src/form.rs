//! Form controls: input, textarea, checkbox, switch, radio group, slider, select.

use std::ops::RangeInclusive;

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Response, RichText, Sense, Stroke, StrokeKind,
    TextEdit, Ui, Vec2, Widget, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::{IntoChanged as _, Tokens, paint_focus_ring};

fn text_frame(tokens: &Tokens) -> Frame {
    Frame::new()
        .fill(tokens.field)
        .stroke(tokens.border_stroke())
        .corner_radius(tokens.control_radius())
        .inner_margin(Margin::symmetric(12, 8))
}

fn add_text_edit(ui: &mut Ui, edit: TextEdit<'_>, tokens: &Tokens) -> Response {
    let response = ui.add(
        edit.frame(text_frame(tokens))
            .text_color(tokens.foreground)
            .font(tokens.body_font()),
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect,
            tokens.control_radius(),
            tokens.ring_stroke(),
            StrokeKind::Outside,
        );
    }
    response
}

/// shadcn's Input: a single-line text field.
#[must_use = "add it with `ui.add(input)`"]
pub struct Input<'a> {
    text: &'a mut String,
    placeholder: Option<String>,
    password: bool,
    width: Option<f32>,
}

impl<'a> Input<'a> {
    /// A text field editing `text`.
    pub fn new(text: &'a mut String) -> Self {
        Self {
            text,
            placeholder: None,
            password: false,
            width: None,
        }
    }

    /// Sets the hint shown while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Masks the text, like `type="password"`.
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    /// Sets the width; the default is the available width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

impl Widget for Input<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let mut edit = TextEdit::singleline(self.text)
            .password(self.password)
            .desired_width(self.width.unwrap_or(f32::INFINITY));
        if let Some(placeholder) = self.placeholder {
            edit = edit.hint_text(RichText::new(placeholder).color(tokens.muted_foreground));
        }
        add_text_edit(ui, edit, &tokens)
    }
}

/// shadcn's Textarea: a multi-line text field.
#[must_use = "add it with `ui.add(textarea)`"]
pub struct Textarea<'a> {
    text: &'a mut String,
    placeholder: Option<String>,
    rows: usize,
}

impl<'a> Textarea<'a> {
    /// A text area editing `text`, three rows high.
    pub fn new(text: &'a mut String) -> Self {
        Self {
            text,
            placeholder: None,
            rows: 3,
        }
    }

    /// Sets the hint shown while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Sets the minimum height in rows.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows;
        self
    }
}

impl Widget for Textarea<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let mut edit = TextEdit::multiline(self.text)
            .desired_rows(self.rows)
            .desired_width(f32::INFINITY);
        if let Some(placeholder) = self.placeholder {
            edit = edit.hint_text(RichText::new(placeholder).color(tokens.muted_foreground));
        }
        add_text_edit(ui, edit, &tokens)
    }
}

/// Lays out an optional text label right of a `control_size` control and
/// returns the control's rect and the shared response.
fn labeled_control(ui: &mut Ui, control_size: Vec2, label: Option<&str>) -> (egui::Rect, Response) {
    let tokens = Tokens::current(ui.ctx());
    let galley = label.map(|text| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), tokens.body_font(), Color32::PLACEHOLDER)
    });
    let gap = 8.0;
    let width = control_size.x + galley.as_ref().map_or(0.0, |g| gap + g.size().x);
    let height = control_size
        .y
        .max(galley.as_ref().map_or(0.0, |g| g.size().y));
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let control = Align2::LEFT_CENTER.anchor_size(rect.left_center(), control_size);
    if let Some(galley) = galley {
        let pos = pos2(
            control.right() + gap,
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(pos, galley, tokens.foreground);
    }
    (control, response)
}

/// shadcn's Checkbox.
#[must_use = "add it with `ui.add(checkbox)`"]
pub struct Checkbox<'a> {
    checked: &'a mut bool,
    label: Option<String>,
}

impl<'a> Checkbox<'a> {
    /// A checkbox bound to `checked`, with no label.
    pub fn new(checked: &'a mut bool) -> Self {
        Self {
            checked,
            label: None,
        }
    }

    /// Sets the text right of the box; clicking it toggles the box too.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Widget for Checkbox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (control, mut response) = labeled_control(ui, Vec2::splat(16.0), self.label.as_deref());
        if response.clicked() {
            *self.checked = !*self.checked;
            response.mark_changed();
        }
        let checked = *self.checked;
        let label = self.label.unwrap_or_default();
        response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, checked, &label));
        let painter = ui.painter();
        let radius = CornerRadius::same(4);
        if checked {
            painter.rect_filled(control, radius, tokens.primary);
            let c = control.center();
            painter.line(
                vec![
                    c + vec2(-4.0, 0.0),
                    c + vec2(-1.0, 3.0),
                    c + vec2(4.5, -3.5),
                ],
                Stroke::new(2.0, tokens.primary_foreground),
            );
        } else {
            painter.rect_stroke(
                control,
                radius,
                Stroke::new(1.0, tokens.muted_foreground),
                StrokeKind::Inside,
            );
        }
        paint_focus_ring(ui, &response, control, radius);
        response
    }
}

/// shadcn's Switch: an animated on/off track.
#[must_use = "add it with `ui.add(switch)`"]
pub struct Switch<'a> {
    on: &'a mut bool,
    label: Option<String>,
}

impl<'a> Switch<'a> {
    /// A switch bound to `on`, with no label.
    pub fn new(on: &'a mut bool) -> Self {
        Self { on, label: None }
    }

    /// Sets the text right of the switch.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Widget for Switch<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (track, mut response) = labeled_control(ui, vec2(32.0, 18.0), self.label.as_deref());
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        let on = *self.on;
        let label = self.label.unwrap_or_default();
        response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, on, &label));
        let t = ui.ctx().animate_bool_responsive(response.id, on);
        let painter = ui.painter();
        let radius = CornerRadius::same(u8::MAX);
        // Off is a dark, bordered pill with a dim knob, like the web
        // interface's; on fills with the accent and the border fades out.
        painter.rect_filled(track, radius, tokens.field.lerp_to_gamma(tokens.primary, t));
        painter.rect_stroke(
            track,
            radius,
            Stroke::new(1.0, tokens.border.gamma_multiply(1.0 - t)),
            StrokeKind::Inside,
        );
        let knob_radius = track.height() / 2.0 - 3.0;
        let x = egui::lerp(
            (track.left() + knob_radius + 3.0)..=(track.right() - knob_radius - 3.0),
            t,
        );
        let knob = tokens
            .muted_foreground
            .lerp_to_gamma(tokens.primary_foreground, t);
        painter.circle_filled(pos2(x, track.center().y), knob_radius, knob);
        paint_focus_ring(ui, &response, track, radius);
        response
    }
}

/// shadcn's RadioGroup: a vertical list where exactly one option is selected.
#[must_use = "add it with `ui.add(group)`"]
pub struct RadioGroup<'a, T: AsRef<str>> {
    selected: &'a mut usize,
    options: &'a [T],
}

impl<'a, T: AsRef<str>> RadioGroup<'a, T> {
    /// A group of `options` with the selected index in `selected`.
    pub fn new(selected: &'a mut usize, options: &'a [T]) -> Self {
        Self { selected, options }
    }
}

impl<T: AsRef<str>> Widget for RadioGroup<'_, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            let mut changed = false;
            for (index, option) in self.options.iter().enumerate() {
                let text = option.as_ref();
                let (control, response) = labeled_control(ui, Vec2::splat(16.0), Some(text));
                if response.clicked() && *self.selected != index {
                    *self.selected = index;
                    changed = true;
                }
                let on = *self.selected == index;
                response
                    .widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, on, text));
                let painter = ui.painter();
                painter.circle_stroke(control.center(), 7.5, Stroke::new(1.0, tokens.primary));
                if on {
                    painter.circle_filled(control.center(), 4.0, tokens.primary);
                }
                paint_focus_ring(ui, &response, control, CornerRadius::same(u8::MAX));
            }
            changed
        })
        .into_changed()
    }
}

/// shadcn's Slider: a draggable thumb on a track.
#[must_use = "add it with `ui.add(slider)`"]
pub struct Slider<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    step: Option<f32>,
    width: Option<f32>,
}

impl<'a> Slider<'a> {
    /// A slider editing `value` within `range`.
    pub fn new(value: &'a mut f32, range: RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            step: None,
            width: None,
        }
    }

    /// Snaps the value to multiples of `step` from the range start.
    pub fn step(mut self, step: f32) -> Self {
        self.step = (step > 0.0).then_some(step);
        self
    }

    /// Sets the width; the default is the available width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

impl Widget for Slider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (start, end) = (*self.range.start(), *self.range.end());
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, 20.0), Sense::click_and_drag());
        let thumb_radius = 8.0;
        let track_x = (rect.left() + thumb_radius)..=(rect.right() - thumb_radius);

        let mut new_value = *self.value;
        if let Some(pointer) = response.interact_pointer_pos() {
            let t = egui::remap_clamp(pointer.x, track_x.clone(), 0.0..=1.0);
            new_value = egui::lerp(start..=end, t);
        }
        if response.has_focus() {
            let step = self.step.unwrap_or((end - start) / 100.0);
            ui.input(|input| {
                if input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::ArrowUp)
                {
                    new_value += step;
                }
                if input.key_pressed(egui::Key::ArrowLeft)
                    || input.key_pressed(egui::Key::ArrowDown)
                {
                    new_value -= step;
                }
            });
        }
        if let Some(step) = self.step {
            new_value = start + ((new_value - start) / step).round() * step;
        }
        new_value = new_value.clamp(start.min(end), start.max(end));
        if new_value != *self.value {
            *self.value = new_value;
            response.mark_changed();
        }
        let value = *self.value;
        response.widget_info(|| WidgetInfo::slider(true, f64::from(value), ""));

        let t = if end == start {
            0.0
        } else {
            (value - start) / (end - start)
        };
        let painter = ui.painter();
        let track = egui::Rect::from_x_y_ranges(
            rect.x_range(),
            (rect.center().y - 3.0)..=(rect.center().y + 3.0),
        );
        let radius = CornerRadius::same(u8::MAX);
        painter.rect_filled(track, radius, tokens.muted);
        let x = egui::lerp(track_x, t);
        let mut range = track;
        range.set_right(x);
        painter.rect_filled(range, radius, tokens.primary);
        let center = pos2(x, rect.center().y);
        painter.circle_filled(center, thumb_radius, tokens.background);
        painter.circle_stroke(center, thumb_radius, Stroke::new(1.5, tokens.primary));
        if response.has_focus() {
            painter.circle_stroke(
                center,
                thumb_radius + 3.0,
                Stroke::new(2.0, tokens.ring.gamma_multiply(0.5)),
            );
        }
        response
    }
}

/// shadcn's Select: a button that opens a list of options.
#[must_use = "add it with `ui.add(select)`"]
pub struct Select<'a, T: AsRef<str>> {
    id_salt: egui::Id,
    selected: &'a mut Option<usize>,
    options: &'a [T],
    placeholder: String,
    width: f32,
}

impl<'a, T: AsRef<str>> Select<'a, T> {
    /// A select over `options`, with the chosen index in `selected`.
    ///
    /// `id_salt` must be unique among selects in the same `Ui`.
    pub fn new(
        id_salt: impl egui::AsId,
        selected: &'a mut Option<usize>,
        options: &'a [T],
    ) -> Self {
        Self {
            id_salt: egui::Id::new(id_salt),
            selected,
            options,
            placeholder: "Select…".to_owned(),
            width: 180.0,
        }
    }

    /// Sets the text shown while nothing is selected.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets the width of the trigger and the list.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

impl<T: AsRef<str>> Widget for Select<'_, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let current = self.selected.and_then(|index| self.options.get(index));
        let (text, color) = match current {
            Some(option) => (option.as_ref().to_owned(), tokens.foreground),
            None => (self.placeholder.clone(), tokens.muted_foreground),
        };
        let mut changed = false;
        let mut response = ui
            .scope(|ui| {
                let visuals = &mut ui.style_mut().visuals;
                for state in [
                    &mut visuals.widgets.inactive,
                    &mut visuals.widgets.hovered,
                    &mut visuals.widgets.active,
                    &mut visuals.widgets.open,
                ] {
                    state.weak_bg_fill = Color32::TRANSPARENT;
                    state.bg_stroke = tokens.border_stroke();
                    state.corner_radius = tokens.control_radius();
                    state.fg_stroke = Stroke::new(1.0, tokens.foreground);
                }
                visuals.window_fill = tokens.card;
                visuals.window_stroke = tokens.border_stroke();
                visuals.selection.bg_fill = tokens.hover;
                visuals.selection.stroke = Stroke::new(1.0, tokens.foreground);
                egui::ComboBox::from_id_salt(self.id_salt)
                    .width(self.width)
                    .selected_text(RichText::new(text).color(color))
                    .show_ui(ui, |ui| {
                        for (index, option) in self.options.iter().enumerate() {
                            let on = *self.selected == Some(index);
                            if ui.selectable_label(on, option.as_ref()).clicked() && !on {
                                *self.selected = Some(index);
                                changed = true;
                            }
                        }
                    })
                    .response
            })
            .inner;
        if changed {
            response.mark_changed();
        }
        response
    }
}
