//! The machine the server runs on, as the admin's AI panel shows it.

use serde::{Deserialize, Serialize};

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
