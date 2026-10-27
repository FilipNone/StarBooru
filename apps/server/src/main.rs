//! StarBooru server: JSON API, media storage and web app hosting.

mod auth;
mod config;
mod error;
mod fetch;
mod media;
mod probe;
mod routes;
mod similar;
mod state;
mod storage;
#[cfg(test)]
mod testing;
mod throttle;

use std::process::ExitCode;
use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

type StartupError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck();
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn")),
        )
        // Plain text in containers, so log viewers don't show escape codes.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("fatal: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), StartupError> {
    let config = Config::from_env()?;
    let db = connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    info!("database migrations applied");

    let storage = storage::Storage::open(&config.data_dir)
        .await?
        .with_tools(config.tools.clone());
    let state = AppState::new(config, db, storage);
    let app = routes::router(state.clone());
    tokio::spawn(routes::auto_purge(state.clone()));
    let backfill = state.clone();
    tokio::spawn(async move {
        routes::backfill_media(backfill.clone()).await;
        routes::backfill_hashes(backfill).await;
    });

    let listener = tokio::net::TcpListener::bind(state.config.bind).await?;
    info!("StarBooru listening on http://{}", state.config.bind);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    info!("shut down cleanly");
    Ok(())
}

/// `starbooru-server healthcheck`: probes `/healthz` on the local listener so
/// the container image needs no curl. Exit code 0 means healthy.
fn healthcheck() -> ExitCode {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};

    let bind = std::env::var("STARBOORU_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let port = bind.parse::<SocketAddr>().map(|a| a.port()).unwrap_or(8080);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let timeout = Duration::from_secs(3);
    let probe = || -> std::io::Result<bool> {
        let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
        let mut head = [0u8; 12];
        stream.read_exact(&mut head)?;
        Ok(head.ends_with(b" 200"))
    };
    match probe() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("healthcheck: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The database container may still be starting; retry for about a minute.
async fn connect(url: &str) -> Result<PgPool, StartupError> {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(Duration::from_secs(10))
            .connect(url)
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(e) if attempt < 30 => {
                warn!("database not reachable yet (attempt {attempt}): {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            warn!("cannot listen for Ctrl-C: {e}");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                warn!("cannot listen for SIGTERM: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown requested");
}
