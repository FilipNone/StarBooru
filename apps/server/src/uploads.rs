//! Chunked uploads under way: a file that is big for one request, or for the
//! proxies in front of the server, arrives a chunk at a time and in any order.
//!
//! Each upload is a file of its final size in the scratch directory, which each
//! chunk is written into at its place. The list is kept in memory as the
//! scratch directory is cleared at every start: after a restart the client
//! hears that the upload is gone and sends the file again.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use starbooru_core::Rating;
use tokio::sync::RwLock;
use tracing::warn;

use crate::state::AppState;

/// Uploads an account can have open. Each may reach the upload limit on disk,
/// so this also caps what one account can fill without making a post. Opening
/// one more drops the one left alone longest, likely from a closed app.
const MAX_OPEN_PER_USER: usize = 8;
/// An upload nothing has reached for this long is dropped with its file.
const IDLE_LIMIT: Duration = Duration::from_secs(3600);
const SWEEP_EVERY: Duration = Duration::from_secs(600);

/// What goes on a new post besides its file.
#[derive(Debug, Clone, Default)]
pub struct PostFields {
    pub tags: Vec<String>,
    pub rating: Rating,
    pub source: Option<String>,
    /// Whether AI tags the post. `None` leaves it to the server's setting.
    pub autotag: Option<bool>,
}

/// What has become of an upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Receiving,
    /// Finished with this new post. Kept so that a client whose answer was lost
    /// on the way can ask again.
    Added(i64),
    /// Finished, and the same file is already this post.
    Exists(i64),
    /// Failed or dropped; its file is gone.
    Gone,
}

pub struct Upload {
    pub id: String,
    pub user_id: i64,
    pub path: PathBuf,
    pub size: u64,
    pub chunk_size: u64,
    pub fields: PostFields,
    /// Chunks hold this shared while writing and finishing holds it alone, so
    /// no chunk can change a file after it has been hashed.
    pub phase: RwLock<Phase>,
    received: Vec<AtomicBool>,
    touched: Mutex<Instant>,
}

impl Upload {
    pub fn new(
        id: String,
        user_id: i64,
        path: PathBuf,
        size: u64,
        chunk_size: u64,
        chunks: u32,
        fields: PostFields,
    ) -> Self {
        Self {
            id,
            user_id,
            path,
            size,
            chunk_size,
            fields,
            phase: RwLock::new(Phase::Receiving),
            received: (0..chunks).map(|_| AtomicBool::new(false)).collect(),
            touched: Mutex::new(Instant::now()),
        }
    }

    pub fn mark_received(&self, index: u32) {
        let slot = usize::try_from(index)
            .ok()
            .and_then(|i| self.received.get(i));
        if let Some(slot) = slot {
            slot.store(true, Ordering::Release);
        }
    }

    /// How many chunks are still to come.
    pub fn missing(&self) -> usize {
        self.received
            .iter()
            .filter(|chunk| !chunk.load(Ordering::Acquire))
            .count()
    }

    fn touch(&self) {
        *self.touched.lock().unwrap_or_else(PoisonError::into_inner) = Instant::now();
    }

    fn touched(&self) -> Instant {
        *self.touched.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ends the upload and removes its file, once no chunk is being written.
    pub async fn discard(&self) {
        *self.phase.write().await = Phase::Gone;
        self.remove_file().await;
    }

    /// Removes the upload's file, which may already be gone or moved into place.
    pub async fn remove_file(&self) {
        match tokio::fs::remove_file(&self.path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => warn!("cannot remove an unfinished upload: {e}"),
        }
    }
}

/// The uploads under way.
#[derive(Default)]
pub struct Uploads {
    open: Mutex<HashMap<String, Arc<Upload>>>,
}

impl Uploads {
    fn open(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Upload>>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds an upload. Returns the one it pushed out, for the caller to discard.
    pub fn add(&self, upload: Arc<Upload>) -> Option<Arc<Upload>> {
        let mut open = self.open();
        let theirs = open.values().filter(|u| u.user_id == upload.user_id);
        let dropped = if theirs.clone().count() >= MAX_OPEN_PER_USER {
            let oldest = theirs.min_by_key(|u| u.touched()).map(|u| u.id.clone());
            oldest.and_then(|id| open.remove(&id))
        } else {
            None
        };
        open.insert(upload.id.clone(), upload);
        dropped
    }

    /// `user_id`'s upload `id`, which counts as using it. Someone else's is as
    /// good as missing.
    pub fn get(&self, id: &str, user_id: i64) -> Option<Arc<Upload>> {
        let upload = self
            .open()
            .get(id)
            .filter(|u| u.user_id == user_id)?
            .clone();
        upload.touch();
        Some(upload)
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Upload>> {
        self.open().remove(id)
    }

    /// Takes the uploads nothing has reached since `cutoff`.
    fn take_idle(&self, cutoff: Instant) -> Vec<Arc<Upload>> {
        let mut open = self.open();
        let idle: Vec<String> = open
            .values()
            .filter(|u| u.touched() < cutoff)
            .map(|u| u.id.clone())
            .collect();
        idle.iter().filter_map(|id| open.remove(id)).collect()
    }
}

/// Drops abandoned uploads, for good.
pub async fn sweep(state: AppState) {
    let mut tick = tokio::time::interval(SWEEP_EVERY);
    loop {
        tick.tick().await;
        let Some(cutoff) = Instant::now().checked_sub(IDLE_LIMIT) else {
            continue;
        };
        for upload in state.uploads.take_idle(cutoff) {
            upload.discard().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upload(id: &str, user_id: i64, chunks: u32) -> Arc<Upload> {
        Arc::new(Upload::new(
            id.into(),
            user_id,
            PathBuf::from("/nowhere"),
            10,
            5,
            chunks,
            PostFields::default(),
        ))
    }

    #[test]
    fn counts_the_chunks_still_to_come() {
        let upload = upload("a", 1, 3);
        assert_eq!(upload.missing(), 3);
        upload.mark_received(1);
        upload.mark_received(1);
        // Past the end: nothing to mark, and no panic.
        upload.mark_received(3);
        upload.mark_received(u32::MAX);
        assert_eq!(upload.missing(), 2);
        upload.mark_received(0);
        upload.mark_received(2);
        assert_eq!(upload.missing(), 0);
    }

    #[test]
    fn an_upload_belongs_to_its_account() {
        let uploads = Uploads::default();
        assert!(uploads.add(upload("a", 1, 1)).is_none());
        assert!(uploads.get("a", 1).is_some());
        assert!(uploads.get("a", 2).is_none());
        assert!(uploads.get("b", 1).is_none());
    }

    #[test]
    fn a_new_upload_pushes_out_the_one_left_alone_longest() {
        let uploads = Uploads::default();
        for n in 0..MAX_OPEN_PER_USER {
            assert!(uploads.add(upload(&format!("u{n}"), 1, 1)).is_none());
            std::thread::sleep(Duration::from_millis(2));
        }
        // Someone else's uploads don't count towards the limit.
        assert!(uploads.add(upload("other", 2, 1)).is_none());
        // The oldest is in use, so the next oldest goes.
        assert!(uploads.get("u0", 1).is_some());
        let dropped = uploads.add(upload("new", 1, 1)).unwrap();
        assert_eq!(dropped.id, "u1");
        assert!(uploads.get("u1", 1).is_none());
        assert!(uploads.get("new", 1).is_some());
    }

    #[test]
    fn takes_only_idle_uploads() {
        let uploads = Uploads::default();
        uploads.add(upload("old", 1, 1));
        std::thread::sleep(Duration::from_millis(5));
        let cutoff = Instant::now();
        std::thread::sleep(Duration::from_millis(5));
        uploads.add(upload("new", 1, 1));
        let idle = uploads.take_idle(cutoff);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].id, "old");
        assert!(uploads.get("old", 1).is_none());
        assert!(uploads.get("new", 1).is_some());
    }
}
