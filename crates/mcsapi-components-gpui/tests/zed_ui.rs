//! Components drawn by Zed's `ui`, in GPUI's test platform.

#![cfg(feature = "gpui")]

use std::{cell::Cell, rc::Rc};

use gpui::{
    AssetSource as _, Context, IntoElement, Modifiers, ParentElement as _, Render, TestAppContext,
    Window, div, prelude::*,
};
use mcsapi_components_gpui::{
    Assets, Button, Checkbox, Switch, Tokens, install_theme, theme, theme::ActiveTheme as _, ui,
};

struct Controls {
    checked: Rc<Cell<Option<bool>>>,
    clicks: Rc<Cell<u32>>,
}

impl Render for Controls {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let checked = self.checked.clone();
        let clicks = self.clicks.clone();
        div()
            .flex()
            .flex_col()
            .child(
                div().debug_selector(|| "CHECKBOX".into()).child(
                    Checkbox::new("check", false)
                        .label("Check")
                        .on_change(move |on, _, _| checked.set(Some(*on))),
                ),
            )
            .child(
                div().debug_selector(|| "BUTTON".into()).child(
                    Button::new("Press").on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
                ),
            )
            .child(Switch::new("switch", true).label("Switch"))
    }
}

#[gpui::test]
fn components_install_a_theme_when_the_app_has_none(cx: &mut TestAppContext) {
    let checked = Rc::new(Cell::new(None));
    let clicks = Rc::new(Cell::new(0));
    let (_view, cx) = cx.add_window_view({
        let checked = checked.clone();
        let clicks = clicks.clone();
        |_, _| Controls { checked, clicks }
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| theme::is_installed(cx)));

    let checkbox = cx.debug_bounds("CHECKBOX").expect("checkbox bounds");
    cx.simulate_click(checkbox.center(), Modifiers::default());
    assert_eq!(
        checked.get(),
        Some(true),
        "Zed's checkbox reports the new state"
    );

    let button = cx.debug_bounds("BUTTON").expect("button bounds");
    cx.simulate_click(button.center(), Modifiers::default());
    assert_eq!(clicks.get(), 1);
}

#[gpui::test]
fn installing_tokens_retints_the_zed_theme(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install_theme(&mcsapi_ui::theme::Theme::dark(), cx);
        let dark = background(cx);
        Tokens::from_spec(&mcsapi_ui::theme::Theme::light()).install(cx);
        let light = background(cx);
        // The Zed theme follows the installed tokens, through the 8-bit
        // colors the theming engine works in.
        assert!(light.l > dark.l);
        assert!((light.l - Tokens::get(cx).background.l).abs() < 0.01);
    });
}

fn background(cx: &gpui::App) -> gpui::Hsla {
    cx.theme().colors().background
}

#[test]
fn assets_serve_the_icons_zed_ui_draws() -> gpui::Result<()> {
    let assets = Assets::new();
    let check = assets.load(&ui::IconName::Check.path())?;
    assert!(check.is_some_and(|svg| svg.starts_with(b"<svg")));
    assert!(assets.load("icons/no_such_icon.svg")?.is_none());
    assert!(
        assets
            .list("icons/")?
            .iter()
            .any(|path| path.as_ref() == "icons/check.svg")
    );
    Ok(())
}
