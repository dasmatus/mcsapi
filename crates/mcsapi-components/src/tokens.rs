//! Design tokens derived from the shell [`Theme`] by the theming engine
//! (`mcsapi-theme`), as egui colors.

use egui::{Color32, CornerRadius, FontId, Stroke};
use mcsapi_ui::Theme;

/// shadcn/ui's semantic color and shape tokens, derived from a shell [`Theme`].
///
/// shadcn components name colors by role (`primary`, `muted`, `destructive`,
/// ...) instead of by hue. Deriving those roles from the five shell colors keeps
/// every component in step with the desktop when the theme changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// Page background.
    pub background: Color32,
    /// Default text.
    pub foreground: Color32,
    /// Card and popover background.
    pub card: Color32,
    /// Subtle fill for secondary controls, skeletons, and tracks.
    pub muted: Color32,
    /// De-emphasized text such as descriptions and placeholders.
    pub muted_foreground: Color32,
    /// Fill of primary controls.
    pub primary: Color32,
    /// Text on a primary fill.
    pub primary_foreground: Color32,
    /// Fill of secondary controls.
    pub secondary: Color32,
    /// Fill shown under hovered ghost and outline controls.
    pub hover: Color32,
    /// Fill of destructive controls and the text of destructive alerts.
    pub destructive: Color32,
    /// Text on a destructive fill.
    pub destructive_foreground: Color32,
    /// Borders and separators.
    pub border: Color32,
    /// Focus ring.
    pub ring: Color32,
    /// Backdrop behind dialogs.
    pub overlay: Color32,
    /// Corner radius of controls; cards use twice this.
    pub radius: u8,
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

    /// Makes `self` the tokens every component in `ctx` draws with.
    ///
    /// Call this once per frame, or whenever the theme changes, before adding
    /// components. Components fall back to [`Tokens::default`] otherwise.
    pub fn install(self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::NULL, self));
    }

    /// The tokens installed in `ctx`, or the defaults.
    pub fn current(ctx: &egui::Context) -> Self {
        ctx.data(|data| data.get_temp(egui::Id::NULL))
            .unwrap_or_default()
    }

    /// Corner radius of controls.
    pub fn control_radius(&self) -> CornerRadius {
        CornerRadius::same(self.radius)
    }

    /// Corner radius of cards, dialogs, and alerts.
    pub fn card_radius(&self) -> CornerRadius {
        CornerRadius::same(self.radius.saturating_mul(2))
    }

    /// One-pixel border stroke.
    pub fn border_stroke(&self) -> Stroke {
        Stroke::new(1.0, self.border)
    }

    /// Focus ring stroke.
    pub fn ring_stroke(&self) -> Stroke {
        Stroke::new(2.0, self.ring)
    }

    /// Body text font (shadcn's `text-sm`).
    pub fn body_font(&self) -> FontId {
        FontId::proportional(14.0)
    }

    /// Small text font (shadcn's `text-xs`).
    pub fn small_font(&self) -> FontId {
        FontId::proportional(12.0)
    }
}

impl Default for Tokens {
    fn default() -> Self {
        Self::from_theme(&Theme::default())
    }
}

impl From<mcsapi_ui::theme::Tokens> for Tokens {
    fn from(t: mcsapi_ui::theme::Tokens) -> Self {
        let c = |c: mcsapi_ui::theme::Color| Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a);
        Self {
            background: c(t.background),
            foreground: c(t.foreground),
            card: c(t.card),
            muted: c(t.muted),
            muted_foreground: c(t.muted_foreground),
            primary: c(t.primary),
            primary_foreground: c(t.primary_foreground),
            secondary: c(t.secondary),
            hover: c(t.hover),
            destructive: c(t.destructive),
            destructive_foreground: c(t.destructive_foreground),
            border: c(t.border),
            ring: c(t.ring),
            overlay: c(t.overlay),
            radius: t.radius,
        }
    }
}

impl From<&Theme> for Tokens {
    fn from(theme: &Theme) -> Self {
        Self::from_theme(theme)
    }
}
