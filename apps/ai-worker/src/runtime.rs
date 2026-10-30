//! ONNX Runtime, and the models currently loaded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ort::environment::Environment;
#[cfg(any(feature = "webgpu", feature = "cuda"))]
use ort::ep::ExecutionProvider;
use ort::session::Session;
use ort::session::builder::{GraphOptimizationLevel, SessionBuilder};
use starbooru_core::ai_worker::DAMAGED_MODEL;
use tokenizers::Tokenizer;

use crate::tag::TagList;
use crate::translate::GemmaStart;

/// More threads than this mostly wait on each other.
const MAX_THREADS: usize = 8;
/// Left free when a model loads, for the database and everything else.
const KEEP_FREE_BYTES: u64 = 256 * 1024 * 1024;

pub struct Runtime {
    _env: Environment,
    threads: usize,
    models: PathBuf,
    /// Most recently used last.
    sessions: Vec<Loaded>,
    pub tokenizers: HashMap<String, Rc<Tokenizer>>,
    pub tags: HashMap<String, Rc<TagList>>,
    pub gemma_start: Option<GemmaStart>,
}

struct Loaded {
    path: PathBuf,
    gpu: bool,
    bytes: u64,
    session: Session,
}

/// The graphics card interface of this build, when it works here.
pub fn gpu_name() -> Option<String> {
    #[cfg(feature = "cuda")]
    if matches!(ort::ep::CUDA::is_available(), Ok(true)) {
        return Some("cuda".into());
    }
    #[cfg(feature = "webgpu")]
    if matches!(ort::ep::WebGPU::is_available(), Ok(true)) {
        return Some("webgpu".into());
    }
    None
}

#[cfg(feature = "dynamic")]
fn environment() -> Result<Environment, String> {
    let lib = std::env::var_os("STARBOORU_ORT_LIB")
        .map(PathBuf::from)
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            let name = format!(
                "{}onnxruntime{}",
                std::env::consts::DLL_PREFIX,
                std::env::consts::DLL_SUFFIX
            );
            Some(exe.parent()?.join(name))
        })
        .ok_or("the AI runtime library is missing")?;
    ort::init_from(&lib)
        .map_err(|e| format!("the AI runtime library doesn't load: {e}"))?
        .build()
        .map_err(|e| e.to_string())
}

#[cfg(not(feature = "dynamic"))]
fn environment() -> Result<Environment, String> {
    ort::init().build().map_err(|e| e.to_string())
}

/// One thread per physical core, as far as this process may use them.
fn threads() -> usize {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let smt = std::fs::read_to_string("/sys/devices/system/cpu/smt/active")
        .is_ok_and(|s| s.trim() == "1");
    let cores = if smt && cpus > 2 { cpus / 2 } else { cpus };
    cores.clamp(1, MAX_THREADS)
}

fn number(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// Memory this process can still get, counting a container's limit.
fn free_bytes() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = meminfo.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kib: u64 = number(line.split_whitespace().nth(1)?)?;
    let system = kib.saturating_mul(1024);
    let read = |name: &str| {
        std::fs::read_to_string(Path::new("/sys/fs/cgroup").join(name))
            .ok()
            .and_then(|s| number(&s))
    };
    let container = match (read("memory.max"), read("memory.current")) {
        (Some(max), Some(current)) => Some(max.saturating_sub(current)),
        _ => None,
    };
    Some(container.map_or(system, |c| c.min(system)))
}

/// The model's files, external weights included.
fn model_bytes(path: &Path) -> u64 {
    let Some(dir) = path.parent() else { return 0 };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(name)))
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

impl Runtime {
    pub fn start() -> Result<Self, String> {
        let models = std::env::var_os("STARBOORU_AI_MODELS")
            .map_or_else(|| PathBuf::from("models"), PathBuf::from);
        Ok(Self {
            _env: environment()?,
            threads: threads(),
            models,
            sessions: Vec::new(),
            tokenizers: HashMap::new(),
            tags: HashMap::new(),
            gemma_start: None,
        })
    }

    pub fn unload(&mut self) {
        self.sessions.clear();
        self.tokenizers.clear();
        self.tags.clear();
        self.gemma_start = None;
    }

    /// A file of an installed component.
    pub fn file(&self, component: &str, name: &str) -> Result<PathBuf, String> {
        let path = self.models.join(component).join(name);
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!("{component} is not installed"))
        }
    }

    fn builder(&self, gpu: bool) -> Result<SessionBuilder, String> {
        let builder = Session::builder(&self._env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(self.threads)
            .map_err(|e| e.to_string())?
            .with_inter_threads(1)
            .map_err(|e| e.to_string())?;
        with_gpu(builder, gpu)
    }

    /// A loaded model. Others leave first when memory is short; each of them
    /// comes back the next time it's needed.
    pub fn session(&mut self, path: &Path, gpu: bool) -> Result<&mut Session, String> {
        let found = self
            .sessions
            .iter()
            .position(|l| l.gpu == gpu && l.path == path);
        if let Some(found) = found {
            let last = self.sessions.len().saturating_sub(1);
            self.sessions.swap(found, last);
        } else {
            let bytes = model_bytes(path);
            self.make_room(bytes);
            let session = self.builder(gpu)?.commit_from_file(path).map_err(|e| {
                eprintln!("{e}");
                DAMAGED_MODEL.to_string()
            })?;
            self.sessions.push(Loaded {
                path: path.to_path_buf(),
                gpu,
                bytes,
                session,
            });
        }
        self.sessions
            .last_mut()
            .map(|l| &mut l.session)
            .ok_or_else(|| "the model doesn't load".to_string())
    }

    /// A model built here rather than read from a file, not kept.
    pub fn session_from_memory(&self, model: &[u8], gpu: bool) -> Result<Session, String> {
        self.builder(gpu)?
            .commit_from_memory(model)
            .map_err(|e| format!("the model doesn't load: {e}"))
    }

    fn make_room(&mut self, bytes: u64) {
        // Loading takes about twice the file, briefly.
        let needed = bytes.saturating_mul(2).saturating_add(KEEP_FREE_BYTES);
        while !self.sessions.is_empty() && free_bytes().is_some_and(|free| free < needed) {
            let oldest = self.sessions.remove(0);
            // Freed before the next look at the free memory.
            drop((oldest.bytes, oldest.session));
        }
    }
}

#[cfg(feature = "webgpu")]
fn with_gpu(builder: SessionBuilder, gpu: bool) -> Result<SessionBuilder, String> {
    use ort::ep::WebGPU;
    use ort::ep::webgpu::DawnBackendType;
    if !gpu {
        return Ok(builder);
    }
    let ep = WebGPU::default().with_dawn_backend_type(DawnBackendType::Vulkan);
    builder
        .with_execution_providers([ep.build().error_on_failure()])
        .map_err(|e| e.to_string())
}

#[cfg(feature = "cuda")]
fn with_gpu(builder: SessionBuilder, gpu: bool) -> Result<SessionBuilder, String> {
    if !gpu {
        return Ok(builder);
    }
    builder
        .with_execution_providers([ort::ep::CUDA::default().build().error_on_failure()])
        .map_err(|e| e.to_string())
}

#[cfg(not(any(feature = "webgpu", feature = "cuda")))]
fn with_gpu(builder: SessionBuilder, gpu: bool) -> Result<SessionBuilder, String> {
    if gpu {
        return Err("this build has no graphics card support".into());
    }
    Ok(builder)
}
