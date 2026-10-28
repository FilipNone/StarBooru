//! Local AI: models that run on the server's own hardware through an
//! [Ollama](https://ollama.com) runtime, and the check that tells an admin
//! whether a model will run well there.
//!
//! Three presets cover the usual machines, from a Raspberry Pi to a desktop
//! with a graphics card. All are Qwen3-VL instruct models: one model per
//! tier reads images (tagging, comics) and translates, so only one is kept
//! in memory.

use serde::{Deserialize, Serialize};

/// A billion bytes, the unit model sizes are published in.
const GB: u64 = 1_000_000_000;
/// Memory the operating system and StarBooru keep for themselves.
const RAM_HEADROOM: u64 = 3 * GB / 2;
/// Longest Ollama model name accepted (`library/name:tag`).
pub const MAX_MODEL_CHARS: usize = 200;

/// The presets, smallest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocalTier {
    /// A Raspberry Pi 5 or another small board: CPU only, 8 GB of memory.
    Small,
    /// A laptop or mini PC with an integrated GPU and 8 GB or more.
    Medium,
    /// A desktop with a graphics card of 8 GB or more.
    Large,
}

/// A tier's model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierPreset {
    pub tier: LocalTier,
    /// Ollama model name.
    pub model: &'static str,
    /// Download size.
    pub file_bytes: u64,
}

pub const TIERS: [TierPreset; 3] = [
    TierPreset {
        tier: LocalTier::Small,
        model: "qwen3-vl:2b-instruct",
        file_bytes: 1_900_000_000,
    },
    TierPreset {
        tier: LocalTier::Medium,
        model: "qwen3-vl:4b-instruct",
        file_bytes: 3_300_000_000,
    },
    TierPreset {
        tier: LocalTier::Large,
        model: "qwen3-vl:8b-instruct",
        file_bytes: 6_200_000_000,
    },
];

impl LocalTier {
    pub fn preset(self) -> TierPreset {
        match self {
            Self::Small => TIERS[0],
            Self::Medium => TIERS[1],
            Self::Large => TIERS[2],
        }
    }

    /// The tier whose preset is `model`, if any.
    pub fn of_model(model: &str) -> Option<Self> {
        TIERS.iter().find(|p| p.model == model).map(|p| p.tier)
    }
}

/// Memory a model needs while it runs: its weights plus the context and
/// the image encoder's working space.
pub fn memory_needed(file_bytes: u64) -> u64 {
    file_bytes
        .saturating_add(file_bytes.saturating_mul(15) / 100)
        .saturating_add(4 * GB / 10)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuKind {
    /// A graphics card with its own memory.
    Dedicated,
    /// Built into the processor; shares the system memory.
    Integrated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gpu {
    /// Vendor and model, as far as the system tells.
    pub name: String,
    pub kind: GpuKind,
    /// Its own memory, when known.
    pub vram_bytes: Option<u64>,
    /// The part of it a model can use: free now, plus what the AI runtime
    /// already holds. The desktop and other programs use the rest.
    #[serde(default)]
    pub vram_free_bytes: Option<u64>,
}

/// The machine the server runs on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hardware {
    /// `x86_64`, `aarch64`, …
    pub arch: String,
    /// The board, on single-board computers ("Raspberry Pi 5 Model B").
    pub board: Option<String>,
    /// Processor model, when known.
    pub cpu: Option<String>,
    pub threads: u32,
    /// 0 when unknown.
    pub ram_total: u64,
    pub ram_available: u64,
    pub gpus: Vec<Gpu>,
}

/// How well a model will run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Answers in seconds.
    Good,
    /// Works, but takes a minute or so per image: fine in the background.
    Slow,
    /// Fits in memory but is too slow to be useful.
    Poor,
    /// Not enough memory.
    NoFit,
    /// The hardware couldn't be read.
    Unknown,
}

/// Where the model would run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Processor {
    Gpu,
    /// Split between the graphics card and the processor.
    PartlyGpu,
    /// The integrated GPU, in shared memory.
    SharedGpu,
    Cpu,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FitCheck {
    pub fit: Fit,
    pub runs_on: Processor,
    /// Memory the model needs while it runs.
    pub need_bytes: u64,
}

/// Predicts how a model of `file_bytes` runs on `hw`.
pub fn assess(hw: &Hardware, file_bytes: u64) -> FitCheck {
    let need = memory_needed(file_bytes);
    let check = |fit, runs_on| FitCheck {
        fit,
        runs_on,
        need_bytes: need,
    };
    if hw.ram_total == 0 {
        return check(Fit::Unknown, Processor::None);
    }
    let ram = hw.ram_total.saturating_sub(RAM_HEADROOM);
    let fast_cpu = hw.threads >= 8;
    let vram = hw
        .gpus
        .iter()
        .filter(|g| g.kind == GpuKind::Dedicated)
        .filter_map(|g| g.vram_free_bytes.or(g.vram_bytes))
        .max()
        .unwrap_or(0);
    if vram >= need {
        return check(Fit::Good, Processor::Gpu);
    }
    if vram >= need / 2 && ram >= need {
        // The layers left on the processor set the pace.
        let mostly = vram >= need / 4 * 3;
        let fit = if fast_cpu && mostly {
            Fit::Good
        } else {
            Fit::Slow
        };
        return check(fit, Processor::PartlyGpu);
    }
    if ram < need {
        return check(Fit::NoFit, Processor::None);
    }
    if hw.gpus.iter().any(|g| g.kind == GpuKind::Integrated) {
        let fit = if need <= 5 * GB { Fit::Good } else { Fit::Slow };
        return check(fit, Processor::SharedGpu);
    }
    let fit = match need {
        n if n <= 3 * GB => match hw.threads {
            8.. => Fit::Good,
            4.. => Fit::Slow,
            _ => Fit::Poor,
        },
        n if n <= 11 * GB / 2 => {
            if fast_cpu {
                Fit::Slow
            } else {
                Fit::Poor
            }
        }
        _ => {
            if hw.threads >= 16 {
                Fit::Slow
            } else {
                Fit::Poor
            }
        }
    };
    check(fit, Processor::Cpu)
}

/// The largest preset that runs well, else the largest that runs at all,
/// else the smallest.
pub fn recommended_tier(hw: &Hardware) -> LocalTier {
    let fits = |want: &[Fit]| {
        TIERS
            .iter()
            .rev()
            .find(|p| want.contains(&assess(hw, p.file_bytes).fit))
            .map(|p| p.tier)
    };
    fits(&[Fit::Good])
        .or_else(|| fits(&[Fit::Slow]))
        .unwrap_or(LocalTier::Small)
}

/// A measured run: how long a small tagging request took.
pub fn fit_from_seconds(seconds: f64) -> Fit {
    if seconds < 20.0 {
        Fit::Good
    } else if seconds < 120.0 {
        Fit::Slow
    } else {
        Fit::Poor
    }
}

/// Checks an Ollama model name (`qwen3-vl:4b-instruct`, `library/x:tag`).
pub fn clean_model_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("enter a model name".into());
    }
    if name.chars().count() > MAX_MODEL_CHARS
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c))
        || name.starts_with(['.', '/', '-', ':'])
        || name.contains("..")
    {
        return Err("that is not a model name".into());
    }
    Ok(name.to_owned())
}

/// One preset, checked against this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierCheck {
    pub tier: LocalTier,
    pub model: String,
    pub file_bytes: u64,
    pub check: FitCheck,
    /// Already downloaded by the runtime.
    pub installed: bool,
}

/// A model the runtime has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModel {
    pub name: String,
    pub file_bytes: u64,
    pub check: FitCheck,
}

/// The Ollama runtime, when it answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalRuntime {
    pub version: String,
    pub models: Vec<LocalModel>,
}

/// A model download.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullProgress {
    pub model: String,
    /// Ollama's status line ("pulling manifest", "success").
    pub status: String,
    pub completed: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}

/// `GET /admin/ai/local`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalAiStatus {
    pub hardware: Hardware,
    pub tiers: Vec<TierCheck>,
    pub recommended: Option<LocalTier>,
    /// `None` when the runtime doesn't answer; see `runtime_error`.
    pub runtime: Option<LocalRuntime>,
    pub runtime_error: Option<String>,
    /// The latest download, while it runs and after.
    pub pull: Option<PullProgress>,
}

/// `POST /admin/ai/local/pull`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    pub model: String,
}

/// `POST /admin/ai/local/test`: one tagging request on a small image.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocalTestResult {
    pub seconds: f64,
    /// Share of the model in GPU memory, as the runtime reports it.
    pub gpu_percent: Option<u8>,
    pub fit: Fit,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(kind: GpuKind, vram_gb: Option<u64>) -> Gpu {
        Gpu {
            name: "test".into(),
            kind,
            vram_bytes: vram_gb.map(|g| g * GB),
            vram_free_bytes: None,
        }
    }

    fn machine(threads: u32, ram_gb: u64, gpus: Vec<Gpu>) -> Hardware {
        Hardware {
            arch: "x86_64".into(),
            threads,
            ram_total: ram_gb * GB,
            ram_available: ram_gb * GB / 2,
            gpus,
            ..Hardware::default()
        }
    }

    fn fits(hw: &Hardware) -> Vec<(Fit, Processor)> {
        TIERS
            .iter()
            .map(|p| {
                let c = assess(hw, p.file_bytes);
                (c.fit, c.runs_on)
            })
            .collect()
    }

    #[test]
    fn raspberry_pi() {
        let pi5 = Hardware {
            arch: "aarch64".into(),
            board: Some("Raspberry Pi 5 Model B Rev 1.0".into()),
            ..machine(4, 8, vec![])
        };
        assert_eq!(
            fits(&pi5),
            vec![
                (Fit::Slow, Processor::Cpu),
                (Fit::Poor, Processor::Cpu),
                (Fit::NoFit, Processor::None),
            ]
        );
        assert_eq!(recommended_tier(&pi5), LocalTier::Small);
        let pi4 = machine(4, 4, vec![]);
        assert_eq!(assess(&pi4, TIERS[0].file_bytes).fit, Fit::NoFit);
        assert_eq!(recommended_tier(&pi4), LocalTier::Small);
    }

    #[test]
    fn laptop_with_integrated_gpu() {
        let laptop = machine(8, 16, vec![gpu(GpuKind::Integrated, None)]);
        assert_eq!(
            fits(&laptop),
            vec![
                (Fit::Good, Processor::SharedGpu),
                (Fit::Good, Processor::SharedGpu),
                (Fit::Slow, Processor::SharedGpu),
            ]
        );
        assert_eq!(recommended_tier(&laptop), LocalTier::Medium);
        let small = machine(4, 8, vec![gpu(GpuKind::Integrated, Some(1))]);
        assert_eq!(assess(&small, TIERS[2].file_bytes).fit, Fit::NoFit);
    }

    #[test]
    fn desktop_with_a_graphics_card() {
        // A Ryzen 9 with 32 GB and an 8 GB Radeon.
        let desktop = Hardware {
            gpus: vec![Gpu {
                name: "AMD".into(),
                kind: GpuKind::Dedicated,
                vram_bytes: Some(8_539_602_944),
                vram_free_bytes: None,
            }],
            ..machine(24, 32, vec![])
        };
        assert!(
            fits(&desktop)
                .iter()
                .all(|f| *f == (Fit::Good, Processor::Gpu))
        );
        assert_eq!(recommended_tier(&desktop), LocalTier::Large);
        // A 4 GB card holds part of the large model.
        let older = machine(6, 16, vec![gpu(GpuKind::Dedicated, Some(4))]);
        assert_eq!(
            assess(&older, TIERS[2].file_bytes),
            FitCheck {
                fit: Fit::Slow,
                runs_on: Processor::PartlyGpu,
                need_bytes: memory_needed(TIERS[2].file_bytes),
            }
        );
        // With the desktop using 3.8 GB of the card, the large model
        // spills onto the processor (measured: 63% on the card, 13-22 s).
        let busy = Hardware {
            gpus: vec![Gpu {
                vram_free_bytes: Some(4_740_000_000),
                ..desktop.gpus[0].clone()
            }],
            ..desktop.clone()
        };
        assert_eq!(
            fits(&busy),
            vec![
                (Fit::Good, Processor::Gpu),
                (Fit::Good, Processor::Gpu),
                (Fit::Slow, Processor::PartlyGpu),
            ]
        );
        assert_eq!(recommended_tier(&busy), LocalTier::Medium);
        // Unknown VRAM counts as none.
        let unknown = machine(16, 32, vec![gpu(GpuKind::Dedicated, None)]);
        assert_eq!(
            assess(&unknown, TIERS[2].file_bytes).runs_on,
            Processor::Cpu
        );
    }

    #[test]
    fn unknown_hardware() {
        let hw = Hardware::default();
        assert_eq!(assess(&hw, TIERS[0].file_bytes).fit, Fit::Unknown);
        assert_eq!(recommended_tier(&hw), LocalTier::Small);
    }

    #[test]
    fn presets() {
        for p in TIERS {
            assert_eq!(p.tier.preset(), p);
            assert_eq!(LocalTier::of_model(p.model), Some(p.tier));
            assert_eq!(clean_model_name(p.model).as_deref(), Ok(p.model));
        }
        assert_eq!(LocalTier::of_model("llava"), None);
        assert!(memory_needed(u64::MAX) == u64::MAX);
    }

    #[test]
    fn measured_fit() {
        assert_eq!(fit_from_seconds(3.0), Fit::Good);
        assert_eq!(fit_from_seconds(45.0), Fit::Slow);
        assert_eq!(fit_from_seconds(600.0), Fit::Poor);
    }

    #[test]
    fn model_names() {
        for ok in [
            "llava",
            " gemma3:4b ",
            "library/qwen3-vl:2b-instruct",
            "a.b_c",
        ] {
            assert_eq!(clean_model_name(ok), Ok(ok.trim().to_owned()), "{ok}");
        }
        for bad in [
            "",
            "a b",
            "x;rm",
            "../x",
            "/abs",
            "-flag",
            "é",
            &"a".repeat(201),
        ] {
            assert!(clean_model_name(bad).is_err(), "{bad}");
        }
    }
}
