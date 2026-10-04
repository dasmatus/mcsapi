//! Design tokens as GPUI colors.

use gpui::{App, Global, Hsla, Pixels, Rgba, px};
use mcsapi_ui::Theme;

/// shadcn/ui's semantic colors as GPUI [`Hsla`], derived from a shell
/// [`Theme`] the same way as [`mcsapi_components::Tokens`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// Page background.
    pub background: Hsla,
    /// Default text.
    pub foreground: Hsla,
    /// Card and popover background.
    pub card: Hsla,
    /// Subtle fill for secondary controls, skeletons, and tracks.
    pub muted: Hsla,
    /// De-emphasized text such as descriptions and placeholders.
    pub muted_foreground: Hsla,
    /// Fill of primary controls.
    pub primary: Hsla,
    /// Text on a primary fill.
    pub primary_foreground: Hsla,
    /// Fill of secondary controls.
    pub secondary: Hsla,
    /// Fill shown under hovered ghost and outline controls.
    pub hover: Hsla,
    /// Fill of destructive controls and the text of destructive alerts.
    pub destructive: Hsla,
    /// Text on a destructive fill.
    pub destructive_foreground: Hsla,
    /// Borders and separators.
    pub border: Hsla,
    /// Focus ring.
    pub ring: Hsla,
    /// Backdrop behind dialogs.
    pub overlay: Hsla,
    /// Corner radius of controls; cards use twice this.
    pub radius: Pixels,
}

impl Global for Tokens {}

/// Converts an egui color to GPUI.
pub(crate) fn hsla(color: mcsapi_ui::egui::Color32) -> Hsla {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a: f32::from(a) / 255.0,
    }
    .into()
}

impl Tokens {
    /// Derives the tokens from `theme`.
    pub fn from_theme(theme: &Theme) -> Self {
        let t = mcsapi_components::Tokens::from_theme(theme);
        Self {
            background: hsla(t.background),
            foreground: hsla(t.foreground),
            card: hsla(t.card),
            muted: hsla(t.muted),
            muted_foreground: hsla(t.muted_foreground),
            primary: hsla(t.primary),
            primary_foreground: hsla(t.primary_foreground),
            secondary: hsla(t.secondary),
            hover: hsla(t.hover),
            destructive: hsla(t.destructive),
            destructive_foreground: hsla(t.destructive_foreground),
            border: hsla(t.border),
            ring: hsla(t.ring),
            overlay: hsla(t.overlay),
            radius: px(f32::from(t.radius)),
        }
    }

    /// Makes `self` the tokens every component in `cx` draws with.
    pub fn install(self, cx: &mut App) {
        cx.set_global(self);
    }

    /// The installed tokens, or those of the default theme.
    pub fn get(cx: &App) -> Self {
        cx.try_global::<Self>()
            .copied()
            .unwrap_or_else(|| Self::from_theme(&Theme::default()))
    }

    /// Corner radius of cards, dialogs, and alerts.
    pub fn card_radius(&self) -> Pixels {
        self.radius * 2.0
    }
}

impl Default for Tokens {
    fn default() -> Self {
        Self::from_theme(&Theme::default())
    }
}
