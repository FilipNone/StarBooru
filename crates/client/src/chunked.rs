//! Sends a file in chunks, as many at a time as the connection bears.
//!
//! A chunk goes out alone first, as the first one can get the file refused.
//! Quick answers then double the number in flight; a slow answer or a failure
//! halves it. That matters most behind Cloudflare over HTTP/3, where all the
//! requests share about 100 KB/s between them: 8 chunks at once took so long
//! that they failed, and one at a time gets through.

use std::collections::VecDeque;
use std::future::Future;
use std::time::Duration;

use bytes::Bytes;
use chrono::Utc;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use reqwest::{Method, header};
use starbooru_core::{Post, Rating, UPLOAD_CHUNK_SIZE, UploadSession, UploadStart};

use crate::{Client, ClientError, Result, Upload, sleep};

const MAX_PARALLEL_CHUNKS: usize = 8;
/// An answer within this many milliseconds means the connection has room.
const QUICK_MS: i64 = 5_000;
/// An answer later than this means too many chunks are sharing it.
const SLOW_MS: i64 = 20_000;
/// How many times each request is sent again after a passing failure.
const RETRIES: u32 = 5;
const CHUNK_TIMEOUT: Duration = Duration::from_secs(120);
/// Finishing includes the server hashing the file and scanning a video.
const FINISH_TIMEOUT: Duration = Duration::from_secs(600);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(30);

/// One file to upload, read a piece at a time as it is sent.
pub struct FileUpload {
    pub file_name: String,
    pub size: u64,
    pub tags: Vec<String>,
    pub rating: Rating,
    pub source: Option<String>,
    /// Whether AI tags the post. `None` leaves it to the server's setting.
    pub autotag: Option<bool>,
}

/// How many chunks to have in flight.
struct Pace {
    parallel: usize,
}

impl Pace {
    fn new() -> Self {
        Self { parallel: 1 }
    }

    fn answered(&mut self, took_ms: i64) {
        if took_ms <= QUICK_MS {
            self.parallel = self.parallel.saturating_mul(2).min(MAX_PARALLEL_CHUNKS);
        } else if took_ms > SLOW_MS {
            self.back_off();
        }
    }

    fn back_off(&mut self) {
        self.parallel = (self.parallel / 2).max(1);
    }
}

/// Whether sending the same request again may work: the network, a proxy in
/// front of the server, or a server that is restarting.
fn is_passing(error: &ClientError) -> bool {
    match error {
        ClientError::Http(_) | ClientError::RateLimited { .. } => true,
        ClientError::Api { status, .. } => {
            matches!(status, 408 | 425 | 502 | 503 | 504 | 520..=527 | 530)
        }
        _ => false,
    }
}

/// How long to wait before sending a request again: 1 s, 2 s, 4 s, …
fn backoff(retry: u32) -> Duration {
    Duration::from_secs(1u64 << retry.min(5))
}

async fn retrying<T, Fut>(mut call: impl FnMut() -> Fut) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
{
    let mut retry = 0;
    loop {
        match call().await {
            Err(e) if is_passing(&e) && retry < RETRIES => {
                sleep(backoff(retry)).await;
                retry += 1;
            }
            other => return other,
        }
    }
}

/// A chunk on its way, and how many times it has been sent before.
#[derive(Clone, Copy)]
struct Chunk {
    index: u32,
    attempt: u32,
}

impl Client {
    /// Uploads a file, calling `read(offset, len)` for each piece of it as it
    /// is sent, so a big file is never in memory whole. Anything that fits in
    /// one chunk goes in one `POST /posts`, as it does to a server too old for
    /// chunks. `sent` hears how many bytes the server has taken so far.
    pub async fn upload_file<F, Fut>(
        &self,
        upload: FileUpload,
        read: F,
        sent: &dyn Fn(u64),
    ) -> Result<Post>
    where
        F: Fn(u64, u64) -> Fut,
        Fut: Future<Output = std::result::Result<Bytes, String>>,
    {
        if upload.size <= UPLOAD_CHUNK_SIZE {
            return self.upload_whole(upload, &read).await;
        }
        let start = UploadStart {
            size: upload.size,
            tags: upload.tags.clone(),
            rating: upload.rating,
            source: upload.source.clone(),
            autotag: upload.autotag,
        };
        let started: Result<UploadSession> =
            retrying(|| self.json(Method::POST, "/uploads", &start)).await;
        let session = match started {
            Err(ClientError::Api { status: 404, .. }) => {
                return self.upload_whole(upload, &read).await;
            }
            other => other?,
        };

        let outcome = match self.send_chunks(&session, upload.size, &read, sent).await {
            Ok(()) => self.finish_upload(&session.id).await,
            Err(e) => Err(e),
        };
        if let Err(e) = &outcome
            && e.code() != Some("upload_gone")
        {
            self.cancel_upload(&session.id).await;
        }
        outcome
    }

    async fn upload_whole<F, Fut>(&self, upload: FileUpload, read: &F) -> Result<Post>
    where
        F: Fn(u64, u64) -> Fut,
        Fut: Future<Output = std::result::Result<Bytes, String>>,
    {
        let bytes = read_exactly(read, 0, upload.size).await?;
        self.upload(Upload {
            file_name: upload.file_name,
            bytes: bytes.to_vec(),
            tags: upload.tags,
            rating: upload.rating,
            source: upload.source,
            autotag: upload.autotag,
        })
        .await
    }

    async fn send_chunks<F, Fut>(
        &self,
        session: &UploadSession,
        size: u64,
        read: &F,
        sent: &dyn Fn(u64),
    ) -> Result<()>
    where
        F: Fn(u64, u64) -> Fut,
        Fut: Future<Output = std::result::Result<Bytes, String>>,
    {
        let mut pace = Pace::new();
        // Bytes of the chunks the server has taken, in whatever order.
        let mut taken = 0u64;
        let mut next_index = 0;
        let mut retries = VecDeque::new();
        let mut flying = FuturesUnordered::new();
        loop {
            while flying.len() < pace.parallel {
                let chunk = retries.pop_front().or_else(|| {
                    let index = next_index;
                    (index < session.chunks).then(|| {
                        next_index += 1;
                        Chunk { index, attempt: 0 }
                    })
                });
                let Some(chunk) = chunk else {
                    break;
                };
                flying.push(
                    async move { (chunk, self.send_chunk(session, size, read, chunk).await) },
                );
            }
            let Some((chunk, result)) = flying.next().await else {
                return Ok(());
            };
            match result {
                Ok(took_ms) => {
                    pace.answered(took_ms);
                    let len =
                        starbooru_core::upload_chunk_range(size, session.chunk_size, chunk.index)
                            .map_or(0, |(_, len)| len);
                    taken = taken.saturating_add(len).min(size);
                    sent(taken);
                }
                Err(e) if is_passing(&e) && chunk.attempt < RETRIES => {
                    pace.back_off();
                    retries.push_back(Chunk {
                        index: chunk.index,
                        attempt: chunk.attempt.saturating_add(1),
                    });
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Reads and sends one chunk, and tells how long the server took to take it.
    async fn send_chunk<F, Fut>(
        &self,
        session: &UploadSession,
        size: u64,
        read: &F,
        chunk: Chunk,
    ) -> Result<i64>
    where
        F: Fn(u64, u64) -> Fut,
        Fut: Future<Output = std::result::Result<Bytes, String>>,
    {
        if let Some(retry) = chunk.attempt.checked_sub(1) {
            sleep(backoff(retry)).await;
        }
        let (offset, len) =
            starbooru_core::upload_chunk_range(size, session.chunk_size, chunk.index)
                .ok_or_else(|| ClientError::Read("the server asked for a wrong piece".into()))?;
        let body = read_exactly(read, offset, len).await?;
        let path = format!("/uploads/{}/chunks/{}", session.id, chunk.index);
        let request = self
            .request(Method::PUT, &path)?
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .timeout(CHUNK_TIMEOUT)
            .body(body);
        let started = Utc::now();
        Self::send_empty(request).await?;
        Ok((Utc::now() - started).num_milliseconds())
    }

    async fn finish_upload(&self, id: &str) -> Result<Post> {
        let path = format!("/uploads/{id}/finish");
        retrying(|| async {
            let request = self.request(Method::POST, &path)?.timeout(FINISH_TIMEOUT);
            Self::send(request).await
        })
        .await
    }

    /// Gives up on an upload, so that its file doesn't wait on the server's disk
    /// for the sweep. If this fails too, the sweep gets it.
    async fn cancel_upload(&self, id: &str) {
        let path = format!("/uploads/{id}");
        if let Ok(request) = self.request(Method::DELETE, &path) {
            let _ = Self::send_empty(request.timeout(CANCEL_TIMEOUT)).await;
        }
    }
}

async fn read_exactly<F, Fut>(read: &F, offset: u64, len: u64) -> Result<Bytes>
where
    F: Fn(u64, u64) -> Fut,
    Fut: Future<Output = std::result::Result<Bytes, String>>,
{
    let bytes = read(offset, len).await.map_err(ClientError::Read)?;
    if u64::try_from(bytes.len()).ok() != Some(len) {
        return Err(ClientError::Read(
            "the file changed while it was being sent".into(),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use starbooru_core::{ApiError, upload_chunks};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    use super::*;

    const SESSION_ID: &str = "abc";
    /// Smaller than the server's, so a test doesn't move megabytes.
    const CHUNK: u64 = 256 << 10;

    fn file(size: u64) -> Vec<u8> {
        (0..size).map(|n| (n % 251) as u8).collect()
    }

    fn reader(
        file: Vec<u8>,
    ) -> impl Fn(u64, u64) -> std::future::Ready<std::result::Result<Bytes, String>> {
        move |offset, len| {
            let range = offset as usize..(offset + len) as usize;
            std::future::ready(Ok(Bytes::copy_from_slice(&file[range])))
        }
    }

    fn request(size: u64) -> FileUpload {
        FileUpload {
            file_name: "a.mp4".into(),
            size,
            tags: vec!["cat".into()],
            rating: Rating::default(),
            source: None,
            autotag: Some(false),
        }
    }

    /// A chunked-upload server. `fail` says what to answer a chunk the nth time
    /// it arrives (counting from 1) instead of taking it.
    #[derive(Default)]
    struct Fake {
        log: Mutex<Vec<String>>,
        chunks: Mutex<HashMap<u32, Vec<u8>>>,
        arrivals: Mutex<HashMap<u32, u32>>,
        flying: AtomicUsize,
        most_flying: AtomicUsize,
        /// The body of `POST /uploads`.
        start: Mutex<Option<serde_json::Value>>,
    }

    impl Fake {
        fn logged(&self, what: &str) -> usize {
            self.log
                .lock()
                .unwrap()
                .iter()
                .filter(|l| l.starts_with(what))
                .count()
        }

        fn reassembled(&self, size: u64) -> Vec<u8> {
            let chunks = self.chunks.lock().unwrap();
            let mut out = Vec::new();
            for index in 0..upload_chunks(size, CHUNK) {
                out.extend_from_slice(&chunks[&index]);
            }
            out
        }
    }

    type Fail = fn(index: u32, arrival: u32) -> Option<u16>;

    async fn answer(sock: &mut tokio::net::TcpStream, status: u16, body: &str) {
        let head = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        sock.write_all(head.as_bytes()).await.unwrap();
        sock.write_all(body.as_bytes()).await.unwrap();
        sock.shutdown().await.unwrap();
    }

    fn post_json() -> String {
        serde_json::json!({
            "id": 7, "kind": "video", "mime": "video/mp4", "file_size": 1, "width": 1,
            "height": 1, "sha256": "00", "rating": "safe", "score": 0, "my_vote": 0,
            "source": null, "tags": [], "created_at": "2026-01-01T00:00:00Z",
            "deleted": false, "file_url": "/f", "thumb_url": null,
        })
        .to_string()
    }

    fn api_error(code: &str) -> String {
        serde_json::to_string(&ApiError {
            code: code.into(),
            message: code.into(),
        })
        .unwrap()
    }

    async fn serve_one(mut sock: tokio::net::TcpStream, fake: Arc<Fake>, size: u64, fail: Fail) {
        let mut reader = BufReader::new(&mut sock);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_owned();
        let path = parts.next().unwrap_or_default().to_owned();
        let mut length = 0usize;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).await.unwrap();
            if header.trim().is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).await.unwrap();

        let path = path.trim_start_matches("/api/v1").to_owned();
        fake.log.lock().unwrap().push(format!("{method} {path}"));
        if method == "POST" && path == "/uploads" {
            *fake.start.lock().unwrap() = serde_json::from_slice(&body).ok();
            let session = UploadSession {
                id: SESSION_ID.into(),
                chunk_size: CHUNK,
                chunks: upload_chunks(size, CHUNK),
            };
            return answer(&mut sock, 200, &serde_json::to_string(&session).unwrap()).await;
        }
        if method == "PUT" {
            let index: u32 = path.rsplit('/').next().unwrap().parse().unwrap();
            let now = fake.flying.fetch_add(1, Ordering::SeqCst) + 1;
            fake.most_flying.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            fake.flying.fetch_sub(1, Ordering::SeqCst);
            let arrival = {
                let mut arrivals = fake.arrivals.lock().unwrap();
                let n = arrivals.entry(index).or_insert(0);
                *n += 1;
                *n
            };
            if let Some(status) = fail(index, arrival) {
                return answer(&mut sock, status, "error code: 502").await;
            }
            fake.chunks.lock().unwrap().insert(index, body);
            return answer(&mut sock, 204, "").await;
        }
        if method == "POST" && path.ends_with("/finish") {
            return answer(&mut sock, 200, &post_json()).await;
        }
        if method == "DELETE" {
            return answer(&mut sock, 204, "").await;
        }
        answer(&mut sock, 500, &api_error("unexpected")).await;
    }

    async fn fake_server(size: u64, fail: Fail) -> (Client, Arc<Fake>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = Client::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let fake = Arc::new(Fake::default());
        let shared = fake.clone();
        tokio::spawn(async move {
            loop {
                let (sock, _) = listener.accept().await.unwrap();
                tokio::spawn(serve_one(sock, shared.clone(), size, fail));
            }
        });
        (client, fake)
    }

    const BIG: u64 = UPLOAD_CHUNK_SIZE + 10;

    #[tokio::test]
    async fn sends_a_big_file_in_parallel_chunks() {
        let (client, fake) = fake_server(BIG, |_, _| None).await;
        let bytes = file(BIG);
        let sent = std::cell::RefCell::new(Vec::new());
        let post = client
            .upload_file(request(BIG), reader(bytes.clone()), &|done| {
                sent.borrow_mut().push(done)
            })
            .await
            .unwrap();
        assert_eq!(post.id, 7);
        assert_eq!(fake.reassembled(BIG), bytes);
        // The start request carries the tags and the AI choice, before any chunk.
        let start = fake.start.lock().unwrap().clone().unwrap();
        assert_eq!(start["tags"], serde_json::json!(["cat"]));
        assert_eq!(start["autotag"], false);
        // One report per chunk, each further along, ending at the whole file.
        let sent = sent.into_inner();
        assert_eq!(sent.len() as u32, upload_chunks(BIG, CHUNK));
        assert!(sent.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(sent.last(), Some(&BIG));
        assert_eq!(fake.logged("POST /uploads/abc/finish"), 1);
        assert_eq!(fake.logged("DELETE"), 0);
        let most = fake.most_flying.load(Ordering::SeqCst);
        assert!(most > 1, "chunks went one at a time");
        assert!(most <= MAX_PARALLEL_CHUNKS, "{most} chunks at once");
    }

    #[tokio::test]
    async fn the_first_chunk_goes_alone() {
        let (client, fake) = fake_server(BIG, |_, _| None).await;
        client
            .upload_file(request(BIG), reader(file(BIG)), &|_| {})
            .await
            .unwrap();
        let log = fake.log.lock().unwrap();
        assert_eq!(log[1], "PUT /uploads/abc/chunks/0");
    }

    #[tokio::test]
    async fn sends_a_chunk_again_after_a_proxy_fails() {
        let (client, fake) = fake_server(BIG, |index, arrival| {
            (index == 3 && arrival == 1).then_some(502)
        })
        .await;
        let bytes = file(BIG);
        client
            .upload_file(request(BIG), reader(bytes.clone()), &|_| {})
            .await
            .unwrap();
        assert_eq!(fake.reassembled(BIG), bytes);
        assert_eq!(fake.arrivals.lock().unwrap()[&3], 2);
    }

    #[tokio::test]
    async fn a_refusal_stops_the_upload_and_cleans_up() {
        let (client, fake) = fake_server(BIG, |_, _| Some(415)).await;
        let err = client
            .upload_file(request(BIG), reader(file(BIG)), &|_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::Api { status: 415, .. }), "{err}");
        assert_eq!(fake.logged("PUT"), 1);
        assert_eq!(fake.logged("POST /uploads/abc/finish"), 0);
        assert_eq!(fake.logged("DELETE /uploads/abc"), 1);
    }

    #[tokio::test]
    async fn a_file_that_cannot_be_read_stops_the_upload() {
        let (client, fake) = fake_server(BIG, |_, _| None).await;
        let unreadable = |_, _| std::future::ready(Err("gone".to_owned()));
        let err = client
            .upload_file(request(BIG), unreadable, &|_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::Read(_)), "{err}");
        assert_eq!(fake.logged("PUT"), 0);
        assert_eq!(fake.logged("DELETE"), 1);

        let short = |_, _| std::future::ready(Ok(Bytes::from_static(b"short")));
        let err = client
            .upload_file(request(BIG), short, &|_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::Read(_)), "{err}");
    }

    #[tokio::test]
    async fn a_small_file_goes_in_one_request() {
        let (client, fake) = fake_server(10, |_, _| None).await;
        // The fake doesn't know `/posts`, which answers 500.
        let err = client
            .upload_file(request(10), reader(file(10)), &|_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::Api { status: 500, .. }), "{err}");
        assert_eq!(fake.logged("POST /posts"), 1);
        assert_eq!(fake.logged("POST /uploads"), 0);
    }

    #[tokio::test]
    async fn a_server_without_chunks_gets_the_whole_file() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = Client::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut reader = BufReader::new(&mut sock);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                let mut length = 0usize;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).await.unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).await.unwrap();
                log.lock().unwrap().push((line.clone(), length));
                if line.contains("/uploads") {
                    answer(&mut sock, 404, &api_error("not_found")).await;
                } else {
                    answer(&mut sock, 200, &post_json()).await;
                }
            }
        });
        let post = client
            .upload_file(request(BIG), reader(file(BIG)), &|_| {})
            .await
            .unwrap();
        assert_eq!(post.id, 7);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen[1].0.contains("/posts"), "{:?}", seen[1].0);
        assert!(seen[1].1 as u64 > BIG, "{}", seen[1].1);
    }

    #[test]
    fn pace_rises_with_quick_answers_and_falls_with_slow_ones() {
        let mut pace = Pace::new();
        assert_eq!(pace.parallel, 1);
        // In between: as measured with one chunk over HTTP/3, no room to grow.
        pace.answered(9_000);
        assert_eq!(pace.parallel, 1);
        pace.answered(100);
        assert_eq!(pace.parallel, 2);
        for _ in 0..10 {
            pace.answered(100);
        }
        assert_eq!(pace.parallel, MAX_PARALLEL_CHUNKS);
        pace.answered(30_000);
        assert_eq!(pace.parallel, MAX_PARALLEL_CHUNKS / 2);
        for _ in 0..10 {
            pace.back_off();
        }
        assert_eq!(pace.parallel, 1);
    }

    #[test]
    fn tells_a_passing_failure_from_a_refusal() {
        let api = |status| ClientError::Api {
            status,
            error: ApiError {
                code: "x".into(),
                message: "x".into(),
            },
        };
        for status in [408, 425, 502, 503, 504, 520, 524, 527, 530] {
            assert!(is_passing(&api(status)), "{status}");
        }
        for status in [400, 401, 403, 404, 409, 413, 415, 500, 507] {
            assert!(!is_passing(&api(status)), "{status}");
        }
        assert!(is_passing(&ClientError::RateLimited { retry_after: None }));
        assert!(!is_passing(&ClientError::Read("gone".into())));
    }

    #[test]
    fn backs_off_further_each_time() {
        assert_eq!(backoff(0), Duration::from_secs(1));
        assert_eq!(backoff(4), Duration::from_secs(16));
        assert_eq!(backoff(u32::MAX), Duration::from_secs(32));
    }
}
