//! Layout components: sheet, drawer, resizable, scroll area, sidebar,
//! carousel, button group, input group, field, form, and item.

use egui::{
    Align, Align2, Area, Color32, Context, CornerRadius, CursorIcon, Frame, Id, InnerResponse,
    Layout, Margin, Order, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    Vec2, Widget, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::{Button, ButtonVariant, Tokens, paint_focus_ring};

/// The edge a [`Sheet`] slides in from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// From the left edge.
    Left,
    /// From the right edge.
    #[default]
    Right,
    /// From the top edge.
    Top,
    /// From the bottom edge.
    Bottom,
}

fn slide_panel<R>(
    ctx: &Context,
    id: Id,
    open: &mut bool,
    side: Side,
    extent: f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let t = ctx.animate_bool_with_time(id.with("open"), *open, 0.3);
    if t <= 0.0 {
        return None;
    }
    let tokens = Tokens::current(ctx);
    let screen = ctx.content_rect();
    let eased = 1.0 - (1.0 - t).powi(3);
    let shown = extent * eased;
    let panel = match side {
        Side::Left => Rect::from_min_size(
            pos2(screen.left() - extent + shown, screen.top()),
            vec2(extent, screen.height()),
        ),
        Side::Right => Rect::from_min_size(
            pos2(screen.right() - shown, screen.top()),
            vec2(extent, screen.height()),
        ),
        Side::Top => Rect::from_min_size(
            pos2(screen.left(), screen.top() - extent + shown),
            vec2(screen.width(), extent),
        ),
        Side::Bottom => Rect::from_min_size(
            pos2(screen.left(), screen.bottom() - shown),
            vec2(screen.width(), extent),
        ),
    };
    let backdrop = Area::new(id.with("backdrop"))
        .order(Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, tokens.overlay.gamma_multiply(t));
            response
        })
        .inner;
    let inner = Area::new(id.with("panel"))
        .order(Order::Foreground)
        .fixed_pos(panel.min)
        .show(ctx, |ui| {
            Frame::new()
                .fill(tokens.background)
                .stroke(tokens.border_stroke())
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_min_size(panel.size() - Vec2::splat(48.0));
                    ui.set_max_size(panel.size() - Vec2::splat(48.0));
                    content(ui)
                })
                .inner
        })
        .inner;
    let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if *open && (backdrop.clicked() || escape) {
        *open = false;
    }
    Some(inner)
}

fn sheet_header(ui: &mut Ui, tokens: &Tokens, title: &str, description: Option<&str>) {
    ui.label(
        RichText::new(title)
            .size(18.0)
            .strong()
            .color(tokens.foreground),
    );
    if let Some(description) = description {
        ui.label(
            RichText::new(description)
                .font(tokens.body_font())
                .color(tokens.muted_foreground),
        );
    }
    ui.add_space(16.0);
}

/// shadcn's Sheet: a panel that slides over the screen from one edge.
///
/// Escape and clicks on the backdrop set `open` to `false`.
#[must_use = "draw it with `sheet.show(ctx, ...)`"]
pub struct Sheet<'a> {
    id: Id,
    open: &'a mut bool,
    side: Side,
    title: String,
    description: Option<String>,
    size: f32,
}

impl<'a> Sheet<'a> {
    /// A right-side sheet titled `title`, shown while `open` is true.
    pub fn new(id_salt: impl egui::AsId, open: &'a mut bool, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id_salt),
            open,
            side: Side::Right,
            title: title.into(),
            description: None,
            size: 384.0,
        }
    }

    /// Sets the edge it slides from.
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the panel's width (left/right) or height (top/bottom).
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Draws the sheet while open or animating closed.
    pub fn show<R>(self, ctx: &Context, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let tokens = Tokens::current(ctx);
        let (title, description) = (self.title, self.description);
        slide_panel(ctx, self.id, self.open, self.side, self.size, |ui| {
            sheet_header(ui, &tokens, &title, description.as_deref());
            content(ui)
        })
    }
}

/// shadcn's Drawer (Vaul): a bottom sheet with a grab handle.
#[must_use = "draw it with `drawer.show(ctx, ...)`"]
pub struct Drawer<'a> {
    id: Id,
    open: &'a mut bool,
    title: String,
    description: Option<String>,
    height: f32,
}

impl<'a> Drawer<'a> {
    /// A drawer titled `title`, shown while `open` is true.
    pub fn new(id_salt: impl egui::AsId, open: &'a mut bool, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id_salt),
            open,
            title: title.into(),
            description: None,
            height: 320.0,
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the drawer height.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Draws the drawer while open or animating closed.
    pub fn show<R>(self, ctx: &Context, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let tokens = Tokens::current(ctx);
        let (title, description) = (self.title, self.description);
        slide_panel(ctx, self.id, self.open, Side::Bottom, self.height, |ui| {
            ui.vertical_centered(|ui| {
                let (handle, _) = ui.allocate_exact_size(vec2(100.0, 8.0), Sense::hover());
                ui.painter()
                    .rect_filled(handle, CornerRadius::same(4), tokens.muted);
                ui.add_space(8.0);
                sheet_header(ui, &tokens, &title, description.as_deref());
            });
            content(ui)
        })
    }
}

/// Direction of a [`Resizable`] split.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Panels side by side.
    #[default]
    Horizontal,
    /// Panels stacked.
    Vertical,
}

/// shadcn's Resizable: two panels split by a draggable handle.
#[must_use = "draw it with `resizable.show(ui, ...)`"]
pub struct Resizable<'a> {
    ratio: &'a mut f32,
    direction: Direction,
    size: Vec2,
    min: f32,
}

impl<'a> Resizable<'a> {
    /// A side-by-side split of `size` with the first panel's share in `ratio` (0–1).
    pub fn new(ratio: &'a mut f32, size: impl Into<Vec2>) -> Self {
        Self {
            ratio,
            direction: Direction::Horizontal,
            size: size.into(),
            min: 0.1,
        }
    }

    /// Stacks the panels instead.
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Sets the smallest share either panel may shrink to (default 0.1).
    pub fn min(mut self, min: f32) -> Self {
        self.min = min.clamp(0.0, 0.5);
        self
    }

    /// Draws both panels; `first` and `second` draw their contents.
    pub fn show(
        self,
        ui: &mut Ui,
        first: impl FnOnce(&mut Ui),
        second: impl FnOnce(&mut Ui),
    ) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::hover());
        let horizontal = self.direction == Direction::Horizontal;
        let length = if horizontal {
            rect.width()
        } else {
            rect.height()
        };
        let split = |ratio: f32| {
            if horizontal {
                rect.left() + length * ratio
            } else {
                rect.top() + length * ratio
            }
        };
        let at = split(*self.ratio);
        let handle_rect = if horizontal {
            Rect::from_center_size(pos2(at, rect.center().y), vec2(8.0, rect.height()))
        } else {
            Rect::from_center_size(pos2(rect.center().x, at), vec2(rect.width(), 8.0))
        };
        let handle = ui.interact(handle_rect, response.id.with("handle"), Sense::drag());
        if handle.hovered() || handle.dragged() {
            ui.ctx().set_cursor_icon(if horizontal {
                CursorIcon::ResizeHorizontal
            } else {
                CursorIcon::ResizeVertical
            });
        }
        if let Some(pointer) = handle.interact_pointer_pos()
            && handle.dragged()
        {
            let p = if horizontal {
                pointer.x - rect.left()
            } else {
                pointer.y - rect.top()
            };
            let new = (p / length).clamp(self.min, 1.0 - self.min);
            if new != *self.ratio {
                *self.ratio = new;
                response.mark_changed();
            }
        }
        let at = split(*self.ratio);
        let (a, b) = if horizontal {
            (
                Rect::from_min_max(rect.min, pos2(at, rect.bottom())),
                Rect::from_min_max(pos2(at, rect.top()), rect.max),
            )
        } else {
            (
                Rect::from_min_max(rect.min, pos2(rect.right(), at)),
                Rect::from_min_max(pos2(rect.left(), at), rect.max),
            )
        };
        let painter = ui.painter();
        painter.rect_stroke(
            rect,
            tokens.card_radius(),
            tokens.border_stroke(),
            StrokeKind::Inside,
        );
        first(&mut ui.new_child(UiBuilder::new().max_rect(a.shrink(16.0))));
        second(&mut ui.new_child(UiBuilder::new().max_rect(b.shrink(16.0))));
        let painter = ui.painter();
        if horizontal {
            painter.vline(at, rect.y_range(), tokens.border_stroke());
        } else {
            painter.hline(rect.x_range(), at, tokens.border_stroke());
        }
        let grip = Rect::from_center_size(
            handle_rect.center(),
            if horizontal {
                vec2(12.0, 16.0)
            } else {
                vec2(16.0, 12.0)
            },
        );
        painter.rect_filled(grip, CornerRadius::same(2), tokens.border);
        response
    }
}

/// shadcn's ScrollArea: egui's scroll area with thin, rounded bars.
pub fn scroll_area<R>(
    ui: &mut Ui,
    max_height: f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let tokens = Tokens::current(ui.ctx());
    ui.scope(|ui| {
        let style = ui.style_mut();
        style.spacing.scroll = egui::style::ScrollStyle::thin();
        style.visuals.widgets.inactive.bg_fill = tokens.border;
        style.visuals.widgets.hovered.bg_fill = tokens.muted_foreground;
        style.visuals.widgets.active.bg_fill = tokens.muted_foreground;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .show(ui, content)
            .inner
    })
}

/// shadcn's Sidebar: a collapsible navigation column.
///
/// While collapsed it shrinks to an icon rail. Add rows with [`sidebar_group`]
/// and [`sidebar_item`].
#[must_use = "draw it with `sidebar.show(ui, ...)`"]
pub struct Sidebar<'a> {
    expanded: &'a mut bool,
    width: f32,
}

impl<'a> Sidebar<'a> {
    /// A 256 pt sidebar that collapses to 48 pt while `expanded` is false.
    pub fn new(expanded: &'a mut bool) -> Self {
        Self {
            expanded,
            width: 256.0,
        }
    }

    /// Draws the sidebar as a column of the available height.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui, bool) -> R,
    ) -> InnerResponse<R> {
        let tokens = Tokens::current(ui.ctx());
        let id = ui.id().with("mcsapi_sidebar");
        let t = ui.ctx().animate_bool_with_time(id, *self.expanded, 0.2);
        let width = egui::lerp(48.0..=self.width, t);
        let height = ui.available_height().max(200.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, tokens.card);
        ui.painter()
            .vline(rect.right(), rect.y_range(), tokens.border_stroke());
        let expanded = *self.expanded;
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(8.0))
                .layout(Layout::top_down(Align::Min)),
        );
        child.set_clip_rect(rect);
        let toggle = child.add(
            Button::new(if expanded { "⟨" } else { "⟩" })
                .variant(ButtonVariant::Ghost)
                .size(crate::ButtonSize::Icon),
        );
        if toggle.clicked() {
            *self.expanded = !*self.expanded;
        }
        let inner = content(&mut child, expanded && t > 0.9);
        InnerResponse::new(inner, response)
    }
}

/// A muted heading over a group of sidebar rows. Hidden while collapsed.
pub fn sidebar_group(ui: &mut Ui, expanded: bool, label: &str) {
    if expanded {
        let tokens = Tokens::current(ui.ctx());
        ui.add_space(8.0);
        ui.label(
            RichText::new(label)
                .font(tokens.small_font())
                .color(tokens.muted_foreground),
        );
    }
}

/// A sidebar row: an icon glyph and, while expanded, a label.
pub fn sidebar_item(
    ui: &mut Ui,
    expanded: bool,
    icon: &str,
    label: &str,
    active: bool,
) -> Response {
    let tokens = Tokens::current(ui.ctx());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, active, label));
    let painter = ui.painter();
    if active || response.hovered() {
        painter.rect_filled(rect, tokens.control_radius(), tokens.muted);
    }
    painter.text(
        rect.left_center() + vec2(16.0, 0.0),
        Align2::CENTER_CENTER,
        icon,
        tokens.body_font(),
        tokens.foreground,
    );
    if expanded {
        painter.text(
            rect.left_center() + vec2(36.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            tokens.body_font(),
            if active {
                tokens.foreground
            } else {
                tokens.foreground.gamma_multiply(0.85)
            },
        );
    } else {
        return crate::tooltip(response, label);
    }
    paint_focus_ring(ui, &response, rect, tokens.control_radius());
    response
}

/// shadcn's Carousel (Embla): slides with previous / next buttons.
#[must_use = "draw it with `carousel.show(ui, ...)`"]
pub struct Carousel<'a> {
    index: &'a mut usize,
    count: usize,
    size: Vec2,
}

impl<'a> Carousel<'a> {
    /// A carousel of `count` slides of `size`, showing slide `index`.
    pub fn new(index: &'a mut usize, count: usize, size: impl Into<Vec2>) -> Self {
        Self {
            index,
            count,
            size: size.into(),
        }
    }

    /// Draws the carousel; `slide` draws slide `i`.
    pub fn show(self, ui: &mut Ui, mut slide: impl FnMut(&mut Ui, usize)) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let count = self.count.max(1);
        let button = 32.0;
        let gap = 16.0;
        let (rect, mut response) =
            ui.allocate_exact_size(self.size + vec2(2.0 * (button + gap), 0.0), Sense::hover());
        let viewport = Rect::from_center_size(rect.center(), self.size);
        let x = ui
            .ctx()
            .animate_value_with_time(response.id.with("x"), *self.index as f32, 0.3);
        for i in 0..count {
            let offset = (i as f32 - x) * self.size.x;
            let slide_rect = viewport.translate(vec2(offset, 0.0));
            if !slide_rect.intersects(viewport) {
                continue;
            }
            let clip = slide_rect.intersect(viewport);
            let painter = ui.painter().with_clip_rect(clip);
            painter.rect_filled(slide_rect.shrink(4.0), tokens.card_radius(), tokens.card);
            painter.rect_stroke(
                slide_rect.shrink(4.0),
                tokens.card_radius(),
                tokens.border_stroke(),
                StrokeKind::Inside,
            );
            let mut child = ui.new_child(UiBuilder::new().max_rect(slide_rect.shrink(24.0)));
            child.set_clip_rect(clip);
            slide(&mut child, i);
        }
        for (dir, center, glyph) in [
            (
                -1_isize,
                pos2(rect.left() + button / 2.0, rect.center().y),
                "←",
            ),
            (1, pos2(rect.right() - button / 2.0, rect.center().y), "→"),
        ] {
            let enabled = if dir < 0 {
                *self.index > 0
            } else {
                *self.index + 1 < count
            };
            let b = Rect::from_center_size(center, Vec2::splat(button));
            let r = ui.interact(
                b,
                response.id.with(("nav", dir)),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            r.widget_info(|| {
                WidgetInfo::labeled(
                    WidgetType::Button,
                    enabled,
                    if dir < 0 {
                        "Previous slide"
                    } else {
                        "Next slide"
                    },
                )
            });
            if r.clicked() {
                *self.index = (*self.index as isize + dir) as usize;
                response.mark_changed();
            }
            let painter = ui.painter();
            painter.circle_stroke(center, button / 2.0, tokens.border_stroke());
            if enabled && r.hovered() {
                painter.circle_filled(center, button / 2.0 - 1.0, tokens.muted);
            }
            let color = if enabled {
                tokens.foreground
            } else {
                tokens.muted_foreground.gamma_multiply(0.5)
            };
            painter.text(
                center,
                Align2::CENTER_CENTER,
                glyph,
                tokens.body_font(),
                color,
            );
        }
        response
    }
}

/// shadcn's ButtonGroup: outline buttons joined into one bordered strip.
///
/// Returns the clicked button's index in [`InnerResponse::inner`].
pub fn button_group<T: AsRef<str>>(ui: &mut Ui, labels: &[T]) -> InnerResponse<Option<usize>> {
    let tokens = Tokens::current(ui.ctx());
    let galleys: Vec<_> = labels
        .iter()
        .map(|l| {
            ui.painter().layout_no_wrap(
                l.as_ref().to_owned(),
                tokens.body_font(),
                Color32::PLACEHOLDER,
            )
        })
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 24.0).collect();
    let (rect, response) = ui.allocate_exact_size(vec2(widths.iter().sum(), 36.0), Sense::hover());
    let mut clicked = None;
    let mut x = rect.left();
    let count = labels.len();
    for (i, (galley, width)) in galleys.into_iter().zip(widths).enumerate() {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(width, rect.height()));
        x += width;
        let r = ui.interact(cell, response.id.with(i), Sense::click());
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, labels[i].as_ref()));
        if r.clicked() {
            clicked = Some(i);
        }
        let radius = tokens.radius;
        let corners = CornerRadius {
            nw: if i == 0 { radius } else { 0 },
            sw: if i == 0 { radius } else { 0 },
            ne: if i + 1 == count { radius } else { 0 },
            se: if i + 1 == count { radius } else { 0 },
        };
        let painter = ui.painter();
        if r.hovered() {
            painter.rect_filled(cell, corners, tokens.muted);
        }
        painter.rect_stroke(cell, corners, tokens.border_stroke(), StrokeKind::Inside);
        painter.galley(
            Align2::CENTER_CENTER
                .anchor_size(cell.center(), galley.size())
                .min,
            galley,
            tokens.foreground,
        );
        paint_focus_ring(ui, &r, cell, corners);
    }
    InnerResponse::new(clicked, response)
}

/// shadcn's InputGroup: a text field with text add-ons inside its border,
/// such as `https://` before or `.com` after.
#[must_use = "add it with `ui.add(group)`"]
pub struct InputGroup<'a> {
    text: &'a mut String,
    leading: Option<String>,
    trailing: Option<String>,
    placeholder: Option<String>,
    width: f32,
}

impl<'a> InputGroup<'a> {
    /// A 280 pt input group editing `text`.
    pub fn new(text: &'a mut String) -> Self {
        Self {
            text,
            leading: None,
            trailing: None,
            placeholder: None,
            width: 280.0,
        }
    }

    /// Sets the add-on before the text.
    pub fn leading(mut self, addon: impl Into<String>) -> Self {
        self.leading = Some(addon.into());
        self
    }

    /// Sets the add-on after the text.
    pub fn trailing(mut self, addon: impl Into<String>) -> Self {
        self.trailing = Some(addon.into());
        self
    }

    /// Sets the hint shown while empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }
}

impl Widget for InputGroup<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let inner = Frame::new()
            .stroke(tokens.border_stroke())
            .corner_radius(tokens.control_radius())
            .inner_margin(Margin::symmetric(12, 0))
            .show(ui, |ui| {
                ui.set_width(self.width - 24.0);
                ui.horizontal(|ui| {
                    ui.set_height(34.0);
                    if let Some(addon) = &self.leading {
                        ui.label(
                            RichText::new(addon)
                                .font(tokens.body_font())
                                .color(tokens.muted_foreground),
                        );
                    }
                    let mut edit = egui::TextEdit::singleline(self.text)
                        .frame(Frame::NONE)
                        .text_color(tokens.foreground)
                        .font(tokens.body_font())
                        .desired_width(
                            ui.available_width()
                                - self
                                    .trailing
                                    .as_ref()
                                    .map_or(0.0, |t| t.len() as f32 * 8.0 + 8.0),
                        );
                    if let Some(placeholder) = &self.placeholder {
                        edit = edit
                            .hint_text(RichText::new(placeholder).color(tokens.muted_foreground));
                    }
                    let response = ui.add(edit);
                    if let Some(addon) = &self.trailing {
                        ui.label(
                            RichText::new(addon)
                                .font(tokens.body_font())
                                .color(tokens.muted_foreground),
                        );
                    }
                    response
                })
                .inner
            });
        let edit = inner.inner;
        if edit.has_focus() {
            ui.painter().rect_stroke(
                inner.response.rect,
                tokens.control_radius(),
                tokens.ring_stroke(),
                StrokeKind::Outside,
            );
        }
        edit
    }
}

/// shadcn's Field: a label, a control, and a description or error message.
///
/// An error replaces the description and turns the label destructive.
pub fn field<R>(
    ui: &mut Ui,
    label: &str,
    description: Option<&str>,
    error: Option<&str>,
    control: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let tokens = Tokens::current(ui.ctx());
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        let label_color = if error.is_some() {
            tokens.destructive
        } else {
            tokens.foreground
        };
        ui.label(
            RichText::new(label)
                .font(tokens.body_font())
                .strong()
                .color(label_color),
        );
        let inner = control(ui);
        match (error, description) {
            (Some(error), _) => {
                ui.label(
                    RichText::new(error)
                        .font(tokens.body_font())
                        .color(tokens.destructive),
                );
            }
            (None, Some(description)) => {
                ui.label(
                    RichText::new(description)
                        .font(tokens.body_font())
                        .color(tokens.muted_foreground),
                );
            }
            (None, None) => {}
        }
        inner
    })
}

/// shadcn's Form: fields followed by a submit button.
///
/// `fields` draws the fields and returns whether the input is valid. Returns
/// `true` in [`InnerResponse::inner`] on the frame the user submits valid input.
pub fn form(
    ui: &mut Ui,
    submit: &str,
    fields: impl FnOnce(&mut Ui) -> bool,
) -> InnerResponse<bool> {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 24.0;
        let valid = fields(ui);
        let submitted = ui.add(Button::new(submit).enabled(valid)).clicked();
        submitted && valid
    })
}

/// Visual style of an [`Item`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemVariant {
    /// No border.
    #[default]
    Default,
    /// A bordered row.
    Outline,
    /// A muted fill.
    Muted,
}

/// shadcn's Item: a list row with an optional media glyph, a title, a
/// description, and trailing actions.
#[must_use = "draw it with `item.show(ui, ...)`"]
pub struct Item {
    title: String,
    description: Option<String>,
    media: Option<String>,
    variant: ItemVariant,
}

impl Item {
    /// An item titled `title`.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            media: None,
            variant: ItemVariant::Default,
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets a glyph shown in a muted tile on the left.
    pub fn media(mut self, media: impl Into<String>) -> Self {
        self.media = Some(media.into());
        self
    }

    /// Sets the visual style.
    pub fn variant(mut self, variant: ItemVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Draws the item with `actions` (usually buttons) on the right.
    pub fn show<R>(self, ui: &mut Ui, actions: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let tokens = Tokens::current(ui.ctx());
        let (fill, stroke) = match self.variant {
            ItemVariant::Default => (Color32::TRANSPARENT, Stroke::NONE),
            ItemVariant::Outline => (Color32::TRANSPARENT, tokens.border_stroke()),
            ItemVariant::Muted => (tokens.muted.gamma_multiply(0.5), Stroke::NONE),
        };
        Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(tokens.control_radius())
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    if let Some(media) = &self.media {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                        ui.painter()
                            .rect_filled(rect, tokens.control_radius(), tokens.muted);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            media,
                            egui::FontId::proportional(18.0),
                            tokens.foreground,
                        );
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.label(
                            RichText::new(&self.title)
                                .font(tokens.body_font())
                                .strong()
                                .color(tokens.foreground),
                        );
                        if let Some(description) = &self.description {
                            ui.label(
                                RichText::new(description)
                                    .font(tokens.body_font())
                                    .color(tokens.muted_foreground),
                            );
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), actions)
                        .inner
                })
                .inner
            })
    }
}
