//! sRGB colors with alpha.

use std::{fmt, str::FromStr};

/// An sRGB color with straight (not premultiplied) alpha.
///
/// Written as `#rrggbb`, or `#rrggbbaa` when not opaque. `#rgb` is accepted
/// when parsing.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha; 255 is opaque.
    pub a: u8,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    /// An opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A color with alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Black with the given opacity, as used for dialog backdrops.
    pub const fn black_alpha(a: u8) -> Self {
        Self::rgba(0, 0, 0, a)
    }

    /// This color with a different alpha.
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// The channels as `[r, g, b, a]`.
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// The channels scaled to `0.0..=1.0`, still in sRGB.
    pub fn to_f32(self) -> [f32; 4] {
        self.to_array().map(|c| f32::from(c) / 255.0)
    }

    /// Mixes toward `other` by `t` (0 is `self`, 1 is `other`) per sRGB
    /// channel.
    ///
    /// Rounds exactly like egui's `Color32::lerp_to_gamma`, so tokens derived
    /// here are bit-identical to the ones `mcsapi-components` derived before
    /// the theme engine existed.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let channel = |a: u8, b: u8| {
            let value = (1.0 - t) * f32::from(a) + t * f32::from(b);
            // Same as egui's `fast_round`: the cast saturates out of range.
            (value + 0.5) as u8
        };
        Self::rgba(
            channel(self.r, other.r),
            channel(self.g, other.g),
            channel(self.b, other.b),
            channel(self.a, other.a),
        )
    }

    /// WCAG 2 relative luminance, ignoring alpha.
    pub fn luminance(self) -> f32 {
        let linear = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    /// WCAG 2 contrast ratio against `other`, from 1 to 21, ignoring alpha.
    pub fn contrast(self, other: Self) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        let (light, dark) = if a > b { (a, b) } else { (b, a) };
        (light + 0.05) / (dark + 0.05)
    }

    /// Whether this color reads as light, so text on it should be dark.
    pub fn is_light(self) -> bool {
        // The luminance where black and white text have equal contrast.
        self.luminance() > 0.179
    }

    /// Moves this color toward black or white, whichever is further from
    /// `background`, until it has at least `ratio` contrast against it.
    ///
    /// Returns the first step (in twentieths) that reaches `ratio`, so a color
    /// that already passes is returned unchanged and a hue is kept as long as
    /// possible.
    pub fn legible_on(self, background: Self, ratio: f32) -> Self {
        let target = if background.is_light() {
            Self::BLACK
        } else {
            Self::WHITE
        };
        (0..=20)
            .map(|step| self.mix(target.with_alpha(self.a), step as f32 / 20.0))
            .find(|color| color.contrast(background) >= ratio)
            .unwrap_or(target)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

/// Why a color could not be parsed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, miette::Diagnostic)]
#[error("expected a color like #rrggbb, #rrggbbaa or #rgb")]
#[diagnostic(code(mcsapi_theme::color))]
pub struct ParseColorError;

impl FromStr for Color {
    type Err = ParseColorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let hex = text.trim().strip_prefix('#').ok_or(ParseColorError)?;
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseColorError);
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ParseColorError);
        let nibble = |i: usize| {
            u8::from_str_radix(&hex[i..=i], 16)
                .map(|n| n * 17)
                .map_err(|_| ParseColorError)
        };
        match hex.len() {
            3 => Ok(Self::rgb(nibble(0)?, nibble(1)?, nibble(2)?)),
            6 => Ok(Self::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Ok(Self::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => Err(ParseColorError),
        }
    }
}
