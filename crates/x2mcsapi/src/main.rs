//! Command-line front end for x2mcsapi.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use tracing::warn;
use tracing_subscriber::EnvFilter;
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

/// Why x2mcsapi stopped.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
enum Error {
    /// The command line is wrong; the usage text is the help.
    #[error("{message}")]
    #[diagnostic(code(x2mcsapi::usage))]
    Usage {
        message: String,
        #[help]
        usage: &'static str,
    },
    #[error("cannot read theme file {name}")]
    #[diagnostic(code(x2mcsapi::theme_read))]
    ThemeRead {
        name: String,
        #[source]
        source: std::io::Error,
    },
    #[error("theme file {name} is invalid")]
    #[diagnostic(code(x2mcsapi::theme_parse))]
    ThemeParse {
        name: String,
        #[source]
        source: mcsapi_theme::ParseError,
    },
    #[error(transparent)]
    #[diagnostic(transparent)]
    Theme(#[from] mcsapi_theme::LoadError),
    #[error("cannot write to {}", dir.display())]
    #[diagnostic(code(x2mcsapi::write))]
    Write {
        dir: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot run {program}")]
    #[diagnostic(code(x2mcsapi::run))]
    Run {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    fn usage(message: impl Into<String>) -> Self {
        Self::Usage {
            message: message.into(),
            usage: USAGE,
        }
    }

    /// Usage errors exit with 2, like other command-line tools, so a script
    /// can tell a wrong invocation from a failed one.
    fn exit_code(&self) -> ExitCode {
        match self {
            Self::Usage { .. } => ExitCode::from(2),
            _ => ExitCode::FAILURE,
        }
    }
}

/// Loads `--theme`'s argument: a path when it names an existing file,
/// otherwise a theme ID.
fn load_theme(name: &str) -> Result<Style, Error> {
    let library = mcsapi_theme::Library::xdg("derisk");
    let path = std::path::Path::new(name);
    let parsed = if path.is_file() {
        let text = std::fs::read_to_string(path).map_err(|source| Error::ThemeRead {
            name: name.to_owned(),
            source,
        })?;
        mcsapi_theme::Theme::parse(&text, |id| library.load(id).ok().map(|p| p.theme)).map_err(
            |source| Error::ThemeParse {
                name: name.to_owned(),
                source,
            },
        )?
    } else {
        library.load(name)?
    };
    for warning in &parsed.warnings {
        warn!(theme = %name, line = warning.line, "{}", warning.message);
    }
    Ok(Style::from_spec(&parsed.theme))
}

/// Logs to standard error, which is free: standard output carries the
/// generated targets and the paths `install` wrote. `RUST_LOG` filters, and
/// `info` applies when it is unset or invalid.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        // A journal or a pipe gets plain text, without colour escapes.
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}

/// Keeps `ExitCode` rather than `miette::Result`: `run` passes the program's
/// own exit status through, and usage errors exit with 2.
fn main() -> ExitCode {
    init_tracing();
    // The usage text in a report's help is laid out already; rewrapped to the
    // terminal it would break mid-line.
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(miette::MietteHandlerOpts::new().wrap_lines(false).build())
    }));
    match run() {
        Ok(code) => code,
        Err(error) => {
            let code = error.exit_code();
            eprintln!("{:?}", miette::Report::new(error));
            code
        }
    }
}

fn run() -> Result<ExitCode, Error> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let style = if args.first().map(String::as_str) == Some("--theme") {
        let Some(name) = args.get(1).cloned() else {
            return Err(Error::usage("--theme needs a theme ID or file"));
        };
        args.drain(..2);
        load_theme(&name)?
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
                _ => return Err(Error::usage("print needs a target")),
            };
            print!("{text}");
            Ok(ExitCode::SUCCESS)
        }
        Some("install") => {
            let Some(dir) = args.get(1).map(PathBuf::from).or_else(data_home) else {
                return Err(Error::usage(
                    "no DIR given and neither XDG_DATA_HOME nor HOME is set",
                ));
            };
            let paths =
                x2mcsapi::install(&style, &dir).map_err(|source| Error::Write { dir, source })?;
            for path in paths {
                println!("{}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Some("run") => {
            let Some(separator) = args.iter().position(|arg| arg == "--") else {
                return Err(Error::usage("run needs -- PROGRAM"));
            };
            let (mut qt, mut electron) = (false, false);
            for flag in &args[1..separator] {
                match flag.as_str() {
                    "--qt" => qt = true,
                    "--electron" => electron = true,
                    _ => return Err(Error::usage(format!("unknown run option {flag}"))),
                }
            }
            let Some((program, program_args)) = args[separator + 1..].split_first() else {
                return Err(Error::usage("run needs -- PROGRAM"));
            };
            let Some(dir) = data_home() else {
                return Err(Error::usage("neither XDG_DATA_HOME nor HOME is set"));
            };
            if let Err(source) = x2mcsapi::install(&style, &dir) {
                return Err(Error::Write { dir, source });
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
            let status = child
                .and_then(|mut child| child.wait())
                .map_err(|source| Error::Run {
                    program: program.clone(),
                    source,
                })?;
            Ok(status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .map_or(ExitCode::FAILURE, ExitCode::from))
        }
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(Error::usage("unknown command")),
    }
}
