//! Data components: chart, data table, and one-time-password input.

use egui::{
    Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, Response, RichText, Sense, Shape,
    Stroke, StrokeKind, Ui, Vec2, Widget, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::{Checkbox, Input, Pagination, Tokens};

/// shadcn's five chart colors (`--chart-1` … `--chart-5`), derived from the theme accent.
pub fn chart_colors(tokens: &Tokens) -> [Color32; 5] {
    let base = tokens.primary;
    [
        base,
        base.lerp_to_gamma(Color32::from_rgb(56, 189, 248), 0.6),
        base.lerp_to_gamma(Color32::from_rgb(168, 85, 247), 0.6),
        base.lerp_to_gamma(Color32::from_rgb(251, 146, 60), 0.6),
        base.lerp_to_gamma(Color32::from_rgb(244, 63, 94), 0.6),
    ]
}

/// Kind of [`Chart`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartKind {
    /// Grouped vertical bars.
    #[default]
    Bar,
    /// Lines through each point.
    Line,
    /// Lines with the area under them filled.
    Area,
}

/// One data series of a [`Chart`].
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// Name shown in the legend and tooltip.
    pub name: String,
    /// One value per category.
    pub values: Vec<f32>,
}

/// shadcn's Chart (Recharts): a bar, line, or area chart with a grid,
/// category labels, legend, and hover tooltip.
#[must_use = "add it with `ui.add(chart)`"]
pub struct Chart<'a> {
    categories: &'a [&'a str],
    series: &'a [Series],
    kind: ChartKind,
    size: Vec2,
}

impl<'a> Chart<'a> {
    /// A bar chart of `series` over `categories`.
    pub fn new(categories: &'a [&'a str], series: &'a [Series]) -> Self {
        Self {
            categories,
            series,
            kind: ChartKind::Bar,
            size: vec2(480.0, 240.0),
        }
    }

    /// Sets the chart kind.
    pub fn kind(mut self, kind: ChartKind) -> Self {
        self.kind = kind;
        self
    }

    /// Sets the overall size.
    pub fn size(mut self, size: impl Into<Vec2>) -> Self {
        self.size = size.into();
        self
    }
}

/// A "nice" upper bound for an axis whose data peaks at `max`.
pub fn nice_ceiling(max: f32) -> f32 {
    if max <= 0.0 {
        return 1.0;
    }
    let magnitude = 10f32.powf(max.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .into_iter()
        .find(|s| s * magnitude >= max)
        .unwrap_or(10.0);
    step * magnitude
}

impl Widget for Chart<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let colors = chart_colors(&tokens);
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Chart"));
        let legend_h = 24.0;
        let label_h = 20.0;
        let plot = Rect::from_min_max(
            rect.min + vec2(8.0, 8.0),
            pos2(rect.right() - 8.0, rect.bottom() - legend_h - label_h),
        );
        let max = self
            .series
            .iter()
            .flat_map(|s| s.values.iter().copied())
            .fold(0.0, f32::max);
        let top = nice_ceiling(max);
        let y_of = |v: f32| plot.bottom() - plot.height() * (v / top).clamp(0.0, 1.0);
        let painter = ui.painter();
        for i in 0..=4 {
            let y = plot.bottom() - plot.height() * i as f32 / 4.0;
            painter.hline(
                plot.x_range(),
                y,
                Stroke::new(1.0, tokens.border.gamma_multiply(0.6)),
            );
        }
        let n = self.categories.len().max(1);
        let band = plot.width() / n as f32;
        for (i, category) in self.categories.iter().enumerate() {
            painter.text(
                pos2(plot.left() + band * (i as f32 + 0.5), plot.bottom() + 6.0),
                Align2::CENTER_TOP,
                *category,
                tokens.small_font(),
                tokens.muted_foreground,
            );
        }
        let hovered_band = response
            .hover_pos()
            .filter(|p| plot.contains(*p))
            .map(|p| (((p.x - plot.left()) / band) as usize).min(n - 1));
        if let Some(b) = hovered_band
            && self.kind == ChartKind::Bar
        {
            let r = Rect::from_x_y_ranges(
                (plot.left() + band * b as f32)..=(plot.left() + band * (b + 1) as f32),
                plot.y_range(),
            );
            painter.rect_filled(r, 0.0, tokens.muted.gamma_multiply(0.4));
        }
        let series_count = self.series.len().max(1);
        match self.kind {
            ChartKind::Bar => {
                let inner = band * 0.7;
                let bar_w = inner / series_count as f32 - 2.0;
                for (s, series) in self.series.iter().enumerate() {
                    for (i, value) in series.values.iter().enumerate().take(n) {
                        let x = plot.left()
                            + band * i as f32
                            + (band - inner) / 2.0
                            + s as f32 * (bar_w + 2.0);
                        let bar = Rect::from_min_max(
                            pos2(x, y_of(*value)),
                            pos2(x + bar_w, plot.bottom()),
                        );
                        painter.rect_filled(
                            bar,
                            CornerRadius {
                                nw: 4,
                                ne: 4,
                                sw: 0,
                                se: 0,
                            },
                            colors[s % 5],
                        );
                    }
                }
            }
            ChartKind::Line | ChartKind::Area => {
                for (s, series) in self.series.iter().enumerate() {
                    let points: Vec<Pos2> = series
                        .values
                        .iter()
                        .take(n)
                        .enumerate()
                        .map(|(i, v)| pos2(plot.left() + band * (i as f32 + 0.5), y_of(*v)))
                        .collect();
                    let color = colors[s % 5];
                    if self.kind == ChartKind::Area && points.len() > 1 {
                        let mut mesh = egui::Mesh::default();
                        for p in &points {
                            mesh.colored_vertex(*p, color.gamma_multiply(0.4));
                            mesh.colored_vertex(
                                pos2(p.x, plot.bottom()),
                                color.gamma_multiply(0.05),
                            );
                        }
                        for i in 0..points.len() as u32 - 1 {
                            let (a, b, c, d) = (2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3);
                            mesh.add_triangle(a, b, c);
                            mesh.add_triangle(b, d, c);
                        }
                        painter.add(Shape::mesh(mesh));
                    }
                    painter.add(Shape::line(points.clone(), Stroke::new(2.0, color)));
                    if let Some(b) = hovered_band
                        && let Some(p) = points.get(b)
                    {
                        painter.circle_filled(*p, 4.0, color);
                    }
                }
                if let Some(b) = hovered_band {
                    let x = plot.left() + band * (b as f32 + 0.5);
                    painter.vline(x, plot.y_range(), Stroke::new(1.0, tokens.border));
                }
            }
        }
        // Legend.
        let mut x = rect.center().x
            - self
                .series
                .iter()
                .map(|s| s.name.len() as f32 * 7.0 + 24.0)
                .sum::<f32>()
                / 2.0;
        let y = rect.bottom() - legend_h / 2.0;
        for (s, series) in self.series.iter().enumerate() {
            painter.rect_filled(
                Rect::from_center_size(pos2(x + 5.0, y), Vec2::splat(10.0)),
                CornerRadius::same(2),
                colors[s % 5],
            );
            let galley =
                painter.layout_no_wrap(series.name.clone(), tokens.small_font(), tokens.foreground);
            let w = galley.size().x;
            painter.galley(
                pos2(x + 16.0, y - galley.size().y / 2.0),
                galley,
                tokens.foreground,
            );
            x += w + 32.0;
        }
        // Tooltip.
        if let (Some(b), Some(pointer)) = (hovered_band, response.hover_pos()) {
            let lines: Vec<(Color32, String)> = self
                .series
                .iter()
                .enumerate()
                .filter_map(|(s, series)| {
                    series
                        .values
                        .get(b)
                        .map(|v| (colors[s % 5], format!("{}  {}", series.name, v)))
                })
                .collect();
            let height = 24.0 + lines.len() as f32 * 18.0;
            let tip = Rect::from_min_size(pointer + vec2(12.0, -height / 2.0), vec2(150.0, height));
            let painter = painter
                .clone()
                .with_layer_id(egui::LayerId::new(egui::Order::Tooltip, response.id));
            painter.rect_filled(tip, tokens.control_radius(), tokens.background);
            painter.rect_stroke(
                tip,
                tokens.control_radius(),
                tokens.border_stroke(),
                StrokeKind::Inside,
            );
            painter.text(
                tip.min + vec2(8.0, 6.0),
                Align2::LEFT_TOP,
                self.categories[b],
                tokens.small_font(),
                tokens.foreground,
            );
            for (i, (color, text)) in lines.into_iter().enumerate() {
                let y = tip.top() + 26.0 + i as f32 * 18.0;
                painter.rect_filled(
                    Rect::from_min_size(pos2(tip.left() + 8.0, y + 2.0), Vec2::splat(8.0)),
                    CornerRadius::same(2),
                    color,
                );
                painter.text(
                    pos2(tip.left() + 22.0, y),
                    Align2::LEFT_TOP,
                    text,
                    tokens.small_font(),
                    tokens.muted_foreground,
                );
            }
        }
        response
    }
}

/// Sort order of a [`DataTable`] column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    /// Smallest first.
    Ascending,
    /// Largest first.
    Descending,
}

/// What the user has done to a [`DataTable`]: filter text, sort, selection, and page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataTableState {
    /// Text that rows must contain (in any cell, ignoring case) to be shown.
    pub filter: String,
    /// Sorted column and direction.
    pub sort: Option<(usize, SortOrder)>,
    /// Indices into the original rows of the selected rows.
    pub selected: Vec<usize>,
    /// Current page, zero-based.
    pub page: usize,
}

/// shadcn's DataTable (TanStack Table): a filterable, sortable table with
/// row selection and pagination.
pub struct DataTable<'a> {
    header: &'a [&'a str],
    rows: &'a [Vec<String>],
    page_size: usize,
}

/// Compares two cells numerically when both parse as numbers, else as text.
fn compare_cells(a: &str, b: &str) -> std::cmp::Ordering {
    let num = |s: &str| {
        s.trim_start_matches('$')
            .replace(',', "")
            .parse::<f64>()
            .ok()
    };
    match (num(a), num(b)) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

impl<'a> DataTable<'a> {
    /// A table with column titles `header` over string cells `rows`, 10 rows per page.
    pub fn new(header: &'a [&'a str], rows: &'a [Vec<String>]) -> Self {
        Self {
            header,
            rows,
            page_size: 10,
        }
    }

    /// Sets the rows per page.
    pub fn page_size(mut self, size: usize) -> Self {
        self.page_size = size.max(1);
        self
    }

    /// The original row indices shown on the current page of `state`, in display order.
    pub fn visible_rows(&self, state: &DataTableState) -> Vec<usize> {
        let filter = state.filter.to_lowercase();
        let mut indices: Vec<usize> = (0..self.rows.len())
            .filter(|&i| {
                filter.is_empty()
                    || self.rows[i]
                        .iter()
                        .any(|c| c.to_lowercase().contains(&filter))
            })
            .collect();
        if let Some((column, order)) = state.sort {
            indices.sort_by(|&a, &b| {
                let (x, y) = (self.rows[a].get(column), self.rows[b].get(column));
                let ord = compare_cells(x.map_or("", |s| s), y.map_or("", |s| s));
                if order == SortOrder::Descending {
                    ord.reverse()
                } else {
                    ord
                }
            });
        }
        indices
            .into_iter()
            .skip(state.page * self.page_size)
            .take(self.page_size)
            .collect()
    }

    fn filtered_len(&self, state: &DataTableState) -> usize {
        let filter = state.filter.to_lowercase();
        (0..self.rows.len())
            .filter(|&i| {
                filter.is_empty()
                    || self.rows[i]
                        .iter()
                        .any(|c| c.to_lowercase().contains(&filter))
            })
            .count()
    }

    /// Draws the table, editing `state`.
    pub fn show(self, ui: &mut Ui, state: &mut DataTableState) -> Response {
        let tokens = Tokens::current(ui.ctx());
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            let before = state.filter.clone();
            ui.add(
                Input::new(&mut state.filter)
                    .placeholder("Filter…")
                    .width(260.0),
            );
            if state.filter != before {
                state.page = 0;
            }
            let width = ui.available_width();
            let check_w = 40.0;
            let col_w = (width - check_w) / self.header.len().max(1) as f32;
            let visible = self.visible_rows(state);
            ui.spacing_mut().item_spacing.y = 0.0;
            let frame = egui::Frame::new()
                .stroke(tokens.border_stroke())
                .corner_radius(tokens.control_radius())
                .show(ui, |ui| {
                    let (head, _) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
                    let mut all =
                        !visible.is_empty() && visible.iter().all(|i| state.selected.contains(i));
                    let mut header_check = ui.new_child(egui::UiBuilder::new().max_rect(
                        Rect::from_min_size(head.min + vec2(12.0, 12.0), Vec2::splat(16.0)),
                    ));
                    if header_check.add(Checkbox::new(&mut all)).changed() {
                        for i in &visible {
                            state.selected.retain(|s| s != i);
                            if all {
                                state.selected.push(*i);
                            }
                        }
                    }
                    for (c, title) in self.header.iter().enumerate() {
                        let cell = Rect::from_min_size(
                            pos2(head.left() + check_w + c as f32 * col_w, head.top()),
                            vec2(col_w, head.height()),
                        );
                        let r = ui.interact(cell, ui.id().with(("sort", c)), Sense::click());
                        let arrow = match state.sort {
                            Some((sc, SortOrder::Ascending)) if sc == c => " ↑",
                            Some((sc, SortOrder::Descending)) if sc == c => " ↓",
                            _ => " ⇅",
                        };
                        r.widget_info(|| {
                            WidgetInfo::labeled(
                                WidgetType::Button,
                                true,
                                format!("Sort by {title}"),
                            )
                        });
                        if r.clicked() {
                            state.sort = match state.sort {
                                Some((sc, SortOrder::Ascending)) if sc == c => {
                                    Some((c, SortOrder::Descending))
                                }
                                Some((sc, SortOrder::Descending)) if sc == c => None,
                                _ => Some((c, SortOrder::Ascending)),
                            };
                        }
                        let color = if r.hovered() {
                            tokens.foreground
                        } else {
                            tokens.muted_foreground
                        };
                        ui.painter().text(
                            cell.left_center() + vec2(8.0, 0.0),
                            Align2::LEFT_CENTER,
                            format!("{title}{arrow}"),
                            tokens.body_font(),
                            color,
                        );
                    }
                    ui.painter()
                        .hline(head.x_range(), head.bottom(), tokens.border_stroke());
                    if visible.is_empty() {
                        let (row, _) = ui.allocate_exact_size(vec2(width, 64.0), Sense::hover());
                        ui.painter().text(
                            row.center(),
                            Align2::CENTER_CENTER,
                            "No results.",
                            tokens.body_font(),
                            tokens.muted_foreground,
                        );
                    }
                    for (n, &index) in visible.iter().enumerate() {
                        let (row, r) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
                        let mut selected = state.selected.contains(&index);
                        if selected || r.hovered() {
                            ui.painter()
                                .rect_filled(row, 0.0, tokens.muted.gamma_multiply(0.5));
                        }
                        let mut check = ui.new_child(egui::UiBuilder::new().max_rect(
                            Rect::from_min_size(row.min + vec2(12.0, 14.0), Vec2::splat(16.0)),
                        ));
                        if check.add(Checkbox::new(&mut selected)).changed() {
                            state.selected.retain(|s| *s != index);
                            if selected {
                                state.selected.push(index);
                            }
                        }
                        for (c, text) in self.rows[index].iter().take(self.header.len()).enumerate()
                        {
                            let cell = Rect::from_min_size(
                                pos2(row.left() + check_w + c as f32 * col_w, row.top()),
                                vec2(col_w, row.height()),
                            );
                            ui.painter().with_clip_rect(cell).text(
                                cell.left_center() + vec2(8.0, 0.0),
                                Align2::LEFT_CENTER,
                                text,
                                tokens.body_font(),
                                tokens.foreground,
                            );
                        }
                        if n + 1 < visible.len() {
                            ui.painter()
                                .hline(row.x_range(), row.bottom(), tokens.border_stroke());
                        }
                    }
                })
                .response;
            ui.add_space(12.0);
            let total = self.filtered_len(state);
            let pages = total.div_ceil(self.page_size).max(1);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} of {} row(s) selected.",
                        state.selected.len(),
                        self.rows.len()
                    ))
                    .font(tokens.body_font())
                    .color(tokens.muted_foreground),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(Pagination::new(&mut state.page, pages));
                });
            });
            frame
        })
        .inner
    }
}

/// shadcn's InputOTP: a row of single-character slots for a one-time code.
///
/// Typing fills the slots left to right; Backspace clears the last one.
/// `separator_after` puts a dash after that many slots, for `123-456` codes.
#[must_use = "add it with `ui.add(otp)`"]
pub struct InputOtp<'a> {
    code: &'a mut String,
    length: usize,
    separator_after: Option<usize>,
    digits_only: bool,
}

impl<'a> InputOtp<'a> {
    /// A `length`-slot code input editing `code`.
    pub fn new(code: &'a mut String, length: usize) -> Self {
        Self {
            code,
            length,
            separator_after: None,
            digits_only: true,
        }
    }

    /// Puts a dash after `slots` slots.
    pub fn separator_after(mut self, slots: usize) -> Self {
        self.separator_after = Some(slots);
        self
    }

    /// Accepts letters as well as digits.
    pub fn alphanumeric(mut self) -> Self {
        self.digits_only = false;
        self
    }
}

impl Widget for InputOtp<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let slot = 40.0;
        let sep_w = if self.separator_after.is_some() {
            24.0
        } else {
            0.0
        };
        let (rect, mut response) = ui.allocate_exact_size(
            vec2(slot * self.length as f32 + sep_w, slot),
            Sense::click(),
        );
        if response.clicked() {
            response.request_focus();
        }
        let code_text = self.code.clone();
        response.widget_info(|| WidgetInfo::text_edit(true, "", &code_text, ""));
        if response.has_focus() {
            let mut changed = false;
            ui.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) | egui::Event::Paste(text) => {
                            for c in text.chars() {
                                let ok = if self.digits_only {
                                    c.is_ascii_digit()
                                } else {
                                    c.is_ascii_alphanumeric()
                                };
                                if ok && self.code.chars().count() < self.length {
                                    self.code.push(c);
                                    changed = true;
                                }
                            }
                        }
                        egui::Event::Key {
                            key: Key::Backspace,
                            pressed: true,
                            ..
                        } => {
                            changed |= self.code.pop().is_some();
                        }
                        _ => {}
                    }
                }
            });
            if changed {
                response.mark_changed();
            }
        }
        let chars: Vec<char> = self.code.chars().collect();
        let painter = ui.painter();
        let mut x = rect.left();
        for i in 0..self.length {
            let cell = Rect::from_min_size(pos2(x, rect.top()), Vec2::splat(slot));
            x += slot;
            let corners = CornerRadius {
                nw: if i == 0 || Some(i) == self.separator_after {
                    tokens.radius
                } else {
                    0
                },
                sw: if i == 0 || Some(i) == self.separator_after {
                    tokens.radius
                } else {
                    0
                },
                ne: if i + 1 == self.length || Some(i + 1) == self.separator_after {
                    tokens.radius
                } else {
                    0
                },
                se: if i + 1 == self.length || Some(i + 1) == self.separator_after {
                    tokens.radius
                } else {
                    0
                },
            };
            painter.rect_stroke(cell, corners, tokens.border_stroke(), StrokeKind::Inside);
            if let Some(c) = chars.get(i) {
                painter.text(
                    cell.center(),
                    Align2::CENTER_CENTER,
                    c.to_string(),
                    FontId::proportional(16.0),
                    tokens.foreground,
                );
            }
            let active = response.has_focus() && i == chars.len().min(self.length - 1);
            if active {
                painter.rect_stroke(cell, corners, tokens.ring_stroke(), StrokeKind::Inside);
                if chars.len() < self.length {
                    let blink = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
                    if blink {
                        painter.vline(
                            cell.center().x,
                            (cell.center().y - 8.0)..=(cell.center().y + 8.0),
                            Stroke::new(1.0, tokens.foreground),
                        );
                    }
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(500));
                }
            }
            if Some(i + 1) == self.separator_after {
                painter.text(
                    pos2(x + sep_w / 2.0, rect.center().y),
                    Align2::CENTER_CENTER,
                    "–",
                    tokens.body_font(),
                    tokens.muted_foreground,
                );
                x += sep_w;
            }
        }
        response
    }
}
