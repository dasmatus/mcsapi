//! The gallery drawn with GPUI and `mcsapi-components-gpui`.
//!
//! [`GalleryView`] lists the same [`Specimen`]s as the egui [`Gallery`](crate::Gallery);
//! `RENDERERS` maps each specimen name to a function that draws its GPUI
//! version. The crate's tests fail while a specimen has no renderer.

use gpui::{
    AnyElement, App, Bounds, Context, Div, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, TitlebarOptions, Window, WindowBounds, WindowOptions, div, prelude::*,
    px, size,
};
use mcsapi::{Desktop, WindowId, WorkspaceId, widgets::gpui_workspace_bar};
use mcsapi_components_gpui::{
    Alert, AlertDialog, AlertDialogAction, AlertVariant, AspectRatio, Avatar, Badge, BadgeVariant,
    Button, ButtonSize, ButtonVariant, Card, Checkbox, Collapsible, Dialog, Empty, Input, Kbd,
    Label, Pagination, Progress, RadioGroup, Select, Separator, Skeleton, Slider, Spinner, Switch,
    Table, Tabs, TextInput, Textarea, Toaster, Toggle, ToggleGroup, Tokens, accordion_item,
    blockquote, breadcrumb, page_window, toast, toasts, tooltip, typography,
};
use mcsapi_ui::Theme;

use crate::{Category, PRESETS, Specimen, find, specimens};

type Renderer = fn(&mut GalleryView, &Tokens, &mut Window, &mut Context<GalleryView>) -> Div;

/// Each specimen's GPUI renderer, by specimen name.
pub(crate) const RENDERERS: &[(&str, Renderer)] = &[
    ("Button", button),
    ("Badge", badge),
    ("Toggle", toggle),
    ("Toggle Group", toggle_group),
    ("Kbd", kbd),
    ("Input", input),
    ("Textarea", textarea),
    ("Label", label),
    ("Checkbox", checkbox),
    ("Switch", switch),
    ("Radio Group", radio_group),
    ("Slider", slider),
    ("Select", select),
    ("Card", card),
    ("Alert", alert),
    ("Avatar", avatar),
    ("Separator", separator),
    ("Progress", progress),
    ("Spinner", spinner),
    ("Skeleton", skeleton),
    ("Empty", empty),
    ("Aspect Ratio", aspect_ratio),
    ("Typography", type_scale),
    ("Tabs", tabs),
    ("Breadcrumb", breadcrumbs),
    ("Pagination", pagination),
    ("Collapsible", collapsible),
    ("Accordion", accordion),
    ("Table", table),
    ("Dialog", dialog),
    ("Alert Dialog", alert_dialog),
    ("Tooltip", tooltips),
    ("Toast", toast_demo),
    ("Workspace Bar", workspace_bar),
];

/// Where the window starts.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Theme preset name; the desktop theme when `None`.
    pub preset: Option<String>,
    /// Initial search text.
    pub search: Option<String>,
    /// Specimen to show alone.
    pub selected: Option<String>,
}

/// Opens the gallery in a GPUI window and runs until it closes.
pub fn run(options: Options) {
    gpui_platform::application().run(move |cx: &mut App| {
        mcsapi_components_gpui::bind_text_input_keys(cx);
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("mcsapi widget gallery".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| GalleryView::new(&options, window, cx)),
        )
        .expect("the gallery window opens");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}

/// The gallery's root view.
pub struct GalleryView {
    preset: usize,
    selected: Option<&'static str>,
    search: Entity<TextInput>,
    // Actions
    bold: bool,
    italic: bool,
    underline: bool,
    align: Option<usize>,
    view: Option<usize>,
    // Forms
    name: Entity<TextInput>,
    password: Entity<TextInput>,
    read_only: Entity<TextInput>,
    message: Entity<TextInput>,
    read_only_area: Entity<TextInput>,
    email: Entity<TextInput>,
    terms: bool,
    updates: bool,
    airplane: bool,
    wifi: bool,
    density: usize,
    volume: f32,
    step: f32,
    fruit: Option<usize>,
    timezone: Option<usize>,
    // Navigation
    tab: usize,
    crumb: Option<usize>,
    short: usize,
    long: usize,
    // Overlays
    profile_open: bool,
    profile_name: Entity<TextInput>,
    delete_open: bool,
    publish_open: bool,
    answer: Option<String>,
    // Shell
    desktop: Desktop,
}

fn text(
    cx: &mut Context<GalleryView>,
    build: impl FnOnce(TextInput) -> TextInput,
) -> Entity<TextInput> {
    let input = cx.new(|cx| build(TextInput::new(cx)));
    cx.observe(&input, |_, _, cx| cx.notify()).detach();
    input
}

fn sample_desktop(workspaces: u64) -> Desktop {
    let mut desktop = Desktop::new((1..=workspaces).filter_map(WorkspaceId::new))
        .expect("workspace IDs are distinct and nonzero");
    for window in (1..=3).filter_map(WindowId::new) {
        desktop.insert(window).expect("window IDs are distinct");
    }
    desktop
}

impl GalleryView {
    fn new(options: &Options, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let preset = options
            .preset
            .as_deref()
            .and_then(|name| {
                PRESETS
                    .iter()
                    .position(|preset| preset.name.eq_ignore_ascii_case(name))
            })
            .unwrap_or(0);
        let search_text = options.search.clone().unwrap_or_default();
        let read_only = text(cx, |input| input.with_text("Read only"));
        read_only.update(cx, |input, _| input.set_disabled(true));
        let read_only_area = text(cx, |input| input.multiline(true).with_text("Read only"));
        read_only_area.update(cx, |input, _| input.set_disabled(true));
        let view = Self {
            preset,
            selected: options.selected.as_deref().and_then(find).map(|s| s.name),
            search: text(cx, |input| {
                input.placeholder("Search widgets").with_text(search_text)
            }),
            bold: false,
            italic: false,
            underline: false,
            align: None,
            view: Some(1),
            name: text(cx, |input| input.placeholder("Your name")),
            password: text(cx, |input| {
                input.masked(true).placeholder("Type a password")
            }),
            read_only,
            message: text(cx, |input| {
                input.multiline(true).placeholder("Type your message here.")
            }),
            read_only_area,
            email: text(cx, |input| input.placeholder("m@example.com")),
            terms: false,
            updates: true,
            airplane: false,
            wifi: true,
            density: 1,
            volume: 0.6,
            step: 40.0,
            fruit: None,
            timezone: Some(2),
            tab: 0,
            crumb: None,
            short: 0,
            long: 0,
            profile_open: false,
            profile_name: text(cx, |input| input.with_text("Pedro Duarte")),
            delete_open: false,
            publish_open: false,
            answer: None,
            desktop: sample_desktop(4),
        };
        view.install_theme(cx);
        view
    }

    fn theme(&self) -> Theme {
        PRESETS[self.preset].theme.unwrap_or_default()
    }

    fn install_theme(&self, cx: &mut App) {
        Tokens::from_theme(&self.theme()).install(cx);
    }

    fn sidebar(&mut self, t: &Tokens, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let query = self.search.read(cx).text().to_owned();
        let mut sidebar = div()
            .id("sidebar")
            .flex()
            .flex_col()
            .flex_none()
            .overflow_y_scroll()
            .w(px(200.0))
            .pr(px(16.0))
            .gap(px(2.0))
            .child(Input::new(&self.search))
            .child(div().h(px(8.0)))
            .child(nav_button(
                "All widgets",
                self.selected.is_none(),
                t,
                cx.listener(|this, _, _, cx| {
                    this.selected = None;
                    cx.notify();
                }),
            ));
        for category in Category::ALL {
            let items: Vec<&'static Specimen> = specimens()
                .filter(|s| s.category == category && s.matches(&query))
                .collect();
            if items.is_empty() {
                continue;
            }
            sidebar = sidebar.child(
                div()
                    .flex_none()
                    .pt(px(12.0))
                    .pb(px(4.0))
                    .text_size(px(12.0))
                    .text_color(t.muted_foreground)
                    .child(category.name().to_uppercase()),
            );
            for specimen in items {
                let name = specimen.name;
                sidebar = sidebar.child(nav_button(
                    name,
                    self.selected == Some(name),
                    t,
                    cx.listener(move |this, _, _, cx| {
                        this.selected = Some(name);
                        cx.notify();
                    }),
                ));
            }
        }
        sidebar
    }

    fn specimen(
        &mut self,
        specimen: &'static Specimen,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let render = RENDERERS
            .iter()
            .find(|(name, _)| *name == specimen.name)
            .map(|(_, render)| *render);
        div()
            .id(specimen.name)
            .flex()
            .flex_col()
            .gap(px(4.0))
            .p(px(20.0))
            .border_1()
            .border_color(t.border)
            .rounded(t.card_radius())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(specimen.name),
                    )
                    .child(Badge::new(specimen.source).variant(BadgeVariant::Outline)),
            )
            .child(typography::muted(t, specimen.summary))
            .child(
                div().flex().flex_wrap().gap(px(6.0)).children(
                    specimen
                        .api
                        .iter()
                        .map(|item| typography::inline_code(t, *item)),
                ),
            )
            .child(div().h(px(12.0)))
            .children(render.map(|render| render(self, t, window, cx)))
    }
}

fn nav_button(
    text: &'static str,
    selected: bool,
    tokens: &Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let hover = tokens.hover;
    div()
        .id(text)
        .flex_none()
        .w_full()
        .h(px(32.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(tokens.radius)
        .cursor_pointer()
        .text_color(tokens.foreground)
        .when(selected, |item| item.bg(tokens.secondary))
        .hover(move |style| style.bg(hover))
        .child(text)
        .on_click(on_click)
}

impl Render for GalleryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Tokens::get(cx);
        let query = self.search.read(cx).text().to_owned();
        let shown: Vec<&'static Specimen> = match self.selected.and_then(find) {
            Some(specimen) => vec![specimen],
            None => specimens().filter(|s| s.matches(&query)).collect(),
        };
        let names = PRESETS.map(|preset| preset.name);
        let header = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .pb(px(12.0))
            .border_b_1()
            .border_color(t.border)
            .child(
                div()
                    .text_size(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("mcsapi widget gallery"),
            )
            .child(typography::muted(
                &t,
                format!("{} widgets", specimens().count()),
            ))
            .child(div().flex_1())
            .child(typography::muted(&t, "Theme"))
            .child(
                Select::new("theme", names, Some(self.preset))
                    .width(140.0)
                    .on_change(cx.listener(|this, preset: &Option<usize>, _, cx| {
                        this.preset = preset.unwrap_or(0);
                        this.install_theme(cx);
                        cx.notify();
                    })),
            );
        let sidebar = self.sidebar(&t, cx);
        let mut content = div()
            .id("content")
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(16.0))
            .pr(px(8.0))
            .overflow_y_scroll();
        if shown.is_empty() {
            content = content.child(
                Empty::new("No widgets match")
                    .description("Try another name, or clear the search."),
            );
        }
        for specimen in shown {
            content = content.child(self.specimen(specimen, &t, window, cx));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .p(px(16.0))
            .gap(px(12.0))
            .bg(t.background)
            .text_color(t.foreground)
            .text_size(px(14.0))
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar.border_r_1().border_color(t.border))
                    .child(div().w(px(16.0)))
                    .child(content),
            )
            .child(Toaster)
    }
}

/// One labeled row of a specimen: a muted caption, then the items.
fn row(
    t: &Tokens,
    caption: impl Into<SharedString>,
    items: impl IntoIterator<Item = AnyElement>,
) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(8.0))
        .child(
            div()
                .flex_none()
                .w(px(120.0))
                .min_h(px(36.0))
                .flex()
                .items_center()
                .text_size(px(12.0))
                .text_color(t.muted_foreground)
                .child(caption.into()),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .min_h(px(36.0))
                .children(items),
        )
}

fn rows(items: impl IntoIterator<Item = Div>) -> Div {
    div().flex().flex_col().gap(px(8.0)).children(items)
}

fn any(element: impl IntoElement) -> AnyElement {
    element.into_any_element()
}

const VARIANTS: [(ButtonVariant, &str); 6] = [
    (ButtonVariant::Default, "Default"),
    (ButtonVariant::Secondary, "Secondary"),
    (ButtonVariant::Destructive, "Destructive"),
    (ButtonVariant::Outline, "Outline"),
    (ButtonVariant::Ghost, "Ghost"),
    (ButtonVariant::Link, "Link"),
];

fn button(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "Variants",
            VARIANTS.map(|(variant, name)| {
                any(Button::new(name)
                    .variant(variant)
                    .on_click(move |_, _, cx| toast(cx, format!("{name} button clicked"), None)))
            }),
        ),
        row(
            t,
            "Sizes",
            [
                any(Button::new("Small").size(ButtonSize::Sm)),
                any(Button::new("Default")),
                any(Button::new("Large").size(ButtonSize::Lg)),
                any(Button::new("+")
                    .size(ButtonSize::Icon)
                    .variant(ButtonVariant::Outline)),
            ],
        ),
        row(
            t,
            "Disabled",
            VARIANTS.map(|(variant, name)| {
                any(Button::new(name)
                    .id(SharedString::from(format!("disabled-{name}")))
                    .variant(variant)
                    .enabled(false))
            }),
        ),
    ])
}

fn badge(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "Variants",
            [
                any(Badge::new("Default")),
                any(Badge::new("Secondary").variant(BadgeVariant::Secondary)),
                any(Badge::new("Destructive").variant(BadgeVariant::Destructive)),
                any(Badge::new("Outline").variant(BadgeVariant::Outline)),
            ],
        ),
        row(
            t,
            "In context",
            [
                any("Inbox"),
                any(Badge::new("12")),
                any("Build"),
                any(Badge::new("failing").variant(BadgeVariant::Destructive)),
            ],
        ),
    ])
}

fn toggle(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(
            t,
            "Interactive",
            [
                any(Toggle::new("B", view.bold).on_toggle(cx.listener(
                    |this, on: &bool, _, cx| {
                        this.bold = *on;
                        cx.notify();
                    },
                ))),
                any(Toggle::new("I", view.italic).on_toggle(cx.listener(
                    |this, on: &bool, _, cx| {
                        this.italic = *on;
                        cx.notify();
                    },
                ))),
                any(Toggle::new("U", view.underline).on_toggle(cx.listener(
                    |this, on: &bool, _, cx| {
                        this.underline = *on;
                        cx.notify();
                    },
                ))),
            ],
        ),
        row(
            t,
            "States",
            [any(Toggle::new("Off", false)), any(Toggle::new("On", true))],
        ),
        row(
            t,
            "Disabled",
            [
                any(Toggle::new("Off", false).id("off-disabled").enabled(false)),
                any(Toggle::new("On", true).id("on-disabled").enabled(false)),
            ],
        ),
    ])
}

fn toggle_group(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(
            t,
            "Text alignment",
            [any(ToggleGroup::new(
                "align",
                ["Left", "Center", "Right"],
                view.align,
            )
            .on_select(cx.listener(
                |this, selected: &Option<usize>, _, cx| {
                    this.align = *selected;
                    cx.notify();
                },
            )))],
        ),
        row(
            t,
            "Preselected",
            [any(ToggleGroup::new(
                "view",
                ["List", "Grid", "Columns"],
                view.view,
            )
            .on_select(cx.listener(
                |this, selected: &Option<usize>, _, cx| {
                    this.view = *selected;
                    cx.notify();
                },
            )))],
        ),
        row(
            t,
            "Disabled",
            [any(ToggleGroup::new(
                "range",
                ["Day", "Week", "Month"],
                Some(0),
            )
            .enabled(false))],
        ),
    ])
}

fn kbd(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "Keys",
            ["Esc", "Tab", "Enter", "Space", "←", "→"].map(|key| any(Kbd::new(key))),
        ),
        row(
            t,
            "Shortcut",
            [
                any(Kbd::new("Super")),
                any("+"),
                any(Kbd::new("Shift")),
                any("+"),
                any(Kbd::new("Q")),
            ],
        ),
    ])
}

fn input(view: &mut GalleryView, t: &Tokens, _: &mut Window, cx: &mut Context<GalleryView>) -> Div {
    let revealed = !view.password.read(cx).is_masked();
    rows([
        row(t, "Placeholder", [any(Input::new(&view.name).width(240.0))]),
        row(
            t,
            "Password",
            [
                any(Input::new(&view.password).width(240.0)),
                any(Checkbox::new("reveal", revealed)
                    .label("Show")
                    .on_change(cx.listener(|this, on: &bool, _, cx| {
                        this.password
                            .update(cx, |input, cx| input.set_masked(!*on, cx));
                    }))),
            ],
        ),
        row(
            t,
            "Disabled",
            [any(Input::new(&view.read_only).width(240.0))],
        ),
    ])
}

fn textarea(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    _: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(
            t,
            "Placeholder",
            [any(Textarea::new(&view.message).rows(3))],
        ),
        row(
            t,
            "Disabled",
            [any(Textarea::new(&view.read_only_area).rows(2))],
        ),
    ])
}

fn label(view: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([row(
        t,
        "With a field",
        [any(div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(Label::new("Email"))
            .child(Input::new(&view.email).width(240.0)))],
    )])
}

fn checkbox(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(
            t,
            "Interactive",
            [
                any(Checkbox::new("terms", view.terms)
                    .label("Accept terms and conditions")
                    .on_change(cx.listener(|this, on: &bool, _, cx| {
                        this.terms = *on;
                        cx.notify();
                    }))),
                any(Checkbox::new("updates", view.updates)
                    .label("Email me about updates")
                    .on_change(cx.listener(|this, on: &bool, _, cx| {
                        this.updates = *on;
                        cx.notify();
                    }))),
            ],
        ),
        row(
            t,
            "States",
            [
                any(Checkbox::new("unchecked", false).label("Unchecked")),
                any(Checkbox::new("checked", true).label("Checked")),
                any(Checkbox::new("bare", true)),
            ],
        ),
        row(
            t,
            "Disabled",
            [
                any(Checkbox::new("off", false)
                    .label("Unchecked")
                    .enabled(false)),
                any(Checkbox::new("on", true).label("Checked").enabled(false)),
            ],
        ),
    ])
}

fn switch(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(
            t,
            "Interactive",
            [
                any(Switch::new("airplane", view.airplane)
                    .label("Airplane mode")
                    .on_change(cx.listener(|this, on: &bool, _, cx| {
                        this.airplane = *on;
                        cx.notify();
                    }))),
                any(Switch::new("wifi", view.wifi)
                    .label("Wi-Fi")
                    .on_change(cx.listener(|this, on: &bool, _, cx| {
                        this.wifi = *on;
                        cx.notify();
                    }))),
            ],
        ),
        row(
            t,
            "States",
            [
                any(Switch::new("off", false).label("Off")),
                any(Switch::new("on", true).label("On")),
                any(Switch::new("bare", true)),
            ],
        ),
        row(
            t,
            "Disabled",
            [
                any(Switch::new("off-disabled", false)
                    .label("Off")
                    .enabled(false)),
                any(Switch::new("on-disabled", true).label("On").enabled(false)),
            ],
        ),
    ])
}

fn radio_group(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(
            t,
            "Interactive",
            [any(RadioGroup::new(
                "density",
                ["Default", "Comfortable", "Compact"],
                view.density,
            )
            .on_change(cx.listener(|this, index: &usize, _, cx| {
                this.density = *index;
                cx.notify();
            })))],
        ),
        row(
            t,
            "Disabled",
            [any(
                RadioGroup::new("disabled", ["Yes", "No"], 0).enabled(false)
            )],
        ),
    ])
}

fn slider(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(
            t,
            "Continuous",
            [
                any(Slider::new("volume", view.volume, 0.0..=1.0)
                    .width(240.0)
                    .on_change(cx.listener(|this, value: &f32, _, cx| {
                        this.volume = *value;
                        cx.notify();
                    }))),
                any(format!("{:.0}%", view.volume * 100.0)),
            ],
        ),
        row(
            t,
            "Stepped by 10",
            [
                any(Slider::new("step", view.step, 0.0..=100.0)
                    .step(10.0)
                    .width(240.0)
                    .on_change(cx.listener(|this, value: &f32, _, cx| {
                        this.step = *value;
                        cx.notify();
                    }))),
                any(format!("{:.0}", view.step)),
            ],
        ),
        row(
            t,
            "Disabled",
            [any(Slider::new("disabled", 0.3, 0.0..=1.0)
                .width(240.0)
                .enabled(false))],
        ),
    ])
}

const FRUITS: [&str; 5] = ["Apple", "Banana", "Blueberry", "Grapes", "Pineapple"];
const TIMEZONES: [&str; 4] = ["UTC", "Europe/London", "Europe/Prague", "America/New_York"];

fn select(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(
            t,
            "Placeholder",
            [any(Select::new("fruit", FRUITS, view.fruit)
                .placeholder("Select a fruit")
                .on_change(cx.listener(
                    |this, index: &Option<usize>, _, cx| {
                        this.fruit = *index;
                        cx.notify();
                    },
                )))],
        ),
        row(
            t,
            "Selected",
            [any(Select::new("timezone", TIMEZONES, view.timezone)
                .width(220.0)
                .on_change(cx.listener(
                    |this, index: &Option<usize>, _, cx| {
                        this.timezone = *index;
                        cx.notify();
                    },
                )))],
        ),
        row(
            t,
            "Disabled",
            [any(Select::new("disabled", FRUITS, Some(0)).enabled(false))],
        ),
    ])
}

fn card(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "Header and body",
            [any(div().w(px(360.0)).child(
                Card::new()
                    .title("Create project")
                    .description("Deploy your new project in one click.")
                    .child("Name and framework go here.")
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(Button::new("Deploy"))
                            .child(Button::new("Cancel").variant(ButtonVariant::Outline)),
                    ),
            ))],
        ),
        row(
            t,
            "Body only",
            [any(div()
                .w(px(360.0))
                .child(Card::new().child("A card with no header.")))],
        ),
    ])
}

fn alert(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    let sized = |alert: Alert| any(div().w(px(480.0)).child(alert));
    rows([
        row(
            t,
            "Default",
            [sized(Alert::new("Heads up!").description(
                "You can add components to your app using the gallery.",
            ))],
        ),
        row(
            t,
            "Destructive",
            [sized(
                Alert::new("Error")
                    .description("Your session has expired. Please log in again.")
                    .variant(AlertVariant::Destructive),
            )],
        ),
        row(t, "Title only", [sized(Alert::new("Saved."))]),
    ])
}

fn avatar(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "Initials",
            ["Matus", "Ada Lovelace", "Grace Brewster Hopper", ""]
                .map(|name| any(Avatar::new(name))),
        ),
        row(
            t,
            "Sizes",
            [24.0, 32.0, 40.0, 56.0].map(|size| any(Avatar::new("Linus Torvalds").size(size))),
        ),
    ])
}

fn separator(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "Horizontal",
            [any(div()
                .w(px(320.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child("Radix Primitives"),
                )
                .child(typography::muted(t, "An open-source UI component library."))
                .child(Separator::horizontal()))],
        ),
        row(
            t,
            "Vertical",
            [any(div()
                .flex()
                .h(px(20.0))
                .gap(px(12.0))
                .child("Blog")
                .child(Separator::vertical())
                .child("Docs")
                .child(Separator::vertical())
                .child("Source"))],
        ),
    ])
}

fn progress(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([0.0f32, 0.33, 0.66, 1.0].map(|value| {
        row(
            t,
            format!("{:.0}%", value * 100.0),
            [any(Progress::new(value).width(320.0))],
        )
    }))
}

fn spinner(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "Sizes",
            [12.0, 16.0, 24.0, 32.0].map(|size| any(Spinner::new().size(size))),
        ),
        row(
            t,
            "In a button",
            [
                any(Spinner::new()),
                any(Button::new("Please wait").enabled(false)),
            ],
        ),
    ])
}

fn skeleton(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "Profile",
            [
                any(Skeleton::circle(48.0)),
                any(div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(Skeleton::new(240.0, 16.0))
                    .child(Skeleton::new(180.0, 16.0))),
            ],
        ),
        row(
            t,
            "Card",
            [any(div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(Skeleton::new(280.0, 120.0))
                .child(Skeleton::new(280.0, 16.0))
                .child(Skeleton::new(200.0, 16.0)))],
        ),
    ])
}

fn empty(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([
        row(
            t,
            "With action",
            [any(div().w(px(420.0)).child(
                Empty::new("No projects yet")
                    .description("You haven't created any projects. Get started by creating one.")
                    .icon("📁")
                    .child(Button::new("Create project")),
            ))],
        ),
        row(
            t,
            "Text only",
            [any(div().w(px(420.0)).child(Empty::new("Nothing found")))],
        ),
    ])
}

fn aspect_ratio(
    _: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    _: &mut Context<GalleryView>,
) -> Div {
    rows(
        [("16 : 9", 16.0 / 9.0), ("1 : 1", 1.0)].map(|(caption, ratio)| {
            row(
                t,
                caption,
                [any(AspectRatio::new(ratio).width(240.0).child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(t.radius)
                        .bg(t.muted)
                        .text_color(t.muted_foreground)
                        .child(caption),
                ))],
            )
        }),
    )
}

fn type_scale(
    _: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    _: &mut Context<GalleryView>,
) -> Div {
    rows([
        row(t, "h1", [any(typography::h1(t, "Taxing Laughter"))]),
        row(
            t,
            "h2",
            [any(typography::h2(t, "The People of the Kingdom"))],
        ),
        row(t, "h3", [any(typography::h3(t, "The Joke Tax"))]),
        row(
            t,
            "h4",
            [any(typography::h4(t, "People stopped telling jokes"))],
        ),
        row(
            t,
            "p",
            [any(typography::p(
                t,
                "The king thought long and hard, and finally came up with a brilliant plan.",
            ))],
        ),
        row(
            t,
            "lead",
            [any(typography::lead(
                t,
                "A modal dialog that interrupts the user.",
            ))],
        ),
        row(
            t,
            "large",
            [any(typography::large(t, "Are you absolutely sure?"))],
        ),
        row(t, "small", [any(typography::small(t, "Email address"))]),
        row(
            t,
            "muted",
            [any(typography::muted(t, "Enter your email address."))],
        ),
        row(
            t,
            "inline_code",
            [any(typography::inline_code(
                t,
                "cargo run -p mcsapi-gallery --features gpui",
            ))],
        ),
        row(
            t,
            "blockquote",
            [any(blockquote(
                t,
                typography::blockquote(
                    t,
                    "\"After all,\" he said, \"everyone enjoys a good joke.\"",
                ),
            ))],
        ),
    ])
}

fn tabs(view: &mut GalleryView, t: &Tokens, _: &mut Window, cx: &mut Context<GalleryView>) -> Div {
    const TABS: [&str; 3] = ["Account", "Password", "Notifications"];
    rows([
        row(
            t,
            "Interactive",
            [any(div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(Tabs::new("account", TABS, view.tab).on_change(cx.listener(
                    |this, index: &usize, _, cx| {
                        this.tab = *index;
                        cx.notify();
                    },
                )))
                .child(typography::muted(
                    t,
                    format!("Showing the {} tab.", TABS[view.tab]),
                )))],
        ),
        row(
            t,
            "Two tabs",
            [any(Tabs::new("code", ["Preview", "Code"], 1))],
        ),
    ])
}

fn breadcrumbs(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    const PATH: [&str; 4] = ["Home", "Components", "Navigation", "Breadcrumb"];
    let status = match view.crumb {
        Some(index) => format!("Last clicked: {}", PATH[index]),
        None => "Click a link.".to_owned(),
    };
    rows([
        row(
            t,
            "Interactive",
            [any(div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(breadcrumb(
                    "path",
                    PATH,
                    t,
                    cx.listener(|this, index: &usize, _, cx| {
                        this.crumb = Some(*index);
                        cx.notify();
                    }),
                ))
                .child(typography::muted(t, status)))],
        ),
        row(
            t,
            "Single item",
            [any(breadcrumb("home", ["Home"], t, |_, _, _| {}))],
        ),
    ])
}

fn pagination(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    let window: Vec<String> = page_window(view.long, 20)
        .into_iter()
        .map(|page| page.map_or("…".to_owned(), |page| (page + 1).to_string()))
        .collect();
    rows([
        row(
            t,
            "5 pages",
            [any(Pagination::new("short", view.short, 5).on_change(
                cx.listener(|this, page: &usize, _, cx| {
                    this.short = *page;
                    cx.notify();
                }),
            ))],
        ),
        row(
            t,
            "20 pages",
            [any(div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    Pagination::new("long", view.long, 20).on_change(cx.listener(
                        |this, page: &usize, _, cx| {
                            this.long = *page;
                            cx.notify();
                        },
                    )),
                )
                .child(typography::muted(
                    t,
                    format!("page_window: {}", window.join(" ")),
                )))],
        ),
    ])
}

fn collapsible(
    _: &mut GalleryView,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<GalleryView>,
) -> Div {
    div()
        .w(px(420.0))
        .flex()
        .flex_col()
        .child(
            Collapsible::new("starred", "@peduarte starred 3 repositories")
                .default_open(true)
                .children([
                    "@radix-ui/primitives",
                    "@radix-ui/colors",
                    "@stitches/react",
                ]),
        )
        .child(Collapsible::new("closed", "Starts closed").child("Hidden until opened."))
}

fn accordion(_: &mut GalleryView, _: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    div()
        .w(px(420.0))
        .flex()
        .flex_col()
        .child(
            accordion_item("accessible", "Is it accessible?")
                .child("Yes. Headers are focusable and activate from the keyboard."),
        )
        .child(
            accordion_item("styled", "Is it styled?")
                .child("Yes. It reads its colors from the shell theme."),
        )
        .child(
            accordion_item("animated", "Is it animated?").child("The chevron flips as it opens."),
        )
}

fn table(_: &mut GalleryView, _: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    div().w(px(560.0)).child(
        Table::new(
            ["Invoice", "Status", "Method", "Amount"],
            [
                ["INV001", "Paid", "Credit Card", "$250.00"],
                ["INV002", "Pending", "PayPal", "$150.00"],
                ["INV003", "Unpaid", "Bank Transfer", "$350.00"],
                ["INV004", "Paid", "Credit Card", "$450.00"],
            ],
        )
        .caption("A list of your recent invoices."),
    )
}

fn dialog(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    let name = view.profile_name.clone();
    let entity = cx.entity();
    rows([row(
        t,
        "Edit profile",
        [
            any(Button::new("Open dialog")
                .variant(ButtonVariant::Outline)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.profile_open = true;
                    cx.notify();
                }))),
            any(Dialog::new(view.profile_open, "Edit profile")
                .description("Make changes to your profile here. Click save when you're done.")
                .on_close(move |_, cx| {
                    entity.update(cx, |this, cx| {
                        this.profile_open = false;
                        cx.notify();
                    })
                })
                .child(Label::new("Name"))
                .child(Input::new(&view.profile_name))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .child(Button::new("Save changes").on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.profile_open = false;
                                let saved = name.read(cx).text().to_owned();
                                toast(cx, "Profile saved", Some(saved.into()));
                                cx.notify();
                            },
                        ))),
                )),
        ],
    )])
}

fn alert_dialog(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    let answered = |dialog: &'static str| {
        cx.listener(
            move |this: &mut GalleryView, action: &AlertDialogAction, _, cx| {
                this.delete_open = false;
                this.publish_open = false;
                let verb = match action {
                    AlertDialogAction::Confirm => "confirmed",
                    _ => "cancelled",
                };
                this.answer = Some(format!("{dialog}: {verb}"));
                cx.notify();
            },
        )
    };
    let mut rows_list = vec![row(
        t,
        "Variants",
        [
            any(Button::new("Delete account")
                .variant(ButtonVariant::Destructive)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.delete_open = true;
                    cx.notify();
                }))),
            any(Button::new("Publish")
                .variant(ButtonVariant::Outline)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.publish_open = true;
                    cx.notify();
                }))),
            any(AlertDialog::new(view.delete_open, "Are you absolutely sure?")
                .description(
                    "This permanently deletes your account and removes your data from our servers.",
                )
                .confirm_text("Delete")
                .destructive(true)
                .on_action(answered("Delete"))),
            any(AlertDialog::new(view.publish_open, "Publish this post?")
                .description("Everyone with the link will be able to read it.")
                .confirm_text("Publish")
                .cancel_text("Not yet")
                .on_action(answered("Publish"))),
        ],
    )];
    if let Some(answer) = &view.answer {
        rows_list.push(row(
            t,
            "Last answer",
            [any(typography::muted(t, answer.clone()))],
        ));
    }
    rows(rows_list)
}

fn tooltips(_: &mut GalleryView, t: &Tokens, _: &mut Window, _: &mut Context<GalleryView>) -> Div {
    rows([row(
        t,
        "Hover these",
        [
            any(tooltip(
                "library",
                "Add to library",
                Button::new("Hover").variant(ButtonVariant::Outline),
            )),
            any(tooltip(
                "any",
                "Tooltips wrap any element.",
                Button::new("?").variant(ButtonVariant::Ghost),
            )),
        ],
    )])
}

fn toast_demo(
    _: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    let queued = toasts(cx).len();
    rows([
        row(
            t,
            "Show",
            [
                any(Button::new("Title only")
                    .variant(ButtonVariant::Outline)
                    .on_click(|_, _, cx| toast(cx, "Event has been created", None))),
                any(Button::new("With description")
                    .variant(ButtonVariant::Outline)
                    .on_click(|_, _, cx| {
                        toast(
                            cx,
                            "Event has been created",
                            Some("Sunday, December 03, 2023 at 9:00 AM".into()),
                        )
                    })),
            ],
        ),
        row(
            t,
            "Queued",
            [any(typography::muted(
                t,
                format!("{queued} showing; click one to dismiss it."),
            ))],
        ),
    ])
}

fn workspace_bar(
    view: &mut GalleryView,
    t: &Tokens,
    _: &mut Window,
    cx: &mut Context<GalleryView>,
) -> Div {
    let theme = view.theme();
    let entity = cx.entity();
    let status = format!(
        "Workspace {} is active with {} windows.",
        view.desktop.active().id(),
        view.desktop.active().windows().len()
    );
    rows([
        row(
            t,
            "Interactive",
            [any(div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(gpui_workspace_bar(
                    &view.desktop,
                    theme,
                    move |id, _, cx| {
                        entity.update(cx, |view, cx| {
                            view.desktop
                                .switch_to(id)
                                .expect("the bar only offers existing workspaces");
                            cx.notify();
                        });
                    },
                ))
                .child(typography::muted(t, status)))],
        ),
        row(
            t,
            "One workspace",
            [any(gpui_workspace_bar(
                &sample_desktop(1),
                theme,
                |_, _, _| {},
            ))],
        ),
    ])
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::RENDERERS;
    use crate::{find, specimens};

    /// Items of mcsapi-components-gpui that no specimen needs to list.
    const NOT_SHOWN: &[&str] = &[
        // Every specimen is drawn with the tokens and every text field with
        // these; the theme select and the form specimens exercise them.
        "Tokens",
        "TextInput",
        "bind_text_input_keys",
        "Handler",
    ];

    #[test]
    fn every_specimen_has_one_renderer() {
        let mut rendered = HashSet::new();
        for (name, _) in RENDERERS {
            assert!(find(name).is_some(), "renderer for unknown specimen {name}");
            assert!(rendered.insert(*name), "two renderers for {name}");
        }
        let missing: Vec<&str> = specimens()
            .map(|s| s.name)
            .filter(|name| !rendered.contains(name))
            .collect();
        assert!(
            missing.is_empty(),
            "specimens {missing:?} have no GPUI renderer; add one to RENDERERS in src/gpui_gallery.rs"
        );
    }

    #[test]
    fn every_gpui_component_has_a_specimen() {
        let source = include_str!("../../mcsapi-components-gpui/src/lib.rs");
        let mut exports: Vec<&str> = source
            .split("pub use ")
            .skip(1)
            .filter_map(|statement| statement.split(';').next())
            .flat_map(|statement| match statement.split_once('{') {
                Some((_, list)) => list.trim_end_matches(['}', ' ', '\n']).split(','),
                None => statement.rsplit("::").next().unwrap_or_default().split(','),
            })
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        exports.push("Handler");
        assert!(exports.contains(&"Button"), "parsed {exports:?}");
        let shown: HashSet<&str> = specimens().flat_map(|s| s.api.iter().copied()).collect();
        let missing: Vec<&str> = exports
            .into_iter()
            .filter(|name| !shown.contains(name) && !NOT_SHOWN.contains(name))
            .collect();
        assert!(
            missing.is_empty(),
            "mcsapi-components-gpui exports {missing:?} but no specimen lists them in `api`"
        );
    }
}
