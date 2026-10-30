//! Keeps the AI worker process running. It's started when it's first needed
//! and answers one request at a time. When it dies, runs out of memory or
//! hangs, it's replaced without anyone having to do anything.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use starbooru_core::ai_worker::{WorkerReply, WorkerRequest};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::error::{AppError, AppResult};

const WORKER: &str = "starbooru-ai-worker";
/// A build with graphics card support, when the image has one.
const GPU_WORKER: &str = "starbooru-ai-worker-gpu";
/// Tells a worker which runtime library to load.
const ORT_LIB: &str = "STARBOORU_ORT_LIB";
/// Starting up also loads the runtime library.
const HELLO_TIMEOUT: Duration = Duration::from_secs(30);

struct Process {
    /// Killed when dropped.
    _child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    gpu: Option<String>,
}

struct Slot {
    process: Option<Process>,
    last_used: Instant,
}

pub struct Worker {
    programs: Vec<PathBuf>,
    models: PathBuf,
    library: Option<PathBuf>,
    slot: Mutex<Slot>,
}

async fn exchange(process: &mut Process, line: &str, timeout: Duration) -> Result<String, Failure> {
    let write = async {
        process.stdin.write_all(line.as_bytes()).await?;
        process.stdin.write_all(b"\n").await?;
        process.stdin.flush().await
    };
    let read = async {
        write.await.map_err(|_| Failure::Died)?;
        match process.stdout.next_line().await {
            Ok(Some(answer)) => Ok(answer),
            _ => Err(Failure::Died),
        }
    };
    tokio::time::timeout(timeout, read)
        .await
        .map_err(|_| Failure::TimedOut)?
}

enum Failure {
    Died,
    TimedOut,
}

async fn start(program: &Path, models: &Path, library: Option<&Path>) -> Result<Process, String> {
    let mut command = Command::new(program);
    if let Some(library) = library {
        command.env(ORT_LIB, library);
    }
    let mut child = command
        .env("STARBOORU_AI_MODELS", models)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("the AI worker doesn't start: {e}"))?;
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err("the AI worker doesn't start".into());
    };
    // Runtime and driver notices. They carry no content.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            debug!(target: "ai_worker", "{line}");
        }
    });
    let mut process = Process {
        _child: child,
        stdin,
        stdout: BufReader::new(stdout).lines(),
        gpu: None,
    };
    let hello = serde_json::to_string(&WorkerRequest::Hello).map_err(|e| e.to_string())?;
    let answer = exchange(&mut process, &hello, HELLO_TIMEOUT)
        .await
        .map_err(|_| "the AI worker doesn't answer".to_string())?;
    match serde_json::from_str(&answer) {
        Ok(WorkerReply::Hello { gpu }) => {
            process.gpu = gpu;
            Ok(process)
        }
        Ok(WorkerReply::Error { message }) => Err(message),
        _ => Err("the AI worker doesn't answer".into()),
    }
}

impl Worker {
    /// `program`: a fixed worker. Else the ones next to the server's binary.
    pub fn new(program: Option<PathBuf>, models: PathBuf, library: PathBuf) -> Self {
        // A worker chosen by hand comes with its own setup.
        let library = program.is_none().then_some(library);
        let programs = program.map_or_else(
            || {
                let dir = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
                    .unwrap_or_default();
                vec![dir.join(GPU_WORKER), dir.join(WORKER)]
            },
            |p| vec![p],
        );
        Self {
            programs,
            models,
            library,
            slot: Mutex::new(Slot {
                process: None,
                last_used: Instant::now(),
            }),
        }
    }

    /// The runtime library the server has to provide: none is set, and the
    /// workers have none next to them, as they do in the Docker image.
    pub fn library_to_provide(&self) -> Option<&Path> {
        let library = self.library.as_deref()?;
        let program = self.programs.iter().find(|p| p.is_file())?;
        let beside = program.with_file_name(format!(
            "{}onnxruntime{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
        (std::env::var_os(ORT_LIB).is_none() && !beside.is_file()).then_some(library)
    }

    /// The first build that runs here. A graphics card build only counts when
    /// it finds a card.
    async fn launch(&self) -> Result<Process, String> {
        let mut reason = "this build has no AI worker".to_string();
        let count = self.programs.len();
        for (i, program) in self.programs.iter().enumerate() {
            if !program.is_file() {
                continue;
            }
            let fallback_left = i.saturating_add(1) < count;
            match start(program, &self.models, self.library_to_provide()).await {
                Ok(p) if p.gpu.is_none() && fallback_left => {}
                Ok(p) => return Ok(p),
                Err(e) => reason = e,
            }
        }
        Err(reason)
    }

    /// Sends one request. A worker that died is replaced and asked once more:
    /// the same request killing it twice is the request's fault.
    pub async fn request(
        &self,
        request: &WorkerRequest,
        timeout: Duration,
    ) -> AppResult<WorkerReply> {
        let line = serde_json::to_string(request).map_err(AppError::internal)?;
        let mut slot = self.slot.lock().await;
        let mut died = false;
        loop {
            if slot.process.is_none() {
                let process = self.launch().await.map_err(AppError::Upstream)?;
                slot.process = Some(process);
            }
            let Some(process) = slot.process.as_mut() else {
                return Err(AppError::Upstream("the AI worker doesn't start".into()));
            };
            let answer = exchange(process, &line, timeout).await;
            slot.last_used = Instant::now();
            match answer {
                Ok(text) => {
                    return match serde_json::from_str(&text) {
                        Ok(WorkerReply::Error { message }) => Err(AppError::Upstream(message)),
                        Ok(reply) => Ok(reply),
                        Err(_) => {
                            slot.process = None;
                            Err(AppError::Upstream(
                                "the AI worker's answer is not understood".into(),
                            ))
                        }
                    };
                }
                Err(Failure::TimedOut) => {
                    slot.process = None;
                    warn!("the AI worker took too long and was restarted");
                    return Err(AppError::Upstream(
                        "the AI model took too long to answer".into(),
                    ));
                }
                Err(Failure::Died) => {
                    slot.process = None;
                    if died {
                        return Err(AppError::Upstream(
                            "the AI model ran out of memory or failed on this request".into(),
                        ));
                    }
                    warn!("the AI worker stopped and is being restarted");
                    died = true;
                }
            }
        }
    }

    /// The graphics card interface of the running worker, starting one.
    pub async fn check(&self) -> Result<Option<String>, String> {
        let mut slot = self.slot.lock().await;
        if slot.process.is_none() {
            slot.process = Some(self.launch().await?);
        }
        Ok(slot.process.as_ref().and_then(|p| p.gpu.clone()))
    }

    /// Frees the memory of a worker that has been idle for `after`. The
    /// next request starts another one.
    pub async fn stop_when_idle(&self, after: Duration) {
        // Busy: it isn't idle.
        if let Ok(mut slot) = self.slot.try_lock()
            && slot.process.is_some()
            && slot.last_used.elapsed() >= after
        {
            slot.process = None;
        }
    }
}
