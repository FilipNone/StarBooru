//! The AI models StarBooru can run by itself, and which one suits a machine.
//!
//! Every model is made for its job (see `docs/architecture.md` › Local AI) and
//! every file is pinned to a commit and a SHA-256 (`ai_files.rs`). The admin can
//! run any of them; the presets choose one by how fast this machine runs it:
//!
//! - Fast: the most accurate model that feels instant here. None: greyed out.
//! - Balanced: the most accurate model within 5 seconds, or else the quickest
//!   one, so it's there whenever anything fits in memory.
//! - Accurate: the most accurate model backed by published benchmarks that
//!   answers within 30 seconds here. None: greyed out.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ai_files as files;

/// One download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelFile {
    pub repo: &'static str,
    pub rev: &'static str,
    pub path: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

impl ModelFile {
    /// Where it comes from.
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.rev, self.path
        )
    }

    /// The file's name in its component's folder.
    pub fn name(&self) -> &'static str {
        self.path.rsplit('/').next().unwrap_or(self.path)
    }
}

/// A set of files installed together in `<models>/<id>/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Component {
    pub id: &'static str,
    /// The tool that owns it. Uninstalling a model of another tool that uses
    /// it leaves it in place.
    pub tool: AiTool,
    pub files: &'static [ModelFile],
}

impl Component {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// What an AI feature is.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AiTool {
    #[default]
    Tagging,
    Translation,
    Comics,
}

impl AiTool {
    pub const ALL: [AiTool; 3] = [AiTool::Tagging, AiTool::Translation, AiTool::Comics];

    pub fn as_str(self) -> &'static str {
        match self {
            AiTool::Tagging => "tagging",
            AiTool::Translation => "translation",
            AiTool::Comics => "comics",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

/// Where an AI feature runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMode {
    #[default]
    Off,
    /// StarBooru's own models, on this server.
    Local,
    /// An OpenAI-compatible API, hosted or on the LAN.
    Api,
}

impl AiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AiMode::Off => "off",
            AiMode::Local => "local",
            AiMode::Api => "api",
        }
    }

    /// `hosted` is the older name of `api`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "off" => Some(AiMode::Off),
            "local" => Some(AiMode::Local),
            "api" | "hosted" => Some(AiMode::Api),
            _ => None,
        }
    }
}

/// How long an answer may take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiPreset {
    Fast,
    #[default]
    Balanced,
    Accurate,
}

impl AiPreset {
    pub const ALL: [AiPreset; 3] = [AiPreset::Fast, AiPreset::Balanced, AiPreset::Accurate];

    /// The longest one answer may take, in milliseconds, with the model
    /// loaded. "Instant" is what still feels immediate.
    pub fn budget_ms(self) -> u32 {
        match self {
            AiPreset::Fast => 1_000,
            AiPreset::Balanced => 5_000,
            AiPreset::Accurate => 30_000,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AiPreset::Fast => "fast",
            AiPreset::Balanced => "balanced",
            AiPreset::Accurate => "accurate",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

/// The processor work a model mostly does, which ages differently between
/// machines: some processors are much better at one than the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kernel {
    Float,
    Int8,
}

/// How a model runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Engine {
    /// ML-Danbooru: picture stretched to `size`², with its own tag list.
    MlDanbooru {
        size: u32,
        file: &'static str,
    },
    /// WD tagger: picture padded to a white square, with its own tag list.
    Wd {
        threshold: f32,
    },
    /// Like the WD taggers, with channels first and values from -1 to 1.
    DanbooruV1 {
        threshold: f32,
    },
    /// Trained on photographs as well as drawings.
    JoyTag {
        threshold: f32,
    },
    /// Language pairs, through English when there's no direct one.
    Opus,
    M2m,
    /// T5 model told the target language at the start of the text.
    Madlad,
    Gemma,
    /// Finds text, reads it and hands it to a translator.
    Comics {
        detector: &'static str,
        translator: &'static str,
    },
}

/// A choice for one tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatalogModel {
    pub id: &'static str,
    pub name: &'static str,
    pub tool: AiTool,
    /// Higher is more accurate, within a tool.
    pub rank: u8,
    /// Published benchmarks put it at the top of its field: what the Accurate
    /// preset may use.
    pub high_accuracy: bool,
    pub engine: Engine,
    pub components: &'static [&'static str],
    pub kernel: Kernel,
    /// Answer time measured on two cores of a Ryzen 9 5900X.
    pub cpu_ms: u32,
    /// Answer time measured on a Radeon RX 9060 XT through Vulkan. 0: no
    /// gain expected from a graphics card.
    pub gpu_ms: u32,
    /// Peak memory of the worker with this model loaded.
    pub ram_mb: u32,
    pub license: &'static str,
}

/// How much a machine must keep for the server and the database.
pub const RESERVED_RAM_MB: u64 = 1536;
/// What the calibration takes on the machine the catalog was measured on.
pub const REF_CALIBRATE_FLOAT_MS: f64 = 4.87;
pub const REF_CALIBRATE_INT8_MS: f64 = 3.30;
pub const REF_CALIBRATE_GPU_MS: f64 = 1.31;

const fn part(id: &'static str, tool: AiTool, files: &'static [ModelFile]) -> Component {
    Component { id, tool, files }
}

pub const COMPONENTS: &[Component] = &[
    part("tresnet", AiTool::Tagging, files::TRESNET),
    part("ml-caformer", AiTool::Tagging, files::ML_CAFORMER),
    part("joytag", AiTool::Tagging, files::JOYTAG),
    part("wd-vit", AiTool::Tagging, files::WD_VIT),
    part("wd-convnext", AiTool::Tagging, files::WD_CONVNEXT),
    part("wd-swinv2", AiTool::Tagging, files::WD_SWINV2),
    part("wd-vit-large", AiTool::Tagging, files::WD_VIT_LARGE),
    part("danbooru-v1", AiTool::Tagging, files::DANBOORU_V1),
    part("wd-eva02", AiTool::Tagging, files::WD_EVA02),
    part("m2m", AiTool::Translation, files::M2M),
    part("madlad", AiTool::Translation, files::MADLAD),
    part("gemma", AiTool::Translation, files::GEMMA),
    part("det-small", AiTool::Comics, files::DET_SMALL),
    part("det-large", AiTool::Comics, files::DET_LARGE),
    part("manga-ocr", AiTool::Comics, files::MANGA_OCR),
    part("opus-ja-en", AiTool::Translation, files::OPUS_JA_EN),
    part("opus-zh-en", AiTool::Translation, files::OPUS_ZH_EN),
    part("opus-ko-en", AiTool::Translation, files::OPUS_KO_EN),
    part("opus-ru-en", AiTool::Translation, files::OPUS_RU_EN),
    part("opus-fr-en", AiTool::Translation, files::OPUS_FR_EN),
    part("opus-de-en", AiTool::Translation, files::OPUS_DE_EN),
    part("opus-es-en", AiTool::Translation, files::OPUS_ES_EN),
    part("opus-it-en", AiTool::Translation, files::OPUS_IT_EN),
    part("opus-en-it", AiTool::Translation, files::OPUS_EN_IT),
    part("opus-en-fr", AiTool::Translation, files::OPUS_EN_FR),
    part("opus-en-de", AiTool::Translation, files::OPUS_EN_DE),
    part("opus-en-es", AiTool::Translation, files::OPUS_EN_ES),
    part("opus-en-ru", AiTool::Translation, files::OPUS_EN_RU),
    part("opus-en-zh", AiTool::Translation, files::OPUS_EN_ZH),
];

/// Installed with the fast translator; other pairs follow when first needed.
const OPUS_BASE: &[&str] = &["opus-ja-en", "opus-zh-en", "opus-ko-en"];

const TRESNET_FILE: &str = "TResnet-D-FLq_ema_6-30000.onnx";

pub const MODELS: &[CatalogModel] = &[
    CatalogModel {
        id: "tresnet-320",
        name: "ML-Danbooru TResNet (320 px)",
        tool: AiTool::Tagging,
        rank: 1,
        high_accuracy: false,
        engine: Engine::MlDanbooru {
            size: 320,
            file: TRESNET_FILE,
        },
        components: &["tresnet"],
        kernel: Kernel::Float,
        cpu_ms: 174,
        gpu_ms: 33,
        ram_mb: 504,
        license: "MIT",
    },
    CatalogModel {
        id: "tresnet-448",
        name: "ML-Danbooru TResNet (448 px)",
        tool: AiTool::Tagging,
        rank: 2,
        high_accuracy: false,
        engine: Engine::MlDanbooru {
            size: 448,
            file: TRESNET_FILE,
        },
        components: &["tresnet"],
        kernel: Kernel::Float,
        cpu_ms: 338,
        gpu_ms: 45,
        ram_mb: 547,
        license: "MIT",
    },
    CatalogModel {
        id: "joytag",
        name: "JoyTag (also photographs)",
        tool: AiTool::Tagging,
        rank: 3,
        high_accuracy: false,
        engine: Engine::JoyTag { threshold: 0.4 },
        components: &["joytag"],
        kernel: Kernel::Float,
        cpu_ms: 885,
        gpu_ms: 65,
        ram_mb: 755,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "ml-caformer",
        name: "ML-Danbooru CAFormer",
        tool: AiTool::Tagging,
        rank: 4,
        high_accuracy: false,
        engine: Engine::MlDanbooru {
            size: 448,
            file: "ml_caformer_m36_dec-5-97527.onnx",
        },
        components: &["ml-caformer"],
        kernel: Kernel::Float,
        cpu_ms: 1107,
        gpu_ms: 177,
        ram_mb: 853,
        license: "MIT",
    },
    CatalogModel {
        id: "wd-vit",
        name: "WD ViT v3",
        tool: AiTool::Tagging,
        rank: 5,
        high_accuracy: false,
        engine: Engine::Wd { threshold: 0.35 },
        components: &["wd-vit"],
        kernel: Kernel::Float,
        cpu_ms: 814,
        gpu_ms: 71,
        ram_mb: 802,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "wd-convnext",
        name: "WD ConvNext v3",
        tool: AiTool::Tagging,
        rank: 6,
        high_accuracy: false,
        engine: Engine::Wd { threshold: 0.35 },
        components: &["wd-convnext"],
        kernel: Kernel::Float,
        cpu_ms: 745,
        gpu_ms: 61,
        ram_mb: 844,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "wd-swinv2",
        name: "WD SwinV2 v3",
        tool: AiTool::Tagging,
        rank: 7,
        high_accuracy: false,
        engine: Engine::Wd { threshold: 0.35 },
        components: &["wd-swinv2"],
        kernel: Kernel::Float,
        cpu_ms: 850,
        gpu_ms: 90,
        ram_mb: 948,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "wd-vit-large",
        name: "WD ViT Large v3",
        tool: AiTool::Tagging,
        rank: 8,
        high_accuracy: true,
        engine: Engine::Wd { threshold: 0.35 },
        components: &["wd-vit-large"],
        kernel: Kernel::Float,
        cpu_ms: 3834,
        gpu_ms: 246,
        ram_mb: 1940,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "danbooru-v1",
        name: "Danbooru Tagger v1 (24,000 tags, no characters)",
        tool: AiTool::Tagging,
        rank: 9,
        high_accuracy: true,
        engine: Engine::DanbooruV1 { threshold: 0.5 },
        components: &["danbooru-v1"],
        kernel: Kernel::Float,
        cpu_ms: 4553,
        gpu_ms: 292,
        ram_mb: 2923,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "wd-eva02",
        name: "WD EVA02 Large v3",
        tool: AiTool::Tagging,
        rank: 10,
        high_accuracy: true,
        engine: Engine::Wd { threshold: 0.5 },
        components: &["wd-eva02"],
        kernel: Kernel::Float,
        cpu_ms: 3726,
        gpu_ms: 388,
        ram_mb: 2107,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "opus",
        name: "Opus-MT",
        tool: AiTool::Translation,
        rank: 1,
        high_accuracy: false,
        engine: Engine::Opus,
        components: OPUS_BASE,
        kernel: Kernel::Int8,
        cpu_ms: 518,
        gpu_ms: 0,
        ram_mb: 800,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "m2m",
        name: "M2M100",
        tool: AiTool::Translation,
        rank: 2,
        high_accuracy: false,
        engine: Engine::M2m,
        components: &["m2m"],
        kernel: Kernel::Int8,
        cpu_ms: 1311,
        gpu_ms: 0,
        ram_mb: 2245,
        license: "MIT",
    },
    CatalogModel {
        id: "madlad",
        name: "MADLAD-400 3B",
        tool: AiTool::Translation,
        rank: 3,
        high_accuracy: false,
        engine: Engine::Madlad,
        components: &["madlad"],
        kernel: Kernel::Int8,
        cpu_ms: 1390,
        gpu_ms: 0,
        ram_mb: 5351,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "gemma",
        name: "TranslateGemma 4B",
        tool: AiTool::Translation,
        rank: 4,
        high_accuracy: true,
        engine: Engine::Gemma,
        components: &["gemma"],
        kernel: Kernel::Int8,
        cpu_ms: 10694,
        gpu_ms: 1479,
        ram_mb: 2657,
        license: "Gemma Terms of Use",
    },
    CatalogModel {
        id: "comics-opus",
        name: "Comic detector S + Manga OCR + Opus-MT",
        tool: AiTool::Comics,
        rank: 1,
        high_accuracy: false,
        engine: Engine::Comics {
            detector: "det-small",
            translator: "opus",
        },
        components: &[
            "det-small",
            "manga-ocr",
            "opus-ja-en",
            "opus-zh-en",
            "opus-ko-en",
        ],
        kernel: Kernel::Int8,
        cpu_ms: 3931,
        gpu_ms: 0,
        ram_mb: 1100,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "comics-opus-large",
        name: "Comic detector L + Manga OCR + Opus-MT",
        tool: AiTool::Comics,
        rank: 2,
        high_accuracy: false,
        engine: Engine::Comics {
            detector: "det-large",
            translator: "opus",
        },
        components: &[
            "det-large",
            "manga-ocr",
            "opus-ja-en",
            "opus-zh-en",
            "opus-ko-en",
        ],
        kernel: Kernel::Int8,
        cpu_ms: 4458,
        gpu_ms: 0,
        ram_mb: 1200,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "comics-m2m-small",
        name: "Comic detector S + Manga OCR + M2M100",
        tool: AiTool::Comics,
        rank: 3,
        high_accuracy: false,
        engine: Engine::Comics {
            detector: "det-small",
            translator: "m2m",
        },
        components: &["det-small", "manga-ocr", "m2m"],
        kernel: Kernel::Int8,
        cpu_ms: 4515,
        gpu_ms: 0,
        ram_mb: 2571,
        license: "Apache-2.0, MIT",
    },
    CatalogModel {
        id: "comics-m2m",
        name: "Comic detector L + Manga OCR + M2M100",
        tool: AiTool::Comics,
        rank: 4,
        high_accuracy: false,
        engine: Engine::Comics {
            detector: "det-large",
            translator: "m2m",
        },
        components: &["det-large", "manga-ocr", "m2m"],
        kernel: Kernel::Int8,
        cpu_ms: 4946,
        gpu_ms: 0,
        ram_mb: 2663,
        license: "Apache-2.0, MIT",
    },
    CatalogModel {
        id: "comics-madlad",
        name: "Comic detector L + Manga OCR + MADLAD-400",
        tool: AiTool::Comics,
        rank: 5,
        high_accuracy: false,
        engine: Engine::Comics {
            detector: "det-large",
            translator: "madlad",
        },
        components: &["det-large", "manga-ocr", "madlad"],
        kernel: Kernel::Int8,
        cpu_ms: 5153,
        gpu_ms: 0,
        ram_mb: 5669,
        license: "Apache-2.0",
    },
    CatalogModel {
        id: "comics-gemma",
        name: "Comic detector L + Manga OCR + TranslateGemma",
        tool: AiTool::Comics,
        rank: 6,
        high_accuracy: true,
        engine: Engine::Comics {
            detector: "det-large",
            translator: "gemma",
        },
        components: &["det-large", "manga-ocr", "gemma"],
        kernel: Kernel::Int8,
        cpu_ms: 37757,
        gpu_ms: 6049,
        ram_mb: 3109,
        license: "Apache-2.0, Gemma Terms of Use",
    },
];

pub fn model(id: &str) -> Option<&'static CatalogModel> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn component(id: &str) -> Option<&'static Component> {
    COMPONENTS.iter().find(|c| c.id == id)
}

impl CatalogModel {
    pub fn parts(&self) -> impl Iterator<Item = &'static Component> + '_ {
        self.components.iter().filter_map(|id| component(id))
    }

    pub fn size(&self) -> u64 {
        self.parts().map(|c| c.size()).sum()
    }
}

/// The primary language of a BCP 47 code: `pt-BR` is `pt`.
pub fn primary_language(code: &str) -> String {
    code.split(['-', '_'])
        .next()
        .unwrap_or(code)
        .to_ascii_lowercase()
}

/// The language pairs from `from` to `to`, through English when there's no
/// direct one. Empty when they're the same language; `None` when Opus-MT
/// doesn't have them.
pub fn opus_route(from: &str, to: &str) -> Option<Vec<&'static Component>> {
    let (from, to) = (primary_language(from), primary_language(to));
    if from == to {
        return Some(Vec::new());
    }
    let pair = |a: &str, b: &str| component(&format!("opus-{a}-{b}"));
    if let Some(direct) = pair(&from, &to) {
        return Some(vec![direct]);
    }
    Some(vec![pair(&from, "en")?, pair("en", &to)?])
}

/// English names of the languages M2M100 and TranslateGemma are asked for.
const LANGUAGES: &[(&str, &str)] = &[
    ("af", "Afrikaans"),
    ("ar", "Arabic"),
    ("bg", "Bulgarian"),
    ("bn", "Bengali"),
    ("ca", "Catalan"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("de", "German"),
    ("el", "Greek"),
    ("en", "English"),
    ("es", "Spanish"),
    ("et", "Estonian"),
    ("fa", "Persian"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("he", "Hebrew"),
    ("hi", "Hindi"),
    ("hr", "Croatian"),
    ("hu", "Hungarian"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("lt", "Lithuanian"),
    ("lv", "Latvian"),
    ("nl", "Dutch"),
    ("no", "Norwegian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("ru", "Russian"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("sr", "Serbian"),
    ("sv", "Swedish"),
    ("th", "Thai"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("vi", "Vietnamese"),
    ("zh", "Chinese"),
];

/// The English name of a language the big translators are asked for.
pub fn language_name(code: &str) -> Option<&'static str> {
    let code = primary_language(code);
    LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
}

/// How a machine measured up.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    /// What the hardware was: a change means measuring again.
    pub fingerprint: String,
    pub float_ms: f64,
    pub int8_ms: f64,
    /// `None`: no usable graphics card.
    pub gpu_ms: Option<f64>,
    /// Real timings of installed models, by model id.
    #[serde(default)]
    pub measured: BTreeMap<String, Measured>,
}

/// A model's real answer time here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measured {
    pub ms: u32,
    /// It runs on the graphics card: faster than the processor, with the
    /// same results.
    pub gpu: bool,
}

/// What selection knows about the machine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Machine {
    pub calibration: Calibration,
    /// Memory StarBooru may use, in MiB (0: unknown).
    pub ram_mb: u64,
    /// Models that failed when they were timed here, by id.
    pub failed: BTreeSet<String>,
}

/// How much slower than the reference machine. Not measured yet (0): the same.
fn scale(ms: f64, reference: f64) -> f64 {
    if ms == 0.0 { 1.0 } else { ms / reference }
}

impl Machine {
    /// A model's answer time here: measured when it ran, else estimated from
    /// the catalog and the calibration.
    pub fn estimate_ms(&self, m: &CatalogModel) -> u32 {
        if let Some(measured) = self.calibration.measured.get(m.id) {
            return measured.ms;
        }
        let scale = match m.kernel {
            Kernel::Float => scale(self.calibration.float_ms, REF_CALIBRATE_FLOAT_MS),
            Kernel::Int8 => scale(self.calibration.int8_ms, REF_CALIBRATE_INT8_MS),
        };
        let cpu = f64::from(m.cpu_ms) * scale;
        let gpu = self
            .calibration
            .gpu_ms
            .filter(|_| m.gpu_ms > 0)
            .map(|ms| f64::from(m.gpu_ms) * ms / REF_CALIBRATE_GPU_MS);
        let best = gpu.map_or(cpu, |g| g.min(cpu));
        // Saturating: a corrupt calibration can only make a model look slow.
        if best.is_finite() && best >= 0.0 {
            best.min(f64::from(u32::MAX)) as u32
        } else {
            u32::MAX
        }
    }

    pub fn fits(&self, m: &CatalogModel) -> bool {
        self.ram_mb == 0 || u64::from(m.ram_mb).saturating_add(RESERVED_RAM_MB) <= self.ram_mb
    }

    /// What a preset chooses from: it fits, and it worked if it was tried.
    fn usable(&self, tool: AiTool) -> impl Iterator<Item = &'static CatalogModel> + '_ {
        MODELS
            .iter()
            .filter(move |m| m.tool == tool && self.fits(m) && !self.failed.contains(m.id))
    }

    /// The most accurate usable model within `budget_ms`.
    fn best_within(&self, tool: AiTool, budget_ms: u32) -> Option<&'static CatalogModel> {
        self.usable(tool)
            .filter(|m| self.estimate_ms(m) <= budget_ms)
            .max_by_key(|m| m.rank)
    }

    /// The fastest usable model of `tool`.
    pub fn lightest(&self, tool: AiTool) -> Option<&'static CatalogModel> {
        self.usable(tool).min_by_key(|m| self.estimate_ms(m))
    }

    /// The model a preset chooses here. `None`: the preset is greyed out.
    pub fn pick(&self, tool: AiTool, preset: AiPreset) -> Option<&'static CatalogModel> {
        let within = self.best_within(tool, preset.budget_ms());
        match preset {
            AiPreset::Fast => within,
            AiPreset::Balanced => within.or_else(|| self.lightest(tool)),
            AiPreset::Accurate => self
                .usable(tool)
                .filter(|m| m.high_accuracy && self.estimate_ms(m) <= preset.budget_ms())
                .max_by_key(|m| m.rank),
        }
    }

    /// The model for a preset, which Balanced stands in for when this machine
    /// can't meet it any more.
    pub fn resolve(&self, tool: AiTool, preset: AiPreset) -> Option<&'static CatalogModel> {
        self.pick(tool, preset)
            .or_else(|| self.pick(tool, AiPreset::Balanced))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(scale: f64, ram_mb: u64) -> Machine {
        Machine {
            calibration: Calibration {
                float_ms: REF_CALIBRATE_FLOAT_MS * scale,
                int8_ms: REF_CALIBRATE_INT8_MS * scale,
                ..Calibration::default()
            },
            ram_mb,
            failed: BTreeSet::new(),
        }
    }

    #[test]
    fn every_model_has_all_its_parts() {
        for m in MODELS {
            assert_eq!(m.parts().count(), m.components.len(), "{}", m.id);
            assert!(m.size() > 0, "{}", m.id);
            if let Engine::Comics {
                detector,
                translator,
            } = m.engine
            {
                assert!(m.components.contains(&detector), "{}", m.id);
                let translator = model(translator).unwrap();
                assert_eq!(translator.tool, AiTool::Translation);
                for part in translator.components {
                    assert!(m.components.contains(part), "{} needs {part}", m.id);
                }
            }
        }
    }

    #[test]
    fn every_file_is_pinned() {
        let is_hex = |s: &str, len: usize| {
            s.len() == len
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        for c in COMPONENTS {
            assert!(!c.files.is_empty(), "{}", c.id);
            for f in c.files {
                assert!(is_hex(f.rev, 40), "{}: {}", c.id, f.rev);
                assert!(is_hex(f.sha256, 64), "{}: {}", c.id, f.sha256);
                assert!(f.size > 0);
                assert!(f.url().starts_with("https://huggingface.co/"));
                assert!(!f.name().contains('/'));
            }
        }
    }

    #[test]
    fn presets_get_the_best_model_within_budget() {
        let here = machine(1.0, 8192);
        let id = |t, p| here.pick(t, p).map(|m| m.id);
        assert_eq!(id(AiTool::Tagging, AiPreset::Fast), Some("wd-swinv2"));
        assert_eq!(id(AiTool::Tagging, AiPreset::Balanced), Some("wd-eva02"));
        assert_eq!(id(AiTool::Tagging, AiPreset::Accurate), Some("wd-eva02"));
        assert_eq!(id(AiTool::Translation, AiPreset::Fast), Some("opus"));
        assert_eq!(id(AiTool::Translation, AiPreset::Balanced), Some("madlad"));
        assert_eq!(id(AiTool::Translation, AiPreset::Accurate), Some("gemma"));
        // A whole page can't be instant on two cores.
        assert_eq!(id(AiTool::Comics, AiPreset::Fast), None);
    }

    #[test]
    fn the_catalog_is_in_order_of_accuracy() {
        for tool in AiTool::ALL {
            let ranks: Vec<u8> = MODELS
                .iter()
                .filter(|m| m.tool == tool)
                .map(|m| m.rank)
                .collect();
            assert!(ranks.windows(2).all(|w| w[0] < w[1]), "{tool:?}: {ranks:?}");
            // Every tool has something for the Accurate preset.
            assert!(MODELS.iter().any(|m| m.tool == tool && m.high_accuracy));
        }
    }

    #[test]
    fn fast_is_for_whatever_is_instant_here() {
        let quick = machine(0.1, 16384);
        assert_eq!(
            quick.pick(AiTool::Tagging, AiPreset::Fast).map(|m| m.id),
            Some("wd-eva02"),
            "a fast machine gets the most demanding model"
        );
        let hopeless = machine(100.0, 8192);
        assert_eq!(hopeless.pick(AiTool::Tagging, AiPreset::Fast), None);
    }

    #[test]
    fn balanced_is_there_even_when_nothing_meets_its_target() {
        let hopeless = machine(1000.0, 8192);
        for tool in AiTool::ALL {
            let quickest = hopeless.lightest(tool).unwrap();
            assert!(hopeless.estimate_ms(quickest) > AiPreset::Balanced.budget_ms());
            assert_eq!(hopeless.pick(tool, AiPreset::Balanced), Some(quickest));
        }
    }

    #[test]
    fn accurate_needs_a_high_accuracy_model_that_runs_well_here() {
        // Plenty of models within 30 seconds, but none of the good ones.
        let slow = machine(30.0, 8192);
        assert!(slow.best_within(AiTool::Tagging, 30_000).is_some());
        assert_eq!(slow.pick(AiTool::Tagging, AiPreset::Accurate), None);
        // It can't be reached by a model that doesn't fit, either.
        let small = machine(1.0, 2100);
        assert_eq!(small.pick(AiTool::Tagging, AiPreset::Accurate), None);
        assert!(small.pick(AiTool::Tagging, AiPreset::Balanced).is_some());
    }

    #[test]
    fn a_model_that_failed_here_isnt_chosen() {
        let mut here = machine(1.0, 8192);
        here.failed.insert("wd-eva02".into());
        here.failed.insert("danbooru-v1".into());
        assert_eq!(
            here.pick(AiTool::Tagging, AiPreset::Accurate).map(|m| m.id),
            Some("wd-vit-large")
        );
    }

    #[test]
    fn memory_limits_the_choice() {
        let small = machine(1.0, 2100);
        assert_eq!(
            small
                .pick(AiTool::Tagging, AiPreset::Balanced)
                .map(|m| m.id),
            Some("tresnet-448")
        );
        assert_eq!(small.pick(AiTool::Translation, AiPreset::Balanced), None);
        // An unknown amount doesn't block anything.
        assert!(
            machine(1.0, 0)
                .pick(AiTool::Translation, AiPreset::Accurate)
                .is_some()
        );
    }

    #[test]
    fn real_timings_replace_estimates() {
        let mut here = machine(1.0, 8192);
        for id in ["wd-eva02", "danbooru-v1", "wd-vit-large"] {
            here.calibration.measured.insert(
                id.into(),
                Measured {
                    ms: 60_000,
                    gpu: false,
                },
            );
        }
        assert_eq!(here.pick(AiTool::Tagging, AiPreset::Accurate), None);
        assert_eq!(
            here.pick(AiTool::Tagging, AiPreset::Balanced).map(|m| m.id),
            Some("wd-swinv2")
        );
    }

    #[test]
    fn a_graphics_card_counts_where_it_helps() {
        let mut here = machine(1.0, 8192);
        here.calibration.gpu_ms = Some(REF_CALIBRATE_GPU_MS);
        let eva = model("wd-eva02").unwrap();
        assert_eq!(here.estimate_ms(eva), eva.gpu_ms);
        let m2m = model("m2m").unwrap();
        assert_eq!(here.estimate_ms(m2m), m2m.cpu_ms);
        // What takes a minute on the processor comes within the budgets.
        let picked = |tool, preset| here.pick(tool, preset).map(|m| m.id);
        assert_eq!(
            picked(AiTool::Translation, AiPreset::Balanced),
            Some("gemma")
        );
        assert_eq!(
            picked(AiTool::Comics, AiPreset::Accurate),
            Some("comics-gemma")
        );
        assert_eq!(picked(AiTool::Tagging, AiPreset::Fast), Some("wd-eva02"));
    }

    #[test]
    fn hostile_calibrations_mean_slow() {
        let mut here = machine(1.0, 8192);
        here.calibration.float_ms = f64::NAN;
        let eva = model("wd-eva02").unwrap();
        assert_eq!(here.estimate_ms(eva), u32::MAX);
        here.calibration.float_ms = f64::INFINITY;
        assert_eq!(here.estimate_ms(eva), u32::MAX);
        here.calibration.float_ms = -5.0;
        assert_eq!(here.estimate_ms(eva), u32::MAX);
    }

    #[test]
    fn a_preset_out_of_reach_falls_back_on_balanced() {
        let slow = machine(5.0, 8192);
        // Nothing translates instantly here.
        let m = slow.resolve(AiTool::Translation, AiPreset::Fast).unwrap();
        assert_eq!(Some(m), slow.pick(AiTool::Translation, AiPreset::Balanced));
        assert_eq!(
            machine(1.0, 1024).resolve(AiTool::Tagging, AiPreset::Fast),
            None
        );
    }

    #[test]
    fn opus_goes_through_english() {
        let ids = |a, b| opus_route(a, b).map(|r| r.iter().map(|c| c.id).collect::<Vec<_>>());
        assert_eq!(ids("ja", "en"), Some(vec!["opus-ja-en"]));
        assert_eq!(ids("en", "it"), Some(vec!["opus-en-it"]));
        assert_eq!(ids("ja", "it"), Some(vec!["opus-ja-en", "opus-en-it"]));
        assert_eq!(ids("pt-BR", "pt"), Some(vec![]));
        assert_eq!(ids("zh-Hant", "de"), Some(vec!["opus-zh-en", "opus-en-de"]));
        // No pair for these.
        assert_eq!(ids("en", "ja"), None);
        assert_eq!(ids("xx", "en"), None);
    }

    #[test]
    fn names_languages() {
        assert_eq!(language_name("ja"), Some("Japanese"));
        assert_eq!(language_name("pt-BR"), Some("Portuguese"));
        assert_eq!(language_name("EN"), Some("English"));
        assert_eq!(language_name("xx"), None);
    }

    #[test]
    fn modes_and_presets_read_their_names() {
        assert_eq!(AiMode::parse("hosted"), Some(AiMode::Api));
        assert_eq!(AiMode::parse("api"), Some(AiMode::Api));
        assert_eq!(AiMode::parse("nope"), None);
        for p in AiPreset::ALL {
            assert_eq!(AiPreset::parse(p.as_str()), Some(p));
        }
        for t in AiTool::ALL {
            assert_eq!(AiTool::parse(t.as_str()), Some(t));
        }
    }
}
