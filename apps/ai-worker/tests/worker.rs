//! Talks to the real worker binary over stdin and stdout.
//!
//! The tests that run models need `STARBOORU_ORT_LIB` and, for real models,
//! `STARBOORU_AI_MODELS`; without them they say so and pass.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use base64::Engine as _;
use starbooru_core::ai_catalog::{AiTool, MODELS};
use starbooru_core::ai_worker::{DAMAGED_MODEL, WorkerReply, WorkerRequest};

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Worker {
    fn start(ort: &str, models: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_starbooru-ai-worker"))
            .env("STARBOORU_ORT_LIB", ort)
            .env("STARBOORU_AI_MODELS", models)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn line(&mut self, text: &str) -> WorkerReply {
        writeln!(self.stdin, "{text}").unwrap();
        self.stdin.flush().unwrap();
        let mut reply = String::new();
        self.stdout.read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap_or_else(|_| panic!("not a reply: {reply:?}"))
    }

    fn ask(&mut self, request: &WorkerRequest) -> WorkerReply {
        self.line(&serde_json::to_string(request).unwrap())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A photo-sized JPEG of nothing in particular.
fn jpeg() -> String {
    let image = image::RgbImage::from_fn(640, 480, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut out)
        .encode_image(&image)
        .unwrap();
    base64::engine::general_purpose::STANDARD.encode(out)
}

fn error_of(reply: &WorkerReply) -> &str {
    match reply {
        WorkerReply::Error { message } => message,
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn without_the_runtime_it_answers_with_the_reason_and_stays_up() {
    let mut worker = Worker::start("/nonexistent/libonnxruntime.so", "/nonexistent");
    assert!(!error_of(&worker.ask(&WorkerRequest::Hello)).is_empty());
    assert!(!error_of(&worker.ask(&WorkerRequest::Calibrate)).is_empty());
    assert!(worker.child.try_wait().unwrap().is_none());
}

#[test]
fn nonsense_gets_an_answer_too() {
    let mut worker = Worker::start("/nonexistent/libonnxruntime.so", "/nonexistent");
    assert_eq!(error_of(&worker.line("not json")), "unknown request");
    assert_eq!(
        error_of(&worker.line(r#"{"op":"nope"}"#)),
        "unknown request"
    );
    assert!(worker.child.try_wait().unwrap().is_none());
}

fn runtime() -> Option<String> {
    let lib = std::env::var("STARBOORU_ORT_LIB").ok();
    if lib.is_none() {
        eprintln!("skipped: STARBOORU_ORT_LIB is not set");
    }
    lib
}

#[test]
fn broken_models_and_pictures_are_errors() {
    let Some(ort) = runtime() else { return };
    let dir = std::env::temp_dir().join(format!("sb-worker-test-{}", std::process::id()));
    let component = dir.join("tresnet");
    std::fs::create_dir_all(&component).unwrap();
    for file in starbooru_core::ai_catalog::component("tresnet")
        .unwrap()
        .files
    {
        let name = Path::new(file.path).file_name().unwrap();
        std::fs::write(component.join(name), b"this is not a model").unwrap();
    }
    let mut worker = Worker::start(&ort, dir.to_str().unwrap());
    assert!(matches!(
        worker.ask(&WorkerRequest::Hello),
        WorkerReply::Hello { .. }
    ));
    let tag = |jpeg: String| WorkerRequest::Tag {
        model: "tresnet-320".into(),
        gpu: false,
        jpeg,
    };
    // A corrupt file on disk, an unknown model, a broken picture.
    assert_eq!(error_of(&worker.ask(&tag(jpeg()))), DAMAGED_MODEL);
    let unknown = WorkerRequest::Tag {
        model: "nope".into(),
        gpu: false,
        jpeg: jpeg(),
    };
    assert!(!error_of(&worker.ask(&unknown)).is_empty());
    assert!(!error_of(&worker.ask(&tag("AAAA".into()))).is_empty());
    assert!(!error_of(&worker.ask(&tag("%%%".into()))).is_empty());
    assert!(worker.child.try_wait().unwrap().is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn calibrates_without_any_download() {
    let Some(ort) = runtime() else { return };
    let mut worker = Worker::start(&ort, "/nonexistent");
    match worker.ask(&WorkerRequest::Calibrate) {
        WorkerReply::Calibrated {
            float_ms, int8_ms, ..
        } => assert!(float_ms > 0.0 && int8_ms > 0.0),
        other => panic!("{other:?}"),
    }
}

fn installed(models: &str, components: &[&str]) -> bool {
    components.iter().all(|c| {
        std::fs::read_dir(Path::new(models).join(c)).is_ok_and(|mut files| files.next().is_some())
    })
}

#[test]
fn real_models_answer() {
    let Some(ort) = runtime() else { return };
    let Ok(models) = std::env::var("STARBOORU_AI_MODELS") else {
        eprintln!("skipped: STARBOORU_AI_MODELS is not set");
        return;
    };
    let mut worker = Worker::start(&ort, &models);
    for model in MODELS.iter().filter(|m| m.tool == AiTool::Tagging) {
        // Each is there only if a test machine has it.
        if !installed(&models, model.components) {
            eprintln!("skipped: {} is not installed", model.id);
            continue;
        }
        let tagged = worker.ask(&WorkerRequest::Tag {
            model: model.id.into(),
            gpu: false,
            jpeg: jpeg(),
        });
        assert!(
            matches!(tagged, WorkerReply::Tags { .. }),
            "{}: {tagged:?}",
            model.id
        );
    }
    let translated = worker.ask(&WorkerRequest::Translate {
        model: "opus".into(),
        gpu: false,
        from: "ja".into(),
        to: "en".into(),
        texts: vec!["今日はいい天気ですね。".into()],
    });
    match translated {
        WorkerReply::Translated { texts } => {
            assert!(texts.first().is_some_and(|t| !t.trim().is_empty()));
        }
        other => panic!("{other:?}"),
    }
    // Twice, as the second time starts from what it remembers of the first.
    if installed(&models, &["gemma"]) {
        let ask = |worker: &mut Worker| {
            worker.ask(&WorkerRequest::Translate {
                model: "gemma".into(),
                gpu: false,
                from: "ja".into(),
                to: "it".into(),
                texts: vec!["今日はいい天気ですね。".into()],
            })
        };
        let first = ask(&mut worker);
        assert!(matches!(first, WorkerReply::Translated { .. }), "{first:?}");
        assert_eq!(format!("{first:?}"), format!("{:?}", ask(&mut worker)));
    }
    if installed(&models, &["madlad"]) {
        let translated = worker.ask(&WorkerRequest::Translate {
            model: "madlad".into(),
            gpu: false,
            from: "en".into(),
            to: "it".into(),
            texts: vec!["The weather is lovely today.".into()],
        });
        match translated {
            WorkerReply::Translated { texts } => {
                assert!(texts.first().is_some_and(|t| !t.trim().is_empty()));
            }
            other => panic!("{other:?}"),
        }
        // A language it doesn't know is an answer too.
        let unknown = worker.ask(&WorkerRequest::Translate {
            model: "madlad".into(),
            gpu: false,
            from: "en".into(),
            to: "xx".into(),
            texts: vec!["Hello there.".into()],
        });
        assert!(error_of(&unknown).contains("no model"), "{unknown:?}");
    }
}
