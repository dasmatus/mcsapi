//! Design tokens as GPUI colors.

use gpui::{App, Global, Hsla, Pixels, Rgba, px};
use mcsapi_ui::Theme;

/// shadcn/ui's semantic colors as GPUI [`Hsla`], derived by the theming
/// engine (`mcsapi-theme`) exactly like [`mcsapi_components::Tokens`].
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
    /// Fill of text fields, switch tracks that are off and sunken panels.
    pub field: Hsla,
    /// Corner radius of controls; cards use two more.
    pub radius: Pixels,
}

impl Global for Tokens {}

/// Converts a theme color to GPUI.
pub(crate) fn hsla(color: mcsapi_ui::theme::Color) -> Hsla {
    let [r, g, b, a] = color.to_f32();
    Rgba { r, g, b, a }.into()
}

impl Tokens {
    /// Derives the tokens from the shell colors in `theme`, with the default
    /// radius and destructive color.
    pub fn from_theme(theme: &Theme) -> Self {
        mcsapi_ui::theme::Tokens::derive(&theme.palette(), 6).into()
    }

    /// The tokens of a full theme from the theming engine, including its
    /// radius and destructive color.
    pub fn from_spec(theme: &mcsapi_ui::theme::Theme) -> Self {
        theme.tokens().into()
    }

    /// Makes `self` the tokens every component in `cx` draws with, and the
    /// Zed theme built from them the one the components built on Zed's `ui`
    /// draw with. Fonts already installed (by [`install_theme`]) are kept;
    /// otherwise `ui` gets the default theme's fonts.
    pub fn install(self, cx: &mut App) {
        cx.set_global(self);
        let tokens = self.to_spec_tokens();
        let appearance = if tokens.background.is_light() {
            theme::Appearance::Light
        } else {
            theme::Appearance::Dark
        };
        let zed_theme = theme::theme_from_tokens("mcsapi", appearance, &tokens);
        if theme::is_installed(cx) {
            theme::GlobalTheme::update_theme(cx, std::sync::Arc::new(zed_theme));
            cx.refresh_windows();
        } else {
            theme::install(zed_theme, theme::McsapiThemeSettings::default(), cx);
        }
    }

    /// The tokens as the theming engine's colors, for building Zed themes.
    fn to_spec_tokens(self) -> mcsapi_ui::theme::Tokens {
        let color = |color: Hsla| {
            let Rgba { r, g, b, a } = color.to_rgb();
            let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            mcsapi_ui::theme::Color::rgba(channel(r), channel(g), channel(b), channel(a))
        };
        mcsapi_ui::theme::Tokens {
            background: color(self.background),
            foreground: color(self.foreground),
            card: color(self.card),
            muted: color(self.muted),
            muted_foreground: color(self.muted_foreground),
            primary: color(self.primary),
            primary_foreground: color(self.primary_foreground),
            secondary: color(self.secondary),
            hover: color(self.hover),
            destructive: color(self.destructive),
            destructive_foreground: color(self.destructive_foreground),
            border: color(self.border),
            ring: color(self.ring),
            overlay: color(self.overlay),
            field: color(self.field),
            // GPUI tokens do not carry the selection color; the engine derives
            // it from the accent the same way.
            selection: color(self.primary).with_alpha(0x55),
            radius: self.radius.as_f32().clamp(0.0, 255.0).round() as u8,
        }
    }

    /// The installed tokens, or those of the default theme.
    pub fn get(cx: &App) -> Self {
        cx.try_global::<Self>()
            .copied()
            .unwrap_or_else(|| Self::from_theme(&Theme::default()))
    }

    /// Corner radius of cards, dialogs, and alerts: two more than controls,
    /// like the web interface's 6 px controls in 8 px cards.
    pub fn card_radius(&self) -> Pixels {
        self.radius + px(2.0)
    }
}

impl From<mcsapi_ui::theme::Tokens> for Tokens {
    fn from(t: mcsapi_ui::theme::Tokens) -> Self {
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
            field: hsla(t.field),
            radius: px(f32::from(t.radius)),
        }
    }
}

/// Installs a full theme from the theming engine: its [`Tokens`], and the Zed
/// theme and fonts for the components built on Zed's `ui`. Prefer it over
/// `Tokens::from_spec(theme).install(cx)`, which cannot pass on the fonts.
pub fn install_theme(theme: &mcsapi_ui::theme::Theme, cx: &mut App) {
    cx.set_global(Tokens::from_spec(theme));
    theme::init_mcsapi(theme, cx);
}

/// Installs the default theme's colors and fonts unless a theme is installed,
/// so components built on Zed's `ui`, which require one, also render for apps
/// that never call [`Tokens::install`], as every component here always has.
pub(crate) fn ensure_installed(cx: &mut App) {
    if !theme::is_installed(cx) {
        Tokens::get(cx).install(cx);
    }
}

impl Default for Tokens {
    fn default() -> Self {
        Self::from_theme(&Theme::default())
    }
}
