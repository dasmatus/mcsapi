//! Serves the mcsapi MCP tools over stateless Streamable HTTP at `/mcp`.
//!
//! `MCSAPI_MCP_ADDR` sets the listen address (default `127.0.0.1:8787`).

use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = std::env::var("MCSAPI_MCP_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let shutdown = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("mcsapi MCP server on http://{}/mcp", listener.local_addr()?);
    axum::serve(listener, mcsapi_mcp::router(shutdown.clone()))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown.cancel();
        })
        .await
}
