//! Work that outlives a page: the file upload queue and the server's import
//! jobs. Both live at the app's root, so leaving the Upload or Other boorus
//! page doesn't stop them. Files are read on this device, so uploads stop if
//! the app closes; imports run on the server and finish regardless.

use std::collections::{HashMap, HashSet};

use dioxus::core::spawn_forever;
use dioxus::html::FileData;
use dioxus::prelude::*;
use starbooru_client::{ClientError, FileUpload};
use starbooru_core::{
    ImportJob, ImportRequest, JobState, MAX_IMPORT_BATCH, Meter, Rating, Transfer, parse_tag_list,
};

use crate::pages::notify;
use crate::store;

#[derive(Clone, PartialEq)]
pub enum Status {
    /// Waiting for the user to press Upload; its tags can still change.
    Draft,
    /// Handed to the runner with its final tags.
    Ready(Vec<String>, Rating),
    Uploading,
    Done(i64),
    Failed(String),
}

#[derive(Clone, PartialEq)]
pub enum Source {
    File(FileData),
    Url(String),
}

#[derive(Clone, PartialEq)]
pub struct Item {
    pub key: u64,
    pub source: Source,
    /// Tags for this item only, on top of the shared ones.
    pub tags: String,
    pub status: Status,
    /// How far its upload has got, once the first piece is through.
    pub progress: Option<Transfer>,
}

impl Item {
    pub fn name(&self) -> String {
        match &self.source {
            Source::File(f) => f.name(),
            Source::Url(u) => u.clone(),
        }
    }

    /// Sent or about to be, and not finished.
    fn pending(&self) -> bool {
        matches!(self.status, Status::Ready(..) | Status::Uploading)
    }
}

/// Files and addresses picked on the Upload page.
pub static QUEUE: GlobalSignal<Vec<Item>> = Signal::global(Vec::new);
/// The server's import jobs: running ones and those finished in the last day.
pub static JOBS: GlobalSignal<Vec<ImportJob>> = Signal::global(Vec::new);
static NEXT_KEY: GlobalSignal<u64> = Signal::global(|| 0);
/// Whether the upload runner is going.
static RUNNING: GlobalSignal<bool> = Signal::global(|| false);
/// Set to ask the poller for the job list on its next tick.
static STALE: GlobalSignal<bool> = Signal::global(|| true);
/// Jobs seen running since the list was last idle, for the notice when it is.
static WATCHED: GlobalSignal<HashSet<i64>> = Signal::global(HashSet::new);

/// Seconds between job list checks while a download is going (its progress
/// shows), while some are waiting, and otherwise.
const POLL_RUNNING: u32 = 1;
const POLL_BUSY: u32 = 2;
const POLL_IDLE: u32 = 60;

/// How often to ask for the job list, given what is on it.
fn poll_every(jobs: &[ImportJob]) -> u32 {
    if jobs.iter().any(|j| j.state == JobState::Running) {
        POLL_RUNNING
    } else if jobs.iter().any(|j| j.state.is_active()) {
        POLL_BUSY
    } else {
        POLL_IDLE
    }
}

/// Adds a draft and returns its key.
pub fn add(source: Source, tags: String) -> u64 {
    let key = *NEXT_KEY.peek();
    *NEXT_KEY.write() = key.wrapping_add(1);
    QUEUE.write().push(Item {
        key,
        source,
        tags,
        status: Status::Draft,
        progress: None,
    });
    key
}

pub fn set_tags(key: u64, tags: String) {
    if let Some(item) = QUEUE.write().iter_mut().find(|i| i.key == key) {
        item.tags = tags;
    }
}

/// Drops an item that isn't being uploaded right now.
pub fn remove(key: u64) {
    QUEUE
        .write()
        .retain(|i| i.key != key || i.status == Status::Uploading);
}

/// Empties the list, keeping only the upload in flight.
pub fn clear() {
    QUEUE.write().retain(|i| i.status == Status::Uploading);
}

/// How many uploads and imports are still to finish.
pub fn pending() -> usize {
    let uploads = QUEUE.read().iter().filter(|i| i.pending()).count();
    let imports = JOBS.read().iter().filter(|j| j.state.is_active()).count();
    uploads.saturating_add(imports)
}

/// How many uploads and imports have finished, for pages showing their results.
pub fn finished_count() -> usize {
    let uploads = QUEUE
        .read()
        .iter()
        .filter(|i| matches!(i.status, Status::Done(_)))
        .count();
    let imports = JOBS
        .read()
        .iter()
        .filter(|j| j.state == JobState::Done)
        .count();
    uploads.saturating_add(imports)
}

/// Sends every draft.
pub fn start(shared: &str, rating: Rating) {
    send(shared, rating, |_| true);
}

/// Sends these drafts and leaves any others waiting on the Upload page.
pub fn start_only(keys: &[u64], shared: &str, rating: Rating) {
    send(shared, rating, |key| keys.contains(&key));
}

/// Files go to the upload runner and addresses to the server's import queue,
/// each with the shared tags and rating.
fn send(shared: &str, rating: Rating, wanted: impl Fn(u64) -> bool) {
    let mut batch = Vec::new();
    for item in QUEUE.write().iter_mut() {
        if item.status != Status::Draft || !wanted(item.key) {
            continue;
        }
        let tags = match parse_tag_list(&format!("{shared} {}", item.tags)) {
            Ok(tags) => tags,
            Err(e) => {
                item.status = Status::Failed(e.to_string());
                continue;
            }
        };
        item.status = match &item.source {
            Source::File(_) => Status::Ready(tags, rating),
            Source::Url(url) => {
                batch.push((
                    item.key,
                    ImportRequest {
                        url: url.clone(),
                        tags,
                        rating,
                        source: None,
                    },
                ));
                Status::Uploading
            }
        };
    }
    if !batch.is_empty() {
        spawn_forever(send_imports(batch));
    }
    if !*RUNNING.peek() && QUEUE.peek().iter().any(|i| i.pending()) {
        *RUNNING.write() = true;
        spawn_forever(run_uploads());
    }
}

/// Queues addresses on the server; on success they leave the list and show
/// up as jobs.
async fn send_imports(batch: Vec<(u64, ImportRequest)>) {
    let keys: Vec<u64> = batch.iter().map(|(k, _)| *k).collect();
    let requests = batch.into_iter().map(|(_, r)| r).collect();
    let result = match store::client() {
        Some(client) => queue_imports(&client, requests).await,
        None => Err(t!("activity-signed-out")),
    };
    let mut queue = QUEUE.write();
    match result {
        Ok(()) => queue.retain(|i| !keys.contains(&i.key)),
        Err(message) => {
            for item in queue.iter_mut().filter(|i| keys.contains(&i.key)) {
                item.status = Status::Failed(message.clone());
            }
        }
    }
}

/// Asks the server to import `requests` and adds the new jobs to the list.
pub async fn queue_imports(
    client: &starbooru_client::Client,
    requests: Vec<ImportRequest>,
) -> Result<(), String> {
    for chunk in requests.chunks(MAX_IMPORT_BATCH) {
        let jobs = client
            .queue_imports(chunk.to_vec())
            .await
            .map_err(|e| e.to_string())?;
        WATCHED.write().extend(jobs.iter().map(|j| j.id));
        let list = merge(&JOBS.peek(), jobs);
        *JOBS.write() = list;
    }
    *STALE.write() = true;
    Ok(())
}

/// Uploads files one at a time until none is ready.
async fn run_uploads() {
    let (mut done, mut failed) = (0usize, 0usize);
    loop {
        let next = QUEUE
            .peek()
            .iter()
            .find_map(|i| match (&i.status, &i.source) {
                (Status::Ready(tags, rating), Source::File(file)) => {
                    Some((i.key, file.clone(), tags.clone(), *rating))
                }
                _ => None,
            });
        let Some((key, file, tags, rating)) = next else {
            break;
        };
        set_status(key, Status::Uploading);
        let status = upload(key, file, tags, rating).await;
        match status {
            Status::Done(_) => done = done.saturating_add(1),
            _ => failed = failed.saturating_add(1),
        }
        set_status(key, status);
    }
    *RUNNING.write() = false;
    if done.saturating_add(failed) > 0 {
        notify(summary(done, 0, failed, true));
    }
}

async fn upload(key: u64, file: FileData, tags: Vec<String>, rating: Rating) -> Status {
    let Some(client) = store::client() else {
        return Status::Failed(t!("activity-signed-out"));
    };
    let request = FileUpload {
        file_name: file.name(),
        size: file.size(),
        tags,
        rating,
        source: None,
    };
    let read = |offset, len| crate::util::read_range(&file, offset, len);
    let total = file.size();
    let meter = std::cell::RefCell::new(Meter::new(chrono::Utc::now()));
    let sent = |done: u64| {
        let speed = meter.borrow_mut().seen(done, chrono::Utc::now());
        let progress = Transfer {
            done,
            total: Some(total),
            speed,
        };
        if let Some(item) = QUEUE.write().iter_mut().find(|i| i.key == key) {
            item.progress = Some(progress);
        }
    };
    match client.upload_file(request, read, &sent).await {
        Ok(post) => Status::Done(post.id),
        Err(ClientError::Read(e)) => Status::Failed(t!("upload-unreadable", error = e)),
        Err(e) => Status::Failed(e.to_string()),
    }
}

fn set_status(key: u64, status: Status) {
    if let Some(item) = QUEUE.write().iter_mut().find(|i| i.key == key) {
        item.status = status;
    }
}

/// Fetches the job list on the next check.
pub fn refresh() {
    *STALE.write() = true;
}

/// Cancels a job that hasn't started.
pub async fn cancel(id: i64) -> Result<(), String> {
    let client = store::client().ok_or_else(|| t!("activity-signed-out"))?;
    client.delete_import(id).await.map_err(|e| e.to_string())?;
    JOBS.write().retain(|j| j.id != id);
    Ok(())
}

/// Forgets the finished jobs.
pub async fn clear_jobs() -> Result<(), String> {
    let client = store::client().ok_or_else(|| t!("activity-signed-out"))?;
    client.clear_imports().await.map_err(|e| e.to_string())?;
    JOBS.write().retain(|j| j.state.is_active());
    Ok(())
}

/// Keeps `JOBS` current while signed in, and warns before a web page with
/// uploads left is closed. Mounted once, by the shell.
#[component]
pub fn Activity() -> Element {
    use_future(|| async {
        let mut waited = 0u32;
        loop {
            let every = poll_every(&JOBS.peek());
            if *STALE.peek() || waited >= every {
                *STALE.write() = false;
                waited = 0;
                poll().await;
            }
            wait_ms(1000).await;
            waited = waited.saturating_add(1);
        }
    });
    use_drop(|| JOBS.write().clear());
    #[cfg(feature = "web")]
    use_effect(|| {
        let busy = QUEUE.read().iter().any(|i| i.pending());
        let _ = document::eval(&format!(
            "window.__sbBusy = {busy};
             window.onbeforeunload ??= (e) => {{ if (window.__sbBusy) {{ e.preventDefault(); e.returnValue = ''; }} }};"
        ));
    });
    rsx! {}
}

async fn poll() {
    let Some(client) = store::client() else {
        return;
    };
    // A failed check keeps the old list; the next one tries again.
    let Ok(now) = client.imports().await else {
        return;
    };
    let counts = finished(&mut WATCHED.write(), &now);
    if let Some((done, duplicate, failed)) = counts {
        notify(summary(done, duplicate, failed, false));
    }
    *JOBS.write() = now;
}

/// Waits in the browser and the webviews alike.
pub(crate) async fn wait_ms(ms: u32) {
    let _ = document::eval(&format!(
        "await new Promise((r) => setTimeout(r, {ms})); return true;"
    ))
    .join::<bool>()
    .await;
}

/// `old` with `new` jobs added or replaced, newest first.
fn merge(old: &[ImportJob], new: Vec<ImportJob>) -> Vec<ImportJob> {
    let mut by_id: HashMap<i64, ImportJob> = old.iter().map(|j| (j.id, j.clone())).collect();
    for job in new {
        by_id.insert(job.id, job);
    }
    let mut list: Vec<ImportJob> = by_id.into_values().collect();
    list.sort_by_key(|j| std::cmp::Reverse(j.id));
    list
}

/// Adds `now`'s running jobs to `watched`. Once none is running: how many of
/// the watched ones were imported, already here, and failed, and `watched`
/// starts over.
fn finished(watched: &mut HashSet<i64>, now: &[ImportJob]) -> Option<(usize, usize, usize)> {
    watched.extend(now.iter().filter(|j| j.state.is_active()).map(|j| j.id));
    if watched.is_empty() || now.iter().any(|j| j.state.is_active()) {
        return None;
    }
    let states: HashMap<i64, JobState> = now.iter().map(|j| (j.id, j.state)).collect();
    let (mut done, mut duplicate, mut failed) = (0usize, 0usize, 0usize);
    for id in watched.drain() {
        match states.get(&id) {
            Some(JobState::Done) => done = done.saturating_add(1),
            Some(JobState::Duplicate) => duplicate = duplicate.saturating_add(1),
            Some(JobState::Failed) => failed = failed.saturating_add(1),
            // cancelled, or cleared elsewhere
            _ => {}
        }
    }
    (done.saturating_add(duplicate).saturating_add(failed) > 0).then_some((done, duplicate, failed))
}

fn summary(done: usize, duplicate: usize, failed: usize, uploads: bool) -> String {
    let mut parts = Vec::new();
    if done > 0 || duplicate.saturating_add(failed) == 0 {
        parts.push(if uploads {
            t!("activity-uploaded", count = done)
        } else {
            t!("activity-imported", count = done)
        });
    }
    if duplicate > 0 {
        parts.push(t!("activity-known", count = duplicate));
    }
    if failed > 0 {
        parts.push(t!("activity-failed", count = failed));
    }
    parts.join(" · ")
}

/// The latest job for each source page, for marking search results.
pub fn jobs_by_source(jobs: &[ImportJob]) -> HashMap<String, ImportJob> {
    let mut map: HashMap<String, ImportJob> = HashMap::new();
    for job in jobs {
        let Some(source) = &job.source else { continue };
        if map.get(source).is_none_or(|j| j.id < job.id) {
            map.insert(source.clone(), job.clone());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn job(id: i64, state: JobState, source: Option<&str>) -> ImportJob {
        ImportJob {
            id,
            url: format!("https://example.org/{id}.png"),
            source: source.map(str::to_owned),
            tags: Vec::new(),
            rating: Rating::default(),
            state,
            post_id: None,
            error: None,
            created_at: Utc::now(),
            finished_at: None,
            progress: None,
        }
    }

    #[test]
    fn asks_more_often_while_a_download_shows_its_progress() {
        assert_eq!(poll_every(&[]), POLL_IDLE);
        assert_eq!(poll_every(&[job(1, JobState::Done, None)]), POLL_IDLE);
        assert_eq!(poll_every(&[job(1, JobState::Queued, None)]), POLL_BUSY);
        let jobs = [
            job(1, JobState::Queued, None),
            job(2, JobState::Running, None),
        ];
        assert_eq!(poll_every(&jobs), POLL_RUNNING);
    }

    #[test]
    fn merge_replaces_by_id_newest_first() {
        let old = [
            job(1, JobState::Running, None),
            job(2, JobState::Queued, None),
        ];
        let list = merge(
            &old,
            vec![job(2, JobState::Done, None), job(3, JobState::Queued, None)],
        );
        let ids: Vec<i64> = list.iter().map(|j| j.id).collect();
        assert_eq!(ids, [3, 2, 1]);
        assert_eq!(list[1].state, JobState::Done);
    }

    #[test]
    fn counts_only_when_everything_finished() {
        let mut watched = HashSet::from([3]);
        // 1 and 2 running, 3 (queued from here) already finished
        let first = [
            job(1, JobState::Running, None),
            job(2, JobState::Queued, None),
            job(3, JobState::Failed, None),
            job(4, JobState::Done, None),
        ];
        assert_eq!(finished(&mut watched, &first), None);
        let still = [
            job(1, JobState::Done, None),
            job(2, JobState::Running, None),
        ];
        assert_eq!(finished(&mut watched, &still), None);
        // 1 finished at an earlier check still counts; 4 was never watched
        let now = [
            job(1, JobState::Done, None),
            job(2, JobState::Duplicate, None),
            job(3, JobState::Failed, None),
            job(4, JobState::Done, None),
        ];
        assert_eq!(finished(&mut watched, &now), Some((1, 1, 1)));
        assert!(watched.is_empty());
        // nothing was running since: no notice
        assert_eq!(finished(&mut watched, &now), None);
        // a watched job cancelled or cleared elsewhere isn't counted
        let mut watched = HashSet::from([9]);
        assert_eq!(finished(&mut watched, &[]), None);
    }

    #[test]
    fn latest_job_per_source() {
        let jobs = [
            job(5, JobState::Failed, Some("https://b.org/p/1")),
            job(9, JobState::Done, Some("https://b.org/p/1")),
            job(7, JobState::Queued, Some("https://b.org/p/2")),
            job(8, JobState::Queued, None),
        ];
        let map = jobs_by_source(&jobs);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("https://b.org/p/1").map(|j| j.id), Some(9));
    }
}
