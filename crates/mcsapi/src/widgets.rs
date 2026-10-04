//! Small native shell widgets with shared, restrained visual tokens.
//!
//! Inspired by React Bits' card emphasis, Aceternity's tabs, and shadcn/ui's
//! compact controls, not their implementations. No decorative motion is used.

use crate::{Desktop, WorkspaceId};
use egui::{Button, Color32, RichText, Stroke};

/// Shared opaque colors for the egui and GPUI workspace bars.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    /// Background of the bar.
    pub background: Color32,
    /// Background of an inactive workspace control.
    pub surface: Color32,
    /// Text color.
    pub foreground: Color32,
    /// Border color for inactive controls.
    pub border: Color32,
    /// Border color for the active workspace.
    pub accent: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color32::from_rgb(15, 23, 42),
            surface: Color32::from_rgb(30, 41, 59),
            foreground: Color32::from_rgb(248, 250, 252),
            border: Color32::from_rgb(100, 116, 139),
            accent: Color32::from_rgb(163, 230, 53),
        }
    }
}

fn label(id: WorkspaceId, active: bool) -> String {
    if active {
        format!("Workspace {id} · active")
    } else {
        format!("Workspace {id}")
    }
}

/// Draws native egui buttons and returns a requested workspace switch.
///
/// The host applies the returned ID with [`Desktop::switch_to`]. egui supplies
/// button focus and keyboard activation; this is not an ARIA-style tab list.
/// Calling this function does not mutate desktop state or allocate a collection.
pub fn egui_workspace_bar(
    ui: &mut egui::Ui,
    desktop: &Desktop,
    theme: Theme,
) -> Option<WorkspaceId> {
    egui::Frame::new()
        .fill(theme.background)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let mut requested = None;
                for workspace in desktop.workspaces() {
                    let active = workspace.id() == desktop.active().id();
                    let response = ui.add(
                        Button::new(
                            RichText::new(label(workspace.id(), active)).color(theme.foreground),
                        )
                        .selected(active)
                        .fill(theme.surface)
                        .stroke(Stroke::new(
                            1.0,
                            if active { theme.accent } else { theme.border },
                        ))
                        .corner_radius(6)
                        .min_size(egui::vec2(40.0, 32.0)),
                    );
                    if response.clicked() {
                        requested = Some(workspace.id());
                    }
                }
                requested
            })
            .inner
        })
        .inner
}

/// A window inside a workspace preview, as fractions (0–1) of the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewWindow {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// Whether this is the workspace's focused window.
    pub focused: bool,
}

/// Size of one workspace thumbnail in [`egui_workspace_previews`].
pub const PREVIEW_SIZE: egui::Vec2 = egui::vec2(128.0, 80.0);

/// Top and bottom colors of the default derisk wallpaper, behind previews.
pub const WALLPAPER: (Color32, Color32) =
    (Color32::from_rgb(17, 24, 39), Color32::from_rgb(30, 27, 75));

/// Paints one workspace thumbnail: the wallpaper, a miniature of each window
/// with a title strip, and the workspace number in the bottom-left corner.
///
/// `hot` (active or hovered) draws a 2 px accent border.
pub fn paint_workspace_preview(
    painter: &egui::Painter,
    rect: egui::Rect,
    number: impl std::fmt::Display,
    windows: &[PreviewWindow],
    hot: bool,
    theme: Theme,
) {
    use egui::{Align2, FontId, Mesh, Pos2, Rect, StrokeKind, pos2, vec2};

    let radius = 10;
    let mut mesh = Mesh::default();
    let (top, bottom) = WALLPAPER;
    // Inset so the square mesh corners stay under the rounded border.
    let inner = rect.shrink(2.0);
    for (pos, color) in [
        (inner.left_top(), top),
        (inner.right_top(), top),
        (inner.right_bottom(), bottom),
        (inner.left_bottom(), bottom),
    ] {
        mesh.colored_vertex(pos, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.rect_filled(rect, radius, top);
    painter.add(mesh);

    let mini_line = theme.border.gamma_multiply(0.35);
    for window in windows {
        let at = |fx: f32, fy: f32| -> Pos2 {
            pos2(
                rect.left() + fx.clamp(0.0, 1.0) * rect.width(),
                rect.top() + fy.clamp(0.0, 1.0) * rect.height(),
            )
        };
        let frame = Rect::from_min_max(
            at(window.x, window.y),
            at(window.x + window.w, window.y + window.h),
        );
        if frame.width() < 1.0 || frame.height() < 1.0 {
            continue;
        }
        painter.rect_filled(frame, 3, theme.background);
        let strip =
            Rect::from_min_size(frame.min, vec2(frame.width(), 5.0_f32.min(frame.height())));
        painter.rect_filled(
            strip,
            egui::CornerRadius {
                nw: 3,
                ne: 3,
                sw: 0,
                se: 0,
            },
            if window.focused {
                theme.surface
            } else {
                theme.background
            },
        );
        painter.hline(strip.x_range(), strip.bottom(), Stroke::new(1.0, mini_line));
        painter.rect_stroke(
            frame,
            3,
            Stroke::new(
                1.0,
                if window.focused {
                    theme.accent
                } else {
                    theme.border
                },
            ),
            StrokeKind::Inside,
        );
    }

    let label = pos2(rect.left() + 6.0, rect.bottom() - 4.0);
    let font = FontId::new(11.0, crate::widgets::strong_family(painter.ctx()));
    painter.text(
        label + vec2(0.0, 1.0),
        Align2::LEFT_BOTTOM,
        number.to_string(),
        font.clone(),
        Color32::from_black_alpha(153),
    );
    painter.text(
        label,
        Align2::LEFT_BOTTOM,
        number.to_string(),
        font,
        theme.foreground,
    );
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(
            if hot { 2.0 } else { 1.0 },
            if hot { theme.accent } else { theme.border },
        ),
        StrokeKind::Inside,
    );
}

/// Name of the medium-weight UI font family that `mcsapi_ui::fonts` installs.
pub const STRONG_FAMILY: &str = "strong";

/// The medium UI face when installed (`mcsapi_ui::fonts`), else proportional.
fn strong_family(ctx: &egui::Context) -> egui::FontFamily {
    let family = egui::FontFamily::Name(STRONG_FAMILY.into());
    if ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family)) {
        family
    } else {
        egui::FontFamily::Proportional
    }
}

/// Like [`egui_workspace_bar`], with a live thumbnail of each workspace's
/// windows instead of a text button. `windows` returns a workspace's windows,
/// bottom to top, as fractions of the screen.
pub fn egui_workspace_previews(
    ui: &mut egui::Ui,
    desktop: &Desktop,
    theme: Theme,
    mut windows: impl FnMut(WorkspaceId) -> Vec<PreviewWindow>,
) -> Option<WorkspaceId> {
    egui::Frame::new()
        .fill(theme.background)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                let mut requested = None;
                for workspace in desktop.workspaces() {
                    let id = workspace.id();
                    let active = id == desktop.active().id();
                    let (rect, response) =
                        ui.allocate_exact_size(PREVIEW_SIZE, egui::Sense::click());
                    let hot = active || response.hovered() || response.has_focus();
                    paint_workspace_preview(ui.painter(), rect, id, &windows(id), hot, theme);
                    let response = response.on_hover_text(label(id, active));
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            true,
                            active,
                            label(id, active),
                        )
                    });
                    if response.clicked() {
                        requested = Some(id);
                    }
                }
                requested
            })
            .inner
        })
        .inner
}

/// Builds a GPUI workspace bar directly from an iterator of desktop workspaces.
///
/// The callback should update host state and request a redraw. GPUI controls
/// are focusable and support Enter/Space activation. The host is responsible
/// for application-level focus traversal and accessibility integration.
#[cfg(feature = "gpui")]
pub fn gpui_workspace_bar(
    desktop: &Desktop,
    theme: Theme,
    on_select: impl Fn(WorkspaceId, &mut gpui::Window, &mut gpui::App) + Clone + 'static,
) -> impl gpui::IntoElement {
    use gpui::{
        InteractiveElement, ParentElement, StatefulInteractiveElement, Styled, div, px, rgb,
    };

    let color = |value: Color32| {
        rgb(u32::from(value.r()) << 16 | u32::from(value.g()) << 8 | u32::from(value.b()))
    };
    div()
        .flex()
        .flex_wrap()
        .gap(px(6.0))
        .p(px(8.0))
        .bg(color(theme.background))
        .text_color(color(theme.foreground))
        .children(desktop.workspaces().map(|workspace| {
            let id = workspace.id();
            let active = id == desktop.active().id();
            let click = on_select.clone();
            let keyboard = on_select.clone();
            div()
                .id(("workspace", id.get()))
                .focusable()
                .px(px(12.0))
                .py(px(6.0))
                .min_h(px(32.0))
                .rounded(px(6.0))
                .bg(color(theme.surface))
                .border_1()
                .border_color(color(if active { theme.accent } else { theme.border }))
                .focus(|style| style.border_2().border_color(color(theme.foreground)))
                .child(label(id, active))
                .on_click(move |_, window, cx| click(id, window, cx))
                .on_key_down(move |event, window, cx| {
                    if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                        keyboard(id, window, cx);
                        cx.stop_propagation();
                    }
                })
        }))
}
