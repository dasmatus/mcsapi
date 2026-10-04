//! Command-line front end for x2mcsapi.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use x2mcsapi::Style;

const USAGE: &str = "\
Usage: x2mcsapi [--theme ID|FILE] COMMAND

  --theme ID|FILE
      Style from this theme: a theme file, or the ID of one under
      derisk/themes in an XDG data directory, or a built-in theme
      (derisk-dark, derisk-light, derisk-high-contrast). Default: derisk-dark.

Commands:
  x2mcsapi print <css|script|userscript|gtk3|gtk4|qt>
      Print one generated target to standard output.
  x2mcsapi install [DIR]
      Write every target under DIR (default: $XDG_DATA_HOME or ~/.local/share).
  x2mcsapi run [--qt] [--electron] -- PROGRAM [ARGS...]
      Install, then start PROGRAM with the GTK theme selected. With --qt, also
      pass the Qt style sheet with -stylesheet. With --electron, inject the
      theme into every window and webview of an Electron or Chromium app.";

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

/// Loads `--theme`'s argument: a path when it names an existing file,
/// otherwise a theme ID.
fn load_theme(name: &str) -> Result<Style, String> {
    let library = mcsapi_theme::Library::xdg("derisk");
    let path = std::path::Path::new(name);
    let parsed = if path.is_file() {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
        mcsapi_theme::Theme::parse(&text, |id| library.load(id).ok().map(|p| p.theme))
            .map_err(|e| format!("{name}:{e}"))?
    } else {
        library.load(name).map_err(|e| e.to_string())?
    };
    for warning in &parsed.warnings {
        eprintln!("x2mcsapi: {name}:{warning}");
    }
    Ok(Style::from_spec(&parsed.theme))
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let style = if args.first().map(String::as_str) == Some("--theme") {
        let Some(name) = args.get(1).cloned() else {
            return fail("--theme needs a theme ID or file");
        };
        args.drain(..2);
        match load_theme(&name) {
            Ok(style) => style,
            Err(error) => {
                eprintln!("x2mcsapi: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        Style::default()
    };
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
            let Some(separator) = args.iter().position(|arg| arg == "--") else {
                return fail("run needs -- PROGRAM");
            };
            let (mut qt, mut electron) = (false, false);
            for flag in &args[1..separator] {
                match flag.as_str() {
                    "--qt" => qt = true,
                    "--electron" => electron = true,
                    _ => return fail(&format!("unknown run option {flag}")),
                }
            }
            let Some((program, program_args)) = args[separator + 1..].split_first() else {
                return fail("run needs -- PROGRAM");
            };
            let Some(dir) = data_home() else {
                return fail("neither XDG_DATA_HOME nor HOME is set");
            };
            if let Err(error) = x2mcsapi::install(&style, &dir) {
                eprintln!("x2mcsapi: writing to {}: {error}", dir.display());
                return ExitCode::FAILURE;
            }
            let mut extra: Vec<std::ffi::OsString> = Vec::new();
            if qt {
                extra.push("-stylesheet".into());
                extra.push(dir.join("qt/x2mcsapi.qss").into());
            }
            let all_args = program_args.iter().map(Into::into).chain(extra);
            let mut command = if electron {
                x2mcsapi::electron::command(program)
            } else {
                Command::new(program)
            };
            command.args(all_args).env("GTK_THEME", "x2mcsapi");
            let child = if electron {
                x2mcsapi::electron::spawn(command, x2mcsapi::inject_script(&style))
            } else {
                command.spawn()
            };
            match child.and_then(|mut child| child.wait()) {
                Ok(status) => status
                    .code()
                    .and_then(|code| u8::try_from(code).ok())
                    .map_or(ExitCode::FAILURE, ExitCode::from),
                Err(error) => {
                    eprintln!("x2mcsapi: running {program}: {error}");
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
