//! Restyles apps that are not built on `mcsapi-ui` so they look coherent with
//! an mcsapi desktop such as derisk.
//!
//! Every output is generated from one [`Style`], whose colors are the shell's
//! own [`Theme`], so a theme change reaches foreign apps without a second
//! palette to keep in sync. Each target is plain text the foreign toolkit
//! already understands:
//!
//! | Target | Function | How it is applied |
//! | --- | --- | --- |
//! | Web pages, Electron, webviews | [`inject_script`] | Evaluated in the page (preload, `executeJavaScript`, DevTools) |
//! | Browser userscript managers | [`userscript`] | Installed in Violentmonkey, Tampermonkey, Greasemonkey |
//! | Any CSS host | [`web_css`] | Linked or injected as a stylesheet |
//! | GTK 3 and GTK 4 | [`gtk_css`], [`gtk4_css`] | Theme directory selected with `GTK_THEME` |
//! | Qt Widgets | [`qt_stylesheet`] | `-stylesheet` argument or `QApplication::setStyleSheet` |
//!
//! ```
//! use x2mcsapi::{Style, inject_script};
//!
//! let script = inject_script(&Style::default());
//! assert!(script.contains("x2mcsapi"));
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt::{self, Write as _};

pub use mcsapi_ui::Theme;
use mcsapi_ui::egui::Color32;

/// Sizes the derisk shell uses, applied to foreign apps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Corner radius of panels and menus, in pixels.
    pub panel_radius: f32,
    /// Corner radius of buttons, entries, and other controls, in pixels.
    pub control_radius: f32,
    /// Gap between neighboring controls, in pixels.
    pub spacing: f32,
    /// Body text size, in pixels.
    pub font_size: f32,
    /// Font families, most preferred first. Generic CSS families are allowed.
    pub font_families: &'static [&'static str],
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            panel_radius: 12.0,
            control_radius: 6.0,
            spacing: 6.0,
            font_size: 14.0,
            // egui's default proportional font, which the shell draws with.
            font_families: &["Ubuntu", "Inter", "Cantarell", "system-ui", "sans-serif"],
        }
    }
}

/// Everything needed to restyle a foreign app.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Style {
    /// Shell colors; the single source of truth for every target.
    pub theme: Theme,
    /// Shell sizes.
    pub metrics: Metrics,
}

impl Style {
    /// Colors derived from [`Style::theme`], shared by every target.
    pub fn palette(&self) -> Palette {
        let t = self.theme;
        Palette {
            background: t.background.into(),
            surface: t.surface.into(),
            hover: mix(t.surface, t.foreground, 0.08).into(),
            foreground: t.foreground.into(),
            muted: mix(t.foreground, t.background, 0.35).into(),
            border: t.border.into(),
            accent: t.accent.into(),
            // Text on the accent uses the darkest theme color for contrast.
            on_accent: t.background.into(),
            selection: Rgba::from(t.accent).with_alpha(0x55),
        }
    }
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

/// Named colors every target maps onto its own vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Palette {
    /// Window background.
    pub background: Rgba,
    /// Controls, cards, menus, and input fields.
    pub surface: Rgba,
    /// Hovered controls.
    pub hover: Rgba,
    /// Body text.
    pub foreground: Rgba,
    /// Secondary and disabled text.
    pub muted: Rgba,
    /// Control borders and separators.
    pub border: Rgba,
    /// Focus rings, links, checked and default controls.
    pub accent: Rgba,
    /// Text drawn on the accent.
    pub on_accent: Rgba,
    /// Text selection background.
    pub selection: Rgba,
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

fn font_list(metrics: &Metrics) -> String {
    let generic = [
        "serif",
        "sans-serif",
        "monospace",
        "system-ui",
        "cursive",
        "fantasy",
    ];
    metrics
        .font_families
        .iter()
        .map(|family| {
            if generic.contains(family) {
                (*family).to_owned()
            } else {
                format!("\"{}\"", family.replace(['"', '\\'], ""))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// CSS for web content: custom properties plus rules for common elements.
///
/// Pages that already use `--x2mcsapi-*` variables pick up the theme
/// directly; other pages are restyled through element selectors. Rules use
/// `!important` because they must win over the page's own stylesheet.
pub fn web_css(style: &Style) -> String {
    let p = style.palette();
    let m = &style.metrics;
    let font = font_list(m);
    let mut css = String::new();
    // Infallible: writing to a String cannot fail.
    let _ = write!(
        css,
        r#":root {{
  color-scheme: dark;
  --x2mcsapi-background: {bg};
  --x2mcsapi-surface: {surface};
  --x2mcsapi-hover: {hover};
  --x2mcsapi-foreground: {fg};
  --x2mcsapi-muted: {muted};
  --x2mcsapi-border: {border};
  --x2mcsapi-accent: {accent};
  --x2mcsapi-on-accent: {on_accent};
  --x2mcsapi-selection: {selection};
  --x2mcsapi-panel-radius: {pr}px;
  --x2mcsapi-control-radius: {cr}px;
  --x2mcsapi-spacing: {sp}px;
  --x2mcsapi-font: {font};
  --x2mcsapi-font-size: {fs}px;
  accent-color: var(--x2mcsapi-accent);
  scrollbar-color: var(--x2mcsapi-border) var(--x2mcsapi-background);
}}
html, body {{
  background: var(--x2mcsapi-background) !important;
  color: var(--x2mcsapi-foreground) !important;
  font-family: var(--x2mcsapi-font) !important;
  font-size: var(--x2mcsapi-font-size);
}}
::selection {{
  background: var(--x2mcsapi-selection) !important;
  color: var(--x2mcsapi-foreground) !important;
}}
a, a:visited {{
  color: var(--x2mcsapi-accent) !important;
}}
button, input, select, textarea, [role="button"] {{
  background: var(--x2mcsapi-surface) !important;
  color: var(--x2mcsapi-foreground) !important;
  border: 1px solid var(--x2mcsapi-border) !important;
  border-radius: var(--x2mcsapi-control-radius) !important;
  font-family: var(--x2mcsapi-font) !important;
  padding: 4px var(--x2mcsapi-spacing);
}}
button:hover, select:hover, [role="button"]:hover {{
  background: var(--x2mcsapi-hover) !important;
}}
button[type="submit"], .primary, [aria-pressed="true"] {{
  background: var(--x2mcsapi-accent) !important;
  color: var(--x2mcsapi-on-accent) !important;
  border-color: var(--x2mcsapi-accent) !important;
}}
:focus-visible {{
  outline: 2px solid var(--x2mcsapi-accent) !important;
  outline-offset: 2px;
}}
input::placeholder, textarea::placeholder, :disabled {{
  color: var(--x2mcsapi-muted) !important;
}}
dialog, [role="dialog"], [role="menu"], [role="listbox"] {{
  background: var(--x2mcsapi-surface) !important;
  color: var(--x2mcsapi-foreground) !important;
  border: 1px solid var(--x2mcsapi-border) !important;
  border-radius: var(--x2mcsapi-panel-radius) !important;
}}
hr {{
  border-color: var(--x2mcsapi-border) !important;
}}
"#,
        bg = p.background,
        surface = p.surface,
        hover = p.hover,
        fg = p.foreground,
        muted = p.muted,
        border = p.border,
        accent = p.accent,
        on_accent = p.on_accent,
        selection = p.selection,
        pr = m.panel_radius,
        cr = m.control_radius,
        sp = m.spacing,
        fs = m.font_size,
    );
    css
}

/// Encodes `text` as a JavaScript string literal that is also safe inside an
/// HTML `<script>` element.
fn js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '<' => out.push_str("\\u003c"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A self-contained script that restyles the page it runs in.
///
/// It adds one `<style id="x2mcsapi">` element, keeps it last in `<head>` so
/// later page styles do not override it, and also styles open shadow roots
/// that support constructable stylesheets. Running it again replaces the
/// earlier style instead of stacking copies.
pub fn inject_script(style: &Style) -> String {
    format!(
        r#"(() => {{
  "use strict";
  const css = {css};
  const id = "x2mcsapi";
  const sheet = typeof CSSStyleSheet === "function" && "replaceSync" in CSSStyleSheet.prototype
    ? new CSSStyleSheet()
    : null;
  if (sheet) sheet.replaceSync(css);
  const adopt = (root) => {{
    if (sheet && root && root.adoptedStyleSheets && !root.adoptedStyleSheets.includes(sheet)) {{
      root.adoptedStyleSheets = [...root.adoptedStyleSheets, sheet];
    }}
  }};
  const apply = () => {{
    const head = document.head || document.documentElement;
    if (!head) return;
    let el = document.getElementById(id);
    if (!el) {{
      el = document.createElement("style");
      el.id = id;
    }}
    if (el.textContent !== css) el.textContent = css;
    if (head.lastElementChild !== el) head.appendChild(el);
  }};
  const walk = (node) => {{
    if (node.shadowRoot) adopt(node.shadowRoot);
    if (node.querySelectorAll) node.querySelectorAll("*").forEach((child) => {{
      if (child.shadowRoot) adopt(child.shadowRoot);
    }});
  }};
  apply();
  if (document.documentElement) walk(document.documentElement);
  if (window.__x2mcsapiObserver) window.__x2mcsapiObserver.disconnect();
  const observer = new MutationObserver((records) => {{
    apply();
    for (const record of records) record.addedNodes.forEach(walk);
  }});
  observer.observe(document.documentElement || document, {{ childList: true, subtree: true }});
  window.__x2mcsapiObserver = observer;
}})();
"#,
        css = js_string(&web_css(style))
    )
}

/// [`inject_script`] wrapped as a userscript that runs on every page at
/// document start.
pub fn userscript(style: &Style) -> String {
    format!(
        "// ==UserScript==\n\
         // @name        x2mcsapi\n\
         // @description Restyles pages to match the mcsapi desktop theme\n\
         // @match       *://*/*\n\
         // @match       file:///*\n\
         // @run-at      document-start\n\
         // @grant       none\n\
         // ==/UserScript==\n\n{}",
        inject_script(style)
    )
}

/// A GTK stylesheet for GTK 3 and GTK 4, including libadwaita color names.
/// GTK 4 themes should use [`gtk4_css`], which adds GTK 4 only rules.
///
/// Save it as `gtk.css` in `themes/<name>/gtk-3.0/` and `gtk-4.0/` under an
/// XDG data directory, then launch apps with `GTK_THEME=<name>`. Libadwaita
/// apps ignore `GTK_THEME` for most widgets, but read the named colors when
/// the file is also installed as `~/.config/gtk-4.0/gtk.css`.
pub fn gtk_css(style: &Style) -> String {
    let p = style.palette();
    let m = &style.metrics;
    let font = font_list(m);
    let mut css = String::from("/* Generated by x2mcsapi from the mcsapi shell theme. */\n");
    let colors = [
        // GTK 3 theme names.
        ("theme_bg_color", p.background),
        ("theme_fg_color", p.foreground),
        ("theme_base_color", p.surface),
        ("theme_text_color", p.foreground),
        ("theme_selected_bg_color", p.accent),
        ("theme_selected_fg_color", p.on_accent),
        ("insensitive_fg_color", p.muted),
        ("borders", p.border),
        // libadwaita names.
        ("window_bg_color", p.background),
        ("window_fg_color", p.foreground),
        ("view_bg_color", p.surface),
        ("view_fg_color", p.foreground),
        ("headerbar_bg_color", p.background),
        ("headerbar_fg_color", p.foreground),
        ("card_bg_color", p.surface),
        ("card_fg_color", p.foreground),
        ("popover_bg_color", p.surface),
        ("popover_fg_color", p.foreground),
        ("dialog_bg_color", p.surface),
        ("dialog_fg_color", p.foreground),
        ("sidebar_bg_color", p.background),
        ("sidebar_fg_color", p.foreground),
        ("accent_color", p.accent),
        ("accent_bg_color", p.accent),
        ("accent_fg_color", p.on_accent),
    ];
    for (name, color) in colors {
        let _ = writeln!(css, "@define-color {name} {};", color.css_rgba());
    }
    let _ = write!(
        css,
        r#"
window, .background {{
  background-color: @theme_bg_color;
  color: @theme_fg_color;
  font-family: {font};
  font-size: {fs}px;
}}
headerbar, .titlebar {{
  background: @theme_bg_color;
  color: @theme_fg_color;
  border-bottom: 1px solid @borders;
  box-shadow: none;
}}
button, entry, spinbutton, combobox button, dropdown > button {{
  background: @theme_base_color;
  color: @theme_fg_color;
  border: 1px solid @borders;
  border-radius: {cr}px;
  box-shadow: none;
  min-height: 24px;
  padding: 4px {sp}px;
}}
button:hover {{
  background: {hover};
}}
button:checked, button.suggested-action {{
  background: @accent_bg_color;
  color: @accent_fg_color;
  border-color: @accent_bg_color;
}}
button:disabled, entry:disabled, label:disabled {{
  color: @insensitive_fg_color;
}}
entry:focus, button:focus {{
  outline: 2px solid @accent_color;
  outline-offset: 1px;
}}
popover > contents, menu, .menu, .popup, tooltip {{
  background: @popover_bg_color;
  color: @popover_fg_color;
  border: 1px solid @borders;
  border-radius: {pr}px;
}}
selection, *:selected, row:selected {{
  background-color: {selection};
  color: @theme_fg_color;
}}
separator {{
  background: @borders;
}}
"#,
        fs = m.font_size,
        cr = m.control_radius,
        pr = m.panel_radius,
        sp = m.spacing,
        hover = p.hover.css_rgba(),
        selection = p.selection.css_rgba(),
    );
    css
}

/// [`gtk_css`] plus GTK 4 only rules, such as the focus ring of entries whose
/// text child holds the focus. GTK 3 rejects these selectors.
pub fn gtk4_css(style: &Style) -> String {
    let mut css = gtk_css(style);
    css.push_str(
        "entry:focus-within {\n  outline: 2px solid @accent_color;\n  outline-offset: 1px;\n}\n",
    );
    css
}

/// A Qt Widgets style sheet (QSS).
///
/// Apply it with `app -stylesheet path.qss` or `QApplication::setStyleSheet`.
/// Qt Quick apps do not read QSS.
pub fn qt_stylesheet(style: &Style) -> String {
    let p = style.palette();
    let m = &style.metrics;
    let font = font_list(m);
    format!(
        r#"/* Generated by x2mcsapi from the mcsapi shell theme. */
QWidget {{
  background-color: {bg};
  color: {fg};
  font-family: {font};
  font-size: {fs}px;
  selection-background-color: {accent};
  selection-color: {on_accent};
}}
QPushButton, QToolButton, QComboBox, QLineEdit, QTextEdit, QPlainTextEdit, QSpinBox, QDoubleSpinBox {{
  background-color: {surface};
  border: 1px solid {border};
  border-radius: {cr}px;
  padding: 4px {sp}px;
}}
QPushButton:hover, QToolButton:hover, QComboBox:hover {{
  background-color: {hover};
}}
QPushButton:default, QPushButton:checked, QToolButton:checked {{
  background-color: {accent};
  color: {on_accent};
  border-color: {accent};
}}
QPushButton:focus, QLineEdit:focus, QComboBox:focus, QTextEdit:focus, QPlainTextEdit:focus {{
  border: 1px solid {accent};
}}
QWidget:disabled {{
  color: {muted};
}}
QMenuBar, QStatusBar, QToolBar {{
  background-color: {bg};
  border: none;
}}
QMenu, QToolTip, QAbstractItemView {{
  background-color: {surface};
  border: 1px solid {border};
}}
QMenu {{
  border-radius: {pr}px;
  padding: 4px;
}}
QMenu::item:selected, QAbstractItemView::item:selected, QMenuBar::item:selected {{
  background-color: {accent};
  color: {on_accent};
}}
QTabBar::tab {{
  background-color: {bg};
  border: 1px solid {border};
  padding: 4px {pad}px;
}}
QTabBar::tab:selected {{
  background-color: {surface};
  border-color: {accent};
}}
QCheckBox::indicator:checked, QRadioButton::indicator:checked {{
  background-color: {accent};
  border: 1px solid {accent};
}}
QScrollBar {{
  background-color: {bg};
}}
QScrollBar::handle {{
  background-color: {border};
  border-radius: {cr}px;
}}
"#,
        bg = p.background.css_rgba(),
        surface = p.surface.css_rgba(),
        hover = p.hover.css_rgba(),
        fg = p.foreground.css_rgba(),
        muted = p.muted.css_rgba(),
        border = p.border.css_rgba(),
        accent = p.accent.css_rgba(),
        on_accent = p.on_accent.css_rgba(),
        fs = m.font_size,
        cr = m.control_radius,
        pr = m.panel_radius,
        sp = m.spacing,
        pad = m.spacing * 2.0,
    )
}

/// Every target, with the file path it is installed at under the output
/// directory used by [`install`].
pub fn targets(style: &Style) -> [(&'static str, String); 6] {
    [
        ("web/x2mcsapi.css", web_css(style)),
        ("web/x2mcsapi.js", inject_script(style)),
        ("web/x2mcsapi.user.js", userscript(style)),
        ("themes/x2mcsapi/gtk-3.0/gtk.css", gtk_css(style)),
        ("themes/x2mcsapi/gtk-4.0/gtk.css", gtk4_css(style)),
        ("qt/x2mcsapi.qss", qt_stylesheet(style)),
    ]
}

/// Writes every target from [`targets`] under `dir`, creating directories.
///
/// With `dir` set to an XDG data directory such as `~/.local/share`,
/// `GTK_THEME=x2mcsapi` selects the generated GTK theme.
pub fn install(style: &Style, dir: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut written = Vec::new();
    for (relative, contents) in targets(style) {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, contents)?;
        written.push(path);
    }
    Ok(written)
}
