//! Design tokens derived from the shell [`Theme`].

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
    /// Derives the tokens from `theme`.
    pub fn from_theme(theme: &Theme) -> Self {
        let destructive = Color32::from_rgb(220, 38, 38);
        Self {
            background: theme.background,
            foreground: theme.foreground,
            card: theme.background.lerp_to_gamma(theme.surface, 0.5),
            muted: theme.surface,
            muted_foreground: theme.foreground.lerp_to_gamma(theme.border, 0.55),
            primary: theme.accent,
            primary_foreground: theme.background,
            secondary: theme.surface,
            hover: theme.surface.lerp_to_gamma(theme.border, 0.35),
            destructive,
            destructive_foreground: Color32::from_rgb(254, 242, 242),
            border: theme.surface.lerp_to_gamma(theme.border, 0.5),
            ring: theme.accent,
            overlay: Color32::from_black_alpha(160),
            radius: 6,
        }
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

    /// Text size of touch controls; 16 px also keeps phones from zooming.
    pub const TOUCH_TEXT: f32 = 16.0;

    /// Minimum height of touch controls' tap targets.
    pub const TOUCH_TARGET: f32 = 44.0;

    /// Body font, or the larger touch font when `touch` is set.
    pub fn control_font(&self, touch: bool) -> FontId {
        if touch {
            FontId::proportional(Self::TOUCH_TEXT)
        } else {
            self.body_font()
        }
    }
}

impl Default for Tokens {
    fn default() -> Self {
        Self::from_theme(&Theme::default())
    }
}

impl From<&Theme> for Tokens {
    fn from(theme: &Theme) -> Self {
        Self::from_theme(theme)
    }
}
