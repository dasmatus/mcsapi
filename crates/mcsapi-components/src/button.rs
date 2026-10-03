//! Buttons and button-like controls: button, badge, toggle, toggle group, kbd.

use egui::{
    Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
    Widget, WidgetInfo, WidgetType,
};

use crate::{IntoChanged as _, Tokens, paint_focus_ring};

/// Visual style of a [`Button`], matching shadcn's `variant` prop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Solid primary fill.
    #[default]
    Default,
    /// Muted fill for secondary actions.
    Secondary,
    /// Red fill for destructive actions.
    Destructive,
    /// Transparent with a border.
    Outline,
    /// Transparent until hovered.
    Ghost,
    /// Text that underlines on hover.
    Link,
}

/// Size of a [`Button`], matching shadcn's `size` prop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// 32 px high.
    Sm,
    /// 36 px high.
    #[default]
    Default,
    /// 40 px high.
    Lg,
    /// A 36 px square, for a single glyph.
    Icon,
}

impl ButtonSize {
    fn height(self) -> f32 {
        match self {
            Self::Sm => 32.0,
            Self::Default | Self::Icon => 36.0,
            Self::Lg => 40.0,
        }
    }

    fn padding_x(self) -> f32 {
        match self {
            Self::Sm => 12.0,
            Self::Default => 16.0,
            Self::Lg => 32.0,
            Self::Icon => 0.0,
        }
    }
}

fn variant_colors(tokens: &Tokens, variant: ButtonVariant, hovered: bool) -> (Color32, Color32) {
    let dim = |c: Color32| if hovered { c.gamma_multiply(0.9) } else { c };
    match variant {
        ButtonVariant::Default => (dim(tokens.primary), tokens.primary_foreground),
        ButtonVariant::Secondary => (
            if hovered {
                tokens.hover
            } else {
                tokens.secondary
            },
            tokens.foreground,
        ),
        ButtonVariant::Destructive => (dim(tokens.destructive), tokens.destructive_foreground),
        ButtonVariant::Outline | ButtonVariant::Ghost => (
            if hovered {
                tokens.hover
            } else {
                Color32::TRANSPARENT
            },
            tokens.foreground,
        ),
        ButtonVariant::Link => (Color32::TRANSPARENT, tokens.primary),
    }
}

/// shadcn's Button.
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// use mcsapi_components::{Button, ButtonVariant};
/// if ui.add(Button::new("Delete").variant(ButtonVariant::Destructive)).clicked() {
///     // ...
/// }
/// # });
/// ```
#[must_use = "add it with `ui.add(button)`"]
pub struct Button {
    text: String,
    variant: ButtonVariant,
    size: ButtonSize,
    enabled: bool,
}

impl Button {
    /// A default-variant button labeled `text`.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: ButtonVariant::Default,
            size: ButtonSize::Default,
            enabled: true,
        }
    }

    /// Sets the visual style.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Sets the size.
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Disables the button, dimming it and ignoring clicks.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for Button {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let font = tokens.body_font();
        let galley = ui
            .painter()
            .layout_no_wrap(self.text.clone(), font, Color32::PLACEHOLDER);
        let height = self.size.height();
        let width = if self.size == ButtonSize::Icon {
            height
        } else {
            galley.size().x + 2.0 * self.size.padding_x()
        };
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, self.enabled, &self.text));

        if ui.is_rect_visible(rect) {
            let hovered = self.enabled && response.hovered();
            let (fill, mut text_color) = variant_colors(&tokens, self.variant, hovered);
            if !self.enabled {
                text_color = text_color.gamma_multiply(0.5);
            }
            let fill = if self.enabled {
                fill
            } else {
                fill.gamma_multiply(0.5)
            };
            let painter = ui.painter();
            let radius = tokens.control_radius();
            painter.rect_filled(rect, radius, fill);
            if self.variant == ButtonVariant::Outline {
                painter.rect_stroke(rect, radius, tokens.border_stroke(), StrokeKind::Inside);
            }
            let text_rect = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size());
            if self.variant == ButtonVariant::Link && hovered {
                painter.hline(
                    text_rect.x_range(),
                    text_rect.bottom(),
                    Stroke::new(1.0, text_color),
                );
            }
            painter.galley(text_rect.min, galley, text_color);
            paint_focus_ring(ui, &response, rect, radius);
        }
        response
    }
}

/// Visual style of a [`Badge`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    /// Solid primary fill.
    #[default]
    Default,
    /// Muted fill.
    Secondary,
    /// Red fill.
    Destructive,
    /// Transparent with a border.
    Outline,
}

/// shadcn's Badge: a small, non-interactive pill of text.
#[must_use = "add it with `ui.add(badge)`"]
pub struct Badge {
    text: String,
    variant: BadgeVariant,
}

impl Badge {
    /// A default-variant badge.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: BadgeVariant::Default,
        }
    }

    /// Sets the visual style.
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Widget for Badge {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let galley = ui.painter().layout_no_wrap(
            self.text.clone(),
            tokens.small_font(),
            Color32::PLACEHOLDER,
        );
        let size = galley.size() + Vec2::new(16.0, 4.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &self.text));
        let (fill, text) = match self.variant {
            BadgeVariant::Default => (tokens.primary, tokens.primary_foreground),
            BadgeVariant::Secondary => (tokens.secondary, tokens.foreground),
            BadgeVariant::Destructive => (tokens.destructive, tokens.destructive_foreground),
            BadgeVariant::Outline => (Color32::TRANSPARENT, tokens.foreground),
        };
        let painter = ui.painter();
        let radius = CornerRadius::same(u8::MAX);
        painter.rect_filled(rect, radius, fill);
        if self.variant == BadgeVariant::Outline {
            painter.rect_stroke(rect, radius, tokens.border_stroke(), StrokeKind::Inside);
        }
        painter.galley(
            Align2::CENTER_CENTER
                .anchor_size(rect.center(), galley.size())
                .min,
            galley,
            text,
        );
        response
    }
}

fn paint_pressable(ui: &Ui, response: &Response, rect: Rect, text: &str, on: bool) {
    let tokens = Tokens::current(ui.ctx());
    let fill = if on {
        tokens.hover
    } else if response.hovered() {
        tokens.muted
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, tokens.control_radius(), fill);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        tokens.body_font(),
        if on || response.hovered() {
            tokens.foreground
        } else {
            tokens.muted_foreground
        },
    );
    paint_focus_ring(ui, response, rect, tokens.control_radius());
}

fn pressable_size(ui: &Ui, text: &str) -> Vec2 {
    let width = ui
        .painter()
        .layout_no_wrap(
            text.to_owned(),
            Tokens::current(ui.ctx()).body_font(),
            Color32::PLACEHOLDER,
        )
        .size()
        .x;
    Vec2::new((width + 20.0).max(36.0), 36.0)
}

/// shadcn's Toggle: a two-state button that stays pressed.
#[must_use = "add it with `ui.add(toggle)`"]
pub struct Toggle<'a> {
    pressed: &'a mut bool,
    text: String,
}

impl<'a> Toggle<'a> {
    /// A toggle bound to `pressed`.
    pub fn new(pressed: &'a mut bool, text: impl Into<String>) -> Self {
        Self {
            pressed,
            text: text.into(),
        }
    }
}

impl Widget for Toggle<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) =
            ui.allocate_exact_size(pressable_size(ui, &self.text), Sense::click());
        if response.clicked() {
            *self.pressed = !*self.pressed;
            response.mark_changed();
        }
        let pressed = *self.pressed;
        response
            .widget_info(|| WidgetInfo::selected(WidgetType::Button, true, pressed, &self.text));
        paint_pressable(ui, &response, rect, &self.text, pressed);
        response
    }
}

/// shadcn's ToggleGroup in `type="single"` mode: one pressed item at most.
///
/// Clicking the pressed item releases it, leaving `None`.
#[must_use = "add it with `ui.add(group)`"]
pub struct ToggleGroup<'a, T: AsRef<str>> {
    selected: &'a mut Option<usize>,
    items: &'a [T],
}

impl<'a, T: AsRef<str>> ToggleGroup<'a, T> {
    /// A group of `items` with the pressed index in `selected`.
    pub fn new(selected: &'a mut Option<usize>, items: &'a [T]) -> Self {
        Self { selected, items }
    }
}

impl<T: AsRef<str>> Widget for ToggleGroup<'_, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let mut changed = false;
            for (index, item) in self.items.iter().enumerate() {
                let text = item.as_ref();
                let on = *self.selected == Some(index);
                let (rect, response) =
                    ui.allocate_exact_size(pressable_size(ui, text), Sense::click());
                response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, text));
                if response.clicked() {
                    *self.selected = if on { None } else { Some(index) };
                    changed = true;
                }
                paint_pressable(ui, &response, rect, text, *self.selected == Some(index));
            }
            changed
        })
        .into_changed()
    }
}

/// shadcn's Kbd: a keyboard key cap such as `⌘` or `Ctrl`.
#[must_use = "add it with `ui.add(kbd)`"]
pub struct Kbd(String);

impl Kbd {
    /// A key cap showing `key`.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
}

impl Widget for Kbd {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let font = FontId::monospace(12.0);
        let galley = ui
            .painter()
            .layout_no_wrap(self.0.clone(), font, Color32::PLACEHOLDER);
        let size = Vec2::new((galley.size().x + 8.0).max(20.0), 20.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &self.0));
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(4), tokens.muted);
        painter.galley(
            Align2::CENTER_CENTER
                .anchor_size(rect.center(), galley.size())
                .min,
            galley,
            tokens.muted_foreground,
        );
        response
    }
}
