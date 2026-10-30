//! What this machine has. Read from `/proc` and `/sys` (Linux; elsewhere only
//! the processor count is known), so in Docker it describes the host, within
//! the container's memory limit.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use starbooru_core::{Gpu, GpuKind, Hardware};

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
    (hw.ram_total, hw.ram_available) = memory();
    hw
}

/// Total and available memory in bytes. A container's limit is what counts
/// for it.
fn memory() -> (u64, u64) {
    let (mut total, mut available) = (0, 0);
    if let Some(meminfo) = read_trimmed("/proc/meminfo") {
        total = meminfo_bytes(&meminfo, "MemTotal").unwrap_or(0);
        available = meminfo_bytes(&meminfo, "MemAvailable").unwrap_or(0);
    }
    if let Some(limit) = read_trimmed("/sys/fs/cgroup/memory.max").and_then(|v| v.parse().ok()) {
        total = capped(total, limit);
        available = capped(available, limit);
    }
    (total, available)
}

/// Memory StarBooru may use, in MiB (0: unknown).
pub fn ram_mb() -> u64 {
    memory().0 >> 20
}

/// What the machine is, for telling whether the timings still hold.
pub fn fingerprint(hw: &Hardware, gpu: Option<&str>, cpu_limit: Option<&str>) -> String {
    let cards: Vec<&str> = hw.gpus.iter().map(|g| g.name.as_str()).collect();
    format!(
        "{}|{}|{}|{}|{}",
        hw.cpu.as_deref().unwrap_or(hw.arch.as_str()),
        hw.threads,
        cards.join(","),
        gpu.unwrap_or("cpu"),
        cpu_limit.unwrap_or("max"),
    )
}

/// A container's share of the processor, which the processor count doesn't
/// show below one core.
pub fn cpu_limit() -> Option<String> {
    read_trimmed("/sys/fs/cgroup/cpu.max")
}

fn capped(value: u64, limit: u64) -> u64 {
    if value == 0 { limit } else { value.min(limit) }
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
/// Pi have no PCI GPU and list none: the runtime can't use theirs anyway.
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
    fn reads_nvidia_smi() {
        assert_eq!(
            parse_nvidia("12288, 10000\n8192,8000\nbad\n"),
            vec![(12288 << 20, 10000 << 20), (8192 << 20, 8000 << 20)]
        );
    }

    #[test]
    fn a_container_limit_caps_the_memory() {
        assert_eq!(capped(32, 8), 8);
        assert_eq!(capped(4, 8), 4);
        assert_eq!(capped(0, 8), 8);
    }

    #[test]
    fn a_new_processor_limit_needs_new_timings() {
        let hw = Hardware::default();
        assert_ne!(
            fingerprint(&hw, None, Some("100000 100000")),
            fingerprint(&hw, None, Some("25000 100000")),
        );
        assert_eq!(
            fingerprint(&hw, None, None),
            fingerprint(&hw, None, Some("max")),
        );
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
