//! Form controls: input, textarea, checkbox, switch, radio group, slider, select.

use std::{ops::RangeInclusive, rc::Rc};

use gpui::{
    App, Bounds, ClickEvent, ElementId, Entity, FontWeight, HitboxBehavior, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, RenderOnce,
    SharedString, Styled, Window, anchored, canvas, deferred, div, fill, point, prelude::*, px,
    size,
};

use crate::{Handler, TextInput, Tokens, actions::pressable};

/// shadcn's Input: a single-line field around a [`TextInput`].
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Input {
    input: Entity<TextInput>,
    width: Option<f32>,
    rows: usize,
}

impl Input {
    /// A field showing `input`.
    pub fn new(input: &Entity<TextInput>) -> Self {
        Self {
            input: input.clone(),
            width: None,
            rows: 1,
        }
    }

    /// Sets the width in pixels; it fills its parent otherwise.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

fn text_frame(input: Input, window: &Window, cx: &App) -> gpui::Div {
    let t = Tokens::get(cx);
    let state = input.input.read(cx);
    let focused = state.is_focused(window);
    let disabled = state.is_disabled();
    let entity = input.input.clone();
    let mut frame = div()
        .flex_none()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            // The text only covers its lines; let the whole frame take focus.
            entity.update(cx, |input, cx| {
                if !input.is_focused(window) {
                    let end = input.text().len();
                    input.focus(window, cx);
                    input.move_cursor(end, cx);
                }
            });
        })
        .px(px(12.0))
        .py(px(8.0))
        .min_h(px(20.0 * input.rows as f32 + 16.0))
        .border_1()
        .border_color(if focused { t.ring } else { t.border })
        .rounded(t.radius)
        .child(input.input.clone());
    frame = match input.width {
        Some(width) => frame.w(px(width)),
        None => frame.w_full(),
    };
    if disabled {
        frame = frame.opacity(0.5);
    }
    frame
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        text_frame(self, window, cx)
    }
}

/// shadcn's Textarea: a multi-line field around a [`TextInput`] made with
/// [`TextInput::multiline`].
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Textarea(Input);

impl Textarea {
    /// A field showing `input`, three lines tall until it holds more.
    pub fn new(input: &Entity<TextInput>) -> Self {
        Self(Input {
            rows: 3,
            ..Input::new(input)
        })
    }

    /// Sets the minimum height in lines.
    pub fn rows(mut self, rows: usize) -> Self {
        self.0.rows = rows.max(1);
        self
    }

    /// Sets the width in pixels; it fills its parent otherwise.
    pub fn width(mut self, width: f32) -> Self {
        self.0.width = Some(width);
        self
    }
}

impl RenderOnce for Textarea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        text_frame(self.0, window, cx)
    }
}

/// A box and a label that toggle together, shared by checkbox and switch.
fn labelled(
    id: ElementId,
    control: impl IntoElement,
    label: Option<SharedString>,
    disabled: bool,
    on_click: Option<Handler<ClickEvent>>,
    tokens: &Tokens,
) -> gpui::Stateful<gpui::Div> {
    pressable(id, disabled, on_click, tokens)
        .gap(px(8.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_size(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(tokens.foreground)
        .child(control)
        .children(label)
}

/// shadcn's Checkbox.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Checkbox {
    id: ElementId,
    checked: bool,
    label: Option<SharedString>,
    disabled: bool,
    on_change: Option<Handler<bool>>,
}

impl Checkbox {
    /// A checkbox, currently `checked`. `id` must be unique among its siblings.
    pub fn new(id: impl Into<ElementId>, checked: bool) -> Self {
        Self {
            id: id.into(),
            checked,
            label: None,
            disabled: false,
            on_change: None,
        }
    }

    /// Sets the text beside the box; clicking it toggles too.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Dims the checkbox and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }

    /// Calls `handler` with the new state when toggled.
    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

fn toggle_handler(value: bool, handler: Option<Handler<bool>>) -> Option<Handler<ClickEvent>> {
    handler.map(|handler| {
        Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            handler(&!value, window, cx)
        }) as Handler<ClickEvent>
    })
}

impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let mut check = div()
            .flex_none()
            .size(px(16.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(if self.checked { t.primary } else { t.border })
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.0))
            .text_color(t.primary_foreground);
        if self.checked {
            check = check.bg(t.primary).child("✓");
        }
        labelled(
            self.id,
            check,
            self.label,
            self.disabled,
            toggle_handler(self.checked, self.on_change),
            &t,
        )
    }
}

/// shadcn's Switch.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Switch {
    id: ElementId,
    on: bool,
    label: Option<SharedString>,
    disabled: bool,
    on_change: Option<Handler<bool>>,
}

impl Switch {
    /// A switch, currently `on`. `id` must be unique among its siblings.
    pub fn new(id: impl Into<ElementId>, on: bool) -> Self {
        Self {
            id: id.into(),
            on,
            label: None,
            disabled: false,
            on_change: None,
        }
    }

    /// Sets the text beside the switch; clicking it toggles too.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Dims the switch and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }

    /// Calls `handler` with the new state when toggled.
    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let track = div()
            .flex_none()
            .w(px(36.0))
            .h(px(20.0))
            .p(px(2.0))
            .rounded_full()
            .bg(if self.on { t.primary } else { t.border })
            .flex()
            .when(self.on, |track| track.justify_end())
            .child(div().size(px(16.0)).rounded_full().bg(t.background));
        labelled(
            self.id,
            track,
            self.label,
            self.disabled,
            toggle_handler(self.on, self.on_change),
            &t,
        )
    }
}

/// shadcn's RadioGroup: exactly one choice from a list.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct RadioGroup {
    id: ElementId,
    options: Vec<SharedString>,
    selected: usize,
    disabled: bool,
    on_change: Option<Handler<usize>>,
}

impl RadioGroup {
    /// A group over `options` with `selected` chosen. `id` must be unique among
    /// its siblings.
    pub fn new(
        id: impl Into<ElementId>,
        options: impl IntoIterator<Item = impl Into<SharedString>>,
        selected: usize,
    ) -> Self {
        Self {
            id: id.into(),
            options: options.into_iter().map(Into::into).collect(),
            selected,
            disabled: false,
            on_change: None,
        }
    }

    /// Dims the group and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }

    /// Calls `handler` with the newly chosen index.
    pub fn on_change(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for RadioGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        div().id(self.id).flex().flex_col().gap(px(8.0)).children(
            self.options.into_iter().enumerate().map(|(index, option)| {
                let chosen = index == self.selected;
                let dot = div()
                    .flex_none()
                    .size(px(16.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if chosen { t.primary } else { t.border })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(chosen, |dot| {
                        dot.child(div().size(px(8.0)).rounded_full().bg(t.primary))
                    });
                let on_click = self.on_change.clone().map(|handler| {
                    Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        handler(&index, window, cx)
                    }) as Handler<ClickEvent>
                });
                labelled(index.into(), dot, Some(option), self.disabled, on_click, &t)
            }),
        )
    }
}

/// shadcn's Slider: a value picked from a range by clicking or dragging.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Slider {
    id: ElementId,
    value: f32,
    range: RangeInclusive<f32>,
    step: Option<f32>,
    width: f32,
    disabled: bool,
    on_change: Option<Handler<f32>>,
}

impl Slider {
    /// A slider at `value` within `range`. `id` must be unique among its siblings.
    pub fn new(id: impl Into<ElementId>, value: f32, range: RangeInclusive<f32>) -> Self {
        Self {
            id: id.into(),
            value,
            range,
            step: None,
            width: 200.0,
            disabled: false,
            on_change: None,
        }
    }

    /// Snaps values to multiples of `step` from the start of the range.
    pub fn step(mut self, step: f32) -> Self {
        self.step = Some(step);
        self
    }

    /// Sets the width in pixels (default 200).
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Dims the slider and ignores input when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }

    /// Calls `handler` with the new value while the slider is pressed or dragged.
    pub fn on_change(mut self, handler: impl Fn(&f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

const THUMB: f32 = 16.0;

/// The value under `x` for a slider drawn in `bounds`.
fn slider_value(
    x: Pixels,
    bounds: Bounds<Pixels>,
    range: &RangeInclusive<f32>,
    step: Option<f32>,
) -> f32 {
    let usable = (f32::from(bounds.size.width) - THUMB).max(1.0);
    let t = ((f32::from(x - bounds.left()) - THUMB / 2.0) / usable).clamp(0.0, 1.0);
    let value = range.start() + t * (range.end() - range.start());
    match step {
        Some(step) if step > 0.0 => (range.start()
            + ((value - range.start()) / step).round() * step)
            .clamp(*range.start(), *range.end()),
        _ => value,
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let dragging = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let span = (self.range.end() - self.range.start()).max(f32::EPSILON);
        let fraction = ((self.value - self.range.start()) / span).clamp(0.0, 1.0);
        let Self {
            range,
            step,
            on_change,
            disabled,
            ..
        } = self;
        canvas(
            move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, _| {
                // Quads ignore element opacity, so dim the colors instead.
                let dim = |color: gpui::Hsla| if disabled { color.opacity(0.5) } else { color };
                let track_h = px(6.0);
                let usable = bounds.size.width - px(THUMB);
                let track = Bounds::new(
                    point(bounds.left(), bounds.center().y - track_h / 2.0),
                    size(bounds.size.width, track_h),
                );
                window.paint_quad(fill(track, dim(t.muted)).corner_radii(track_h / 2.0));
                let thumb_x = bounds.left() + usable * fraction;
                let filled = Bounds::new(
                    track.origin,
                    size(thumb_x - bounds.left() + px(THUMB / 2.0), track_h),
                );
                window.paint_quad(fill(filled, dim(t.primary)).corner_radii(track_h / 2.0));
                let thumb = Bounds::new(
                    point(thumb_x, bounds.center().y - px(THUMB / 2.0)),
                    size(px(THUMB), px(THUMB)),
                );
                window.paint_quad(
                    fill(thumb, t.background)
                        .corner_radii(px(THUMB / 2.0))
                        .border_widths(px(2.0))
                        .border_color(dim(t.primary)),
                );
                let Some(handler) = on_change.filter(|_| !disabled) else {
                    return;
                };
                window.set_cursor_style(gpui::CursorStyle::PointingHand, &hitbox);
                {
                    let (dragging, handler, range) =
                        (dragging.clone(), handler.clone(), range.clone());
                    let hitbox = hitbox.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase.bubble()
                            && event.button == MouseButton::Left
                            && hitbox.is_hovered(window)
                        {
                            dragging.update(cx, |d, _| *d = true);
                            handler(
                                &slider_value(event.position.x, bounds, &range, step),
                                window,
                                cx,
                            );
                        }
                    });
                }
                {
                    let (dragging, handler, range) =
                        (dragging.clone(), handler.clone(), range.clone());
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase.bubble()
                            && *dragging.read(cx)
                            && event.pressed_button == Some(MouseButton::Left)
                        {
                            handler(
                                &slider_value(event.position.x, bounds, &range, step),
                                window,
                                cx,
                            );
                        }
                    });
                }
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase.bubble() && *dragging.read(cx) {
                        dragging.update(cx, |d, _| *d = false);
                    }
                });
            },
        )
        .flex_none()
        .w(px(self.width))
        .h(px(20.0))
    }
}

/// shadcn's Select: a button that opens a list of options.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Select {
    id: ElementId,
    options: Vec<SharedString>,
    selected: Option<usize>,
    placeholder: SharedString,
    width: f32,
    disabled: bool,
    on_change: Option<Handler<Option<usize>>>,
}

impl Select {
    /// A select over `options` with `selected` chosen. `id` must be unique
    /// among its siblings; it also keys whether the list is open.
    pub fn new(
        id: impl Into<ElementId>,
        options: impl IntoIterator<Item = impl Into<SharedString>>,
        selected: Option<usize>,
    ) -> Self {
        Self {
            id: id.into(),
            options: options.into_iter().map(Into::into).collect(),
            selected,
            placeholder: "Select…".into(),
            width: 180.0,
            disabled: false,
            on_change: None,
        }
    }

    /// Sets the text shown while nothing is selected.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets the width of the trigger and the list (default 180).
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Dims the select and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }

    /// Calls `handler` with the chosen index.
    pub fn on_change(
        mut self,
        handler: impl Fn(&Option<usize>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Select {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let open = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let is_open = *open.read(cx) && !self.disabled;
        let (text, color) = match self.selected.and_then(|i| self.options.get(i)) {
            Some(option) => (option.clone(), t.foreground),
            None => (self.placeholder.clone(), t.muted_foreground),
        };
        let toggle = open.clone();
        let trigger = pressable(
            "trigger".into(),
            self.disabled,
            Some(Rc::new(
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    toggle.update(cx, |open, cx| {
                        *open = !*open;
                        cx.notify();
                    });
                },
            )),
            &t,
        )
        .w(px(self.width))
        .h(px(36.0))
        .px(px(12.0))
        .justify_between()
        .border_1()
        .border_color(if is_open { t.ring } else { t.border })
        .rounded(t.radius)
        .text_size(px(14.0))
        .text_color(color)
        .child(text)
        .child(div().text_color(t.muted_foreground).child("⌄"));

        let list = is_open.then(|| {
            let close = open.clone();
            let options = self.options.iter().enumerate().map(|(index, option)| {
                let chosen = self.selected == Some(index);
                let (open, handler) = (open.clone(), self.on_change.clone());
                let hover = t.hover;
                div()
                    .id(index)
                    .flex()
                    .justify_between()
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .child(option.clone())
                    .children(chosen.then_some("✓"))
                    .on_click(move |_, window, cx| {
                        open.update(cx, |open, cx| {
                            *open = false;
                            cx.notify();
                        });
                        if let Some(handler) = &handler {
                            handler(&Some(index), window, cx);
                        }
                    })
            });
            let backdrop = crate::overlays::backdrop(window, None, move |_, cx| {
                close.update(cx, |open, cx| {
                    *open = false;
                    cx.notify();
                });
            });
            [
                deferred(backdrop).with_priority(1).into_any_element(),
                deferred(
                    anchored()
                        .position_mode(gpui::AnchoredPositionMode::Local)
                        .position(point(px(0.0), px(40.0)))
                        .snap_to_window()
                        .child(
                            div()
                                .id("list")
                                .occlude()
                                .w(px(self.width))
                                .p(px(4.0))
                                .flex()
                                .flex_col()
                                .bg(t.background)
                                .border_1()
                                .border_color(t.border)
                                .rounded(t.radius)
                                .shadow_md()
                                .text_size(px(14.0))
                                .text_color(t.foreground)
                                .children(options),
                        ),
                )
                .with_priority(2)
                .into_any_element(),
            ]
        });

        div()
            .id(self.id)
            .flex()
            .flex_col()
            .flex_none()
            .child(trigger)
            .children(list.into_iter().flatten())
    }
}
