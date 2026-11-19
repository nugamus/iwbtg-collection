//! `iwbtg-server`: the IWBTG Collection leaderboard server (docs/server.md).
//!
//! Phase 8 skeleton (#152): binds an address and serves `GET /healthz`.

use std::net::SocketAddr;
use std::process::ExitCode;

use clap::Parser;
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(name = "iwbtg-server", version, about = "IWBTG Collection leaderboard server")]
struct Cli {
    /// Address to listen on (env: IWBTG_BIND).
    #[arg(long, env = "IWBTG_BIND", default_value = "127.0.0.1:8080")]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> ExitCode {
    // JSON logs on stdout (docs/server.md, Logs and monitoring).
    tracing_subscriber::fmt().json().init();
    let cli = Cli::parse();
    match serve(cli.bind).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(%err, "server stopped");
            eprintln!("iwbtg-server: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(bind: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    // Log the bound address, not the requested one: port 0 picks a free port.
    let addr = listener.local_addr()?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, iwbtg_server::app()).await
}
