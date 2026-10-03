//! Restyles apps that are not built on `mcsapi-ui` so they look coherent with
//! an mcsapi desktop such as derisk.
//!
//! Every output is generated from one [`Style`], which is read from the
//! current components rather than designed separately: colors, radii and
//! strokes from `mcsapi-components`' [`Tokens`] (which derive from the shell
//! [`Theme`]), control sizes measured by laying out those components, and font
//! families from the egui setup they draw with. Changing a component or the
//! theme changes every foreign app with it. Each target is plain text the
//! foreign toolkit already understands:
//!
//! | Target | Function | How it is applied |
//! | --- | --- | --- |
//! | Electron and Chromium apps | [`electron::spawn`] | Injected into every window and webview over the DevTools protocol |
//! | Web pages, webviews | [`inject_script`] | Evaluated in the page (preload, `executeJavaScript`, DevTools) |
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

use std::fmt::Write as _;

pub use mcsapi_components::Tokens;
pub use mcsapi_ui::Theme;

pub mod electron;
mod style;

pub use style::{Fonts, Geometry, Palette, Rgba, Style};

fn font_list(families: &[String]) -> String {
    let generic = ["serif", "sans-serif", "monospace", "system-ui"];
    families
        .iter()
        .map(|family| {
            if generic.contains(&family.as_str()) {
                family.clone()
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
/// directly; other pages are restyled through element selectors, which
/// follow the components: ordinary buttons look like a secondary `Button`
/// from `mcsapi-components`, submit buttons like a primary one, fields
/// like an `Input`, and dialogs like a `Dialog`. Rules use `!important`
/// because they must win over the page's own stylesheet.
pub fn web_css(style: &Style) -> String {
    let p = style.palette();
    let g = &style.geometry;
    let f = &style.fonts;
    let mut css = String::new();
    // Infallible: writing to a String cannot fail.
    let _ = write!(
        css,
        r#":root {{
  color-scheme: dark;
  --x2mcsapi-background: {background};
  --x2mcsapi-foreground: {foreground};
  --x2mcsapi-card: {card};
  --x2mcsapi-muted: {muted};
  --x2mcsapi-muted-foreground: {muted_foreground};
  --x2mcsapi-primary: {primary};
  --x2mcsapi-primary-foreground: {primary_foreground};
  --x2mcsapi-secondary: {secondary};
  --x2mcsapi-hover: {hover};
  --x2mcsapi-destructive: {destructive};
  --x2mcsapi-destructive-foreground: {destructive_foreground};
  --x2mcsapi-border: {border};
  --x2mcsapi-ring: {ring};
  --x2mcsapi-overlay: {overlay};
  --x2mcsapi-selection: {selection};
  --x2mcsapi-radius: {control_radius}px;
  --x2mcsapi-card-radius: {card_radius}px;
  --x2mcsapi-border-width: {border_width}px;
  --x2mcsapi-ring-width: {ring_width}px;
  --x2mcsapi-control-height: {control_height}px;
  --x2mcsapi-control-padding-x: {control_padding_x}px;
  --x2mcsapi-small-control-height: {small_control_height}px;
  --x2mcsapi-small-padding-x: {small_padding_x}px;
  --x2mcsapi-input-padding: {input_padding_y}px {input_padding_x}px;
  --x2mcsapi-card-padding: {card_padding}px;
  --x2mcsapi-font: {font};
  --x2mcsapi-font-weight: {weight};
  --x2mcsapi-font-size: {body_size}px;
  --x2mcsapi-small-font-size: {small_size}px;
  --x2mcsapi-mono: {mono};
  --x2mcsapi-mono-size: {mono_size}px;
  accent-color: var(--x2mcsapi-primary);
  scrollbar-color: var(--x2mcsapi-border) var(--x2mcsapi-background);
}}
html, body {{
  background: var(--x2mcsapi-background) !important;
  color: var(--x2mcsapi-foreground) !important;
  font-family: var(--x2mcsapi-font) !important;
  font-weight: var(--x2mcsapi-font-weight);
  font-size: var(--x2mcsapi-font-size);
}}
code, pre, kbd, samp {{
  font-family: var(--x2mcsapi-mono) !important;
  font-size: var(--x2mcsapi-mono-size);
  background: var(--x2mcsapi-muted) !important;
  border-radius: var(--x2mcsapi-radius);
}}
small {{
  font-size: var(--x2mcsapi-small-font-size);
  color: var(--x2mcsapi-muted-foreground) !important;
}}
::selection {{
  background: var(--x2mcsapi-selection) !important;
  color: var(--x2mcsapi-foreground) !important;
}}
a, a:visited {{
  color: var(--x2mcsapi-primary) !important;
}}
button, [role="button"], input[type="button"], input[type="reset"], select {{
  background: var(--x2mcsapi-secondary) !important;
  color: var(--x2mcsapi-foreground) !important;
  border: none !important;
  border-radius: var(--x2mcsapi-radius) !important;
  min-height: var(--x2mcsapi-control-height);
  padding: 0 var(--x2mcsapi-control-padding-x);
  font: inherit;
}}
select {{
  padding: 0 var(--x2mcsapi-small-padding-x);
}}
button:hover, [role="button"]:hover, input[type="button"]:hover, select:hover {{
  background: var(--x2mcsapi-hover) !important;
}}
button[type="submit"], input[type="submit"], .primary, [aria-pressed="true"] {{
  background: var(--x2mcsapi-primary) !important;
  color: var(--x2mcsapi-primary-foreground) !important;
  border: none !important;
  border-radius: var(--x2mcsapi-radius) !important;
  min-height: var(--x2mcsapi-control-height);
  padding: 0 var(--x2mcsapi-control-padding-x);
}}
button[type="submit"]:hover, input[type="submit"]:hover, .primary:hover {{
  background: var(--x2mcsapi-primary) !important;
  filter: brightness(0.9);
}}
.destructive, .danger, [data-variant="destructive"] {{
  background: var(--x2mcsapi-destructive) !important;
  color: var(--x2mcsapi-destructive-foreground) !important;
}}
input:not([type="button"]):not([type="submit"]):not([type="reset"]):not([type="checkbox"]):not([type="radio"]):not([type="range"]), textarea {{
  background: transparent !important;
  color: var(--x2mcsapi-foreground) !important;
  border: var(--x2mcsapi-border-width) solid var(--x2mcsapi-border) !important;
  border-radius: var(--x2mcsapi-radius) !important;
  padding: var(--x2mcsapi-input-padding);
  font: inherit;
}}
:focus-visible, input:focus, textarea:focus {{
  outline: var(--x2mcsapi-ring-width) solid var(--x2mcsapi-ring) !important;
  outline-offset: 0;
}}
input::placeholder, textarea::placeholder {{
  color: var(--x2mcsapi-muted-foreground) !important;
}}
:disabled {{
  opacity: 0.5;
}}
dialog, [role="dialog"], [role="alertdialog"] {{
  background: var(--x2mcsapi-background) !important;
  color: var(--x2mcsapi-foreground) !important;
  border: var(--x2mcsapi-border-width) solid var(--x2mcsapi-border) !important;
  border-radius: var(--x2mcsapi-card-radius) !important;
  padding: var(--x2mcsapi-card-padding);
}}
dialog::backdrop {{
  background: var(--x2mcsapi-overlay);
}}
[role="menu"], [role="listbox"], [role="tooltip"], details, fieldset {{
  background: var(--x2mcsapi-card) !important;
  color: var(--x2mcsapi-foreground) !important;
  border: var(--x2mcsapi-border-width) solid var(--x2mcsapi-border) !important;
  border-radius: var(--x2mcsapi-card-radius) !important;
}}
hr {{
  border: none !important;
  border-top: var(--x2mcsapi-border-width) solid var(--x2mcsapi-border) !important;
}}
table, th, td {{
  border-color: var(--x2mcsapi-border) !important;
}}
"#,
        background = p.background,
        foreground = p.foreground,
        card = p.card,
        muted = p.muted,
        muted_foreground = p.muted_foreground,
        primary = p.primary,
        primary_foreground = p.primary_foreground,
        secondary = p.secondary,
        hover = p.hover,
        destructive = p.destructive,
        destructive_foreground = p.destructive_foreground,
        border = p.border,
        ring = p.ring,
        overlay = p.overlay,
        selection = p.selection,
        control_radius = g.control_radius,
        card_radius = g.card_radius,
        border_width = g.border_width,
        ring_width = g.ring_width,
        control_height = g.control_height,
        control_padding_x = g.control_padding_x,
        small_control_height = g.small_control_height,
        small_padding_x = g.small_padding_x,
        input_padding_y = g.input_padding_y,
        input_padding_x = g.input_padding_x,
        card_padding = g.card_padding,
        font = font_list(&f.proportional),
        weight = f.weight,
        body_size = f.body_size,
        small_size = f.small_size,
        mono = font_list(&f.monospace),
        mono_size = f.monospace_size,
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
    let g = &style.geometry;
    let f = &style.fonts;
    let mut css = String::from("/* Generated by x2mcsapi from mcsapi-components. */\n");
    let colors = [
        // GTK 3 theme names.
        ("theme_bg_color", p.background),
        ("theme_fg_color", p.foreground),
        ("theme_base_color", p.background),
        ("theme_text_color", p.foreground),
        ("theme_selected_bg_color", p.primary),
        ("theme_selected_fg_color", p.primary_foreground),
        ("insensitive_fg_color", p.muted_foreground),
        ("borders", p.border),
        // libadwaita names.
        ("window_bg_color", p.background),
        ("window_fg_color", p.foreground),
        ("view_bg_color", p.background),
        ("view_fg_color", p.foreground),
        ("headerbar_bg_color", p.background),
        ("headerbar_fg_color", p.foreground),
        ("card_bg_color", p.card),
        ("card_fg_color", p.foreground),
        ("popover_bg_color", p.card),
        ("popover_fg_color", p.foreground),
        ("dialog_bg_color", p.background),
        ("dialog_fg_color", p.foreground),
        ("sidebar_bg_color", p.card),
        ("sidebar_fg_color", p.foreground),
        ("accent_color", p.primary),
        ("accent_bg_color", p.primary),
        ("accent_fg_color", p.primary_foreground),
        ("destructive_bg_color", p.destructive),
        ("destructive_fg_color", p.destructive_foreground),
    ];
    for (name, color) in colors {
        let _ = writeln!(css, "@define-color {name} {};", color.css_rgba());
    }
    // GTK min-height excludes padding and border.
    let entry_height = (g.control_height - 2.0 * (g.input_padding_y + g.border_width)).max(0.0);
    let _ = write!(
        css,
        r#"
window, .background {{
  background-color: @theme_bg_color;
  color: @theme_fg_color;
  font-family: {font};
  font-weight: {weight};
  font-size: {body_size}px;
}}
headerbar, .titlebar {{
  background: @theme_bg_color;
  color: @theme_fg_color;
  border-bottom: {border_width}px solid @borders;
  box-shadow: none;
}}
button, combobox button, dropdown > button {{
  background: {secondary};
  color: @theme_fg_color;
  border: none;
  border-radius: {control_radius}px;
  box-shadow: none;
  min-height: {control_height}px;
  padding: 0 {control_padding_x}px;
}}
button:hover {{
  background: {hover};
}}
button.suggested-action, button:checked {{
  background: @accent_bg_color;
  color: @accent_fg_color;
}}
button.destructive-action {{
  background: @destructive_bg_color;
  color: @destructive_fg_color;
}}
entry, spinbutton {{
  background: transparent;
  color: @theme_fg_color;
  border: {border_width}px solid @borders;
  border-radius: {control_radius}px;
  box-shadow: none;
  min-height: {entry_height}px;
  padding: {input_padding_y}px {input_padding_x}px;
}}
button:disabled, entry:disabled, label:disabled {{
  opacity: 0.5;
}}
entry:focus, button:focus {{
  outline: {ring_width}px solid {ring};
  outline-offset: 0;
}}
popover > contents, menu, .menu, .popup, tooltip, .card, frame > border {{
  background: @card_bg_color;
  color: @card_fg_color;
  border: {border_width}px solid @borders;
  border-radius: {card_radius}px;
}}
dialog, messagedialog {{
  background: @dialog_bg_color;
  border-radius: {card_radius}px;
}}
selection, *:selected, row:selected {{
  background-color: {selection};
  color: @theme_fg_color;
}}
separator {{
  background: @borders;
  min-width: {border_width}px;
  min-height: {border_width}px;
}}
textview, .monospace {{
  font-family: {mono};
}}
"#,
        font = font_list(&f.proportional),
        weight = f.weight,
        body_size = f.body_size,
        mono = font_list(&f.monospace),
        secondary = p.secondary.css_rgba(),
        hover = p.hover.css_rgba(),
        ring = p.ring.css_rgba(),
        selection = p.selection.css_rgba(),
        border_width = g.border_width,
        ring_width = g.ring_width,
        control_radius = g.control_radius,
        card_radius = g.card_radius,
        control_height = g.control_height,
        control_padding_x = g.control_padding_x,
        input_padding_x = g.input_padding_x,
        input_padding_y = g.input_padding_y,
    );
    css
}

/// [`gtk_css`] plus GTK 4 only rules, such as the focus ring of entries whose
/// text child holds the focus. GTK 3 rejects these selectors.
pub fn gtk4_css(style: &Style) -> String {
    let mut css = gtk_css(style);
    let _ = write!(
        css,
        "entry:focus-within {{\n  outline: {w}px solid {ring};\n  outline-offset: 0;\n}}\n",
        w = style.geometry.ring_width,
        ring = style.palette().ring.css_rgba(),
    );
    css
}

/// A Qt Widgets style sheet (QSS).
///
/// Apply it with `app -stylesheet path.qss` or `QApplication::setStyleSheet`.
/// Qt Quick apps do not read QSS.
pub fn qt_stylesheet(style: &Style) -> String {
    let p = style.palette();
    let g = &style.geometry;
    let f = &style.fonts;
    // QSS takes a single family.
    let first = |families: &[String]| families.first().cloned().unwrap_or_default();
    // Qt's min-height excludes padding and border.
    let entry_height = (g.control_height - 2.0 * (g.input_padding_y + g.border_width)).max(0.0);
    format!(
        r#"/* Generated by x2mcsapi from mcsapi-components. */
QWidget {{
  background-color: {background};
  color: {foreground};
  font-family: "{font}";
  font-weight: {weight};
  font-size: {body_size}px;
  selection-background-color: {primary};
  selection-color: {primary_foreground};
}}
QPlainTextEdit, QTextEdit[readOnly="true"] {{
  font-family: "{mono}";
}}
QPushButton, QToolButton, QComboBox {{
  background-color: {secondary};
  border: none;
  border-radius: {control_radius}px;
  min-height: {control_height}px;
  padding: 0 {control_padding_x}px;
}}
QPushButton:hover, QToolButton:hover, QComboBox:hover {{
  background-color: {hover};
}}
QPushButton:default, QPushButton:checked, QToolButton:checked {{
  background-color: {primary};
  color: {primary_foreground};
}}
QLineEdit, QTextEdit, QPlainTextEdit, QSpinBox, QDoubleSpinBox {{
  background-color: transparent;
  border: {border_width}px solid {border};
  border-radius: {control_radius}px;
  padding: {input_padding_y}px {input_padding_x}px;
}}
QLineEdit, QSpinBox, QDoubleSpinBox {{
  min-height: {entry_height}px;
}}
QPushButton:focus, QToolButton:focus, QComboBox:focus, QLineEdit:focus, QTextEdit:focus, QPlainTextEdit:focus {{
  border: {ring_width}px solid {ring};
}}
QWidget:disabled {{
  color: {muted_foreground};
}}
QMenuBar, QStatusBar, QToolBar {{
  background-color: {background};
  border: none;
}}
QMenu, QToolTip, QAbstractItemView, QGroupBox {{
  background-color: {card};
  border: {border_width}px solid {border};
  border-radius: {card_radius}px;
}}
QMenu {{
  padding: {input_padding_y}px;
}}
QMenu::item {{
  padding: {input_padding_y}px {input_padding_x}px;
  border-radius: {control_radius}px;
}}
QMenu::item:selected, QAbstractItemView::item:selected, QMenuBar::item:selected {{
  background-color: {hover};
  color: {foreground};
}}
QDialog {{
  background-color: {background};
}}
QTabBar::tab {{
  background-color: {muted};
  color: {muted_foreground};
  border: none;
  padding: 0 {small_padding_x}px;
  min-height: {small_control_height}px;
}}
QTabBar::tab:selected {{
  background-color: {background};
  color: {foreground};
}}
QCheckBox::indicator:checked, QRadioButton::indicator:checked {{
  background-color: {primary};
  border: {border_width}px solid {primary};
}}
QScrollBar {{
  background-color: {background};
}}
QScrollBar::handle {{
  background-color: {border};
  border-radius: {control_radius}px;
}}
"#,
        background = p.background.css_rgba(),
        foreground = p.foreground.css_rgba(),
        card = p.card.css_rgba(),
        muted = p.muted.css_rgba(),
        muted_foreground = p.muted_foreground.css_rgba(),
        primary = p.primary.css_rgba(),
        primary_foreground = p.primary_foreground.css_rgba(),
        secondary = p.secondary.css_rgba(),
        hover = p.hover.css_rgba(),
        border = p.border.css_rgba(),
        ring = p.ring.css_rgba(),
        font = first(&f.proportional),
        mono = first(&f.monospace),
        weight = f.weight,
        body_size = f.body_size,
        border_width = g.border_width,
        ring_width = g.ring_width,
        control_radius = g.control_radius,
        card_radius = g.card_radius,
        control_height = g.control_height,
        control_padding_x = g.control_padding_x,
        small_control_height = g.small_control_height,
        small_padding_x = g.small_padding_x,
        input_padding_x = g.input_padding_x,
        input_padding_y = g.input_padding_y,
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
