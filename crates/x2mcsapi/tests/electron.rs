//! End-to-end check of Electron/Chromium injection against a real browser.
//!
//! Runs when `X2MCSAPI_TEST_BROWSER` names a Chromium or Electron binary, or
//! one is found on the usual paths; otherwise it reports a skip.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use x2mcsapi::{Style, electron};

fn browser() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("X2MCSAPI_TEST_BROWSER") {
        return Some(path.into());
    }
    let candidates = [
        "/opt/pw-browsers/chromium",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

#[test]
fn devtools_address_is_parsed() {
    assert_eq!(
        electron::devtools_address("DevTools listening on ws://127.0.0.1:9222/devtools/browser/x"),
        Some("ws://127.0.0.1:9222/devtools/browser/x")
    );
    assert_eq!(electron::devtools_address("[123:ERROR] something"), None);
}

#[test]
fn app_content_follows_the_theme() {
    let Some(browser) = browser() else {
        eprintln!("skipped: no Chromium or Electron binary found");
        return;
    };
    // A local page that reports its body background once the theme reaches it.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let page = format!("http://{}/", listener.local_addr().unwrap());
    let profile = std::env::temp_dir().join(format!("x2mcsapi-profile-{}", std::process::id()));
    let mut command = electron::command(&browser);
    command
        .arg("--headless=new")
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(page);
    let mut child = electron::spawn(command, x2mcsapi::inject_script(&Style::default())).unwrap();

    let (tx, rx) = mpsc::channel();
    // One thread per connection: Chromium opens idle preconnect sockets.
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let tx = tx.clone();
            std::thread::spawn(move || serve(stream, &tx));
        }
    });
    let request = rx.recv_timeout(Duration::from_secs(60));
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(profile);

    let request = request.expect("page never reported a themed background");
    // Default shell background, rgb(10, 14, 18), URL-encoded.
    assert!(request.contains("rgb(10%2C%2014%2C%2018)"), "{request}");
}

/// Serves the test page, and forwards its report request line to `report`.
fn serve(mut stream: TcpStream, report: &mpsc::Sender<String>) {
    let mut request = String::new();
    if BufReader::new(&stream).read_line(&mut request).is_err() {
        return;
    }
    if request.starts_with("GET /report") {
        let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
        let _ = report.send(request);
        return;
    }
    let body = "<body style='background:white'><script>\
        setInterval(()=>{const c=getComputedStyle(document.body).backgroundColor;\
        if(c!=='rgb(255, 255, 255)')fetch('/report/'+encodeURIComponent(c))},100)\
        </script></body>";
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}
