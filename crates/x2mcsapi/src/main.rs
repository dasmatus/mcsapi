//! Command-line front end for x2mcsapi.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use x2mcsapi::Style;

const USAGE: &str = "\
Usage:
  x2mcsapi print <css|script|userscript|gtk3|gtk4|qt>
      Print one generated target to standard output.
  x2mcsapi install [DIR]
      Write every target under DIR (default: $XDG_DATA_HOME or ~/.local/share).
  x2mcsapi run [--qt] -- PROGRAM [ARGS...]
      Install, then start PROGRAM with the GTK theme selected. With --qt, also
      pass the Qt style sheet with -stylesheet.";

fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
}

fn fail(message: &str) -> ExitCode {
    eprintln!("x2mcsapi: {message}\n\n{USAGE}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let style = Style::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("print") => {
            let text = match args.get(1).map(String::as_str) {
                Some("css") => x2mcsapi::web_css(&style),
                Some("script") => x2mcsapi::inject_script(&style),
                Some("userscript") => x2mcsapi::userscript(&style),
                Some("gtk3") => x2mcsapi::gtk_css(&style),
                Some("gtk4") => x2mcsapi::gtk4_css(&style),
                Some("qt") => x2mcsapi::qt_stylesheet(&style),
                _ => return fail("print needs a target"),
            };
            print!("{text}");
            ExitCode::SUCCESS
        }
        Some("install") => {
            let Some(dir) = args.get(1).map(PathBuf::from).or_else(data_home) else {
                return fail("no DIR given and neither XDG_DATA_HOME nor HOME is set");
            };
            match x2mcsapi::install(&style, &dir) {
                Ok(paths) => {
                    for path in paths {
                        println!("{}", path.display());
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("x2mcsapi: writing to {}: {error}", dir.display());
                    ExitCode::FAILURE
                }
            }
        }
        Some("run") => {
            let qt = args.get(1).is_some_and(|arg| arg == "--qt");
            let rest = &args[1 + usize::from(qt)..];
            let program = match rest {
                [separator, program, ..] if separator == "--" => program,
                _ => return fail("run needs -- PROGRAM"),
            };
            let Some(dir) = data_home() else {
                return fail("neither XDG_DATA_HOME nor HOME is set");
            };
            if let Err(error) = x2mcsapi::install(&style, &dir) {
                eprintln!("x2mcsapi: writing to {}: {error}", dir.display());
                return ExitCode::FAILURE;
            }
            let mut command = Command::new(program);
            command.args(&rest[2..]).env("GTK_THEME", "x2mcsapi");
            if qt {
                command.arg("-stylesheet").arg(dir.join("qt/x2mcsapi.qss"));
            }
            match command.status() {
                Ok(status) => status
                    .code()
                    .and_then(|code| u8::try_from(code).ok())
                    .map_or(ExitCode::FAILURE, ExitCode::from),
                Err(error) => {
                    eprintln!("x2mcsapi: starting {program}: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => fail("unknown command"),
    }
}
