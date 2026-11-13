//! StarBooru server: JSON API, media storage and web app hosting.

mod ai;
mod auth;
mod backup;
#[cfg(test)]
mod bench;
mod config;
mod edit;
mod error;
mod fetch;
mod local_ai;
mod media;
mod ocr;
mod oidc;
mod panels;
mod probe;
mod remote;
mod routes;
mod settings;
mod similar;
mod sources;
mod state;
mod storage;
#[cfg(test)]
mod testing;
mod throttle;
mod uploads;
mod web;

use std::process::ExitCode;
use std::time::Duration;

use axum::serve::ListenerExt;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

type StartupError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("healthcheck") => return healthcheck(),
        Some("precompress") => return precompress(std::env::args().nth(2)),
        _ => {}
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
    match routes::seed_vocab(&db).await {
        Ok(true) => info!("starter tag vocabulary added"),
        Ok(false) => {}
        Err(e) => warn!("starter tag vocabulary skipped: {e}"),
    }

    let storage = storage::Storage::open(&config.data_dir)
        .await?
        .with_tools(config.tools.clone())
        .with_samples(config.samples);
    let state = AppState::new(config, db, storage);
    let app = routes::router(state.clone());
    tokio::spawn(routes::auto_purge(state.clone()));
    tokio::spawn(uploads::sweep(state.clone()));
    tokio::spawn(backup::auto_backup(state.clone()));
    tokio::spawn(routes::run_imports(state.clone()));
    tokio::spawn(routes::run_library_imports(state.clone()));
    tokio::spawn(local_ai::run(state.clone()));
    let backfill = state.clone();
    tokio::spawn(async move {
        routes::backfill_media(backfill.clone()).await;
        routes::backfill_thumbs(backfill.clone()).await;
        routes::backfill_file_dates(backfill.clone()).await;
        routes::backfill_hashes(backfill.clone()).await;
        routes::backfill_colors(backfill.clone()).await;
        routes::backfill_samples(backfill.clone()).await;
        routes::backfill_meta_tags(backfill.clone()).await;
        ocr::backfill(backfill).await;
    });

    // Without this the kernel holds back a short answer for up to 40 ms, waiting
    // for more to send with it (Nagle's algorithm against the client's delayed ACK).
    let listener = tokio::net::TcpListener::bind(state.config.bind)
        .await?
        .tap_io(|stream| {
            if let Err(e) = stream.set_nodelay(true) {
                tracing::debug!("cannot set TCP_NODELAY: {e}");
            }
        });
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

/// `starbooru-server precompress <web dir>`: writes the `.br` and `.gz` copies
/// the server sends instead of compressing each time (`web.rs`). A build step.
fn precompress(dir: Option<String>) -> ExitCode {
    let Some(dir) = dir else {
        eprintln!("usage: starbooru-server precompress <web dir>");
        return ExitCode::FAILURE;
    };
    match web::precompress(std::path::Path::new(&dir)) {
        Ok(done) => {
            println!(
                "precompress: {} files, {} bytes, {} with brotli",
                done.files, done.bytes, done.brotli_bytes
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("precompress: {dir}: {e}");
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
