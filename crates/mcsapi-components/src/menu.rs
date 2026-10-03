//! Floating surfaces: popover, hover card, dropdown menu, context menu,
//! menubar, and navigation menu.

use egui::{
    Align2, Color32, Frame, Id, InnerResponse, Margin, Popup, PopupCloseBehavior, RectAlign,
    Response, RichText, Sense, Shadow, Stroke, Ui, Widget, WidgetInfo, WidgetType, vec2,
};

use crate::Tokens;

/// The frame shadcn uses for popovers and menus.
pub(crate) fn surface_frame(tokens: &Tokens, padding: i8) -> Frame {
    Frame::new()
        .fill(tokens.background)
        .stroke(tokens.border_stroke())
        .corner_radius(tokens.control_radius())
        .inner_margin(Margin::same(padding))
        .shadow(Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(60),
        })
}

/// Restyles egui's built-in menu widgets in `ui` to shadcn's look.
pub(crate) fn style_menu(ui: &mut Ui, tokens: &Tokens) {
    let visuals = &mut ui.style_mut().visuals;
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, tokens.foreground);
    visuals.widgets.hovered.weak_bg_fill = tokens.muted;
    visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, tokens.foreground);
    visuals.widgets.active.weak_bg_fill = tokens.hover;
    visuals.widgets.open.weak_bg_fill = tokens.muted;
    for state in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        state.corner_radius = egui::CornerRadius::same(4);
    }
    visuals.window_fill = tokens.background;
    visuals.window_stroke = tokens.border_stroke();
    visuals.override_text_color = Some(tokens.foreground);
}

/// shadcn's Popover: a panel that toggles open when `trigger` is clicked and
/// closes on a click outside it or Escape.
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// use mcsapi_components::{Button, ButtonVariant, popover};
/// let trigger = ui.add(Button::new("Open").variant(ButtonVariant::Outline));
/// popover(&trigger, |ui| ui.label("Settings"));
/// # });
/// ```
pub fn popover<R>(
    trigger: &Response,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    let tokens = Tokens::current(&trigger.ctx);
    Popup::from_toggle_button_response(trigger)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .align(RectAlign::BOTTOM_START)
        .gap(4.0)
        .frame(surface_frame(&tokens, 16))
        .show(|ui| {
            ui.set_min_width(220.0);
            content(ui)
        })
}

/// shadcn's HoverCard: a rich preview card shown while `trigger` is hovered.
pub fn hover_card(trigger: Response, content: impl FnOnce(&mut Ui)) -> Response {
    let tokens = Tokens::current(&trigger.ctx);
    trigger.on_hover_ui(|ui| {
        let style = ui.style_mut();
        style.visuals.window_fill = tokens.background;
        style.visuals.window_stroke = tokens.border_stroke();
        style.visuals.override_text_color = Some(tokens.foreground);
        ui.set_width(256.0);
        content(ui);
    })
}

/// One row of a dropdown, context, or menubar menu, with an optional
/// right-aligned keyboard shortcut. Returns the row's response.
pub fn menu_item(ui: &mut Ui, label: &str, shortcut: Option<&str>) -> Response {
    let tokens = Tokens::current(ui.ctx());
    let width = ui.available_width().max(160.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    let painter = ui.painter();
    if response.hovered() || response.has_focus() {
        painter.rect_filled(rect, egui::CornerRadius::same(4), tokens.muted);
    }
    painter.text(
        rect.left_center() + vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        tokens.body_font(),
        tokens.foreground,
    );
    if let Some(shortcut) = shortcut {
        painter.text(
            rect.right_center() - vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            shortcut,
            tokens.small_font(),
            tokens.muted_foreground,
        );
    }
    if response.clicked() {
        ui.close();
    }
    response
}

/// A checkable menu row. Clicking toggles `checked` and keeps the menu open.
pub fn menu_checkbox(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let tokens = Tokens::current(ui.ctx());
    let width = ui.available_width().max(160.0);
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    let on = *checked;
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, on, label));
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(4), tokens.muted);
    }
    if on {
        let c = rect.left_center() + vec2(14.0, 0.0);
        painter.line(
            vec![
                c + vec2(-4.0, 0.0),
                c + vec2(-1.0, 3.0),
                c + vec2(4.0, -3.0),
            ],
            Stroke::new(1.5, tokens.foreground),
        );
    }
    painter.text(
        rect.left_center() + vec2(28.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        tokens.body_font(),
        tokens.foreground,
    );
    response
}

/// A bold, non-interactive heading inside a menu.
pub fn menu_label(ui: &mut Ui, label: &str) {
    let tokens = Tokens::current(ui.ctx());
    ui.add_space(2.0);
    ui.label(
        RichText::new(label)
            .font(tokens.body_font())
            .strong()
            .color(tokens.foreground),
    );
    ui.add_space(2.0);
}

/// A rule between groups of menu rows.
pub fn menu_separator(ui: &mut Ui) {
    let tokens = Tokens::current(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, tokens.border_stroke());
}

/// shadcn's DropdownMenu: a menu that opens below `trigger` when it is clicked.
///
/// Fill it with [`menu_item`], [`menu_checkbox`], [`menu_label`], and [`menu_separator`].
pub fn dropdown_menu<R>(
    trigger: &Response,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    let tokens = Tokens::current(&trigger.ctx);
    Popup::menu(trigger)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .gap(4.0)
        .frame(surface_frame(&tokens, 4))
        .show(|ui| {
            style_menu(ui, &tokens);
            ui.set_min_width(200.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            content(ui)
        })
}

/// shadcn's ContextMenu: a menu that opens at the pointer when `target` is
/// right-clicked.
pub fn context_menu<R>(
    target: &Response,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    let tokens = Tokens::current(&target.ctx);
    Popup::context_menu(target)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .frame(surface_frame(&tokens, 4))
        .show(|ui| {
            style_menu(ui, &tokens);
            ui.set_min_width(200.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            content(ui)
        })
}

/// shadcn's Menubar: a bordered row of menu triggers, like a desktop app's
/// File / Edit / View bar. Add menus with [`menubar_menu`].
pub fn menubar<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let tokens = Tokens::current(ui.ctx());
    Frame::new()
        .fill(tokens.background)
        .stroke(tokens.border_stroke())
        .corner_radius(tokens.control_radius())
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                content(ui)
            })
            .inner
        })
}

/// One menu in a [`menubar`]. Opens on click, and on hover while another
/// menu in the same bar is open.
pub fn menubar_menu<R>(
    ui: &mut Ui,
    title: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    let tokens = Tokens::current(ui.ctx());
    let bar_id = ui.id().with("mcsapi_menubar_open");
    let galley =
        ui.painter()
            .layout_no_wrap(title.to_owned(), tokens.body_font(), Color32::PLACEHOLDER);
    let (rect, trigger) = ui.allocate_exact_size(galley.size() + vec2(24.0, 12.0), Sense::click());
    trigger.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, title));
    let popup_id = Popup::default_response_id(&trigger);
    let open_now = Popup::is_id_open(ui.ctx(), popup_id);
    let another_open: Option<Id> = ui.data(|d| d.get_temp(bar_id));
    if trigger.hovered()
        && !open_now
        && another_open.is_some_and(|other| other != popup_id && Popup::is_id_open(ui.ctx(), other))
    {
        Popup::open_id(ui.ctx(), popup_id);
    }
    let painter = ui.painter();
    if open_now || trigger.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(4), tokens.muted);
    }
    painter.galley(
        Align2::CENTER_CENTER
            .anchor_size(rect.center(), galley.size())
            .min,
        galley,
        tokens.foreground,
    );
    let inner = dropdown_menu(&trigger, content);
    if inner.is_some() {
        ui.data_mut(|d| d.insert_temp(bar_id, popup_id));
    }
    inner
}

/// shadcn's NavigationMenu trigger: a link-style button that shows a panel
/// of links below it while hovered or clicked.
pub struct NavigationMenu {
    title: String,
}

impl NavigationMenu {
    /// A trigger labeled `title`.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }

    /// Draws the trigger and, while open, the panel with `content`.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> (Response, Option<InnerResponse<R>>) {
        let tokens = Tokens::current(ui.ctx());
        let galley = ui.painter().layout_no_wrap(
            format!("{} ▾", self.title),
            tokens.body_font(),
            Color32::PLACEHOLDER,
        );
        let (rect, trigger) =
            ui.allocate_exact_size(galley.size() + vec2(32.0, 16.0), Sense::click());
        trigger.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &self.title));
        let popup_id = Popup::default_response_id(&trigger);
        let hover_id = popup_id.with("hovering_panel");
        let panel_hovered: bool = ui.data(|d| d.get_temp(hover_id)).unwrap_or(false);
        let open = trigger.hovered() || panel_hovered;
        let painter = ui.painter();
        if open {
            painter.rect_filled(rect, tokens.control_radius(), tokens.muted);
        }
        painter.galley(
            Align2::CENTER_CENTER
                .anchor_size(rect.center(), galley.size())
                .min,
            galley,
            tokens.foreground,
        );
        let inner = Popup::from_response(&trigger)
            .open(open)
            .gap(4.0)
            .frame(surface_frame(&tokens, 12))
            .show(|ui| {
                ui.set_min_width(320.0);
                content(ui)
            });
        let still_hovered = inner.as_ref().is_some_and(|i| {
            ui.ctx()
                .rect_contains_pointer(i.response.layer_id, i.response.rect.expand(4.0))
        });
        ui.data_mut(|d| d.insert_temp(hover_id, still_hovered));
        (trigger, inner)
    }
}

/// A link inside a [`NavigationMenu`] panel: a title over a muted description.
pub fn navigation_link(ui: &mut Ui, title: &str, description: &str) -> Response {
    let tokens = Tokens::current(ui.ctx());
    let response = Frame::new()
        .inner_margin(Margin::same(12))
        .corner_radius(tokens.control_radius())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(title)
                    .font(tokens.body_font())
                    .strong()
                    .color(tokens.foreground),
            );
            ui.label(
                RichText::new(description)
                    .font(tokens.body_font())
                    .color(tokens.muted_foreground),
            );
        })
        .response
        .interact(Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            response.rect,
            tokens.control_radius(),
            tokens.muted.gamma_multiply(0.5),
        );
    }
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Link, true, title));
    response
}

/// A plain trigger used as a [`Widget`]: a ghost button that opens nothing
/// on its own, for top-level links in a navigation bar.
pub struct NavigationLink(pub String);

impl Widget for NavigationLink {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.add(crate::Button::new(self.0).variant(crate::ButtonVariant::Ghost))
    }
}
