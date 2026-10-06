//! Zed themes built from mcsapi themes.
//!
//! Zed's [`ThemeColors`] has well over a hundred roles; an mcsapi theme has a
//! six-color [`Palette`](mcsapi_theme::Palette) and the shadcn/ui roles derived
//! from it ([`Tokens`]). The roles `ui` components draw with (surfaces,
//! elements, borders, text, icons, scrollbars, status) come from the tokens.
//! Editor, terminal, version-control and vim roles keep Zed's defaults for the
//! theme's appearance, since no mcsapi app draws them yet.

use std::sync::Arc;

use gpui::{
    App, Font, FontFeatures, FontWeight, Hsla, Pixels, Rgba, WindowBackgroundAppearance, px,
};
use mcsapi_theme::{Color, Scheme, Tokens};

use crate::{
    AccentColors, Appearance, GlobalTheme, PlayerColor, PlayerColors, StatusColors, SyntaxTheme,
    SystemAppearance, SystemColors, Theme, ThemeColors, ThemeSettingsProvider, ThemeStyles,
    UiDensity, set_theme_settings_provider,
};

/// Contrast text and icons in an accent color need against the background:
/// WCAG 2's minimum for body text.
const ACCENT_TEXT_CONTRAST: f32 = 4.5;

fn hsla(color: Color) -> Hsla {
    let [r, g, b, a] = color.to_f32();
    Rgba { r, g, b, a }.into()
}

/// The [`Theme`] for an mcsapi theme, named after it.
pub fn theme_from_mcsapi(theme: &mcsapi_theme::Theme) -> Theme {
    let appearance = match theme.scheme {
        Scheme::Dark => Appearance::Dark,
        Scheme::Light => Appearance::Light,
    };
    theme_from_tokens(&theme.name, appearance, &theme.tokens())
}

/// The [`Theme`] for a set of mcsapi tokens, for callers that hold tokens
/// without the theme they were derived from.
pub fn theme_from_tokens(name: &str, appearance: Appearance, tokens: &Tokens) -> Theme {
    let t = tokens;
    let transparent = t.background.with_alpha(0);
    // Zed's defaults cover the roles mcsapi has no token for, in the same
    // appearance so they sit naturally next to the mapped ones.
    let (mut colors, mut status, mut player) = match appearance {
        Appearance::Dark => (
            ThemeColors::dark(),
            StatusColors::dark(),
            PlayerColors::dark(),
        ),
        Appearance::Light => (
            ThemeColors::light(),
            StatusColors::light(),
            PlayerColors::light(),
        ),
    };
    let accent_text = t.primary.legible_on(t.background, ACCENT_TEXT_CONTRAST);
    let placeholder = t.muted_foreground.mix(t.background, 0.25);
    let disabled_text = t.muted_foreground.mix(t.background, 0.45);
    let pressed = t.hover.mix(t.foreground, 0.08);

    colors.border = hsla(t.border);
    colors.border_variant = hsla(t.border.mix(t.background, 0.5));
    colors.border_focused = hsla(t.ring);
    colors.border_selected = hsla(t.primary);
    colors.border_transparent = hsla(transparent);
    colors.border_disabled = hsla(t.border.mix(t.background, 0.5));

    // shadcn/ui draws popovers and cards with the same fill, so Zed's
    // elevated surfaces (menus, popovers, modals) use the card color.
    colors.elevated_surface_background = hsla(t.card);
    colors.surface_background = hsla(t.muted);
    colors.background = hsla(t.background);

    colors.element_background = hsla(t.secondary);
    colors.element_hover = hsla(t.hover);
    colors.element_active = hsla(pressed);
    colors.element_selected = hsla(t.hover);
    colors.element_selection_background = hsla(t.selection);
    colors.element_disabled = hsla(t.muted.mix(t.background, 0.5));
    colors.drop_target_background = hsla(t.selection);
    colors.drop_target_border = hsla(t.primary);

    colors.ghost_element_background = hsla(transparent);
    colors.ghost_element_hover = hsla(t.hover);
    colors.ghost_element_active = hsla(pressed);
    colors.ghost_element_selected = hsla(t.hover);
    colors.ghost_element_disabled = hsla(transparent);

    colors.text = hsla(t.foreground);
    colors.text_muted = hsla(t.muted_foreground);
    colors.text_placeholder = hsla(placeholder);
    colors.text_disabled = hsla(disabled_text);
    colors.text_accent = hsla(accent_text);
    colors.icon = hsla(t.foreground);
    colors.icon_muted = hsla(t.muted_foreground);
    colors.icon_disabled = hsla(disabled_text);
    colors.icon_placeholder = hsla(placeholder);
    colors.icon_accent = hsla(accent_text);
    colors.link_text_hover = hsla(accent_text);

    // The shell draws bars and panels on the raised surface and content on the
    // background, like derisk's own chrome.
    colors.status_bar_background = hsla(t.muted);
    colors.title_bar_background = hsla(t.muted);
    colors.title_bar_inactive_background = hsla(t.muted.mix(t.background, 0.5));
    colors.toolbar_background = hsla(t.background);
    colors.tab_bar_background = hsla(t.muted);
    colors.tab_inactive_background = hsla(t.muted);
    colors.tab_active_background = hsla(t.background);
    colors.panel_background = hsla(t.muted);
    colors.panel_focused_border = hsla(t.ring);
    colors.panel_indent_guide = hsla(t.border.mix(t.background, 0.5));
    colors.panel_indent_guide_hover = hsla(t.border);
    colors.panel_indent_guide_active = hsla(t.muted_foreground);
    colors.panel_overlay_background = hsla(t.card);
    colors.panel_overlay_hover = hsla(t.hover);
    colors.pane_focused_border = hsla(t.ring);
    colors.pane_group_border = hsla(t.border);
    colors.search_match_background = hsla(t.selection);
    colors.search_active_match_background = hsla(t.primary.with_alpha(0x88));

    colors.scrollbar_thumb_background = hsla(t.muted_foreground.with_alpha(0x4d));
    colors.scrollbar_thumb_hover_background = hsla(t.muted_foreground.with_alpha(0x80));
    colors.scrollbar_thumb_active_background = hsla(t.muted_foreground.with_alpha(0xb3));
    colors.scrollbar_thumb_border = hsla(transparent);
    colors.scrollbar_track_background = hsla(transparent);
    colors.scrollbar_track_border = hsla(transparent);

    colors.editor_foreground = hsla(t.foreground);
    colors.editor_background = hsla(t.background);
    colors.editor_gutter_background = hsla(t.background);
    colors.editor_line_number = hsla(t.muted_foreground);
    colors.editor_active_line_number = hsla(t.foreground);
    colors.terminal_background = hsla(t.background);
    colors.terminal_foreground = hsla(t.foreground);

    status.error = hsla(t.destructive.legible_on(t.background, ACCENT_TEXT_CONTRAST));
    status.error_background = hsla(t.destructive.with_alpha(0x40));
    status.error_border = hsla(t.destructive);
    status.deleted = status.error;
    status.info = hsla(accent_text);
    status.info_background = hsla(t.selection);
    status.info_border = hsla(t.primary);
    status.hint = hsla(t.muted_foreground);
    status.hint_background = hsla(t.muted);
    status.hint_border = hsla(t.border);
    status.hidden = hsla(disabled_text);
    status.ignored = hsla(disabled_text);

    // The local player is the user: their cursor and selection follow the
    // accent like every other mcsapi selection.
    let local = PlayerColor {
        cursor: hsla(t.primary),
        background: hsla(t.primary),
        selection: hsla(t.selection),
    };
    if let Some(first) = player.0.first_mut() {
        *first = local;
    } else {
        player.0.push(local);
    }

    // Zed only ships a dark syntax theme without its registry; light themes
    // highlight nothing until mcsapi themes carry syntax colors.
    let syntax = match appearance {
        Appearance::Dark => crate::fallback_themes::zed_default_dark().styles.syntax,
        Appearance::Light => Arc::new(SyntaxTheme::default()),
    };

    let mut accents = vec![hsla(t.primary)];
    accents.extend(
        mcsapi_theme::ACCENTS
            .iter()
            .map(|(_, accent)| hsla(accent.legible_on(t.background, 3.0))),
    );

    Theme {
        id: name.to_lowercase().replace(' ', "-"),
        name: name.to_owned().into(),
        appearance,
        styles: ThemeStyles {
            window_background_appearance: WindowBackgroundAppearance::Opaque,
            system: SystemColors::default(),
            accents: AccentColors(Arc::from(accents)),
            colors,
            status,
            player,
            syntax,
        },
    }
}

/// Fonts and sizes for `ui` from an mcsapi theme, as Zed's theme settings
/// would supply them.
#[derive(Clone, Debug)]
pub struct McsapiThemeSettings {
    ui_font: Font,
    buffer_font: Font,
    ui_font_size: Pixels,
    buffer_font_size: Pixels,
    ui_density: UiDensity,
}

impl McsapiThemeSettings {
    /// The fonts of `fonts`, at the default density.
    pub fn new(fonts: &mcsapi_theme::Fonts) -> Self {
        let font = |family: &str, weight: f32| Font {
            family: family.to_owned().into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight(weight),
            style: Default::default(),
        };
        Self {
            ui_font: font(&fonts.sans, f32::from(fonts.sans_weight)),
            buffer_font: font(&fonts.monospace, FontWeight::NORMAL.0),
            ui_font_size: px(fonts.size),
            buffer_font_size: px(fonts.monospace_size),
            ui_density: UiDensity::default(),
        }
    }

    /// These settings at another density.
    pub fn with_density(mut self, density: UiDensity) -> Self {
        self.ui_density = density;
        self
    }
}

impl Default for McsapiThemeSettings {
    fn default() -> Self {
        Self::new(&mcsapi_theme::Fonts::default())
    }
}

impl ThemeSettingsProvider for McsapiThemeSettings {
    fn ui_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.ui_font
    }

    fn buffer_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.buffer_font
    }

    fn ui_font_size(&self, _: &App) -> Pixels {
        self.ui_font_size
    }

    fn buffer_font_size(&self, _: &App) -> Pixels {
        self.buffer_font_size
    }

    fn ui_density(&self, _: &App) -> UiDensity {
        self.ui_density
    }
}

/// Makes `theme` the theme and fonts every `ui` component in `cx` draws
/// with. Call it again after the user changes the theme; open windows pick
/// the new colors up on their next frame.
pub fn init_mcsapi(theme: &mcsapi_theme::Theme, cx: &mut App) {
    install(
        theme_from_mcsapi(theme),
        McsapiThemeSettings::new(&theme.fonts),
        cx,
    );
}

/// Makes `theme` and `settings` active in `cx`, for a theme built by
/// [`theme_from_tokens`] or adjusted after [`theme_from_mcsapi`].
pub fn install(theme: Theme, settings: McsapiThemeSettings, cx: &mut App) {
    *cx.default_global::<crate::GlobalSystemAppearance>() =
        crate::GlobalSystemAppearance(SystemAppearance(theme.appearance));
    GlobalTheme::update_theme(cx, Arc::new(theme));
    set_theme_settings_provider(Box::new(settings), cx);
    cx.refresh_windows();
}

/// Whether [`install`] or [`init_mcsapi`] already ran in `cx`.
pub fn is_installed(cx: &App) -> bool {
    cx.has_global::<GlobalTheme>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_roles_follow_the_tokens() {
        let source = mcsapi_theme::Theme::light();
        let tokens = source.tokens();
        let theme = theme_from_mcsapi(&source);
        assert_eq!(theme.appearance, Appearance::Light);
        assert_eq!(theme.name.as_ref(), "Derisk Light");
        let colors = theme.colors();
        assert_eq!(colors.background, hsla(tokens.background));
        assert_eq!(colors.text, hsla(tokens.foreground));
        assert_eq!(colors.border, hsla(tokens.border));
        assert_eq!(colors.border_focused, hsla(tokens.ring));
        assert_eq!(colors.element_hover, hsla(tokens.hover));
        assert_eq!(colors.elevated_surface_background, hsla(tokens.card));
        assert_eq!(theme.status().error_border, hsla(tokens.destructive));
        assert_eq!(theme.players().local().cursor, hsla(tokens.primary));
    }

    #[test]
    fn accent_text_is_legible() {
        let source = mcsapi_theme::Theme::light();
        let tokens = source.tokens();
        let accent = tokens
            .primary
            .legible_on(tokens.background, ACCENT_TEXT_CONTRAST);
        assert!(accent.contrast(tokens.background) >= ACCENT_TEXT_CONTRAST);
        assert_eq!(
            theme_from_mcsapi(&source).colors().text_accent,
            hsla(accent)
        );
    }

    #[test]
    fn dark_themes_keep_zed_dark_defaults_for_unmapped_roles() {
        let theme = theme_from_mcsapi(&mcsapi_theme::Theme::dark());
        assert_eq!(theme.appearance, Appearance::Dark);
        assert_eq!(
            theme.colors().terminal_ansi_red,
            ThemeColors::dark().terminal_ansi_red
        );
    }
}
