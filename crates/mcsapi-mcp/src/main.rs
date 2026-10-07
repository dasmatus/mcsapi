//! Serves the mcsapi MCP tools over stateless Streamable HTTP at `/mcp`.
//!
//! `MCSAPI_MCP_ADDR` sets the listen address (default `127.0.0.1:8787`).
//! `RUST_LOG` filters the log on standard error (default `info`).

use std::io::IsTerminal;

use miette::{IntoDiagnostic, WrapErr};
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Logs to standard error, where the server's messages have always gone; the
/// tools themselves are served over HTTP. `info` applies when `RUST_LOG` is
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

#[tokio::main]
async fn main() -> miette::Result<()> {
    init_tracing();
    let addr = std::env::var("MCSAPI_MCP_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let shutdown = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot listen on {addr}"))?;
    let local = listener.local_addr().into_diagnostic()?;
    info!("mcsapi MCP server on http://{local}/mcp");
    axum::serve(listener, mcsapi_mcp::router(shutdown.clone()))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown.cancel();
        })
        .await
        .into_diagnostic()
        .wrap_err("MCP server stopped")
}
