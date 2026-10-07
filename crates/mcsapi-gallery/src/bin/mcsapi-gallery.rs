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

use std::io::IsTerminal;

use mcsapi_gallery::{
    PRESETS, find,
    gpui_gallery::{self, Options},
    specimens,
};
use tracing_subscriber::EnvFilter;

/// The command line and what it accepts: printed on `--help`, and the help
/// of a wrong invocation's report.
fn usage() -> String {
    let mut text = String::from(
        "usage: mcsapi-gallery [--theme <preset>] [--search <text>] [widget]\npresets:",
    );
    for preset in &PRESETS {
        text += &format!("\n  {}", preset.name.to_lowercase());
    }
    text += "\nwidgets:";
    for specimen in specimens() {
        text += &format!("\n  {}", specimen.name);
    }
    text
}

/// A command-line argument the gallery does not accept.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{message}")]
#[diagnostic(code(mcsapi_gallery::usage))]
struct UsageError {
    message: String,
    #[help]
    usage: String,
}

fn bad(message: impl Into<String>) -> UsageError {
    UsageError {
        message: message.into(),
        usage: usage(),
    }
}

/// Logs to standard error, filtered by `RUST_LOG`; `info` applies when it is
/// unset or invalid.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        // A journal or a pipe gets plain text, without colour escapes.
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}

fn main() -> miette::Result<()> {
    init_tracing();
    // The usage text in a report's help is laid out already; rewrapped to the
    // terminal it would break mid-line.
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(miette::MietteHandlerOpts::new().wrap_lines(false).build())
    }));
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--theme" => match args.next() {
                Some(name) if PRESETS.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) => {
                    options.preset = Some(name)
                }
                Some(name) => return Err(bad(format!("no theme preset named {name}")).into()),
                None => return Err(bad("--theme needs a preset").into()),
            },
            "--search" => match args.next() {
                Some(text) => options.search = Some(text),
                None => return Err(bad("--search needs text").into()),
            },
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(());
            }
            name if find(name).is_some() => options.selected = Some(name.to_owned()),
            other => return Err(bad(format!("no widget or option named {other}")).into()),
        }
    }
    gpui_gallery::run(options);
    Ok(())
}
