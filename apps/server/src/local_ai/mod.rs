//! StarBooru's own AI: models made for each job, run by a separate process
//! (`apps/ai-worker`) so that a model that runs out of memory can't take the
//! server with it.
//!
//! The server keeps this working by itself. In the background it starts the
//! worker, times this machine, downloads what the enabled tools need and
//! times each model once it's there, and it tries again when something fails.
//! The admin only says whether a tool is on and how long it may take.
//! Never log images, text or translations.

mod hardware;
mod install;
mod worker;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use starbooru_core::ai::ParsedSuggestions;
use starbooru_core::ai_catalog::{
    self, COMPONENTS, Calibration, CatalogModel, Component, Engine, MODELS, Machine, Measured,
};
use starbooru_core::ai_worker::{DAMAGED_MODEL, TextBox, WorkerReply, WorkerRequest};
use starbooru_core::{
    AiAdminStatus, AiInstallProgress, AiMode, AiModelOption, AiPreset, AiPresetOption,
    AiRuntimeStatus, AiTool, AiToolSettings, AiToolState, AiToolStatus, TextBlock,
};
use tokio::sync::Notify;
use tracing::{info, warn};

pub use hardware::hardware;
use worker::Worker;

use crate::ai::ToolSetup;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Longest one answer may take before the worker counts as stuck.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Timing a big model on a small machine loads it and runs it several times.
const MEASURE_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const CALIBRATE_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a request waits for a language pair to download.
const PAIR_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// How often what failed is tried again.
const RETRY_EVERY: Duration = Duration::from_secs(5 * 60);
/// An idle worker gives its memory back, unless a tool must answer at once.
const IDLE_AFTER: Duration = Duration::from_secs(10 * 60);
/// A failed download is tried this many times before reporting it.
const DOWNLOAD_ATTEMPTS: u32 = 3;
const BACKOFF: Duration = Duration::from_secs(5);
/// Most pieces of text read on one page.
const MAX_BOXES: usize = 60;
/// Coordinates of `TextBlock`s.
const BOX_SCALE: u32 = 1000;

#[derive(Default)]
struct Shared {
    calibration: Calibration,
    runtime: AiRuntimeStatus,
    queue: VecDeque<&'static str>,
    /// Being downloaded now.
    current: Option<&'static str>,
    progress: BTreeMap<&'static str, AiInstallProgress>,
    errors: BTreeMap<&'static str, String>,
    /// Failed to time; tried again after a restart.
    unmeasurable: BTreeSet<&'static str>,
}

pub struct LocalAi {
    worker: Worker,
    models: PathBuf,
    data_dir: PathBuf,
    shared: Mutex<Shared>,
    /// One download at a time, also for language pairs fetched on demand.
    download: tokio::sync::Mutex<()>,
    wake: Notify,
}

/// A model that can answer now.
#[derive(Clone, Copy, Debug)]
pub struct Active {
    pub model: &'static CatalogModel,
    pub gpu: bool,
}

impl LocalAi {
    pub fn new(program: Option<PathBuf>, data_dir: &Path) -> Self {
        let models = data_dir.join("ai").join("models");
        Self {
            worker: Worker::new(program, models.clone(), install::runtime_library(data_dir)),
            models,
            data_dir: data_dir.to_path_buf(),
            shared: Mutex::new(Shared::default()),
            download: tokio::sync::Mutex::new(()),
            wake: Notify::new(),
        }
    }

    /// Nothing done under this lock can leave it inconsistent.
    fn shared(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn wake(&self) {
        self.wake.notify_one();
    }

    pub fn runtime(&self) -> AiRuntimeStatus {
        self.shared().runtime.clone()
    }

    fn machine(&self) -> Machine {
        let shared = self.shared();
        Machine {
            calibration: shared.calibration.clone(),
            ram_mb: hardware::ram_mb(),
            failed: shared
                .unmeasurable
                .iter()
                .map(|&id| id.to_owned())
                .collect(),
        }
    }

    pub fn is_installed(&self, model: &CatalogModel) -> bool {
        model
            .parts()
            .all(|c| install::is_installed(&self.models, c))
    }

    /// Queues a model's download.
    pub fn install(&self, id: &str) -> AppResult<()> {
        let model = ai_catalog::model(id).ok_or(AppError::NotFound)?;
        if !self.machine().fits(model) {
            return Err(AppError::BadRequest(
                "this model doesn't fit in this machine's memory".into(),
            ));
        }
        {
            let mut shared = self.shared();
            shared.unmeasurable.remove(model.id);
            shared.errors.remove(model.id);
        }
        self.enqueue(model);
        self.wake();
        Ok(())
    }

    fn enqueue(&self, model: &'static CatalogModel) {
        if self.is_installed(model) {
            return;
        }
        let mut shared = self.shared();
        shared.errors.remove(model.id);
        if shared.current != Some(model.id) && !shared.queue.contains(&model.id) {
            shared.queue.push_back(model.id);
            let done = self.bytes_present(model);
            let total = model.size();
            shared
                .progress
                .insert(model.id, AiInstallProgress { done, total });
        }
    }

    fn bytes_present(&self, model: &CatalogModel) -> u64 {
        model
            .parts()
            .map(|c| install::bytes_present(&self.models, c))
            .sum()
    }

    /// What the tool should be using: the admin's pick or the preset's.
    fn target(&self, machine: &Machine, setup: &ToolSetup) -> Option<&'static CatalogModel> {
        if let Some(chosen) = ai_catalog::model(&setup.model).filter(|m| m.tool == setup.tool) {
            return Some(chosen);
        }
        machine.resolve(setup.tool, setup.preset)
    }

    /// What answers now: the target when it's installed, else the most
    /// accurate model of the tool that is, while the target downloads.
    fn answering(&self, machine: &Machine, setup: &ToolSetup) -> Option<&'static CatalogModel> {
        self.target(machine, setup)
            .filter(|m| self.is_installed(m))
            .or_else(|| {
                MODELS
                    .iter()
                    .filter(|m| m.tool == setup.tool && machine.fits(m) && self.is_installed(m))
                    .max_by_key(|m| m.rank)
            })
    }
}

/// The model that answers for a tool in local mode.
pub fn active(state: &AppState, setup: &ToolSetup) -> AppResult<Active> {
    let ai = &state.local_ai;
    let runtime = ai.runtime();
    if !runtime.ready {
        return Err(AppError::Conflict {
            code: "ai_unavailable",
            message: runtime
                .reason
                .unwrap_or_else(|| "the AI models are still starting".into()),
        });
    }
    let machine = ai.machine();
    match ai.answering(&machine, setup) {
        Some(model) => Ok(Active {
            model,
            gpu: machine
                .calibration
                .measured
                .get(model.id)
                .is_some_and(|m| m.gpu),
        }),
        None => {
            ai.wake();
            let message = if ai.target(&machine, setup).is_none() {
                "this machine doesn't have enough memory for this AI feature"
            } else {
                "the AI model is still downloading; try again in a moment"
            };
            Err(AppError::Conflict {
                code: "ai_unavailable",
                message: message.into(),
            })
        }
    }
}

/// `GET /admin/ai`.
pub async fn admin_status(state: &AppState) -> AppResult<AiAdminStatus> {
    let setup = crate::settings::ai(state).await?;
    let hardware = tokio::task::spawn_blocking(hardware)
        .await
        .map_err(AppError::internal)?;
    let ai = &state.local_ai;
    let machine = ai.machine();
    let tools = AiTool::ALL
        .into_iter()
        .map(|tool| tool_status(ai, &machine, &setup.tool(tool)))
        .collect();
    Ok(AiAdminStatus {
        hardware,
        runtime: ai.runtime(),
        autotag: setup.autotag,
        auto_translate: setup.auto_translate,
        tools,
    })
}

fn tool_status(ai: &LocalAi, machine: &Machine, setup: &ToolSetup) -> AiToolStatus {
    let presets = AiPreset::ALL
        .into_iter()
        .map(|preset| {
            let pick = machine.pick(setup.tool, preset);
            AiPresetOption {
                preset,
                available: pick.is_some(),
                model: pick.map(|m| m.id.to_owned()),
                est_ms: pick.map(|m| machine.estimate_ms(m)),
            }
        })
        .collect();
    let shared = ai.shared();
    let models = MODELS
        .iter()
        .filter(|m| m.tool == setup.tool)
        .map(|m| {
            let measured = machine.calibration.measured.get(m.id);
            AiModelOption {
                id: m.id.to_owned(),
                name: m.name.to_owned(),
                size: m.size(),
                ram_mb: m.ram_mb,
                high_accuracy: m.high_accuracy,
                est_ms: machine.estimate_ms(m),
                measured: measured.is_some(),
                gpu: measured.is_some_and(|x| x.gpu),
                installed: ai.is_installed(m),
                fits: machine.fits(m),
                progress: shared.progress.get(m.id).copied(),
                error: shared.errors.get(m.id).cloned(),
            }
        })
        .collect();
    let target = ai.target(machine, setup);
    let (state, error) = match setup.mode {
        AiMode::Off => (AiToolState::Off, None),
        AiMode::Api if setup.api.enabled() => (AiToolState::Ready, None),
        AiMode::Api => (AiToolState::Off, None),
        AiMode::Local if !shared.runtime.ready => {
            (AiToolState::Failed, shared.runtime.reason.clone())
        }
        AiMode::Local => match target {
            None => (
                AiToolState::Failed,
                Some("not enough memory on this machine".to_owned()),
            ),
            Some(m) if ai.is_installed(m) => (AiToolState::Ready, None),
            Some(m) => match shared.errors.get(m.id) {
                Some(e) => (AiToolState::Failed, Some(e.clone())),
                None => (AiToolState::Installing, None),
            },
        },
    };
    let active = match setup.mode {
        AiMode::Local => ai.answering(machine, setup).map(|m| m.id.to_owned()),
        _ => None,
    };
    AiToolStatus {
        tool: setup.tool,
        settings: AiToolSettings {
            mode: setup.mode,
            preset: setup.preset,
            model: setup.model.clone(),
            api_url: setup.api.url.clone(),
            api_model: setup.api.model.clone(),
            api_key_set: !setup.api.key.is_empty(),
        },
        presets,
        models,
        active,
        state,
        error,
    }
}

fn same_parts(a: &CatalogModel, b: &CatalogModel) -> bool {
    a.components.len() == b.components.len()
        && a.components.iter().all(|c| b.components.contains(c))
}

/// What uninstalling `model` frees: its files that the fewest other installed
/// models need, so that as many of them as possible keep working, and none of
/// what's in use. Two models that are the same files go together.
fn removable(
    ai: &LocalAi,
    model: &CatalogModel,
    in_use: &[&CatalogModel],
) -> Vec<&'static Component> {
    let needed_by = |c: &Component| {
        MODELS
            .iter()
            .filter(|other| {
                other.id != model.id
                    && other.components.contains(&c.id)
                    && !same_parts(other, model)
                    && ai.is_installed(other)
            })
            .count()
    };
    let free: Vec<&'static Component> = model
        .parts()
        .filter(|c| !in_use.iter().any(|m| m.components.contains(&c.id)))
        .collect();
    let fewest = free.iter().map(|c| needed_by(c)).min().unwrap_or(0);
    let mut parts: Vec<&'static Component> = free
        .into_iter()
        .filter(|c| needed_by(c) == fewest)
        .collect();
    // Language pairs fetched when first needed go with the last that uses them.
    if matches!(model.engine, Engine::Opus) && !parts.is_empty() && fewest == 0 {
        parts.extend(
            COMPONENTS
                .iter()
                .filter(|c| c.id.starts_with("opus-") && !model.components.contains(&c.id)),
        );
    }
    parts
}

/// Asks the worker for a model's work. Damaged files are repaired, so the same
/// error doesn't come back on every request.
async fn ask(
    ai: &LocalAi,
    model: &'static CatalogModel,
    request: &WorkerRequest,
    timeout: Duration,
) -> AppResult<WorkerReply> {
    match ai.worker.request(request, timeout).await {
        Err(AppError::Upstream(message)) if message == DAMAGED_MODEL => {
            Err(repair(ai, model).await)
        }
        other => other,
    }
}

/// A model didn't load: removes the files that fail their hash and wakes the
/// sweep to fetch them. What the caller gets to hear.
async fn repair(ai: &LocalAi, model: &'static CatalogModel) -> AppError {
    let uses_opus = model.components.iter().any(|c| c.starts_with("opus-"));
    let parts = COMPONENTS
        .iter()
        .filter(|c| model.components.contains(&c.id) || (uses_opus && c.id.starts_with("opus-")));
    let mut repairing = false;
    for part in parts {
        if !install::is_installed(&ai.models, part) {
            // Another request got here first.
            repairing |= model.components.contains(&part.id);
            continue;
        }
        match install::remove_damaged(&ai.models, part).await {
            Ok(damaged) => repairing |= damaged,
            Err(e) => warn!(part = part.id, "checking an AI model failed: {e}"),
        }
    }
    if !repairing {
        return AppError::Upstream("the model doesn't load".into());
    }
    warn!(
        model = model.id,
        "an AI model was damaged, downloading it again"
    );
    ai.wake();
    AppError::Conflict {
        code: "ai_unavailable",
        message: format!("{} was damaged and is downloading again", model.name),
    }
}

/// Removes a model's files. A model in use stays: the tool would have nothing
/// to answer with.
pub async fn uninstall(state: &AppState, id: &str) -> AppResult<()> {
    let ai = &state.local_ai;
    let model = ai_catalog::model(id).ok_or(AppError::NotFound)?;
    {
        let mut shared = ai.shared();
        if shared.current == Some(model.id) {
            return Err(AppError::BadRequest(
                "this model is downloading; wait for it to finish".into(),
            ));
        }
        shared.queue.retain(|q| *q != model.id);
        shared.progress.remove(model.id);
        shared.errors.remove(model.id);
        shared.unmeasurable.remove(model.id);
    }
    let setup = crate::settings::ai(state).await?;
    let machine = ai.machine();
    let mut in_use: Vec<&CatalogModel> = Vec::new();
    for tool in AiTool::ALL {
        let tool = setup.tool(tool);
        if tool.mode == AiMode::Local {
            in_use.extend(ai.answering(&machine, &tool));
            in_use.extend(ai.target(&machine, &tool));
        }
    }
    let parts = removable(ai, model, &in_use);
    if in_use.iter().any(|m| m.id == model.id) || (parts.is_empty() && ai.is_installed(model)) {
        return Err(AppError::BadRequest(
            "this model is in use; choose another one first".into(),
        ));
    }
    // Loaded models keep their files in use.
    if let Err(e) = ai
        .worker
        .request(&WorkerRequest::Unload, ANSWER_TIMEOUT)
        .await
    {
        warn!("the AI worker didn't unload its models: {e}");
    }
    for part in parts {
        install::remove(&ai.models, part).await?;
    }
    ai.shared().calibration.measured.remove(model.id);
    save_calibration(state).await;
    Ok(())
}

async fn save_calibration(state: &AppState) {
    let calibration = state.local_ai.shared().calibration.clone();
    let Ok(json) = serde_json::to_string(&calibration) else {
        return;
    };
    let saved = sqlx::query(
        "INSERT INTO server_settings (id, ai_calibration) VALUES (true, $1::jsonb) \
         ON CONFLICT (id) DO UPDATE SET ai_calibration = $1::jsonb",
    )
    .bind(&json)
    .execute(&state.db)
    .await;
    if let Err(e) = saved {
        warn!("the AI timings weren't saved: {e}");
    }
}

/// Loads the stored timings, unless the machine has changed since.
async fn load_calibration(state: &AppState, fingerprint: &str) -> Option<Calibration> {
    let stored: Option<Option<String>> =
        sqlx::query_scalar("SELECT ai_calibration::text FROM server_settings")
            .fetch_optional(&state.db)
            .await
            .ok()?;
    serde_json::from_str::<Calibration>(&stored.flatten()?)
        .ok()
        .filter(|c| c.fingerprint == fingerprint)
}

/// Starts the worker and makes sure this machine has been timed.
async fn prepare_runtime(state: &AppState) {
    let ai = &state.local_ai;
    let provided = match ai.worker.library_to_provide() {
        Some(library) => install::install_runtime(library).await,
        None => Ok(()),
    };
    let checked = match provided {
        Ok(()) => ai.worker.check().await,
        Err(reason) => Err(reason),
    };
    let gpu = match checked {
        Ok(gpu) => gpu,
        Err(reason) => {
            warn!("the built-in AI models can't run: {reason}");
            ai.shared().runtime = AiRuntimeStatus {
                ready: false,
                reason: Some(reason),
                gpu: None,
            };
            return;
        }
    };
    let hw = tokio::task::spawn_blocking(hardware)
        .await
        .unwrap_or_default();
    let fingerprint = hardware::fingerprint(&hw, gpu.as_deref(), hardware::cpu_limit().as_deref());
    let calibrated = ai.shared().calibration.fingerprint == fingerprint;
    if !calibrated {
        if let Some(stored) = load_calibration(state, &fingerprint).await {
            ai.shared().calibration = stored;
        } else {
            let reply = ai
                .worker
                .request(&WorkerRequest::Calibrate, CALIBRATE_TIMEOUT)
                .await;
            match reply {
                Ok(WorkerReply::Calibrated {
                    float_ms,
                    int8_ms,
                    gpu_ms,
                }) => {
                    info!(float_ms, int8_ms, ?gpu_ms, "timed this machine for AI");
                    ai.shared().calibration = Calibration {
                        fingerprint,
                        float_ms,
                        int8_ms,
                        gpu_ms,
                        measured: BTreeMap::new(),
                    };
                    save_calibration(state).await;
                }
                other => {
                    let reason = match other {
                        Err(e) => e.to_string(),
                        Ok(_) => "the AI worker's answer is not understood".to_owned(),
                    };
                    warn!("the built-in AI models can't run: {reason}");
                    ai.shared().runtime = AiRuntimeStatus {
                        ready: false,
                        reason: Some(reason),
                        gpu,
                    };
                    return;
                }
            }
        }
    }
    ai.shared().runtime = AiRuntimeStatus {
        ready: true,
        reason: None,
        gpu,
    };
}

/// One model's download, with retries.
async fn download_model(ai: &LocalAi, id: &'static str) {
    let Some(model) = ai_catalog::model(id) else {
        return;
    };
    let total = model.size();
    let mut last_error = None;
    for attempt in 0..DOWNLOAD_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(BACKOFF.saturating_mul(attempt)).await;
        }
        let result = async {
            for part in model.parts() {
                let _turn = ai.download.lock().await;
                install::install(&ai.models, part, |_| {
                    let done = ai.bytes_present(model).min(total);
                    ai.shared()
                        .progress
                        .insert(id, AiInstallProgress { done, total });
                })
                .await?;
            }
            Ok::<_, AppError>(())
        }
        .await;
        match result {
            Ok(()) => {
                last_error = None;
                break;
            }
            Err(e) => last_error = Some(e.to_string()),
        }
    }
    let mut shared = ai.shared();
    shared.progress.remove(id);
    shared.current = None;
    if let Some(reason) = last_error {
        warn!(model = id, "downloading an AI model failed: {reason}");
        shared.errors.insert(id, reason);
    } else {
        shared.errors.remove(id);
    }
}

/// Times one installed model that hasn't been. `false`: there's none left.
async fn measure_next(state: &AppState) -> bool {
    let ai = &state.local_ai;
    let next = {
        let shared = ai.shared();
        if !shared.runtime.ready {
            return false;
        }
        MODELS.iter().find(|m| {
            !shared.calibration.measured.contains_key(m.id)
                && !shared.unmeasurable.contains(m.id)
                && ai.is_installed(m)
        })
    };
    let Some(model) = next else {
        return false;
    };
    let request = WorkerRequest::Measure {
        model: model.id.to_owned(),
    };
    match ask(ai, model, &request, MEASURE_TIMEOUT).await {
        // Being downloaded again; timed once it's back.
        Err(AppError::Conflict { .. }) => {}
        Ok(WorkerReply::Measured {
            cpu_ms,
            gpu_ms,
            gpu_matches,
        }) => {
            let measured = match gpu_ms.filter(|g| gpu_matches && *g < cpu_ms) {
                Some(ms) => Measured { ms, gpu: true },
                None => Measured {
                    ms: cpu_ms,
                    gpu: false,
                },
            };
            info!(
                model = model.id,
                ms = measured.ms,
                gpu = measured.gpu,
                "timed an AI model"
            );
            ai.shared()
                .calibration
                .measured
                .insert(model.id.to_owned(), measured);
            save_calibration(state).await;
        }
        other => {
            let reason = other
                .err()
                .map_or_else(|| "bad reply".to_owned(), |e| e.to_string());
            warn!(model = model.id, "timing an AI model failed: {reason}");
            let mut shared = ai.shared();
            shared.unmeasurable.insert(model.id);
            shared.errors.insert(model.id, reason);
        }
    }
    true
}

/// One round of making things work: what the enabled tools need gets
/// downloaded and timed. `retry` tries again what failed before.
pub(crate) async fn sweep(state: &AppState, retry: bool) {
    let ai = &state.local_ai;
    if !ai.runtime().ready {
        prepare_runtime(state).await;
        // Models that can't run aren't worth their download.
        if !ai.runtime().ready {
            return;
        }
    }
    let setup = match crate::settings::ai(state).await {
        Ok(setup) => setup,
        Err(e) => {
            warn!("the AI settings can't be read: {e}");
            return;
        }
    };
    let machine = ai.machine();
    let mut quick = false;
    for tool in AiTool::ALL {
        let tool = setup.tool(tool);
        if tool.mode != AiMode::Local {
            continue;
        }
        quick |= tool.preset == AiPreset::Fast;
        if let Some(target) = ai.target(&machine, &tool)
            && (retry || !ai.shared().errors.contains_key(target.id))
        {
            ai.enqueue(target);
        }
    }
    loop {
        let next = {
            let mut shared = ai.shared();
            let next = shared.queue.pop_front();
            shared.current = next;
            next
        };
        if let Some(id) = next {
            download_model(ai, id).await;
        } else if !measure_next(state).await {
            break;
        }
    }
    if !quick {
        ai.worker.stop_when_idle(IDLE_AFTER).await;
    }
}

/// Keeps the built-in models working for as long as the server runs.
pub async fn run(state: AppState) {
    let ai = &state.local_ai;
    if let Err(e) = tokio::fs::create_dir_all(&ai.models).await {
        warn!("the AI model folder can't be created: {e}");
    }
    sweep(&state, false).await;
    loop {
        let retry = tokio::select! {
            () = ai.wake.notified() => false,
            () = tokio::time::sleep(RETRY_EVERY) => true,
        };
        sweep(&state, retry).await;
    }
}

fn encoded(jpeg: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(jpeg)
}

/// Tags a picture with a tagging model.
pub async fn tag(state: &AppState, active: &Active, jpeg: &[u8]) -> AppResult<ParsedSuggestions> {
    let request = WorkerRequest::Tag {
        model: active.model.id.to_owned(),
        gpu: active.gpu,
        jpeg: encoded(jpeg),
    };
    match ask(&state.local_ai, active.model, &request, ANSWER_TIMEOUT).await? {
        WorkerReply::Tags { tags, rating } => Ok(ParsedSuggestions::from_found(tags, rating)),
        _ => Err(not_understood()),
    }
}

fn not_understood() -> AppError {
    AppError::Upstream("the AI worker's answer is not understood".into())
}

/// The primary language code of a text; English when it can't tell.
pub fn detect_language(text: &str) -> &'static str {
    use whatlang::Lang;
    let Some(info) = whatlang::detect(text) else {
        return "en";
    };
    match info.lang() {
        Lang::Jpn => "ja",
        Lang::Cmn => "zh",
        Lang::Kor => "ko",
        Lang::Rus => "ru",
        Lang::Ukr => "uk",
        Lang::Fra => "fr",
        Lang::Deu => "de",
        Lang::Spa => "es",
        Lang::Ita => "it",
        Lang::Por => "pt",
        Lang::Nld => "nl",
        Lang::Pol => "pl",
        Lang::Ces => "cs",
        Lang::Swe => "sv",
        Lang::Dan => "da",
        Lang::Nob => "no",
        Lang::Fin => "fi",
        Lang::Tur => "tr",
        Lang::Ara => "ar",
        Lang::Hin => "hi",
        Lang::Tha => "th",
        Lang::Vie => "vi",
        Lang::Ind => "id",
        Lang::Ron => "ro",
        Lang::Hun => "hu",
        Lang::Ell => "el",
        Lang::Heb => "he",
        Lang::Bul => "bg",
        _ => "en",
    }
}

/// Whether the text OCR found on a page reads like a Western language, so
/// that tesseract can read it. On anything else it gives noise, and the page
/// goes to the Japanese reader.
fn reads_as_western(text: &str) -> bool {
    use whatlang::Script;
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let words = tokens
        .iter()
        .filter(|t| t.chars().filter(|c| c.is_alphabetic()).count() >= 3)
        .count();
    // Noise is mostly stray marks and pairs of letters.
    if words < 2 || words.saturating_mul(2) < tokens.len() {
        return false;
    }
    whatlang::detect(text)
        .is_some_and(|info| matches!(info.script(), Script::Latin | Script::Cyrillic))
}

/// Fetches the language pairs Opus-MT needs for this translation, waiting for
/// the download.
async fn ensure_pairs(ai: &LocalAi, model: &CatalogModel, from: &str, to: &str) -> AppResult<()> {
    if !matches!(model.engine, Engine::Opus) {
        return Ok(());
    }
    let route = ai_catalog::opus_route(from, to).ok_or_else(|| {
        AppError::BadRequest(format!(
            "{} can't translate between these languages; choose a bigger model",
            model.name
        ))
    })?;
    for part in route {
        if install::is_installed(&ai.models, part) {
            continue;
        }
        let fetch = async {
            let _turn = ai.download.lock().await;
            install::install(&ai.models, part, |_| {}).await
        };
        tokio::time::timeout(PAIR_TIMEOUT, fetch)
            .await
            .map_err(|_| {
                AppError::Upstream("the language is still downloading; try again".into())
            })??;
    }
    Ok(())
}

/// Translates texts with a translation model.
async fn translate_texts(
    state: &AppState,
    model: &'static CatalogModel,
    gpu: bool,
    from: &str,
    to: &str,
    texts: Vec<String>,
) -> AppResult<Vec<String>> {
    let ai = &state.local_ai;
    ensure_pairs(ai, model, from, to).await?;
    let request = WorkerRequest::Translate {
        model: model.id.to_owned(),
        gpu,
        from: from.to_owned(),
        to: to.to_owned(),
        texts,
    };
    match ask(ai, model, &request, ANSWER_TIMEOUT).await? {
        WorkerReply::Translated { texts } => Ok(texts),
        _ => Err(not_understood()),
    }
}

/// Translates a text with a translation model.
pub async fn translate_text(
    state: &AppState,
    active: &Active,
    text: &str,
    to: &str,
) -> AppResult<String> {
    let from = detect_language(text);
    let texts = translate_texts(
        state,
        active.model,
        active.gpu,
        from,
        to,
        vec![text.to_owned()],
    )
    .await?;
    Ok(texts.join("\n"))
}

/// A box in pixels as a block's place on the 0–1000 scale.
fn scaled_box(b: TextBox, (w, h): (u32, u32)) -> Option<(u16, u16, u16, u16)> {
    let scale = |v: u32, of: u32| -> Option<u16> {
        let v = u64::from(v).saturating_mul(u64::from(BOX_SCALE)) / u64::from(of);
        u16::try_from(v.min(u64::from(BOX_SCALE))).ok()
    };
    if w == 0 || h == 0 {
        return None;
    }
    Some((
        scale(b.x, w)?,
        scale(b.y, h)?,
        scale(b.w, w)?,
        scale(b.h, h)?,
    ))
}

/// Reads the boxes of a page with tesseract, one small image at a time.
async fn read_with_tesseract(
    state: &AppState,
    jpeg: Vec<u8>,
    boxes: Vec<TextBox>,
) -> AppResult<Vec<String>> {
    let Some(ocr) = state.ocr.clone() else {
        return Err(AppError::Internal("tesseract is off".into()));
    };
    let dir = state.local_ai.data_dir.join("ai").join("tmp");
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir).map_err(AppError::internal)?;
        let page = image::load_from_memory(&jpeg).map_err(AppError::internal)?;
        let path = dir.join(format!("crop-{}.png", std::process::id()));
        let mut texts = Vec::with_capacity(boxes.len());
        for b in boxes {
            let crop = page.crop_imm(b.x, b.y, b.w, b.h);
            let saved = crop.save_with_format(&path, image::ImageFormat::Png);
            texts.push(match saved {
                Ok(()) => ocr.scan(&path).unwrap_or_default().replace('\n', " "),
                Err(_) => String::new(),
            });
        }
        // It's a piece of a private page.
        let _ = std::fs::remove_file(&path);
        Ok(texts)
    })
    .await
    .map_err(AppError::internal)?
}

/// Finds, reads and translates the text in a comic page.
pub async fn comics(
    state: &AppState,
    active: &Active,
    jpeg: Vec<u8>,
    size: (u32, u32),
    ocr_text: &str,
    to: &str,
) -> AppResult<Vec<TextBlock>> {
    let Engine::Comics {
        detector,
        translator,
    } = active.model.engine
    else {
        return Err(AppError::Internal("not a comics model".into()));
    };
    let translator = ai_catalog::model(translator)
        .ok_or_else(|| AppError::Internal("unknown translator".into()))?;
    let ai = &state.local_ai;
    let page = encoded(&jpeg);
    let request = WorkerRequest::Detect {
        detector: detector.to_owned(),
        gpu: active.gpu,
        jpeg: page.clone(),
    };
    let mut boxes = match ask(ai, active.model, &request, ANSWER_TIMEOUT).await? {
        WorkerReply::Boxes { boxes } => boxes,
        _ => return Err(not_understood()),
    };
    boxes.truncate(MAX_BOXES);
    if boxes.is_empty() {
        return Ok(Vec::new());
    }
    let western = state.ocr.is_some() && reads_as_western(ocr_text);
    let texts = if western {
        read_with_tesseract(state, jpeg, boxes.clone()).await?
    } else {
        let request = WorkerRequest::Read {
            gpu: active.gpu,
            jpeg: page,
            boxes: boxes.clone(),
        };
        match ask(ai, active.model, &request, ANSWER_TIMEOUT).await? {
            WorkerReply::Texts { texts } => texts,
            _ => return Err(not_understood()),
        }
    };
    let found: Vec<(TextBox, String)> = boxes
        .into_iter()
        .zip(texts)
        .filter(|(_, text)| !text.trim().is_empty())
        .collect();
    if found.is_empty() {
        return Ok(Vec::new());
    }
    let all: Vec<String> = found.iter().map(|(_, t)| t.clone()).collect();
    let from = if western {
        detect_language(&all.join(" "))
    } else {
        "ja"
    };
    let translated = translate_texts(state, translator, active.gpu, from, to, all).await?;
    Ok(found
        .into_iter()
        .zip(translated)
        .filter_map(|((b, text), translation)| {
            let (x, y, w, h) = scaled_box(b, size)?;
            Some(TextBlock {
                x,
                y,
                w,
                h,
                text,
                translation,
            })
        })
        .collect())
}

#[cfg(test)]
pub(crate) fn mark_all_installed(ai: &LocalAi, model: &CatalogModel) {
    for part in model.parts() {
        install::mark_installed(&ai.models, part);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_languages() {
        assert_eq!(
            detect_language("今日はいい天気ですね。公園に行きましょう。"),
            "ja"
        );
        assert_eq!(
            detect_language("Oggi è una bellissima giornata per andare al parco con gli amici."),
            "it"
        );
        assert_eq!(
            detect_language("The weather is lovely today, let's go to the park together."),
            "en"
        );
        assert_eq!(detect_language(""), "en");
    }

    #[test]
    fn tells_what_tesseract_can_read() {
        assert!(reads_as_western(
            "The weather is lovely today, let's go to the park together."
        ));
        assert!(!reads_as_western(
            "今日はいい天気ですね。公園に行きましょう。"
        ));
        assert!(!reads_as_western(""));
        assert!(!reads_as_western("| ~ _ ."));
        assert!(!reads_as_western("Ff ) NN | 1 ~ ae J , oe"));
    }

    #[test]
    fn scales_boxes() {
        let b = TextBox {
            x: 150,
            y: 300,
            w: 75,
            h: 150,
        };
        assert_eq!(scaled_box(b, (300, 600)), Some((500, 500, 250, 250)));
        // Hostile numbers stay on the scale.
        let b = TextBox {
            x: u32::MAX,
            y: 0,
            w: u32::MAX,
            h: 1,
        };
        assert_eq!(scaled_box(b, (1, 1)), Some((1000, 0, 1000, 1000)));
        assert_eq!(scaled_box(b, (0, 10)), None);
    }

    #[test]
    fn uninstalling_keeps_what_other_models_need() {
        let dir = tempfile::tempdir().unwrap();
        let ai = LocalAi::new(None, dir.path());
        let model = |id| ai_catalog::model(id).unwrap();
        let ids = |m: &CatalogModel, in_use: &[&CatalogModel]| {
            removable(&ai, m, in_use)
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>()
        };
        mark_all_installed(&ai, model("comics-opus"));
        mark_all_installed(&ai, model("comics-m2m"));
        mark_all_installed(&ai, model("tresnet-320"));
        // The reader and the translators are needed by more models.
        assert_eq!(ids(model("comics-opus"), &[]), vec!["det-small"]);
        assert_eq!(ids(model("comics-m2m"), &[]), vec!["det-large"]);
        // Both sizes of TResNet are the same files.
        assert_eq!(ids(model("tresnet-448"), &[]), vec!["tresnet"]);
        // A translator goes only if the tool's model is what's asked to go.
        assert_eq!(ids(model("m2m"), &[]), vec!["m2m"]);
        // What's in use stays, whatever the others need.
        assert_eq!(
            ids(model("comics-m2m"), &[model("comics-opus-large")]),
            vec!["m2m"]
        );
        assert!(ids(model("m2m"), &[model("comics-m2m")]).is_empty());
    }

    #[test]
    fn overlapping_models_can_always_be_uninstalled() {
        let dir = tempfile::tempdir().unwrap();
        let ai = LocalAi::new(None, dir.path());
        let comics: Vec<&CatalogModel> =
            MODELS.iter().filter(|m| m.tool == AiTool::Comics).collect();
        for m in &comics {
            mark_all_installed(&ai, m);
        }
        for m in &comics {
            assert!(!removable(&ai, m, &[]).is_empty(), "{}", m.id);
        }
    }

    #[test]
    fn uninstalling_the_small_translator_takes_its_extra_languages() {
        let dir = tempfile::tempdir().unwrap();
        let ai = LocalAi::new(None, dir.path());
        let opus = ai_catalog::model("opus").unwrap();
        mark_all_installed(&ai, opus);
        let parts = removable(&ai, opus, &[]);
        assert!(parts.iter().any(|c| c.id == "opus-ja-en"));
        assert!(parts.iter().any(|c| c.id == "opus-en-it"));
    }

    #[tokio::test]
    async fn a_missing_worker_is_a_reason_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let ai = LocalAi::new(Some(dir.path().join("nope")), dir.path());
        let reason = ai.worker.check().await.unwrap_err();
        assert!(reason.contains("no AI worker"), "{reason}");
    }
}
