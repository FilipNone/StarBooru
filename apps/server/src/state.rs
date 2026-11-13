use std::ops::Deref;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::warn;

use crate::backup::Backups;
use crate::config::Config;
use crate::fetch::{Fetcher, Reach};
use crate::local_ai::LocalAi;
use crate::media::Signer;
use crate::ocr::Ocr;
use crate::oidc::Oidc;
use crate::storage::Storage;
use crate::throttle::Guards;
use crate::transfers::Transfers;
use crate::uploads::Uploads;

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
    /// Reaches private addresses too, for what an administrator asks: a site to
    /// import a library from is usually on the same network. Off with `fetcher`.
    pub trusted_fetcher: Option<Fetcher>,
    /// Sign-in and sign-up limits.
    pub guards: Guards,
    /// Chunked uploads under way.
    pub uploads: Uploads,
    pub backups: Backups,
    /// Sign-in through an OpenID Connect provider, when set up.
    pub oidc: Option<Oidc>,
    /// Text in images; `None` when it is off or tesseract is missing.
    pub ocr: Option<Ocr>,
    /// Woken when background imports are queued.
    pub imports: tokio::sync::Notify,
    /// How far each running import's download has got.
    pub transfers: Transfers,
    /// Woken when a library import is queued.
    pub library_imports: tokio::sync::Notify,
    /// The built-in AI models and the process that runs them.
    pub local_ai: LocalAi,
    pub started_at: DateTime<Utc>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, storage: Storage) -> Self {
        let signer = Signer::new(&config.secret);
        let backups = Backups::new(&config.secret);
        let oidc = config.oidc.clone().and_then(|c| match Oidc::new(c) {
            Ok(o) => Some(o),
            Err(e) => {
                warn!("OpenID Connect sign-in is off: cannot start the HTTP client: {e}");
                None
            }
        });
        let fetcher = config
            .url_import
            .and_then(|reach| match Fetcher::new(reach) {
                Ok(f) => Some(f),
                Err(e) => {
                    warn!("URL import is off: cannot start the HTTP client: {e}");
                    None
                }
            });
        let trusted_fetcher = fetcher.as_ref().and_then(|_| Fetcher::new(Reach::Any).ok());
        let ocr = config.ocr.clone().filter(Ocr::available);
        let local_ai = LocalAi::new(config.ai.worker.clone(), &config.data_dir);
        Self(Arc::new(Inner {
            config,
            db,
            storage,
            signer,
            fetcher,
            trusted_fetcher,
            guards: Guards::default(),
            uploads: Uploads::default(),
            backups,
            oidc,
            ocr,
            imports: tokio::sync::Notify::new(),
            transfers: Transfers::default(),
            library_imports: tokio::sync::Notify::new(),
            local_ai,
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
