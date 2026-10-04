//! The desktop's typefaces: Nerd Font patched Noto Sans and Cousine.
//!
//! [`install`] replaces egui's Ubuntu-Light and Hack with NotoSans Nerd Font
//! Propo (Light for body text, Medium for [`STRONG`]) and Cousine Nerd Font
//! for monospace. Both cover Central European Latin and carry the Nerd Font
//! icons, so glyphs such as [`icon::CLOSE`] draw inline with text. egui's
//! bundled fonts stay as fallbacks for emoji and anything else.
//!
//! [`crate::run_frame`] installs the fonts itself; hosts that run their own
//! egui frames call [`install`] once per context.
//!
//! ```
//! use mcsapi_ui::{egui, fonts};
//!
//! let ctx = egui::Context::default();
//! fonts::install(&ctx);
//! let mut output = ctx.run_ui(Default::default(), |ui| {
//!     ui.label(egui::RichText::new("Příliš žluťoučký kůň").font(fonts::strong(ui.ctx(), 14.0)));
//!     ui.label(egui::RichText::new(fonts::icon::CLOSE).size(9.0));
//! });
//! output.textures_delta.clear();
//! ```

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

const SANS: &str = "NotoSans Nerd Font Propo Light";
const SANS_MEDIUM: &str = "NotoSans Nerd Font Propo Medium";
const MONO: &str = "Cousine Nerd Font";

/// The family name of the medium-weight UI face.
pub const STRONG: &str = mcsapi::widgets::STRONG_FAMILY;

/// Nerd Font icon code points, all drawn in the text color.
pub mod icon {
    /// Codicon `chrome-close`: the close window control.
    pub const CLOSE: &str = "\u{eab8}";
    /// Codicon `chrome-minimize`: the minimize window control.
    pub const MINIMIZE: &str = "\u{eaba}";
    /// Codicon `chrome-maximize`: the maximize window control.
    pub const MAXIMIZE: &str = "\u{eab9}";
    /// Codicon `chrome-restore`: maximize on a maximized window.
    pub const RESTORE: &str = "\u{eabb}";
    /// Font Awesome `search`.
    pub const SEARCH: &str = "\u{f002}";
    /// Font Awesome `check`.
    pub const DONE: &str = "\u{f00c}";
    /// Font Awesome `close` (a bold ×).
    pub const FAILED: &str = "\u{f00d}";
    /// Font Awesome `power-off`.
    pub const POWER: &str = "\u{f011}";
    /// Font Awesome `lock`.
    pub const LOCK: &str = "\u{f023}";
    /// Font Awesome `exclamation-triangle`.
    pub const WARNING: &str = "\u{f071}";
    /// Font Awesome `folder`.
    pub const FOLDER: &str = "\u{f07b}";
    /// Font Awesome `file`.
    pub const FILE: &str = "\u{f15b}";
    /// Font Awesome `cog`.
    pub const SETTINGS: &str = "\u{f013}";
    /// Codicon `sparkle`: the assistant.
    pub const ASSISTANT: &str = "\u{ec10}";
    /// Codicon `window`: an open window.
    pub const WINDOW: &str = "\u{eb7f}";
    /// Codicon `terminal`: a command.
    pub const COMMAND: &str = "\u{ea85}";
    /// Codicon `rocket`: an app to launch.
    pub const APP: &str = "\u{eb44}";
}

/// The font definitions [`install`] applies: egui's defaults with the Nerd
/// Font faces in front.
pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            SANS,
            &include_bytes!("../fonts/NotoSansNerdFontPropo-Light.ttf")[..],
        ),
        (
            SANS_MEDIUM,
            &include_bytes!("../fonts/NotoSansNerdFontPropo-Medium.ttf")[..],
        ),
        (
            MONO,
            &include_bytes!("../fonts/CousineNerdFont-Regular.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let family = |first: &str| {
        let mut list = vec![first.to_owned()];
        list.extend(fallbacks.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, family(SANS));
    fonts
        .families
        .insert(FontFamily::Name(STRONG.into()), family(SANS_MEDIUM));
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, MONO.to_owned());
    fonts
}

#[derive(Clone, Copy)]
struct Installed;

/// Installs [`definitions`] in `ctx`. Cheap after the first call.
pub fn install(ctx: &egui::Context) {
    let id = egui::Id::new("mcsapi_ui::fonts");
    if ctx.data(|data| data.get_temp::<Installed>(id).is_some()) {
        return;
    }
    ctx.set_fonts(definitions());
    ctx.data_mut(|data| data.insert_temp(id, Installed));
}

/// Body text at `size`: NotoSans Nerd Font Propo Light.
pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Emphasized text at `size`: NotoSans Nerd Font Propo Medium, or the body
/// face while `ctx` has no [`install`]ed fonts (egui panics on an unknown
/// family). Call it during a frame.
pub fn strong(ctx: &egui::Context, size: f32) -> FontId {
    let family = FontFamily::Name(STRONG.into());
    if ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family)) {
        FontId::new(size, family)
    } else {
        FontId::proportional(size)
    }
}

/// Monospace text at `size`: Cousine Nerd Font, also the palette prompt.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nerd_faces_come_first_and_keep_fallbacks() {
        let fonts = definitions();
        let proportional = &fonts.families[&FontFamily::Proportional];
        assert_eq!(proportional[0], SANS);
        assert!(proportional.len() > 1, "egui's emoji fallbacks stay");
        assert_eq!(fonts.families[&FontFamily::Monospace][0], MONO);
        assert_eq!(
            fonts.families[&FontFamily::Name(STRONG.into())][0],
            SANS_MEDIUM
        );
    }

    #[test]
    fn strong_falls_back_without_the_fonts() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(strong(ui.ctx(), 14.0), FontId::proportional(14.0));
        });
        output.textures_delta.clear();
        install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(
                strong(ui.ctx(), 14.0).family,
                FontFamily::Name(STRONG.into())
            );
        });
        output.textures_delta.clear();
    }

    #[test]
    fn faces_cover_central_european_text_and_icons() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        ctx.fonts_mut(|fonts| {
            for c in "ČčŘřŠšŽžŁłŐőŰűĎďŤťĽľŃńŚśŹźĘęĄą".chars().chain(
                [icon::CLOSE, icon::MINIMIZE, icon::MAXIMIZE, icon::RESTORE]
                    .iter()
                    .flat_map(|s| s.chars()),
            ) {
                for font in [
                    body(14.0),
                    FontId::new(14.0, FontFamily::Name(STRONG.into())),
                    mono(14.0),
                ] {
                    assert!(fonts.has_glyph(&font, c), "{c:?} missing in {font:?}");
                }
            }
        });
    }
}
