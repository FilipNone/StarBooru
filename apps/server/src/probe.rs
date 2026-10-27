//! Metadata and a still frame for files the `image` crate can't decode
//! (video, AVIF, JPEG XL), through `ffprobe` and `ffmpeg`.
//!
//! Both tools are optional: without them, or when they fail on a file, the
//! post simply has no thumbnail. They run as child processes with a time
//! limit, read only the local file (`-protocol_whitelist file`), and their
//! output is capped.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tracing::warn;

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PROBE_OUTPUT: u64 = 1 << 20;
/// A 1024 px PNG frame is far below this.
const MAX_FRAME_OUTPUT: u64 = 32 << 20;
/// Longest side of the frame handed to the thumbnailer.
const FRAME_SIZE: u32 = 1024;

static MISSING_WARNED: AtomicBool = AtomicBool::new(false);

/// Where to find the tools. `None` turns them off.
#[derive(Debug, Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Probed {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i64>,
    /// A PNG still, at most `FRAME_SIZE` px on its longest side.
    pub frame: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeStream {
    width: Option<i64>,
    height: Option<i64>,
    #[serde(default)]
    side_data_list: Vec<SideData>,
    #[serde(default)]
    tags: Option<StreamTags>,
}

#[derive(Deserialize)]
struct SideData {
    rotation: Option<f64>,
}

#[derive(Deserialize)]
struct StreamTags {
    rotate: Option<String>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

impl Tools {
    /// Probes `path` and grabs a frame (10% in, at most 5 s, for videos).
    /// Call from a blocking thread.
    pub fn probe(&self, path: &Path, is_video: bool) -> Probed {
        let mut out = self.metadata(path).unwrap_or_default();
        let at = match (is_video, out.duration_ms) {
            (true, Some(ms)) => (ms / 10).min(5_000),
            _ => 0,
        };
        out.frame = self.frame(path, at);
        out
    }

    fn metadata(&self, path: &Path) -> Option<Probed> {
        let mut cmd = Command::new(&self.ffprobe);
        cmd.args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height:stream_side_data=rotation:stream_tags=rotate:format=duration",
            "-of",
            "json",
        ])
        .arg(file_arg(path));
        let json = run(&mut cmd, MAX_PROBE_OUTPUT)?;
        parse_probe(&json)
    }

    fn frame(&self, path: &Path, at_ms: i64) -> Option<Vec<u8>> {
        let scale = format!(
            "scale=w='min({FRAME_SIZE},iw)':h='min({FRAME_SIZE},ih)':force_original_aspect_ratio=decrease"
        );
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-protocol_whitelist",
            "file",
        ]);
        if at_ms > 0 {
            cmd.arg("-ss")
                .arg(format!("{}.{:03}", at_ms / 1000, at_ms % 1000));
        }
        cmd.arg("-i").arg(file_arg(path)).args([
            "-frames:v",
            "1",
            "-vf",
            &scale,
            "-f",
            "image2pipe",
            "-c:v",
            "png",
            "-",
        ]);
        let png = run(&mut cmd, MAX_FRAME_OUTPUT)?;
        (!png.is_empty()).then_some(png)
    }
}

/// `file:<path>`, so a name can never be read as another protocol or option.
fn file_arg(path: &Path) -> std::ffi::OsString {
    let mut arg = std::ffi::OsString::from("file:");
    arg.push(path.as_os_str());
    arg
}

fn parse_probe(json: &[u8]) -> Option<Probed> {
    let parsed: ProbeOutput = serde_json::from_slice(json).ok()?;
    let stream = parsed.streams.first();
    let dim = |v: Option<i64>| v.and_then(|v| i32::try_from(v).ok()).filter(|v| *v > 0);
    let mut width = dim(stream.and_then(|s| s.width));
    let mut height = dim(stream.and_then(|s| s.height));
    let rotation = stream.and_then(|s| {
        s.side_data_list
            .iter()
            .find_map(|d| d.rotation)
            .or_else(|| s.tags.as_ref()?.rotate.as_deref()?.parse().ok())
    });
    // Phones store portrait video as landscape plus a rotation.
    if let Some(r) = rotation
        && r.is_finite()
        && (r.abs().round() as i64) % 180 == 90
    {
        std::mem::swap(&mut width, &mut height);
    }
    let duration_ms = parsed
        .format
        .and_then(|f| f.duration)
        .and_then(|d| d.trim().parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d >= 0.0 && *d < 1e9)
        .map(|d| (d * 1000.0).round() as i64);
    Some(Probed {
        width,
        height,
        duration_ms,
        frame: None,
    })
}

/// Runs a tool and returns its stdout if it exits successfully within
/// `TIMEOUT`. Output beyond `max_out` bytes stalls the child until the
/// timeout kills it.
fn run(cmd: &mut Command, max_out: u64) -> Option<Vec<u8>> {
    let mut child = match cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound
                && !MISSING_WARNED.swap(true, Ordering::Relaxed)
            {
                warn!(
                    "{} not found: no thumbnails for video, AVIF or JPEG XL",
                    cmd.get_program().to_string_lossy()
                );
            }
            return None;
        }
    };
    let stdout = child.stdout.take()?;
    // Read on another thread, so a full pipe can't block the wait below.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.take(max_out).read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + TIMEOUT;
    let ok = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let out = reader.join().ok()?;
    ok.then_some(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// The tools on `PATH`, or `None` (and the test is skipped) without them.
    pub fn tools() -> Option<Tools> {
        let found = Command::new("ffmpeg")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !found {
            eprintln!("skipped: ffmpeg is not installed");
            return None;
        }
        Some(Tools {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
        })
    }

    /// Encodes a generated test pattern with ffmpeg (`args` set the codec).
    pub fn generate(dir: &Path, name: &str, input: &str, args: &[&str]) -> Option<Vec<u8>> {
        let path = dir.join(name);
        let ok = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                input,
            ])
            .args(args)
            .arg(&path)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            eprintln!("skipped: this ffmpeg can't write {name}");
            return None;
        }
        std::fs::read(path).ok()
    }

    #[test]
    fn parses_probe_output() {
        let p = parse_probe(
            br#"{"streams":[{"width":1920,"height":1080,"side_data_list":[{"rotation":-90}]}],
                 "format":{"duration":"12.345"}}"#,
        )
        .unwrap();
        assert_eq!(
            (p.width, p.height, p.duration_ms),
            (Some(1080), Some(1920), Some(12_345))
        );
        let p = parse_probe(br#"{"streams":[{"width":64,"height":48,"tags":{"rotate":"180"}}]}"#)
            .unwrap();
        assert_eq!(
            (p.width, p.height, p.duration_ms),
            (Some(64), Some(48), None)
        );
        let p = parse_probe(
            br#"{"streams":[{"width":-1,"height":99999999999}],"format":{"duration":"NaN"}}"#,
        )
        .unwrap();
        assert_eq!(p, Probed::default());
        assert!(parse_probe(b"not json").is_none());
        assert_eq!(parse_probe(b"{}").unwrap(), Probed::default());
    }

    #[test]
    fn missing_tools_mean_no_frame() {
        let tools = Tools {
            ffmpeg: "/nonexistent/ffmpeg".into(),
            ffprobe: "/nonexistent/ffprobe".into(),
        };
        assert_eq!(
            tools.probe(Path::new("/nonexistent/x.mp4"), true),
            Probed::default()
        );
    }

    #[test]
    fn probes_a_video() {
        let Some(tools) = tools() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let Some(_) = generate(
            dir.path(),
            "clip.mp4",
            "testsrc=duration=2:size=160x90:rate=10",
            &["-pix_fmt", "yuv420p"],
        ) else {
            return;
        };
        let p = tools.probe(&dir.path().join("clip.mp4"), true);
        assert_eq!((p.width, p.height), (Some(160), Some(90)));
        assert_eq!(p.duration_ms, Some(2_000));
        let frame = image::load_from_memory(&p.frame.unwrap()).unwrap();
        assert_eq!((frame.width(), frame.height()), (160, 90));
        // Garbage is not an error, just nothing.
        std::fs::write(dir.path().join("junk.mp4"), b"not a video").unwrap();
        let junk = tools.probe(&dir.path().join("junk.mp4"), true);
        assert!(junk.frame.is_none());
    }
}
