//! Restyles the web content of Electron and other Chromium-based apps.
//!
//! Packaged Electron apps ignore preload flags and `NODE_OPTIONS`, but they
//! still accept Chromium's `--remote-debugging-port`. [`spawn`] starts the app
//! with a private, randomly chosen DevTools port, then attaches to every
//! window, webview and later navigation and runs the injection script there,
//! so the app's own content follows the theme.
//!
//! The DevTools endpoint listens on loopback only, but any local process can
//! connect to it while the app runs, as with any app started with remote
//! debugging enabled.

use std::ffi::OsStr;
use std::io::{self, BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tracing::warn;
use tungstenite::Message;

/// How long to wait for the app to print its DevTools address.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// Target types whose documents are app content.
const CONTENT_TARGETS: [&str; 3] = ["page", "webview", "iframe"];

/// A command for `program` with remote debugging on.
///
/// Add the app's own arguments and environment, then pass it to [`spawn`].
/// The switch is the first argument because Chromium stops reading switches
/// at `--`.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.arg("--remote-debugging-port=0");
    command
}

/// Starts `command`, built with [`command`], and injects `script` into all of
/// its web content for as long as it runs.
///
/// The app's standard output and error are captured to find the DevTools
/// address and forwarded to this process's. Injection runs on a background
/// thread; failures there are logged as `tracing` warnings and leave the app
/// running unstyled.
pub fn spawn(mut command: Command, script: String) -> io::Result<Child> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (address_tx, address_rx) = mpsc::channel();
    let stdout_tx = address_tx.clone();
    thread::spawn(move || forward(stdout, io::stdout(), stdout_tx));
    thread::spawn(move || forward(stderr, io::stderr(), address_tx));
    thread::spawn(move || {
        let Ok(address) = address_rx.recv_timeout(STARTUP_TIMEOUT) else {
            warn!("the app did not report a DevTools address; content stays unstyled");
            return;
        };
        if let Err(error) = inject(&address, &script) {
            warn!(%address, %error, "cannot inject the theme; content stays unstyled");
        }
    });
    Ok(child)
}

/// Echoes one of the app's output streams and sends the first DevTools
/// address seen on it. Chromium prints the address on standard error, but
/// wrappers such as `xvfb-run` can move it to standard output.
fn forward(input: impl io::Read, mut out: impl io::Write, address: mpsc::Sender<String>) {
    let mut sent = false;
    for line in BufReader::new(input).lines() {
        let Ok(line) = line else { break };
        if !sent && let Some(url) = devtools_address(&line) {
            sent = address.send(url.to_owned()).is_ok();
        }
        let _ = writeln!(out, "{line}");
    }
}

/// Extracts the browser WebSocket URL Chromium prints at startup.
pub fn devtools_address(line: &str) -> Option<&str> {
    line.strip_prefix("DevTools listening on ")
        .map(str::trim)
        .filter(|url| url.starts_with("ws://"))
}

/// Attaches to every content target behind the browser endpoint `address` and
/// runs `script` in it now and on each new document. Returns when the app
/// closes the connection.
pub fn inject(address: &str, script: &str) -> io::Result<()> {
    let mut socket = connect(address)?;
    let mut next_id = 0_u64;
    let mut send = |socket: &mut tungstenite::WebSocket<_>, message: Value| {
        next_id += 1;
        let mut message = message;
        message["id"] = json!(next_id);
        socket
            .send(Message::text(message.to_string()))
            .map_err(io::Error::other)
    };
    // Reports existing targets and every later one as targetCreated events.
    send(
        &mut socket,
        json!({"method": "Target.setDiscoverTargets", "params": {"discover": true}}),
    )?;
    loop {
        let text = match socket.read() {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => continue,
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(());
            }
            // The app exiting drops the connection without a close frame.
            Err(tungstenite::Error::Io(_) | tungstenite::Error::Protocol(_)) => return Ok(()),
            Err(error) => return Err(io::Error::other(error)),
        };
        let Ok(event) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let params = &event["params"];
        match event["method"].as_str() {
            Some("Target.targetCreated") => {
                let info = &params["targetInfo"];
                if is_content(info) {
                    send(
                        &mut socket,
                        json!({
                            "method": "Target.attachToTarget",
                            "params": {"targetId": info["targetId"], "flatten": true},
                        }),
                    )?;
                }
            }
            Some("Target.attachedToTarget") if is_content(&params["targetInfo"]) => {
                let session = &params["sessionId"];
                for message in [
                    // Page events tell us when a new document has loaded.
                    json!({"method": "Page.enable"}),
                    json!({
                        "method": "Page.addScriptToEvaluateOnNewDocument",
                        "params": {"source": script},
                    }),
                    evaluate(script),
                    json!({"method": "Runtime.runIfWaitingForDebugger"}),
                ] {
                    let mut message = message;
                    message["sessionId"] = session.clone();
                    send(&mut socket, message)?;
                }
            }
            // Some Electron builds skip new-document scripts, so also style
            // each document once its DOM is ready. The script is idempotent.
            Some("Page.domContentEventFired") if event["sessionId"].is_string() => {
                let mut message = evaluate(script);
                message["sessionId"] = event["sessionId"].clone();
                send(&mut socket, message)?;
            }
            _ => {}
        }
    }
}

/// Connects to `address`, retrying while the app finishes starting its
/// DevTools server: Electron prints the address before it accepts upgrades.
fn connect(
    address: &str,
) -> io::Result<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>> {
    let mut attempt = 0;
    loop {
        match tungstenite::connect(address) {
            Ok((socket, _)) => return Ok(socket),
            Err(_) if attempt < 50 => {
                attempt += 1;
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(io::Error::other(error)),
        }
    }
}

fn evaluate(script: &str) -> Value {
    json!({"method": "Runtime.evaluate", "params": {"expression": script}})
}

fn is_content(info: &Value) -> bool {
    info["type"]
        .as_str()
        .is_some_and(|kind| CONTENT_TARGETS.contains(&kind))
}
