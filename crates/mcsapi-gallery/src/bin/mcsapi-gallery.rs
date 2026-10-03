//! Opens the widget gallery in a desktop window.
//!
//! ```console
//! $ cargo run -p mcsapi-gallery --features preview -- [--theme light] [--search forms] [widget]
//! ```
//!
//! `widget` opens one specimen, for example `Button` or `"Alert Dialog"`;
//! without it every widget is listed. `--theme` picks a preset: `desktop`
//! (default), `light`, or `violet`. `--search` starts with a filter, which
//! matches widget names, categories, and API items.

use mcsapi_gallery::{Gallery, PRESETS, specimens};
use mcsapi_ui::{App as _, Theme, egui};

struct Window(Gallery);

impl eframe::App for Window {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ui, |ui| self.0.ui(ui, &Theme::default()));
    }
}

fn usage() -> ! {
    eprintln!("usage: mcsapi-gallery [--theme <preset>] [--search <text>] [widget]");
    eprintln!("presets:");
    for preset in &PRESETS {
        eprintln!("  {}", preset.name.to_lowercase());
    }
    eprintln!("widgets:");
    for specimen in specimens() {
        eprintln!("  {}", specimen.name);
    }
    std::process::exit(2);
}

fn main() -> eframe::Result {
    let mut gallery = Gallery::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let ok = match arg.as_str() {
            "--theme" => args.next().is_some_and(|name| gallery.set_preset(&name)),
            "--search" => args.next().map(|query| gallery.search(query)).is_some(),
            "-h" | "--help" => usage(),
            name => gallery.select(Some(name)),
        };
        if !ok {
            usage();
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mcsapi widget gallery")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "mcsapi widget gallery",
        options,
        Box::new(move |_| Ok(Box::new(Window(gallery)))),
    )
}
