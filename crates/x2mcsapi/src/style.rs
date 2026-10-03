//! The style foreign apps are given, read from the current components.

use std::fmt;

use mcsapi_components::{Button, ButtonSize, Card, Input, Tokens};
use mcsapi_ui::Theme;
use mcsapi_ui::egui::{self, Color32, FontFamily, Rect, Shape, TextStyle};

/// Everything needed to restyle a foreign app.
///
/// Nothing here is a separate design: colors, radii and strokes come from
/// [`Tokens`], sizes are measured by laying out the actual
/// `mcsapi-components` widgets, and font families come from the egui font
/// setup those widgets draw with. Changing a component changes every foreign
/// app with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Component colors by role, plus radii, strokes and font sizes.
    pub tokens: Tokens,
    /// Sizes measured from the rendered components.
    pub geometry: Geometry,
    /// Font families the components draw with.
    pub fonts: Fonts,
}

impl Default for Style {
    fn default() -> Self {
        Self::from_theme(&Theme::default())
    }
}

impl Style {
    /// Reads the style of the components drawn with `theme`.
    pub fn from_theme(theme: &Theme) -> Self {
        Self::from_tokens(Tokens::from_theme(theme))
    }

    /// Reads the style of the components drawn with `tokens`.
    pub fn from_tokens(tokens: Tokens) -> Self {
        let geometry = Geometry::measure(tokens);
        let fonts = Fonts::from_egui(
            &egui::FontDefinitions::default(),
            &egui::Style::default(),
            &tokens,
        );
        Self {
            tokens,
            geometry,
            fonts,
        }
    }

    /// Component colors as [`Rgba`], shared by every target.
    pub fn palette(&self) -> Palette {
        let t = &self.tokens;
        Palette {
            background: t.background.into(),
            foreground: t.foreground.into(),
            card: t.card.into(),
            muted: t.muted.into(),
            muted_foreground: t.muted_foreground.into(),
            primary: t.primary.into(),
            primary_foreground: t.primary_foreground.into(),
            secondary: t.secondary.into(),
            hover: t.hover.into(),
            destructive: t.destructive.into(),
            destructive_foreground: t.destructive_foreground.into(),
            border: t.border.into(),
            ring: t.ring.into(),
            overlay: t.overlay.into(),
            selection: Rgba::from(t.primary).with_alpha(0x55),
        }
    }
}

/// Component colors by shadcn role, as [`Rgba`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Palette {
    /// Page background.
    pub background: Rgba,
    /// Default text.
    pub foreground: Rgba,
    /// Cards, menus and popovers.
    pub card: Rgba,
    /// Subtle fills such as code and tracks.
    pub muted: Rgba,
    /// Placeholders and secondary text.
    pub muted_foreground: Rgba,
    /// Primary buttons, checked controls and links.
    pub primary: Rgba,
    /// Text on a primary fill.
    pub primary_foreground: Rgba,
    /// Ordinary buttons.
    pub secondary: Rgba,
    /// Hovered buttons.
    pub hover: Rgba,
    /// Destructive buttons.
    pub destructive: Rgba,
    /// Text on a destructive fill.
    pub destructive_foreground: Rgba,
    /// Borders and separators.
    pub border: Rgba,
    /// Focus ring.
    pub ring: Rgba,
    /// Backdrop behind dialogs.
    pub overlay: Rgba,
    /// Text selection: the primary color, translucent.
    pub selection: Rgba,
}

/// Sizes of the rendered components, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Height of a default [`Button`].
    pub control_height: f32,
    /// Horizontal padding of a default [`Button`].
    pub control_padding_x: f32,
    /// Height of a small [`Button`], used for compact controls.
    pub small_control_height: f32,
    /// Horizontal padding of a small [`Button`].
    pub small_padding_x: f32,
    /// Horizontal padding inside an [`Input`].
    pub input_padding_x: f32,
    /// Vertical padding inside an [`Input`].
    pub input_padding_y: f32,
    /// Padding inside a [`Card`].
    pub card_padding: f32,
    /// Corner radius of controls.
    pub control_radius: f32,
    /// Corner radius of cards, dialogs and menus.
    pub card_radius: f32,
    /// Width of borders.
    pub border_width: f32,
    /// Width of the focus ring.
    pub ring_width: f32,
}

impl Geometry {
    /// Lays the components out once, off screen, and measures them.
    pub fn measure(tokens: Tokens) -> Self {
        let context = egui::Context::default();
        let mut sizes = None;
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            tokens.install(ui.ctx());
            // An empty label leaves only the padding.
            let button = ui.add(Button::new("")).rect;
            let small = ui.add(Button::new("").size(ButtonSize::Sm)).rect;
            let mut probe = String::from("W");
            let input = ui.add(Input::new(&mut probe).width(200.0)).rect;
            let card = Card::new().show(ui, |ui| ui.max_rect().min);
            sizes = Some((button, small, input, card.response.rect.min, card.inner));
        });
        let text = glyph_rects(&output.shapes);
        // No painter here; drop the font atlas upload.
        output.textures_delta.clear();
        let (button, small, input, card_outer, card_inner) =
            sizes.expect("the measuring frame ran");
        let border_width = tokens.border_stroke().width;
        let (input_padding_x, input_padding_y) = text
            .iter()
            .find(|glyph| input.contains_rect(**glyph))
            .map_or((12.0, 8.0), |glyph| {
                (
                    glyph.left() - input.left() - border_width,
                    glyph.top() - input.top() - border_width,
                )
            });
        Self {
            control_height: button.height(),
            control_padding_x: button.width() / 2.0,
            small_control_height: small.height(),
            small_padding_x: small.width() / 2.0,
            input_padding_x: input_padding_x.round(),
            input_padding_y: input_padding_y.round(),
            card_padding: (card_inner.x - card_outer.x - border_width).round(),
            control_radius: f32::from(tokens.control_radius().nw),
            card_radius: f32::from(tokens.card_radius().nw),
            border_width,
            ring_width: tokens.ring_stroke().width,
        }
    }
}

/// Layout rectangles of the text painted in `shapes`.
fn glyph_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<Rect> {
    fn visit(shape: &Shape, rects: &mut Vec<Rect>) {
        match shape {
            Shape::Text(text) if !text.galley.is_empty() => {
                rects.push(text.galley.rect.translate(text.pos.to_vec2()));
            }
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| visit(shape, rects)),
            _ => {}
        }
    }
    let mut rects = Vec::new();
    for clipped in shapes {
        visit(&clipped.shape, &mut rects);
    }
    rects
}

/// Font families and sizes, most preferred family first.
#[derive(Clone, Debug, PartialEq)]
pub struct Fonts {
    /// Proportional families, ending with a generic CSS family.
    pub proportional: Vec<String>,
    /// Weight of the first proportional family, for example 300 for light.
    pub weight: u16,
    /// Monospace families, ending with a generic CSS family.
    pub monospace: Vec<String>,
    /// Body text size, in pixels.
    pub body_size: f32,
    /// Small text size, in pixels.
    pub small_size: f32,
    /// Monospace text size, in pixels.
    pub monospace_size: f32,
}

impl Fonts {
    /// Maps the egui fonts the components use to installed family names.
    ///
    /// egui names embedded fonts like `Ubuntu-Light`; foreign apps need the
    /// installed family (`Ubuntu`) plus its weight (300). Sizes come from the
    /// component tokens, and the monospace size from egui's text styles.
    pub fn from_egui(
        definitions: &egui::FontDefinitions,
        style: &egui::Style,
        tokens: &Tokens,
    ) -> Self {
        let faces = |family: &FontFamily| -> Vec<(String, Option<u16>)> {
            definitions
                .families
                .get(family)
                .into_iter()
                .flatten()
                .map(|name| font_face(name))
                .collect()
        };
        let names = |faces: Vec<(String, Option<u16>)>, generic: &str| {
            let mut names: Vec<String> = Vec::new();
            for (name, _) in faces {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            names.push(generic.to_owned());
            names
        };
        let body = tokens.body_font();
        let proportional = faces(&body.family);
        let weight = proportional
            .first()
            .and_then(|(_, weight)| *weight)
            .unwrap_or(400);
        Self {
            proportional: names(proportional, "sans-serif"),
            weight,
            monospace: names(faces(&FontFamily::Monospace), "monospace"),
            body_size: body.size,
            small_size: tokens.small_font().size,
            monospace_size: style
                .text_styles
                .get(&TextStyle::Monospace)
                .map_or(12.0, |font| font.size),
        }
    }
}

/// Splits an egui font name such as `NotoEmoji-Regular` into a family name
/// (`Noto Emoji`) and a CSS weight.
fn font_face(name: &str) -> (String, Option<u16>) {
    let weight = |style: &str| match style {
        "Thin" => Some(100),
        "ExtraLight" => Some(200),
        "Light" => Some(300),
        "Regular" => Some(400),
        "Medium" => Some(500),
        "SemiBold" => Some(600),
        "Bold" => Some(700),
        "ExtraBold" => Some(800),
        "Black" => Some(900),
        _ => None,
    };
    let (base, weight) = match name.rsplit_once('-') {
        Some((base, style)) if weight(style).is_some() => (base, weight(style)),
        _ => return (name.to_owned(), None),
    };
    let mut family = String::with_capacity(base.len() + 2);
    let mut previous_lower = false;
    for c in base.chars() {
        if c.is_uppercase() && previous_lower {
            family.push(' ');
        }
        previous_lower = c.is_lowercase();
        family.push(c);
    }
    (family, weight)
}

/// An sRGB color with alpha, formatted as `#rrggbb` or `#rrggbbaa`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha; 255 is opaque.
    pub a: u8,
}

impl Rgba {
    /// Returns this color with a different alpha.
    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Formats as `rgba(r, g, b, a)`, which Qt and GTK 3 accept.
    pub fn css_rgba(self) -> String {
        format!(
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            f32::from(self.a) / 255.0
        )
    }
}

impl From<Color32> for Rgba {
    fn from(c: Color32) -> Self {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        Self { r, g, b, a }
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}
