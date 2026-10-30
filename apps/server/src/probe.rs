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

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::warn;

const TIMEOUT: Duration = Duration::from_secs(30);
/// Trimming copies the whole clip, which can take longer than a probe.
const TRIM_TIMEOUT: Duration = Duration::from_secs(180);
/// Re-encoding a long video is slow.
const ENCODE_TIMEOUT: Duration = Duration::from_secs(900);
/// How much longer than asked a stream-copied trim may run before it is
/// re-encoded instead.
const TRIM_SLACK_MS: i64 = 500;
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
    /// Whether the file has an audio stream; `None` when it wasn't probed.
    pub has_audio: Option<bool>,
    /// The container's creation time, when it has a believable one.
    pub file_date: Option<DateTime<Utc>>,
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
    codec_type: Option<String>,
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
    #[serde(default)]
    tags: Option<FormatTags>,
}

#[derive(Deserialize)]
struct FormatTags {
    creation_time: Option<String>,
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

    /// Dimensions, duration and audio, without a frame. Call from a
    /// blocking thread.
    pub fn metadata(&self, path: &Path) -> Option<Probed> {
        let mut cmd = Command::new(&self.ffprobe);
        cmd.args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file",
            "-show_entries",
            "stream=codec_type,width,height:stream_side_data=rotation:\
             stream_tags=rotate:format=duration:format_tags=creation_time",
            "-of",
            "json",
        ])
        .arg(file_arg(path));
        let json = run(&mut cmd, MAX_PROBE_OUTPUT, TIMEOUT)?;
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
        let png = run(&mut cmd, MAX_FRAME_OUTPUT, TIMEOUT)?;
        (!png.is_empty()).then_some(png)
    }
}

impl Tools {
    /// Writes the part of a video between `start_ms` and `end_ms` to
    /// `output`. A stream copy is tried first: it is fast and lossless, but
    /// starts at the keyframe before `start_ms`. When that lands too early
    /// the cut is re-encoded instead. Returns whether a new, non-empty
    /// `output` was written. Call from a blocking thread.
    pub fn trim(&self, input: &Path, output: &Path, ext: &str, start_ms: i64, end_ms: i64) -> bool {
        // ffmpeg 9 exits 0 when `-n` refuses to overwrite, so check first.
        let Some(format) = container(ext).filter(|_| !output.exists()) else {
            return false;
        };
        let length = end_ms.saturating_sub(start_ms);
        if self.cut(input, output, format, start_ms, length, false) {
            let got = self.metadata(output).and_then(|p| p.duration_ms);
            if start_ms == 0 || got.is_some_and(|ms| ms <= length.saturating_add(TRIM_SLACK_MS)) {
                return true;
            }
        }
        let _ = std::fs::remove_file(output);
        self.cut(input, output, format, start_ms, length, true)
    }

    fn cut(
        &self,
        input: &Path,
        output: &Path,
        format: &str,
        start_ms: i64,
        length_ms: i64,
        encode: bool,
    ) -> bool {
        let at = |ms: i64| format!("{}.{:03}", ms / 1000, ms % 1000);
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-protocol_whitelist",
            "file",
            "-ss",
        ])
        .arg(at(start_ms))
        .arg("-i")
        .arg(file_arg(input))
        .arg("-t")
        .arg(at(length_ms))
        .args(["-map", "0:v:0", "-map", "0:a?", "-sn", "-dn"]);
        match (encode, format) {
            (false, _) => {
                cmd.args(["-c", "copy", "-avoid_negative_ts", "make_zero"]);
            }
            (true, "webm") => {
                cmd.args([
                    "-c:v",
                    "libvpx-vp9",
                    "-crf",
                    "30",
                    "-b:v",
                    "0",
                    "-deadline",
                    "good",
                    "-cpu-used",
                    "4",
                    "-row-mt",
                    "1",
                    "-c:a",
                    "libopus",
                    "-b:a",
                    "128k",
                ]);
            }
            (true, _) => {
                cmd.args([
                    "-c:v", "libx264", "-crf", "18", "-preset", "veryfast", "-c:a", "aac", "-b:a",
                    "192k",
                ]);
            }
        }
        if matches!(format, "mp4" | "mov") {
            cmd.args(["-movflags", "+faststart"]);
        }
        cmd.args(["-f", format, "-n"]).arg(file_arg(output));
        let timeout = if encode { ENCODE_TIMEOUT } else { TRIM_TIMEOUT };
        run(&mut cmd, 0, timeout).is_some() && std::fs::metadata(output).is_ok_and(|m| m.len() > 0)
    }
}

/// ffmpeg's muxer for a stored video's extension.
fn container(ext: &str) -> Option<&'static str> {
    match ext {
        "mp4" => Some("mp4"),
        "webm" => Some("webm"),
        "mkv" => Some("matroska"),
        "mov" => Some("mov"),
        _ => None,
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
    // The first video stream; a stream that doesn't say counts as one.
    let stream = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"))
        .or_else(|| parsed.streams.iter().find(|s| s.codec_type.is_none()));
    let has_audio = parsed
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("audio"));
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
    let file_date = parsed
        .format
        .as_ref()
        .and_then(|f| f.tags.as_ref()?.creation_time.as_deref())
        .and_then(starbooru_core::filter::video_date);
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
        has_audio: Some(has_audio),
        file_date,
        frame: None,
    })
}

/// Runs a tool and returns its stdout if it exits successfully within
/// `timeout`. Output beyond `max_out` bytes stalls the child until the
/// timeout kills it.
fn run(cmd: &mut Command, max_out: u64, timeout: Duration) -> Option<Vec<u8>> {
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
    let deadline = Instant::now() + timeout;
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
        assert_eq!(
            p,
            Probed {
                has_audio: Some(false),
                ..Probed::default()
            }
        );
        assert!(parse_probe(b"not json").is_none());
        let date = |tags: &str| {
            let json = format!(r#"{{"format":{{"tags":{tags}}}}}"#);
            parse_probe(json.as_bytes()).unwrap().file_date
        };
        assert_eq!(
            date(r#"{"creation_time":"2021-05-06T07:08:09.000000Z"}"#),
            "2021-05-06T07:08:09Z".parse().ok()
        );
        // An unset clock, nonsense, and none at all.
        assert_eq!(date(r#"{"creation_time":"1904-01-01T00:00:00Z"}"#), None);
        assert_eq!(date(r#"{"creation_time":"soon"}"#), None);
        assert_eq!(date("{}"), None);
        assert_eq!(parse_probe(b"{}").unwrap().has_audio, Some(false));
        // Audio first, then video: the video stream gives the size.
        let p = parse_probe(
            br#"{"streams":[{"codec_type":"audio"},
                            {"codec_type":"video","width":320,"height":240}]}"#,
        )
        .unwrap();
        assert_eq!(
            (p.width, p.height, p.has_audio),
            (Some(320), Some(240), Some(true))
        );
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
            &[
                "-pix_fmt",
                "yuv420p",
                "-metadata",
                "creation_time=2021-05-06T07:08:09Z",
            ],
        ) else {
            return;
        };
        let p = tools.probe(&dir.path().join("clip.mp4"), true);
        assert_eq!((p.width, p.height), (Some(160), Some(90)));
        assert_eq!(p.file_date, "2021-05-06T07:08:09Z".parse().ok());
        assert_eq!(p.duration_ms, Some(2_000));
        assert_eq!(p.has_audio, Some(false));
        let frame = image::load_from_memory(&p.frame.unwrap()).unwrap();
        assert_eq!((frame.width(), frame.height()), (160, 90));
        // Garbage is not an error, just nothing.
        std::fs::write(dir.path().join("junk.mp4"), b"not a video").unwrap();
        let junk = tools.probe(&dir.path().join("junk.mp4"), true);
        assert!(junk.frame.is_none());
    }

    #[test]
    fn trims_a_video() {
        let Some(tools) = tools() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        // A keyframe every second, so the cut starts at 1 s; a stream copy
        // can run a few frames long.
        let Some(_) = generate(
            dir.path(),
            "clip.mp4",
            "testsrc=duration=4:size=160x90:rate=10",
            &["-pix_fmt", "yuv420p", "-g", "10"],
        ) else {
            return;
        };
        let (input, output) = (dir.path().join("clip.mp4"), dir.path().join("cut.mp4"));
        assert!(tools.trim(&input, &output, "mp4", 1_000, 3_000));
        let p = tools.probe(&output, true);
        assert_eq!((p.width, p.height), (Some(160), Some(90)));
        let ms = p.duration_ms.unwrap();
        assert!((1_900..=2_500).contains(&ms), "{ms}");
        // With one keyframe at the start, a copy would keep the whole
        // clip, so the cut is re-encoded.
        let Some(_) = generate(
            dir.path(),
            "sparse.webm",
            "testsrc=duration=4:size=160x90:rate=10",
            &["-c:v", "libvpx-vp9", "-g", "1000", "-deadline", "realtime"],
        ) else {
            return;
        };
        let sparse = dir.path().join("sparse.webm");
        let cut = dir.path().join("cut.webm");
        assert!(tools.trim(&sparse, &cut, "webm", 1_500, 3_000));
        let ms = tools.probe(&cut, true).duration_ms.unwrap();
        assert!((1_400..=1_600).contains(&ms), "{ms}");
        // Never overwrites, and refuses unknown containers.
        assert!(!tools.trim(&input, &output, "mp4", 0, 1_000));
        assert!(!tools.trim(&input, &dir.path().join("x.avi"), "avi", 0, 1_000));
    }
}
