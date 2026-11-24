//! `iwbtg-server`: the IWBTG Collection leaderboard server (docs/server.md).
//!
//! Opens the SQLite database and runs its migrations, then serves the HTTP surface (#153).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(name = "iwbtg-server", version, about = "IWBTG Collection leaderboard server")]
struct Cli {
    /// Address to listen on (env: IWBTG_BIND).
    #[arg(long, env = "IWBTG_BIND", default_value = "127.0.0.1:8080")]
    bind: SocketAddr,

    /// SQLite database file; its folder is created if missing (env: IWBTG_DB).
    #[arg(long, env = "IWBTG_DB", default_value = iwbtg_server::db::DEFAULT_PATH)]
    db: PathBuf,

    /// Folder holding `catalog.json` and `files/<sha256>` to serve (env: IWBTG_CATALOG_DIR).
    /// Unset, the download routes answer 404.
    #[arg(long, env = "IWBTG_CATALOG_DIR")]
    catalog_dir: Option<PathBuf>,

    /// healthchecks.io ping URL, GET at startup and every 5 minutes (env: HEALTHCHECKS_URL).
    /// Unset, no pings are sent.
    #[arg(long, env = "HEALTHCHECKS_URL")]
    healthchecks_url: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    // JSON logs on stdout (docs/server.md, Logs and monitoring).
    tracing_subscriber::fmt().json().init();
    let cli = Cli::parse();
    match serve(cli.bind, &cli.db, cli.catalog_dir, cli.healthchecks_url).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(%err, "server stopped");
            eprintln!("iwbtg-server: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(
    bind: SocketAddr,
    db: &std::path::Path,
    catalog_dir: Option<PathBuf>,
    healthchecks_url: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // A failed open or migration is fatal: never serve on a half-ready database.
    let pool = iwbtg_server::db::connect(db).await?;
    match &catalog_dir {
        Some(dir) => tracing::info!(dir = %dir.display(), "serving catalog"),
        None => tracing::info!("catalog disabled (IWBTG_CATALOG_DIR unset)"),
    }
    // The check URL is not logged: it is a capability that can silence the check.
    match healthchecks_url {
        Some(url) => {
            tracing::info!("healthchecks pings enabled (every 5 minutes)");
            tokio::spawn(iwbtg_server::health::ping_forever(
                url,
                iwbtg_server::health::INTERVAL,
            ));
        }
        None => tracing::info!("healthchecks pings disabled (HEALTHCHECKS_URL unset)"),
    }
    let listener = TcpListener::bind(bind).await?;
    // Log the bound address, not the requested one: port 0 picks a free port.
    let addr = listener.local_addr()?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, iwbtg_server::app(pool, catalog_dir)).await?;
    Ok(())
}
