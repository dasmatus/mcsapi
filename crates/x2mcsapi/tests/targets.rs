use mcsapi_ui::egui::Color32;
use x2mcsapi::{Style, Theme, Tokens};

fn lime_theme() -> Theme {
    Theme {
        accent: Color32::from_rgb(0x12, 0x34, 0x56),
        ..Theme::default()
    }
}

#[test]
fn every_target_uses_the_shell_theme() {
    let style = Style::default();
    let p = style.palette();
    // Default shell background is rgb(15, 23, 42).
    assert_eq!(p.background.to_string(), "#0f172a");
    assert!(x2mcsapi::web_css(&style).contains("--x2mcsapi-background: #0f172a;"));
    assert!(
        x2mcsapi::gtk_css(&style).contains("@define-color window_bg_color rgba(15, 23, 42, 1);")
    );
    assert!(x2mcsapi::qt_stylesheet(&style).contains("background-color: rgba(15, 23, 42, 1);"));
}

#[test]
fn a_theme_change_reaches_every_target() {
    let style = Style::from_theme(&lime_theme());
    assert_ne!(style.tokens, Tokens::default());
    assert!(x2mcsapi::web_css(&style).contains("--x2mcsapi-primary: #123456;"));
    assert!(x2mcsapi::inject_script(&style).contains("#123456"));
    assert!(
        x2mcsapi::gtk_css(&style).contains("@define-color accent_bg_color rgba(18, 52, 86, 1);")
    );
    assert!(x2mcsapi::qt_stylesheet(&style).contains("rgba(18, 52, 86, 1)"));
}

#[test]
fn style_is_read_from_the_components() {
    let style = Style::default();
    assert_eq!(style.tokens, Tokens::default());
    // Measured from mcsapi-components' Button, Input and Card.
    let g = style.geometry;
    assert_eq!((g.control_height, g.control_padding_x), (36.0, 16.0));
    assert_eq!((g.small_control_height, g.small_padding_x), (32.0, 12.0));
    assert_eq!((g.input_padding_x, g.input_padding_y), (12.0, 8.0));
    assert_eq!(g.card_padding, 24.0);
    assert_eq!((g.control_radius, g.card_radius), (6.0, 12.0));
    assert_eq!((g.border_width, g.ring_width), (1.0, 2.0));
    // egui's embedded Ubuntu-Light and Hack, as installed family names.
    assert_eq!(style.fonts.proportional[0], "Ubuntu");
    assert_eq!(style.fonts.weight, 300);
    assert_eq!(style.fonts.monospace[0], "Hack");
    assert_eq!(style.fonts.body_size, 14.0);
    assert!(style.fonts.proportional.contains(&"Noto Emoji".to_owned()));
    assert_eq!(style.fonts.proportional.last().unwrap(), "sans-serif");
}

#[test]
fn a_token_change_reaches_every_target() {
    let style = Style::from_tokens(Tokens {
        radius: 9,
        ..Tokens::default()
    });
    assert_eq!(style.geometry.card_radius, 18.0);
    assert!(x2mcsapi::web_css(&style).contains("--x2mcsapi-radius: 9px;"));
    assert!(x2mcsapi::gtk_css(&style).contains("border-radius: 9px;"));
    assert!(x2mcsapi::qt_stylesheet(&style).contains("border-radius: 9px;"));
}

#[test]
fn script_embeds_css_as_a_safe_string() {
    let script = x2mcsapi::inject_script(&Style::default());
    // The CSS is one string literal: no raw newlines or closing tags leak out.
    let line = script
        .lines()
        .find(|line| line.trim_start().starts_with("const css = "))
        .expect("css literal");
    assert!(line.trim_end().ends_with("\";"));
    assert!(!script.contains("</"));
    assert!(line.contains(r#"\"Ubuntu\""#));
}

#[test]
fn userscript_runs_at_document_start() {
    let script = x2mcsapi::userscript(&Style::default());
    assert!(script.starts_with("// ==UserScript=="));
    assert!(script.contains("@run-at      document-start"));
}

#[test]
fn css_braces_balance() {
    let style = Style::default();
    for (path, text) in x2mcsapi::targets(&style) {
        if path.ends_with(".js") {
            continue;
        }
        let open = text.matches('{').count();
        let close = text.matches('}').count();
        assert_eq!(open, close, "{path}");
    }
}

#[test]
fn install_writes_a_gtk_theme_directory() {
    let dir = std::env::temp_dir().join(format!("x2mcsapi-test-{}", std::process::id()));
    let written = x2mcsapi::install(&Style::default(), &dir).unwrap();
    assert_eq!(written.len(), 6);
    for version in ["gtk-3.0", "gtk-4.0"] {
        let css =
            std::fs::read_to_string(dir.join("themes/x2mcsapi").join(version).join("gtk.css"))
                .unwrap();
        assert!(css.contains("@define-color theme_bg_color"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}
