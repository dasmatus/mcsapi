//! Opens the widget gallery in a GPUI window.
//!
//! ```console
//! $ cargo run -p mcsapi-gallery --features gpui -- [--theme light] [--search forms] [widget]
//! ```
//!
//! `widget` opens one specimen, for example `Button` or `"Alert Dialog"`;
//! without it every widget is listed. `--theme` picks a preset: `desktop`
//! (default), `light`, or `violet`. `--search` starts with a filter, which
//! matches widget names, categories, and API items.

use mcsapi_gallery::{
    PRESETS, find,
    gpui_gallery::{self, Options},
    specimens,
};

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

fn main() {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--theme" => match args.next() {
                Some(name) if PRESETS.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) => {
                    options.preset = Some(name)
                }
                _ => usage(),
            },
            "--search" => options.search = Some(args.next().unwrap_or_else(|| usage())),
            "-h" | "--help" => usage(),
            name if find(name).is_some() => options.selected = Some(name.to_owned()),
            _ => usage(),
        }
    }
    gpui_gallery::run(options);
}
