//! Downloads the built-in models into `<data>/ai/models/<component>/`.
//!
//! Every file is pinned to a commit and a SHA-256. A download resumes where
//! it stopped and a component only counts once all its files are verified.
//! The AI runtime comes the same way when it isn't next to the worker, as it
//! is in the Docker image.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use sha2::{Digest, Sha256};
use starbooru_core::ai_catalog::{Component, ModelFile};
use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::error::{AppError, AppResult};

const MARKER: &str = ".complete";
/// A big model on a slow line takes hours.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(24 * 3600);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!("StarBooru/", env!("CARGO_PKG_VERSION"));

/// The ONNX Runtime release, the same as in the `Dockerfile`.
const RUNTIME_VERSION: &str = "1.31.1";

struct RuntimePin {
    os: &'static str,
    arch: &'static str,
    package: &'static str,
    archive_sha256: &'static str,
    archive_size: u64,
    library_sha256: &'static str,
}

const RUNTIMES: &[RuntimePin] = &[
    RuntimePin {
        os: "linux",
        arch: "x86_64",
        package: "onnxruntime-linux-x64-1.31.1",
        archive_sha256: "c849e36a1ef3e22d4ff217b9d1ce4a3cf5b9467b975610913ae959ad5b16bdc9",
        archive_size: 11_413_655,
        library_sha256: "a96f132e5eb5ca04e23a7d2fc1078fb95aa591e8d71d0f237702ece3a1e0f4ea",
    },
    RuntimePin {
        os: "linux",
        arch: "aarch64",
        package: "onnxruntime-linux-aarch64-1.31.1",
        archive_sha256: "5317505e2da99be65f8e2775ba3c2afd6d2961fd78aba1bd8f56159f48820c78",
        archive_size: 10_332_381,
        library_sha256: "7e17c55d675be73f67a218dbecb55522395f61dafb3da03ebc7ade56a4c2f813",
    },
];

/// Where the runtime goes when it has to be downloaded.
pub fn runtime_library(data_dir: &Path) -> PathBuf {
    data_dir
        .join("ai")
        .join("runtime")
        .join(format!("libonnxruntime-{RUNTIME_VERSION}.so"))
}

/// Makes sure `library` is the pinned runtime, downloading it when it's missing
/// or damaged. Takes a user-readable reason when it can't.
pub async fn install_runtime(library: &Path) -> Result<(), String> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let Some(pin) = RUNTIMES.iter().find(|r| r.os == os && r.arch == arch) else {
        return Err(format!(
            "no AI runtime for {os} on {arch}: set STARBOORU_ORT_LIB to an ONNX Runtime {RUNTIME_VERSION} library"
        ));
    };
    let current = sha256_of(library.to_path_buf()).await;
    if current.is_ok_and(|hash| hash == pin.library_sha256) {
        return Ok(());
    }
    fetch_runtime(library, pin).await.map_err(|e| {
        warn!("downloading the AI runtime failed: {e}");
        "the AI runtime can't be downloaded: check the server's internet access and free disk space"
            .to_owned()
    })
}

async fn fetch_runtime(library: &Path, pin: &'static RuntimePin) -> AppResult<()> {
    let dir = library
        .parent()
        .ok_or_else(|| AppError::Internal("the AI runtime has no folder".into()))?;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| write_error(&e))?;
    let archive = dir.join(format!("{}.tgz", pin.package));
    let part = dir.join(format!("{}.tgz.part", pin.package));
    let url = format!(
        "https://github.com/microsoft/onnxruntime/releases/download/v{RUNTIME_VERSION}/{}.tgz",
        pin.package
    );
    download(
        &url,
        &archive,
        &part,
        pin.archive_sha256,
        pin.archive_size,
        |_| {},
    )
    .await?;
    let unpacked = dir.join("library.part");
    let member = format!("{}/lib/libonnxruntime.so.{RUNTIME_VERSION}", pin.package);
    let (from, to) = (archive.clone(), unpacked.clone());
    let found = tokio::task::spawn_blocking(move || extract_file(&from, &member, &to))
        .await
        .map_err(AppError::internal)?
        .map_err(|e| write_error(&e));
    let _ = tokio::fs::remove_file(&archive).await;
    if !found? || sha256_of(unpacked.clone()).await? != pin.library_sha256 {
        let _ = tokio::fs::remove_file(&unpacked).await;
        return Err(AppError::Upstream(
            "the AI runtime is not the expected one".into(),
        ));
    }
    tokio::fs::rename(&unpacked, library)
        .await
        .map_err(|e| write_error(&e))
}

/// Copies one regular file out of a `.tgz` to `dest`. Whether it was there.
fn extract_file(archive: &Path, member: &str, dest: &Path) -> std::io::Result<bool> {
    let file = std::fs::File::open(archive)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in tar.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_file() && entry.path()?.as_ref() == Path::new(member) {
            std::io::copy(&mut entry, &mut std::fs::File::create(dest)?)?;
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn component_dir(models: &Path, component: &Component) -> PathBuf {
    models.join(component.id)
}

/// A component is there when its marker is and its files have the right size.
pub fn is_installed(models: &Path, component: &Component) -> bool {
    let dir = component_dir(models, component);
    dir.join(MARKER).is_file()
        && component
            .files
            .iter()
            .all(|f| std::fs::metadata(dir.join(f.name())).is_ok_and(|m| m.len() == f.size))
}

/// Bytes of a component already on disk, for showing progress.
pub fn bytes_present(models: &Path, component: &Component) -> u64 {
    let dir = component_dir(models, component);
    component
        .files
        .iter()
        .map(|f| {
            std::fs::metadata(dir.join(f.name()))
                .or_else(|_| std::fs::metadata(part_path(&dir, f)))
                .map_or(0, |m| m.len())
        })
        .sum()
}

fn part_path(dir: &Path, file: &ModelFile) -> PathBuf {
    dir.join(format!("{}.part", file.name()))
}

pub async fn remove(models: &Path, component: &Component) -> AppResult<()> {
    let dir = component_dir(models, component);
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::internal(e)),
    }
}

fn client() -> AppResult<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // sqlx already uses ring; Err means a provider is installed already.
            let _ = rustls::crypto::ring::default_provider().install_default();
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(DOWNLOAD_TIMEOUT)
                .build()
                .map_err(|e| warn!("cannot start the model download client: {e}"))
                .ok()
        })
        .as_ref()
        .ok_or_else(|| AppError::Internal("the model download client failed to start".into()))
}

fn write_error(e: &std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::StorageFull {
        AppError::Upstream("not enough disk space for this model".into())
    } else {
        AppError::Upstream("the model can't be saved".into())
    }
}

fn network_error(_: reqwest::Error) -> AppError {
    AppError::Upstream("the model can't be downloaded: check the server's internet access".into())
}

async fn sha256_of(path: PathBuf) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(path).map_err(AppError::internal)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 1 << 20];
        loop {
            let read = file.read(&mut buffer).map_err(AppError::internal)?;
            let Some(chunk) = buffer.get(..read).filter(|c| !c.is_empty()) else {
                break;
            };
            hasher.update(chunk);
        }
        Ok(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(AppError::internal)?
}

/// Checks an installed component against its hashes and removes the files that
/// don't match, so installing it again fetches only those. Whether any didn't.
pub async fn remove_damaged(models: &Path, component: &Component) -> AppResult<bool> {
    let dir = component_dir(models, component);
    let mut damaged = false;
    for file in component.files {
        let path = dir.join(file.name());
        if sha256_of(path.clone()).await? != file.sha256 {
            tokio::fs::remove_file(&path)
                .await
                .map_err(AppError::internal)?;
            damaged = true;
        }
    }
    if damaged {
        tokio::fs::remove_file(dir.join(MARKER))
            .await
            .map_err(AppError::internal)?;
    }
    Ok(damaged)
}

/// Downloads one file to `dest` by way of `<dest>.part`, resuming one left
/// by an earlier attempt. `on_bytes` hears about every chunk.
pub async fn download(
    url: &str,
    dest: &Path,
    part: &Path,
    sha256: &str,
    size: u64,
    mut on_bytes: impl FnMut(u64),
) -> AppResult<()> {
    let have = tokio::fs::metadata(part).await.map_or(0, |m| m.len());
    if have < size {
        let mut req = client()?.get(url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut resp = req.send().await.map_err(network_error)?;
        if !resp.status().is_success() {
            return Err(AppError::Upstream(format!(
                "the model can't be downloaded: the server answered {}",
                resp.status().as_u16()
            )));
        }
        // A server that ignores the range sends the file from the start.
        let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut options = tokio::fs::OpenOptions::new();
        options.create(true);
        if resumed {
            options.append(true);
            on_bytes(have);
        } else {
            options.write(true).truncate(true);
        }
        let mut file = options.open(part).await.map_err(|e| write_error(&e))?;
        let mut written = if resumed { have } else { 0 };
        while let Some(chunk) = resp.chunk().await.map_err(network_error)? {
            written = written.saturating_add(chunk.len() as u64);
            if written > size {
                drop(file);
                let _ = tokio::fs::remove_file(part).await;
                return Err(AppError::Upstream(
                    "the model's file is not the expected one".into(),
                ));
            }
            if let Err(e) = file.write_all(&chunk).await {
                drop(file);
                // Disk full: the partial file only makes it worse.
                let _ = tokio::fs::remove_file(part).await;
                return Err(write_error(&e));
            }
            on_bytes(chunk.len() as u64);
        }
        file.flush().await.map_err(|e| write_error(&e))?;
    } else {
        on_bytes(have);
    }
    if sha256_of(part.to_path_buf()).await? != sha256 {
        let _ = tokio::fs::remove_file(part).await;
        return Err(AppError::Upstream(
            "the model's file is not the expected one".into(),
        ));
    }
    tokio::fs::rename(part, dest)
        .await
        .map_err(|e| write_error(&e))
}

/// Downloads what's missing of a component and marks it as complete.
pub async fn install(
    models: &Path,
    component: &Component,
    mut on_bytes: impl FnMut(u64),
) -> AppResult<()> {
    if is_installed(models, component) {
        return Ok(());
    }
    let dir = component_dir(models, component);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| write_error(&e))?;
    for file in component.files {
        let dest = dir.join(file.name());
        if tokio::fs::metadata(&dest)
            .await
            .is_ok_and(|m| m.len() == file.size)
        {
            on_bytes(file.size);
            continue;
        }
        download(
            &file.url(),
            &dest,
            &part_path(&dir, file),
            file.sha256,
            file.size,
            &mut on_bytes,
        )
        .await?;
    }
    tokio::fs::write(dir.join(MARKER), b"")
        .await
        .map_err(|e| write_error(&e))
}

#[cfg(test)]
pub fn mark_installed(models: &Path, component: &Component) {
    let dir = component_dir(models, component);
    std::fs::create_dir_all(&dir).unwrap();
    for file in component.files {
        let f = std::fs::File::create(dir.join(file.name())).unwrap();
        f.set_len(file.size).unwrap();
    }
    std::fs::write(dir.join(MARKER), b"").unwrap();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::get;

    use super::*;

    const BODY: &[u8] = b"model file contents, some bytes of them";

    fn digest(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    /// Serves BODY, honouring ranges, and counts what it sent.
    async fn server() -> (String, Arc<AtomicUsize>) {
        let sent = Arc::new(AtomicUsize::new(0));
        let counter = sent.clone();
        let app = Router::new().route(
            "/file",
            get(move |headers: HeaderMap| {
                let counter = counter.clone();
                async move {
                    let from = headers
                        .get("range")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("bytes="))
                        .and_then(|v| v.strip_suffix('-'))
                        .and_then(|v| v.parse::<usize>().ok());
                    let start = from.unwrap_or(0);
                    let body = BODY.get(start..).unwrap_or_default().to_vec();
                    counter.fetch_add(body.len(), Ordering::SeqCst);
                    let status = if from.is_some() {
                        StatusCode::PARTIAL_CONTENT
                    } else {
                        StatusCode::OK
                    };
                    (status, body)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        (format!("http://{addr}/file"), sent)
    }

    #[tokio::test]
    async fn downloads_and_verifies() {
        let (url, _) = server().await;
        let dir = tempfile::tempdir().unwrap();
        let (dest, part) = (dir.path().join("f"), dir.path().join("f.part"));
        let mut seen = 0;
        download(&url, &dest, &part, &digest(BODY), BODY.len() as u64, |n| {
            seen += n
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), BODY);
        assert!(!part.exists());
        assert_eq!(seen, BODY.len() as u64);
    }

    #[tokio::test]
    async fn resumes_where_it_stopped() {
        let (url, sent) = server().await;
        let dir = tempfile::tempdir().unwrap();
        let (dest, part) = (dir.path().join("f"), dir.path().join("f.part"));
        std::fs::write(&part, &BODY[..10]).unwrap();
        download(&url, &dest, &part, &digest(BODY), BODY.len() as u64, |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), BODY);
        assert_eq!(sent.load(Ordering::SeqCst), BODY.len() - 10);
    }

    #[tokio::test]
    async fn refuses_a_file_that_is_not_the_pinned_one() {
        let (url, _) = server().await;
        let dir = tempfile::tempdir().unwrap();
        let (dest, part) = (dir.path().join("f"), dir.path().join("f.part"));
        let wrong = digest(b"something else");
        let err = download(&url, &dest, &part, &wrong, BODY.len() as u64, |_| {}).await;
        assert!(err.is_err());
        assert!(!dest.exists());
        // The next attempt starts clean.
        assert!(!part.exists());
        // More bytes than the catalog says stop the download early.
        let err = download(&url, &dest, &part, &digest(BODY), 4, |_| {}).await;
        assert!(err.is_err());
        assert!(!dest.exists() && !part.exists());
    }

    #[tokio::test]
    async fn only_files_that_changed_count_as_damaged() {
        static FILES: [ModelFile; 2] = [
            ModelFile {
                repo: "",
                rev: "",
                path: "good.onnx",
                sha256: "b84207a70c0b96bd7164e7e3cce66dcd9dca497720acf4f71c84f0181aca343e",
                size: 39,
            },
            ModelFile {
                repo: "",
                rev: "",
                path: "bad.onnx",
                sha256: "b84207a70c0b96bd7164e7e3cce66dcd9dca497720acf4f71c84f0181aca343e",
                size: 39,
            },
        ];
        let component = Component {
            id: "test",
            tool: starbooru_core::AiTool::Tagging,
            files: &FILES,
        };
        let models = tempfile::tempdir().unwrap();
        let dir = component_dir(models.path(), &component);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good.onnx"), BODY).unwrap();
        std::fs::write(dir.join("bad.onnx"), BODY).unwrap();
        std::fs::write(dir.join(MARKER), b"").unwrap();
        assert!(is_installed(models.path(), &component));
        assert!(!remove_damaged(models.path(), &component).await.unwrap());
        assert!(is_installed(models.path(), &component));

        // Same size, other bytes: only a hash tells.
        let mut changed = BODY.to_vec();
        changed[0] ^= 1;
        std::fs::write(dir.join("bad.onnx"), changed).unwrap();
        assert!(is_installed(models.path(), &component));
        assert!(remove_damaged(models.path(), &component).await.unwrap());
        assert!(!is_installed(models.path(), &component));
        assert!(dir.join("good.onnx").is_file());
        assert!(!dir.join("bad.onnx").exists());
    }

    #[tokio::test]
    async fn an_unreachable_server_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let (dest, part) = (dir.path().join("f"), dir.path().join("f.part"));
        let err = download("http://127.0.0.1:9/f", &dest, &part, "00", 4, |_| {}).await;
        assert!(err.is_err());
    }
}
