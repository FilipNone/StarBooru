use std::ops::Deref;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::warn;

use crate::backup::Backups;
use crate::config::Config;
use crate::fetch::Fetcher;
use crate::media::Signer;
use crate::storage::Storage;
use crate::throttle::Guards;

/// Cheap-to-clone handle shared by all handlers.
#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    pub config: Config,
    pub db: PgPool,
    pub storage: Storage,
    pub signer: Signer,
    /// URL import; `None` when it is off or the HTTP client failed to start.
    pub fetcher: Option<Fetcher>,
    /// Sign-in and sign-up limits.
    pub guards: Guards,
    pub backups: Backups,
    pub started_at: DateTime<Utc>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, storage: Storage) -> Self {
        let signer = Signer::new(&config.secret);
        let backups = Backups::new(&config.secret);
        let fetcher = config
            .url_import
            .and_then(|reach| match Fetcher::new(reach) {
                Ok(f) => Some(f),
                Err(e) => {
                    warn!("URL import is off: cannot start the HTTP client: {e}");
                    None
                }
            });
        Self(Arc::new(Inner {
            config,
            db,
            storage,
            signer,
            fetcher,
            guards: Guards::default(),
            backups,
            started_at: Utc::now(),
        }))
    }
}

impl Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}
