//! Local AI: what this machine can run, and the Ollama runtime that runs it.
//!
//! The hardware is read from `/proc` and `/sys` (Linux; elsewhere only the
//! processor count is known), so in Docker it describes the host. Model
//! downloads go through Ollama's own API; one runs at a time and its
//! progress is kept in `AppState::local_pull`.

use std::io::Cursor;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::Value;
use starbooru_core::ai::TAG_PROMPT;
use starbooru_core::local_ai::{
    Gpu, GpuKind, Hardware, LocalAiStatus, LocalModel, LocalRuntime, LocalTestResult, PullProgress,
    TIERS, TierCheck, assess, fit_from_seconds, machine_tier,
};
use tracing::warn;

use crate::ai::{AiSetup, client, request_error};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Status calls to the runtime must be quick.
const STATUS_TIMEOUT: Duration = Duration::from_secs(5);
/// A download may take hours on a slow line.
const PULL_TIMEOUT: Duration = Duration::from_secs(12 * 3600);
/// Longest line of pull progress read.
const MAX_LINE: usize = 64 * 1024;
/// Most bytes read from a status answer.
const MAX_STATUS_BYTES: usize = 1 << 20;
/// Graphics cards with less memory than this are integrated ones that
/// report their small carve-out.
const MIN_DEDICATED_VRAM: u64 = 4 << 30;

/// The parts of the hardware that don't change while the server runs.
fn fixed() -> &'static Hardware {
    static FIXED: OnceLock<Hardware> = OnceLock::new();
    FIXED.get_or_init(|| Hardware {
        arch: std::env::consts::ARCH.to_owned(),
        board: read_trimmed("/proc/device-tree/model"),
        cpu: cpu_name(),
        threads: std::thread::available_parallelism()
            .map(|n| u32::try_from(n.get()).unwrap_or(u32::MAX))
            .unwrap_or(1),
        ram_total: 0,
        ram_available: 0,
        gpus: Vec::new(),
    })
}

/// This machine, with its memory and free graphics memory as of now.
pub fn hardware() -> Hardware {
    let mut hw = fixed().clone();
    hw.gpus = gpus();
    if let Some(meminfo) = read_trimmed("/proc/meminfo") {
        hw.ram_total = meminfo_bytes(&meminfo, "MemTotal").unwrap_or(0);
        hw.ram_available = meminfo_bytes(&meminfo, "MemAvailable").unwrap_or(0);
    }
    hw
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    (!text.is_empty()).then(|| text.to_owned())
}

/// A `/proc/meminfo` field, in bytes.
fn meminfo_bytes(meminfo: &str, field: &str) -> Option<u64> {
    let line = meminfo
        .lines()
        .find(|l| l.split(':').next() == Some(field))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    kb.checked_mul(1024)
}

fn cpu_name() -> Option<String> {
    let info = read_trimmed("/proc/cpuinfo")?;
    info.lines().find_map(|l| {
        let (key, value) = l.split_once(':')?;
        let value = value.trim();
        (matches!(key.trim(), "model name" | "Model" | "Hardware") && !value.is_empty())
            .then(|| value.to_owned())
    })
}

/// The graphics cards under `/sys/class/drm`. Boards like the Raspberry
/// Pi have no PCI GPU and list none: Ollama can't use theirs anyway.
fn gpus() -> Vec<Gpu> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut cards: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.strip_prefix("card")
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    cards.sort();
    let nvidia_vram = nvidia_vram();
    let mut nvidia_seen = 0usize;
    cards
        .iter()
        .filter_map(|card| {
            let dev = Path::new("/sys/class/drm").join(card).join("device");
            let vendor = read_trimmed(dev.join("vendor"))?;
            let device = read_trimmed(dev.join("device")).unwrap_or_default();
            let vram: Option<u64> =
                read_trimmed(dev.join("mem_info_vram_total")).and_then(|v| v.parse().ok());
            let used: Option<u64> =
                read_trimmed(dev.join("mem_info_vram_used")).and_then(|v| v.parse().ok());
            Some(match vendor.as_str() {
                "0x1002" => {
                    let kind = if vram.is_some_and(|v| v >= MIN_DEDICATED_VRAM) {
                        GpuKind::Dedicated
                    } else {
                        GpuKind::Integrated
                    };
                    let vram = vram.filter(|_| kind == GpuKind::Dedicated);
                    Gpu {
                        name: "AMD Radeon".into(),
                        kind,
                        vram_bytes: vram,
                        vram_free_bytes: vram.zip(used).map(|(t, u)| t.saturating_sub(u)),
                    }
                }
                "0x10de" => {
                    let (total, free) = nvidia_vram
                        .get(nvidia_seen)
                        .copied()
                        .map_or((None, None), |(t, f)| (Some(t), Some(f)));
                    nvidia_seen = nvidia_seen.saturating_add(1);
                    Gpu {
                        name: "NVIDIA".into(),
                        kind: GpuKind::Dedicated,
                        vram_bytes: total,
                        vram_free_bytes: free,
                    }
                }
                "0x8086" => {
                    // Arc cards (Alchemist 0x56xx, Battlemage 0xe2xx) have
                    // their own memory, which the driver doesn't report here.
                    let arc = device.starts_with("0x56") || device.starts_with("0xe2");
                    Gpu {
                        name: if arc { "Intel Arc" } else { "Intel" }.into(),
                        kind: if arc {
                            GpuKind::Dedicated
                        } else {
                            GpuKind::Integrated
                        },
                        vram_bytes: None,
                        vram_free_bytes: None,
                    }
                }
                _ => Gpu {
                    name: format!("GPU {vendor}"),
                    kind: GpuKind::Integrated,
                    vram_bytes: None,
                    vram_free_bytes: None,
                },
            })
        })
        .collect()
}

/// Total and free memory of each NVIDIA card, through `nvidia-smi` when it
/// is installed.
fn nvidia_vram() -> Vec<(u64, u64)> {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=memory.total,memory.free",
        "--format=csv,noheader,nounits",
    ]);
    let Some(out) = crate::ocr::run(&mut cmd, 4096, Duration::from_secs(5)) else {
        return Vec::new();
    };
    parse_nvidia(&String::from_utf8_lossy(&out))
}

fn parse_nvidia(out: &str) -> Vec<(u64, u64)> {
    let mib = |s: &str| s.trim().parse::<u64>().ok()?.checked_mul(1 << 20);
    out.lines()
        .filter_map(|l| {
            let (total, free) = l.split_once(',')?;
            Some((mib(total)?, mib(free)?))
        })
        .collect()
}

async fn get_json(url: &str) -> AppResult<Value> {
    let mut resp = client()?
        .get(url)
        .timeout(STATUS_TIMEOUT)
        .send()
        .await
        .map_err(|e| request_error(&e, true))?;
    if !resp.status().is_success() {
        return Err(AppError::Upstream(format!(
            "local AI runtime answered {}",
            resp.status().as_u16()
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| request_error(&e, true))? {
        if body.len().saturating_add(chunk.len()) > MAX_STATUS_BYTES {
            return Err(AppError::Upstream(
                "the local AI runtime's answer is too long".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body)
        .map_err(|_| AppError::Upstream("the local AI runtime's answer is not understood".into()))
}

/// Ollama's version and models.
async fn runtime(url: &str, hw: &Hardware) -> AppResult<LocalRuntime> {
    let version = get_json(&format!("{url}/api/version")).await?;
    let tags = get_json(&format!("{url}/api/tags")).await?;
    let models = tags
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|m| {
                    let name = m.get("name").and_then(Value::as_str)?.to_owned();
                    let file_bytes = m.get("size").and_then(Value::as_u64).unwrap_or(0);
                    Some(LocalModel {
                        name,
                        file_bytes,
                        check: assess(hw, file_bytes),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(LocalRuntime {
        version: version
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        models,
    })
}

/// GPU memory the runtime's loaded models hold, from Ollama's `/api/ps`.
fn runtime_vram(ps: &Value) -> u64 {
    ps.get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|m| m.get("size_vram").and_then(Value::as_u64))
                .fold(0u64, u64::saturating_add)
        })
        .unwrap_or(0)
}

/// What this machine can run, and what the runtime has.
pub async fn status(state: &AppState, setup: &AiSetup) -> LocalAiStatus {
    let mut hw = hardware();
    if !setup.local_url.is_empty() {
        // A loaded model's memory is the runtime's to use again.
        let held = get_json(&format!("{}/api/ps", setup.local_url))
            .await
            .map(|ps| runtime_vram(&ps))
            .unwrap_or(0);
        if let Some(gpu) = hw
            .gpus
            .iter_mut()
            .filter(|g| g.kind == GpuKind::Dedicated)
            .max_by_key(|g| g.vram_bytes)
            && let Some(free) = gpu.vram_free_bytes.as_mut()
        {
            *free = free
                .saturating_add(held)
                .min(gpu.vram_bytes.unwrap_or(u64::MAX));
        }
    }
    let (runtime, runtime_error) = if setup.local_url.is_empty() {
        (None, None)
    } else {
        match runtime(&setup.local_url, &hw).await {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(e.to_string())),
        }
    };
    let installed = |model: &str| {
        runtime.as_ref().is_some_and(|r| {
            r.models
                .iter()
                .any(|m| m.name == model || m.name == format!("{model}:latest"))
        })
    };
    let tiers = TIERS
        .iter()
        .map(|p| TierCheck {
            tier: p.tier,
            model: p.model.to_owned(),
            file_bytes: p.file_bytes,
            check: assess(&hw, p.file_bytes),
            installed: installed(p.model),
        })
        .collect();
    let recommended = (hw.ram_total > 0).then(|| machine_tier(&hw));
    LocalAiStatus {
        tiers,
        recommended,
        runtime,
        runtime_error,
        pull: state.local_pull.lock().await.clone(),
        hardware: hw,
    }
}

/// Starts downloading `model` in the background. One download at a time.
pub async fn start_pull(state: &AppState, url: String, model: String) -> AppResult<()> {
    {
        let mut pull = state.local_pull.lock().await;
        if pull.as_ref().is_some_and(|p| !p.done) {
            return Err(AppError::Conflict {
                code: "busy",
                message: "a model is already downloading".into(),
            });
        }
        *pull = Some(PullProgress {
            model: model.clone(),
            status: "starting".into(),
            ..PullProgress::default()
        });
    }
    let state = state.clone();
    tokio::spawn(async move {
        let result = pull(&state, &url, &model).await;
        let mut progress = state.local_pull.lock().await;
        if let Some(p) = progress.as_mut() {
            p.done = true;
            if let Err(e) = result {
                warn!("downloading the local AI model failed: {e}");
                p.error = Some(e.to_string());
            }
        }
    });
    Ok(())
}

async fn pull(state: &AppState, url: &str, model: &str) -> AppResult<()> {
    let body = serde_json::to_vec(&serde_json::json!({ "model": model, "stream": true }))
        .map_err(AppError::internal)?;
    let mut resp = client()?
        .post(format!("{url}/api/pull"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .timeout(PULL_TIMEOUT)
        .body(body)
        .send()
        .await
        .map_err(|e| request_error(&e, true))?;
    if !resp.status().is_success() {
        return Err(AppError::Upstream(format!(
            "local AI runtime answered {}",
            resp.status().as_u16()
        )));
    }
    let mut line = Vec::new();
    let mut success = false;
    while let Some(chunk) = resp.chunk().await.map_err(|e| request_error(&e, true))? {
        for byte in chunk {
            if byte != b'\n' {
                if line.len() < MAX_LINE {
                    line.push(byte);
                }
                continue;
            }
            success |= apply_progress(state, &line).await?;
            line.clear();
        }
    }
    success |= apply_progress(state, &line).await?;
    if success {
        Ok(())
    } else {
        Err(AppError::Upstream(
            "the download stopped before it finished".into(),
        ))
    }
}

/// Applies one line of Ollama's pull progress. True on "success".
async fn apply_progress(state: &AppState, line: &[u8]) -> AppResult<bool> {
    let Ok(value) = serde_json::from_slice::<Value>(line) else {
        return Ok(false);
    };
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        let error: String = error
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        return Err(AppError::Upstream(error));
    }
    let status: String = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect();
    let mut progress = state.local_pull.lock().await;
    if let Some(p) = progress.as_mut() {
        if let Some(total) = value.get("total").and_then(Value::as_u64) {
            p.total = total;
            p.completed = value.get("completed").and_then(Value::as_u64).unwrap_or(0);
        }
        if !status.is_empty() {
            p.status.clone_from(&status);
        }
    }
    Ok(status == "success")
}

/// A small generated picture for the speed test: shapes, no personal data.
fn test_image() -> AppResult<Vec<u8>> {
    let img = image::RgbImage::from_fn(384, 384, |x, y| {
        let (dx, dy) = (i64::from(x) - 192, i64::from(y) - 160);
        if dx * dx + dy * dy < 90 * 90 {
            image::Rgb([250, 200, 40])
        } else if y > 280 {
            image::Rgb([40, 140, 60])
        } else {
            let blue = u8::try_from(150 + y / 4).unwrap_or(u8::MAX);
            image::Rgb([90, 160, blue])
        }
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg)
        .map_err(AppError::internal)?;
    Ok(out.into_inner())
}

/// Times one tagging request on the local model, then asks the runtime how
/// much of the model sits in GPU memory.
pub async fn test(setup: &AiSetup) -> AppResult<LocalTestResult> {
    if !setup.local_enabled || setup.local_url.is_empty() {
        return Err(AppError::BadRequest("local AI is turned off".into()));
    }
    let provider = setup.local();
    let jpeg = test_image()?;
    let started = Instant::now();
    crate::ai::chat(&provider, TAG_PROMPT, Some(&jpeg)).await?;
    let seconds = started.elapsed().as_secs_f64();
    let gpu_percent = get_json(&format!("{}/api/ps", setup.local_url))
        .await
        .ok()
        .and_then(|ps| gpu_share(&ps, &provider.model));
    Ok(LocalTestResult {
        seconds,
        gpu_percent,
        fit: fit_from_seconds(seconds),
    })
}

/// The share of `model` in GPU memory, from Ollama's `/api/ps`.
fn gpu_share(ps: &Value, model: &str) -> Option<u8> {
    let m = ps.get("models")?.as_array()?.iter().find(|m| {
        m.get("name")
            .and_then(Value::as_str)
            .is_some_and(|n| n == model || n == format!("{model}:latest"))
    })?;
    let size = m.get("size")?.as_u64()?;
    let vram = m.get("size_vram")?.as_u64()?;
    if size == 0 {
        return None;
    }
    let percent = vram.saturating_mul(100) / size;
    u8::try_from(percent.min(100)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_meminfo() {
        let m = "MemTotal:       32768000 kB\nMemFree: 1 kB\nMemAvailable:   16384000 kB\n";
        assert_eq!(meminfo_bytes(m, "MemTotal"), Some(32_768_000 * 1024));
        assert_eq!(meminfo_bytes(m, "MemAvailable"), Some(16_384_000 * 1024));
        assert_eq!(meminfo_bytes(m, "Mem"), None);
        assert_eq!(meminfo_bytes("MemTotal: lots kB", "MemTotal"), None);
    }

    #[test]
    fn reads_gpu_share() {
        let ps = serde_json::json!({ "models": [
            { "name": "qwen3-vl:2b-instruct", "size": 4000, "size_vram": 1000 },
            { "name": "llava:latest", "size": 10, "size_vram": 10 },
        ]});
        assert_eq!(gpu_share(&ps, "qwen3-vl:2b-instruct"), Some(25));
        assert_eq!(gpu_share(&ps, "llava"), Some(100));
        assert_eq!(gpu_share(&ps, "other"), None);
        assert_eq!(gpu_share(&serde_json::json!({}), "llava"), None);
    }

    #[test]
    fn reads_nvidia_smi() {
        assert_eq!(
            parse_nvidia("12288, 10000\n8192,8000\nbad\n"),
            vec![(12288 << 20, 10000 << 20), (8192 << 20, 8000 << 20)]
        );
        let ps = serde_json::json!({ "models": [{ "size_vram": 5 }, { "size_vram": 7 }, {}] });
        assert_eq!(runtime_vram(&ps), 12);
        assert_eq!(runtime_vram(&serde_json::json!({})), 0);
    }

    #[test]
    fn makes_a_test_image() {
        let jpeg = test_image().unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn reads_this_machine() {
        let hw = hardware();
        assert!(hw.threads >= 1);
        assert!(!hw.arch.is_empty());
        if cfg!(target_os = "linux") {
            assert!(hw.ram_total > 0);
        }
    }
}
