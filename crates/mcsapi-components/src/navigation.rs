//! Navigation and disclosure: tabs, breadcrumb, pagination, accordion,
//! collapsible, and table.

use egui::{
    Align2, Color32, CornerRadius, Id, InnerResponse, Rect, Response, RichText, Sense, Stroke, Ui,
    Widget, WidgetInfo, WidgetType, vec2,
};

use crate::{IntoChanged as _, Tokens, paint_focus_ring};

/// shadcn's TabsList: a segmented row of triggers.
///
/// Draw the selected tab's content yourself after adding the list:
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// use mcsapi_components::Tabs;
/// let mut tab = 0;
/// ui.add(Tabs::new(&mut tab, &["Account", "Password"]));
/// match tab {
///     0 => ui.label("Account settings"),
///     _ => ui.label("Password settings"),
/// };
/// # });
/// ```
#[must_use = "add it with `ui.add(tabs)`"]
pub struct Tabs<'a, T: AsRef<str>> {
    selected: &'a mut usize,
    tabs: &'a [T],
}

impl<'a, T: AsRef<str>> Tabs<'a, T> {
    /// A tab list over `tabs` with the active index in `selected`.
    pub fn new(selected: &'a mut usize, tabs: &'a [T]) -> Self {
        Self { selected, tabs }
    }
}

impl<T: AsRef<str>> Widget for Tabs<'_, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let font = tokens.body_font();
        let galleys: Vec<_> = self
            .tabs
            .iter()
            .map(|tab| {
                ui.painter().layout_no_wrap(
                    tab.as_ref().to_owned(),
                    font.clone(),
                    Color32::PLACEHOLDER,
                )
            })
            .collect();
        let padding = 3.0;
        let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 24.0).collect();
        let total = widths.iter().sum::<f32>() + 2.0 * padding;
        let (rect, mut response) = ui.allocate_exact_size(vec2(total, 36.0), Sense::hover());
        // A sunken, bordered track with the active tab raised out of it, as
        // the web interface draws its segmented switchers.
        ui.painter()
            .rect_filled(rect, tokens.control_radius(), tokens.field);
        ui.painter().rect_stroke(
            rect,
            tokens.control_radius(),
            tokens.border_stroke(),
            egui::StrokeKind::Inside,
        );

        let mut x = rect.left() + padding;
        for (index, (galley, width)) in galleys.into_iter().zip(widths).enumerate() {
            let trigger = Rect::from_min_size(
                egui::pos2(x, rect.top() + padding),
                vec2(width, rect.height() - 2.0 * padding),
            );
            x += width;
            let trigger_response = ui.interact(trigger, response.id.with(index), Sense::click());
            let label = self.tabs[index].as_ref();
            let on = *self.selected == index;
            trigger_response
                .widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, label));
            if trigger_response.clicked() && !on {
                *self.selected = index;
                response.mark_changed();
            }
            let on = *self.selected == index;
            let painter = ui.painter();
            if on {
                painter.rect_filled(trigger, tokens.control_radius(), tokens.muted);
            }
            let color = if on || trigger_response.hovered() {
                tokens.foreground
            } else {
                tokens.muted_foreground
            };
            painter.galley(
                Align2::CENTER_CENTER
                    .anchor_size(trigger.center(), galley.size())
                    .min,
                galley,
                color,
            );
            paint_focus_ring(ui, &trigger_response, trigger, tokens.control_radius());
        }
        response
    }
}

/// shadcn's Breadcrumb. Every item but the last is a link.
///
/// Returns the clicked item's index in [`InnerResponse::inner`].
pub fn breadcrumb<T: AsRef<str>>(ui: &mut Ui, items: &[T]) -> InnerResponse<Option<usize>> {
    let tokens = Tokens::current(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let mut clicked = None;
        for (index, item) in items.iter().enumerate() {
            let last = index + 1 == items.len();
            if last {
                ui.label(
                    RichText::new(item.as_ref())
                        .font(tokens.body_font())
                        .color(tokens.foreground),
                );
            } else {
                let link = ui.add(
                    egui::Label::new(
                        RichText::new(item.as_ref())
                            .font(tokens.body_font())
                            .color(tokens.muted_foreground),
                    )
                    .sense(Sense::click()),
                );
                if link.hovered() {
                    ui.painter().hline(
                        link.rect.x_range(),
                        link.rect.bottom(),
                        Stroke::new(1.0, tokens.muted_foreground),
                    );
                }
                if link.clicked() {
                    clicked = Some(index);
                }
                ui.label(
                    RichText::new("›")
                        .font(tokens.body_font())
                        .color(tokens.muted_foreground),
                );
            }
        }
        clicked
    })
}

/// Page numbers to show for `current` of `total` pages, with `None` as an ellipsis.
///
/// Always shows the first and last page and one neighbor either side of `current`.
pub fn page_window(current: usize, total: usize) -> Vec<Option<usize>> {
    if total <= 7 {
        return (0..total).map(Some).collect();
    }
    let mut pages = vec![Some(0)];
    let low = current.saturating_sub(1).max(1);
    let high = (current + 1).min(total - 2);
    if low > 1 {
        pages.push(None);
    }
    pages.extend((low..=high).map(Some));
    if high < total - 2 {
        pages.push(None);
    }
    pages.push(Some(total - 1));
    pages
}

/// shadcn's Pagination: previous, page numbers, and next.
///
/// `page` is zero-based; the labels are one-based.
#[must_use = "add it with `ui.add(pagination)`"]
pub struct Pagination<'a> {
    page: &'a mut usize,
    total: usize,
}

impl<'a> Pagination<'a> {
    /// Pagination over `total` pages with the current one in `page`.
    pub fn new(page: &'a mut usize, total: usize) -> Self {
        Self { page, total }
    }
}

impl Widget for Pagination<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let total = self.total.max(1);
        *self.page = (*self.page).min(total - 1);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let mut target = None;
            let cell = |ui: &mut Ui, text: &str, active: bool, enabled: bool| -> bool {
                let galley = ui.painter().layout_no_wrap(
                    text.to_owned(),
                    tokens.body_font(),
                    Color32::PLACEHOLDER,
                );
                let size = vec2((galley.size().x + 16.0).max(36.0), 36.0);
                let sense = if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                };
                let (rect, response) = ui.allocate_exact_size(size, sense);
                response.widget_info(|| {
                    WidgetInfo::selected(WidgetType::Button, enabled, active, text)
                });
                let painter = ui.painter();
                if active {
                    painter.rect_stroke(
                        rect,
                        tokens.control_radius(),
                        tokens.border_stroke(),
                        egui::StrokeKind::Inside,
                    );
                } else if enabled && response.hovered() {
                    painter.rect_filled(rect, tokens.control_radius(), tokens.hover);
                }
                let color = if enabled {
                    tokens.foreground
                } else {
                    tokens.muted_foreground.gamma_multiply(0.6)
                };
                painter.galley(
                    Align2::CENTER_CENTER
                        .anchor_size(rect.center(), galley.size())
                        .min,
                    galley,
                    color,
                );
                paint_focus_ring(ui, &response, rect, tokens.control_radius());
                response.clicked()
            };
            let page = *self.page;
            if cell(ui, "‹ Previous", false, page > 0) {
                target = Some(page - 1);
            }
            for entry in page_window(page, total) {
                match entry {
                    Some(n) => {
                        if cell(ui, &(n + 1).to_string(), n == page, true) && n != page {
                            target = Some(n);
                        }
                    }
                    None => {
                        cell(ui, "…", false, false);
                    }
                }
            }
            if cell(ui, "Next ›", false, page + 1 < total) {
                target = Some(page + 1);
            }
            if let Some(target) = target {
                *self.page = target;
            }
            target.is_some()
        })
        .into_changed()
    }
}

/// shadcn's Collapsible: a header that shows or hides its content.
///
/// The open state lives in egui memory under `id_salt`.
#[must_use = "draw it with `collapsible.show(ui, ...)`"]
pub struct Collapsible {
    id: Id,
    title: String,
    default_open: bool,
    divider: bool,
}

impl Collapsible {
    /// A closed collapsible titled `title`.
    pub fn new(id_salt: impl egui::AsId, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id_salt),
            title: title.into(),
            default_open: false,
            divider: false,
        }
    }

    /// Starts open the first time it is shown.
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Draws `content` when open. `inner` is `None` while closed.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        let tokens = Tokens::current(ui.ctx());
        let id = ui.make_persistent_id(self.id);
        let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, self.default_open));
        let width = ui.available_width();
        let (rect, mut header) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
        if header.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_persisted(id, open));
            header.mark_changed();
        }
        header.widget_info(|| {
            WidgetInfo::selected(WidgetType::CollapsingHeader, true, open, &self.title)
        });
        let painter = ui.painter();
        let title = RichText::new(&self.title)
            .font(tokens.body_font())
            .strong()
            .color(tokens.foreground);
        let galley = egui::WidgetText::from(title).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            width,
            egui::TextStyle::Body,
        );
        if header.hovered() {
            painter.hline(
                rect.left()..=(rect.left() + galley.size().x),
                rect.center().y + galley.size().y / 2.0 + 1.0,
                Stroke::new(1.0, tokens.foreground),
            );
        }
        painter.galley(
            egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
            galley,
            tokens.foreground,
        );
        let openness = ui.ctx().animate_bool_responsive(id, open);
        paint_chevron(
            ui,
            egui::pos2(rect.right() - 8.0, rect.center().y),
            openness,
            tokens.muted_foreground,
        );
        paint_focus_ring(ui, &header, rect, CornerRadius::same(4));

        let inner = open.then(|| {
            let inner = content(ui);
            ui.add_space(12.0);
            inner
        });
        if self.divider {
            let y = ui.cursor().top();
            ui.painter()
                .hline(rect.x_range(), y, tokens.border_stroke());
        }
        InnerResponse::new(inner, header)
    }
}

fn paint_chevron(ui: &Ui, center: egui::Pos2, openness: f32, color: Color32) {
    // shadcn's chevron points down when closed and rotates to point up when open.
    let flip = egui::lerp(1.0..=-1.0, openness);
    let points = vec![
        center + vec2(-4.0, -2.0 * flip),
        center + vec2(0.0, 2.0 * flip),
        center + vec2(4.0, -2.0 * flip),
    ];
    ui.painter().line(points, Stroke::new(1.5, color));
}

/// shadcn's AccordionItem: a [`Collapsible`] with a rule under it.
///
/// Stack several to build an accordion.
pub fn accordion_item<R>(
    ui: &mut Ui,
    id_salt: impl egui::AsId,
    title: impl Into<String>,
    content: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let mut item = Collapsible::new(id_salt, title);
    item.divider = true;
    item.show(ui, content)
}

/// shadcn's Table: a header row over striped-on-hover body rows.
#[must_use = "draw it with `table.show(ui)`"]
pub struct Table<'a> {
    header: &'a [&'a str],
    rows: &'a [Vec<String>],
    caption: Option<&'a str>,
}

impl<'a> Table<'a> {
    /// A table with column titles `header` and string cells `rows`.
    pub fn new(header: &'a [&'a str], rows: &'a [Vec<String>]) -> Self {
        Self {
            header,
            rows,
            caption: None,
        }
    }

    /// Sets a muted caption under the table.
    pub fn caption(mut self, caption: &'a str) -> Self {
        self.caption = Some(caption);
        self
    }

    /// Draws the table and returns the hovered row index, if any.
    pub fn show(self, ui: &mut Ui) -> InnerResponse<Option<usize>> {
        let tokens = Tokens::current(ui.ctx());
        let columns = self.header.len().max(1);
        let width = ui.available_width();
        let column_width = width / columns as f32;
        let row_height = 40.0;
        let cell_text = |ui: &Ui, rect: Rect, column: usize, text: &str, color: Color32| {
            let cell = Rect::from_min_size(
                rect.min + vec2(column as f32 * column_width + 8.0, 0.0),
                vec2(column_width - 16.0, rect.height()),
            );
            let galley =
                ui.painter()
                    .layout(text.to_owned(), tokens.body_font(), color, cell.width());
            ui.painter().with_clip_rect(cell).galley(
                egui::pos2(cell.left(), cell.center().y - galley.size().y / 2.0),
                galley,
                color,
            );
        };
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (head, _) = ui.allocate_exact_size(vec2(width, row_height), Sense::hover());
            for (column, title) in self.header.iter().enumerate() {
                cell_text(ui, head, column, title, tokens.muted_foreground);
            }
            ui.painter()
                .hline(head.x_range(), head.bottom(), tokens.border_stroke());
            let mut hovered = None;
            for (index, row) in self.rows.iter().enumerate() {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(width, row_height), Sense::hover());
                if response.hovered() {
                    hovered = Some(index);
                    ui.painter()
                        .rect_filled(rect, 0.0, tokens.muted.gamma_multiply(0.5));
                }
                for (column, text) in row.iter().take(columns).enumerate() {
                    cell_text(ui, rect, column, text, tokens.foreground);
                }
                if index + 1 < self.rows.len() {
                    ui.painter()
                        .hline(rect.x_range(), rect.bottom(), tokens.border_stroke());
                }
            }
            if let Some(caption) = self.caption {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(caption)
                            .font(tokens.body_font())
                            .color(tokens.muted_foreground),
                    );
                });
            }
            hovered
        })
    }
}
