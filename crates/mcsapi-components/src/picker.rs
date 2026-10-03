//! Pickers: calendar, date picker, command palette, and combobox.

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, InnerResponse, Key, Popup, PopupCloseBehavior, Rect,
    Response, RichText, Sense, StrokeKind, TextEdit, Ui, Vec2, Widget, WidgetInfo, WidgetType,
    pos2, vec2,
};

use crate::menu::surface_frame;
use crate::{Tokens, paint_focus_ring};

/// A calendar date, without time zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    /// Full year, for example 2026.
    pub year: i32,
    /// Month, 1–12.
    pub month: u32,
    /// Day of the month, starting at 1.
    pub day: u32,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

impl Date {
    /// The date `year-month-day`, or `None` if it does not exist.
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month))
            .then_some(Self { year, month, day })
    }

    /// Day of the week, 0 for Sunday through 6 for Saturday.
    pub fn weekday(self) -> u32 {
        // Sakamoto's method.
        const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if self.month < 3 {
            self.year - 1
        } else {
            self.year
        };
        let w = y + y.div_euclid(4) - y.div_euclid(100)
            + y.div_euclid(400)
            + T[(self.month - 1) as usize]
            + self.day as i32;
        w.rem_euclid(7) as u32
    }

    /// The first day of the month `delta` months away.
    pub fn add_months(self, delta: i32) -> Self {
        let index = self.year * 12 + self.month as i32 - 1 + delta;
        Self {
            year: index.div_euclid(12),
            month: index.rem_euclid(12) as u32 + 1,
            day: 1,
        }
    }

    /// The month's English name.
    pub fn month_name(self) -> &'static str {
        MONTHS[(self.month - 1) as usize]
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}, {}", self.month_name(), self.day, self.year)
    }
}

/// Number of days in `month` (1–12) of `year`.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// shadcn's Calendar: a month grid with previous / next navigation.
#[must_use = "add it with `ui.add(calendar)`"]
pub struct Calendar<'a> {
    selected: &'a mut Option<Date>,
    month: Date,
    today: Option<Date>,
}

impl<'a> Calendar<'a> {
    /// A calendar bound to `selected`, opening on `month` (any day in it).
    ///
    /// The shown month is remembered in egui memory after the first frame.
    pub fn new(selected: &'a mut Option<Date>, month: Date) -> Self {
        Self {
            selected,
            month,
            today: None,
        }
    }

    /// Outlines `today` in the grid.
    pub fn today(mut self, today: Date) -> Self {
        self.today = Some(today);
        self
    }
}

impl Widget for Calendar<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let cell = 36.0;
        let size = vec2(cell * 7.0 + 24.0, cell * 8.0 + 24.0);
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::hover());
        let id = response.id;
        let shown_id = id.with("month");
        let mut shown: Date = ui
            .data(|d| d.get_temp(shown_id))
            .unwrap_or(self.month.add_months(0));
        let inner = rect.shrink(12.0);
        let painter = ui.painter();
        painter.rect_stroke(
            rect,
            tokens.control_radius(),
            tokens.border_stroke(),
            StrokeKind::Inside,
        );

        // Header: month name between two arrow buttons.
        let header = Rect::from_min_size(inner.min, vec2(inner.width(), cell));
        painter.text(
            header.center(),
            Align2::CENTER_CENTER,
            format!("{} {}", shown.month_name(), shown.year),
            tokens.body_font(),
            tokens.foreground,
        );
        for (dir, at, glyph) in [(-1, header.left(), "‹"), (1, header.right() - 28.0, "›")] {
            let button = Rect::from_min_size(pos2(at, header.top() + 4.0), Vec2::splat(28.0));
            let r = ui.interact(button, id.with(("nav", dir)), Sense::click());
            r.widget_info(|| {
                WidgetInfo::labeled(
                    WidgetType::Button,
                    true,
                    if dir < 0 {
                        "Previous month"
                    } else {
                        "Next month"
                    },
                )
            });
            if r.clicked() {
                shown = shown.add_months(dir);
            }
            let painter = ui.painter();
            painter.rect_stroke(
                button,
                tokens.control_radius(),
                tokens.border_stroke(),
                StrokeKind::Inside,
            );
            if r.hovered() {
                painter.rect_filled(button, tokens.control_radius(), tokens.muted);
            }
            painter.text(
                button.center(),
                Align2::CENTER_CENTER,
                glyph,
                tokens.body_font(),
                tokens.foreground,
            );
        }

        for (i, name) in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
            .iter()
            .enumerate()
        {
            let center = pos2(
                inner.left() + cell * (i as f32 + 0.5),
                header.bottom() + cell * 0.5,
            );
            ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                *name,
                tokens.small_font(),
                tokens.muted_foreground,
            );
        }
        let first = Date { day: 1, ..shown };
        let offset = first.weekday();
        for day in 1..=days_in_month(shown.year, shown.month) {
            let slot = offset + day - 1;
            let (col, row) = (slot % 7, slot / 7);
            let cell_rect = Rect::from_min_size(
                pos2(
                    inner.left() + col as f32 * cell,
                    header.bottom() + cell * (row as f32 + 1.0),
                ),
                Vec2::splat(cell),
            )
            .shrink(2.0);
            let date = Date { day, ..shown };
            let r = ui.interact(cell_rect, id.with(("day", day)), Sense::click());
            let selected = *self.selected == Some(date);
            r.widget_info(|| {
                WidgetInfo::selected(WidgetType::Button, true, selected, date.to_string())
            });
            if r.clicked() {
                *self.selected = Some(date);
                response.mark_changed();
            }
            let selected = *self.selected == Some(date);
            let painter = ui.painter();
            let (fill, text) = if selected {
                (tokens.primary, tokens.primary_foreground)
            } else if r.hovered() {
                (tokens.muted, tokens.foreground)
            } else {
                (Color32::TRANSPARENT, tokens.foreground)
            };
            painter.rect_filled(cell_rect, tokens.control_radius(), fill);
            if self.today == Some(date) && !selected {
                painter.rect_filled(cell_rect, tokens.control_radius(), tokens.muted);
            }
            painter.text(
                cell_rect.center(),
                Align2::CENTER_CENTER,
                day.to_string(),
                tokens.body_font(),
                text,
            );
            paint_focus_ring(ui, &r, cell_rect, tokens.control_radius());
        }
        ui.data_mut(|d| d.insert_temp(shown_id, shown));
        response
    }
}

/// shadcn's DatePicker: an outline button showing the date that opens a
/// [`Calendar`] in a popover.
#[must_use = "add it with `ui.add(picker)`"]
pub struct DatePicker<'a> {
    selected: &'a mut Option<Date>,
    month: Date,
    placeholder: String,
}

impl<'a> DatePicker<'a> {
    /// A picker bound to `selected`; the calendar opens on `month` when nothing is selected.
    pub fn new(selected: &'a mut Option<Date>, month: Date) -> Self {
        Self {
            selected,
            month,
            placeholder: "Pick a date".to_owned(),
        }
    }

    /// Sets the text shown while no date is selected.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
}

impl Widget for DatePicker<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (text, color) = match self.selected {
            Some(date) => (date.to_string(), tokens.foreground),
            None => (self.placeholder.clone(), tokens.muted_foreground),
        };
        let (rect, mut response) = ui.allocate_exact_size(vec2(240.0, 36.0), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::ComboBox, true, &text));
        let painter = ui.painter();
        painter.rect_stroke(
            rect,
            tokens.control_radius(),
            tokens.border_stroke(),
            StrokeKind::Inside,
        );
        if response.hovered() {
            painter.rect_filled(
                rect,
                tokens.control_radius(),
                tokens.muted.gamma_multiply(0.5),
            );
        }
        painter.text(
            rect.left_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            "▦",
            tokens.body_font(),
            tokens.muted_foreground,
        );
        painter.text(
            rect.left_center() + vec2(32.0, 0.0),
            Align2::LEFT_CENTER,
            text,
            tokens.body_font(),
            color,
        );
        paint_focus_ring(ui, &response, rect, tokens.control_radius());
        let month = self.selected.unwrap_or(self.month);
        let before = *self.selected;
        Popup::from_toggle_button_response(&response)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .gap(4.0)
            .frame(surface_frame(&tokens, 0))
            .show(|ui| {
                if ui.add(Calendar::new(self.selected, month)).changed() {
                    ui.close();
                }
            });
        if *self.selected != before {
            response.mark_changed();
        }
        response
    }
}

/// Whether every character of `query` appears in `text` in order, ignoring case.
pub fn fuzzy_match(text: &str, query: &str) -> bool {
    let mut chars = text.chars().flat_map(char::to_lowercase);
    query
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace())
        .all(|q| chars.any(|c| c == q))
}

/// shadcn's Command: a search box over a filtered, keyboard-navigable list.
///
/// Items are `(group, label)` pairs; consecutive items with the same group
/// are listed under one heading. Returns the chosen item's index in
/// [`InnerResponse::inner`] on the frame it is chosen.
pub struct Command<'a, G: AsRef<str>, L: AsRef<str>> {
    id: Id,
    items: &'a [(G, L)],
    placeholder: String,
    max_height: f32,
}

#[derive(Clone, Default)]
struct CommandState {
    query: String,
    highlight: usize,
}

impl<'a, G: AsRef<str>, L: AsRef<str>> Command<'a, G, L> {
    /// A command list over `items`. `id_salt` keeps its query between frames.
    pub fn new(id_salt: impl egui::AsId, items: &'a [(G, L)]) -> Self {
        Self {
            id: Id::new(id_salt),
            items,
            placeholder: "Type a command or search…".to_owned(),
            max_height: 300.0,
        }
    }

    /// Sets the search box hint.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Draws the command list.
    pub fn show(self, ui: &mut Ui) -> InnerResponse<Option<usize>> {
        let tokens = Tokens::current(ui.ctx());
        let mut state: CommandState = ui.data(|d| d.get_temp(self.id)).unwrap_or_default();
        let mut chosen = None;
        let response = ui
            .vertical(|ui| {
                ui.set_min_width(280.0);
                let edit = ui.add(
                    TextEdit::singleline(&mut state.query)
                        .id(self.id.with("query"))
                        .hint_text(RichText::new(&self.placeholder).color(tokens.muted_foreground))
                        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 10)))
                        .text_color(tokens.foreground)
                        .desired_width(f32::INFINITY),
                );
                if edit.changed() {
                    state.highlight = 0;
                }
                let (rule, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().rect_filled(rule, 0.0, tokens.border);
                let matches: Vec<usize> = (0..self.items.len())
                    .filter(|&i| fuzzy_match(self.items[i].1.as_ref(), &state.query))
                    .collect();
                if !matches.is_empty() {
                    state.highlight = state.highlight.min(matches.len() - 1);
                }
                ui.input(|i| {
                    if i.key_pressed(Key::ArrowDown) && !matches.is_empty() {
                        state.highlight = (state.highlight + 1) % matches.len();
                    }
                    if i.key_pressed(Key::ArrowUp) && !matches.is_empty() {
                        state.highlight = (state.highlight + matches.len() - 1) % matches.len();
                    }
                    if i.key_pressed(Key::Enter) {
                        chosen = matches.get(state.highlight).copied();
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt(self.id.with("scroll"))
                    .max_height(self.max_height)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if matches.is_empty() {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("No results found.")
                                        .font(tokens.body_font())
                                        .color(tokens.muted_foreground),
                                );
                            });
                            ui.add_space(20.0);
                        }
                        let mut last_group: Option<&str> = None;
                        for (row, &index) in matches.iter().enumerate() {
                            let (group, label) = (&self.items[index].0, &self.items[index].1);
                            if last_group != Some(group.as_ref()) {
                                if last_group.is_some() {
                                    let (rule, _) = ui.allocate_exact_size(
                                        vec2(ui.available_width(), 9.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().hline(
                                        rule.x_range(),
                                        rule.center().y,
                                        tokens.border_stroke(),
                                    );
                                }
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(group.as_ref())
                                        .font(tokens.small_font())
                                        .color(tokens.muted_foreground),
                                );
                                last_group = Some(group.as_ref());
                            }
                            let (rect, r) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 32.0),
                                Sense::click(),
                            );
                            r.widget_info(|| {
                                WidgetInfo::labeled(WidgetType::Button, true, label.as_ref())
                            });
                            if r.hovered() {
                                state.highlight = row;
                            }
                            if r.clicked() {
                                chosen = Some(index);
                            }
                            let painter = ui.painter();
                            if row == state.highlight {
                                painter.rect_filled(rect, CornerRadius::same(4), tokens.muted);
                            }
                            painter.text(
                                rect.left_center() + vec2(8.0, 0.0),
                                Align2::LEFT_CENTER,
                                label.as_ref(),
                                tokens.body_font(),
                                tokens.foreground,
                            );
                        }
                    });
            })
            .response;
        if chosen.is_some() {
            state = CommandState::default();
        }
        ui.data_mut(|d| d.insert_temp(self.id, state));
        InnerResponse::new(chosen, response)
    }
}

/// shadcn's Combobox: a button that opens a searchable [`Command`] list.
#[must_use = "add it with `ui.add(combobox)`"]
pub struct Combobox<'a, T: AsRef<str>> {
    id: Id,
    selected: &'a mut Option<usize>,
    options: &'a [T],
    placeholder: String,
    width: f32,
}

impl<'a, T: AsRef<str>> Combobox<'a, T> {
    /// A combobox over `options` with the chosen index in `selected`.
    pub fn new(
        id_salt: impl egui::AsId,
        selected: &'a mut Option<usize>,
        options: &'a [T],
    ) -> Self {
        Self {
            id: Id::new(id_salt),
            selected,
            options,
            placeholder: "Select…".to_owned(),
            width: 200.0,
        }
    }

    /// Sets the text shown while nothing is selected.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Sets the trigger width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

impl<T: AsRef<str>> Widget for Combobox<'_, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let current = self.selected.and_then(|i| self.options.get(i));
        let (text, color) = match current {
            Some(option) => (option.as_ref().to_owned(), tokens.foreground),
            None => (self.placeholder.clone(), tokens.muted_foreground),
        };
        let (rect, mut response) = ui.allocate_exact_size(vec2(self.width, 36.0), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::ComboBox, true, &text));
        let painter = ui.painter();
        painter.rect_stroke(
            rect,
            tokens.control_radius(),
            tokens.border_stroke(),
            StrokeKind::Inside,
        );
        painter.text(
            rect.left_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            text,
            tokens.body_font(),
            color,
        );
        painter.text(
            rect.right_center() - vec2(12.0, 0.0),
            Align2::RIGHT_CENTER,
            "⇕",
            FontId::proportional(12.0),
            tokens.muted_foreground,
        );
        paint_focus_ring(ui, &response, rect, tokens.control_radius());
        let items: Vec<(&str, &str)> = self.options.iter().map(|o| ("", o.as_ref())).collect();
        let mut chosen = None;
        Popup::from_toggle_button_response(&response)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .gap(4.0)
            .width(self.width)
            .frame(surface_frame(&tokens, 0))
            .show(|ui| {
                chosen = Command::new(self.id, &items)
                    .placeholder("Search…")
                    .show(ui)
                    .inner;
                if chosen.is_some() {
                    ui.close();
                }
            });
        if let Some(index) = chosen {
            // Choosing the current option again clears it, as in shadcn's example.
            *self.selected = if *self.selected == Some(index) {
                None
            } else {
                Some(index)
            };
            response.mark_changed();
        }
        response
    }
}
