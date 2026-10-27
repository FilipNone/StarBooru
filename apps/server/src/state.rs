use std::ops::Deref;
use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::media::Signer;
use crate::storage::Storage;

/// Cheap-to-clone handle shared by all handlers.
#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    pub config: Config,
    pub db: PgPool,
    pub storage: Storage,
    pub signer: Signer,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, storage: Storage) -> Self {
        let signer = Signer::new(&config.secret);
        Self(Arc::new(Inner {
            config,
            db,
            storage,
            signer,
        }))
    }
}

impl Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}
