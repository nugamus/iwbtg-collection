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

    /// Admin listener address; unset, the review queue is disabled (env: IWBTG_ADMIN_BIND).
    /// Bind it to the tailnet address, never to the public one (security rule 6).
    #[arg(long, env = "IWBTG_ADMIN_BIND")]
    admin_bind: Option<SocketAddr>,

    /// Comma-separated account ids allowed on the admin listener (env: IWBTG_ADMIN_ACCOUNTS).
    #[arg(long, env = "IWBTG_ADMIN_ACCOUNTS")]
    admin_accounts: Option<String>,

    /// Base32 TOTP secret for the admin listener (env: IWBTG_ADMIN_TOTP_SECRET). Never logged.
    #[arg(long, env = "IWBTG_ADMIN_TOTP_SECRET")]
    admin_totp_secret: Option<String>,

    /// Print a new TOTP secret and its otpauth URL, then exit.
    #[arg(long)]
    admin_new_secret: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    // JSON logs on stdout (docs/server.md, Logs and monitoring).
    tracing_subscriber::fmt().json().init();
    let cli = Cli::parse();
    if cli.admin_new_secret {
        let secret = iwbtg_server::admin::new_secret();
        println!("TOTP secret: {secret}");
        println!("otpauth URL: {}", iwbtg_server::admin::otpauth_url(&secret));
        println!("Store it in the authenticator app and as IWBTG_ADMIN_TOTP_SECRET in server.env.");
        return ExitCode::SUCCESS;
    }
    match serve(
        cli.bind,
        &cli.db,
        cli.catalog_dir,
        cli.healthchecks_url,
        cli.admin_bind,
        cli.admin_accounts,
        cli.admin_totp_secret,
    )
    .await
    {
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
    admin_bind: Option<SocketAddr>,
    admin_accounts: Option<String>,
    admin_totp_secret: Option<String>,
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
    // Password-reset email (#160). Unset SMTP_URL/MAIL_FROM only disables delivery; the reset
    // endpoint keeps the same answer, and the rest of the server is unaffected.
    let mailer = iwbtg_server::mail::Mailer::from_env();
    if mailer.is_enabled() {
        tracing::info!("email delivery enabled");
    } else {
        tracing::info!("email delivery disabled (SMTP_URL/MAIL_FROM unset)");
    }
    // The admin listener is opt-in and needs its own credentials. A misconfiguration is fatal:
    // running without the review queue when it was asked for must be visible, never silent.
    let admin = match admin_bind {
        Some(admin_bind) => {
            let accounts = admin_accounts.ok_or("IWBTG_ADMIN_ACCOUNTS is required with IWBTG_ADMIN_BIND")?;
            let ids = accounts
                .split(',')
                .map(|id| id.trim().parse::<i64>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("IWBTG_ADMIN_ACCOUNTS: {error}"))?;
            let secret =
                admin_totp_secret.ok_or("IWBTG_ADMIN_TOTP_SECRET is required with IWBTG_ADMIN_BIND")?;
            let config = iwbtg_server::admin::AdminConfig::new(ids, &secret)
                .map_err(|error| format!("admin listener: {error}"))?;
            Some((admin_bind, config))
        }
        None => {
            if admin_accounts.is_some() || admin_totp_secret.is_some() {
                tracing::warn!("admin settings ignored (IWBTG_ADMIN_BIND unset)");
            }
            None
        }
    };

    let listener = TcpListener::bind(bind).await?;
    // Log the bound address, not the requested one: port 0 picks a free port.
    let addr = listener.local_addr()?;
    tracing::info!(%addr, "listening");
    // Connect info gives the rate limiter a peer address when CF-Connecting-IP is absent
    // (local development); behind cloudflared the header is always the key (#161).
    let app = iwbtg_server::app_with_mailer(pool.clone(), catalog_dir, mailer);
    let public = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>());
    match admin {
        Some((admin_bind, config)) => {
            let admin_listener = TcpListener::bind(admin_bind).await?;
            tracing::info!(
                addr = %admin_listener.local_addr()?,
                "admin listener (tailnet only)"
            );
            // The admin listener shares the pool; its own limiter and mailer are irrelevant
            // (auth's sign-in rate limit keys on the tailnet peer address).
            let state = iwbtg_server::app_state(pool, None, iwbtg_server::mail::Mailer::disabled());
            let admin = axum::serve(
                admin_listener,
                iwbtg_server::admin::app(state, config).into_make_service_with_connect_info::<SocketAddr>(),
            );
            tokio::try_join!(public, admin)?;
        }
        None => public.await?,
    }
    Ok(())
}
