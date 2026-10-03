use mcsapi_components::*;
use mcsapi_ui::{Theme, egui};

use egui::{Event, PointerButton, Pos2, RawInput, Rect, Ui};

/// Runs one frame of `ui_fn` inside a central panel, with `events` and the clock at `time`.
fn frame(
    ctx: &egui::Context,
    time: f64,
    events: Vec<Event>,
    mut ui_fn: impl FnMut(&mut Ui),
) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1024.0, 768.0))),
        time: Some(time),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        Tokens::default().install(ui.ctx());
        ui_fn(ui);
    });
    output.textures_delta.clear();
    output
}

fn press(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

/// Clicks at the center of the rect that `ui_fn` reports on its first frame.
fn click(ctx: &egui::Context, mut ui_fn: impl FnMut(&mut Ui) -> Rect) {
    let mut rect = Rect::NOTHING;
    frame(ctx, 0.0, vec![], |ui| rect = ui_fn(ui));
    let pos = rect.center();
    frame(ctx, 0.1, vec![Event::PointerMoved(pos)], |ui| {
        ui_fn(ui);
    });
    frame(ctx, 0.2, vec![press(pos, true)], |ui| {
        ui_fn(ui);
    });
    frame(ctx, 0.3, vec![press(pos, false)], |ui| {
        ui_fn(ui);
    });
}

#[test]
fn tokens_follow_the_shell_theme() {
    let theme = Theme {
        accent: egui::Color32::from_rgb(1, 2, 3),
        ..Theme::default()
    };
    let tokens = Tokens::from_theme(&theme);
    assert_eq!(tokens.primary, theme.accent);
    assert_eq!(tokens.background, theme.background);
    assert_eq!(tokens.foreground, theme.foreground);

    let ctx = egui::Context::default();
    assert_eq!(Tokens::current(&ctx), Tokens::default());
    tokens.install(&ctx);
    assert_eq!(Tokens::current(&ctx), tokens);
}

#[test]
fn every_component_draws() {
    let ctx = egui::Context::default();
    let mut text = String::from("hello");
    let mut flag = true;
    let mut index = 1;
    let mut choice = Some(0);
    let mut value = 0.5;
    let mut open = true;
    let mut alert_open = true;
    toast(&ctx, "Saved", Some("Your changes are live".into()));
    let output = frame(&ctx, 0.0, vec![], |ui| {
        let tokens = Tokens::current(ui.ctx());
        for variant in [
            ButtonVariant::Default,
            ButtonVariant::Secondary,
            ButtonVariant::Destructive,
            ButtonVariant::Outline,
            ButtonVariant::Ghost,
            ButtonVariant::Link,
        ] {
            ui.add(Button::new("Button").variant(variant));
        }
        ui.add(Button::new("+").size(ButtonSize::Icon).enabled(false));
        for variant in [
            BadgeVariant::Default,
            BadgeVariant::Secondary,
            BadgeVariant::Destructive,
            BadgeVariant::Outline,
        ] {
            ui.add(Badge::new("Badge").variant(variant));
        }
        ui.add(Toggle::new(&mut flag, "B"));
        ui.add(ToggleGroup::new(&mut choice, &["L", "C", "R"]));
        ui.add(Kbd::new("Ctrl"));
        Card::new()
            .title("Card")
            .description("Description")
            .show(ui, |ui| ui.label("Body"));
        ui.add(Alert::new("Heads up").description("Details"));
        ui.add(Alert::new("Error").variant(AlertVariant::Destructive));
        ui.add(Separator::horizontal());
        ui.horizontal(|ui| ui.add(Separator::vertical()));
        ui.add(Label::new("Email"));
        ui.add(Avatar::new("Ada Lovelace"));
        ui.add(Skeleton::new([120.0, 16.0]));
        ui.add(Skeleton::circle(40.0));
        ui.add(Spinner::new());
        ui.add(Progress::new(0.4));
        Empty::new("No projects")
            .icon("□")
            .description("Create one to get started")
            .show(ui, |ui| ui.add(Button::new("Create")));
        AspectRatio::new(16.0 / 9.0).show(ui, |ui| ui.label("Video"));
        ui.label(typography::h1(&tokens, "Title"));
        ui.label(typography::inline_code(&tokens, "cargo"));
        blockquote(ui, |ui| ui.label(typography::blockquote(&tokens, "Quote")));
        ui.add(Input::new(&mut text).placeholder("Email"));
        ui.add(Input::new(&mut text).password(true).width(120.0));
        ui.add(Textarea::new(&mut text).rows(2));
        ui.add(Checkbox::new(&mut flag).label("Accept"));
        ui.add(Switch::new(&mut flag).label("Airplane mode"));
        ui.add(RadioGroup::new(&mut index, &["One", "Two"]));
        ui.add(Slider::new(&mut value, 0.0..=1.0).step(0.1));
        ui.add(Select::new("fruit", &mut choice, &["Apple", "Banana"]));
        ui.add(Tabs::new(&mut index, &["Account", "Password"]));
        breadcrumb(ui, &["Home", "Docs", "Components"]);
        ui.add(Pagination::new(&mut index, 10));
        Collapsible::new("more", "More")
            .default_open(true)
            .show(ui, |ui| ui.label("Hidden"));
        accordion_item(ui, "faq", "Is it accessible?", |ui| ui.label("Yes"));
        let rows = vec![vec!["INV001".to_owned(), "Paid".to_owned()]];
        Table::new(&["Invoice", "Status"], &rows)
            .caption("Invoices")
            .show(ui);
        tooltip(ui.label("hover me"), "Tooltip");
        Dialog::new("dialog", &mut open, "Edit profile")
            .description("Make changes here.")
            .show(ui.ctx(), |ui| ui.label("Form"));
        AlertDialog::new("alert", &mut alert_open, "Are you sure?")
            .destructive(true)
            .show(ui.ctx());
        Toaster::show(ui.ctx());
    });
    assert!(!output.shapes.is_empty());
    assert!(open && alert_open, "dialogs stay open without input");
}

#[test]
fn clicking_toggles_boolean_controls() {
    let ctx = egui::Context::default();
    let mut checked = false;
    click(&ctx, |ui| {
        ui.add(Checkbox::new(&mut checked).label("Accept")).rect
    });
    assert!(checked);

    let ctx = egui::Context::default();
    let mut on = false;
    click(&ctx, |ui| ui.add(Switch::new(&mut on)).rect);
    assert!(on);

    let ctx = egui::Context::default();
    let mut pressed = true;
    click(&ctx, |ui| ui.add(Toggle::new(&mut pressed, "B")).rect);
    assert!(!pressed);
}

#[test]
fn clicking_a_button_reports_a_click() {
    let ctx = egui::Context::default();
    let mut clicks = 0;
    click(&ctx, |ui| {
        let response = ui.add(Button::new("Save"));
        clicks += usize::from(response.clicked());
        response.rect
    });
    assert_eq!(clicks, 1);

    let ctx = egui::Context::default();
    let mut clicks = 0;
    click(&ctx, |ui| {
        let response = ui.add(Button::new("Save").enabled(false));
        clicks += usize::from(response.clicked());
        response.rect
    });
    assert_eq!(clicks, 0, "disabled buttons ignore clicks");
}

#[test]
fn clicking_a_tab_selects_it() {
    let ctx = egui::Context::default();
    let mut tab = 0;
    // The second trigger sits right of center in a two-tab list.
    click(&ctx, |ui| {
        let rect = ui.add(Tabs::new(&mut tab, &["Account", "Password"])).rect;
        Rect::from_center_size(
            egui::pos2(rect.right() - rect.width() / 4.0, rect.center().y),
            egui::Vec2::splat(1.0),
        )
    });
    assert_eq!(tab, 1);
}

#[test]
fn clicking_the_slider_end_sets_the_maximum() {
    let ctx = egui::Context::default();
    let mut value = 0.0;
    click(&ctx, |ui| {
        let rect = ui
            .add(Slider::new(&mut value, 0.0..=10.0).width(200.0))
            .rect;
        Rect::from_center_size(rect.right_center(), egui::Vec2::splat(1.0))
    });
    assert_eq!(value, 10.0);
}

#[test]
fn page_window_elides_distant_pages() {
    assert_eq!(page_window(0, 3), vec![Some(0), Some(1), Some(2)]);
    assert_eq!(
        page_window(5, 20),
        vec![Some(0), None, Some(4), Some(5), Some(6), None, Some(19)]
    );
    assert_eq!(page_window(0, 20), vec![Some(0), Some(1), None, Some(19)]);
    assert_eq!(page_window(19, 20), vec![Some(0), None, Some(18), Some(19)]);
}

#[test]
fn toasts_expire_after_their_duration() {
    let ctx = egui::Context::default();
    frame(&ctx, 0.0, vec![], |ui| toast(ui.ctx(), "Saved", None));
    frame(&ctx, 1.0, vec![], |ui| Toaster::show(ui.ctx()));
    assert_eq!(toasts(&ctx).len(), 1);
    frame(&ctx, 5.0, vec![], |ui| Toaster::show(ui.ctx()));
    assert!(toasts(&ctx).is_empty());
}

#[test]
fn escape_cancels_an_alert_dialog() {
    let ctx = egui::Context::default();
    let mut open = true;
    let mut answer = None;
    frame(&ctx, 0.0, vec![], |ui| {
        AlertDialog::new("confirm", &mut open, "Delete?").show(ui.ctx());
    });
    let escape = Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    frame(&ctx, 0.1, vec![escape], |ui| {
        answer = AlertDialog::new("confirm", &mut open, "Delete?").show(ui.ctx());
    });
    assert_eq!(answer, Some(AlertDialogAction::Cancel));
    assert!(!open);
}
