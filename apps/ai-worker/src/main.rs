//! Runs StarBooru's AI models for the server, in a process of its own: when
//! the memory runs out or a model's native code fails, this process dies and
//! the server starts another one, with the collection and the API untouched.
//!
//! Requests come in one per line on stdin and replies go out on stdout
//! (`starbooru_core::ai_worker`). Nothing else goes to stdout, and nothing a
//! request carries is ever logged.

mod calibrate;
mod comics;
mod onnx;
mod picture;
mod runtime;
mod tag;
mod translate;

use std::io::{BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};

use starbooru_core::ai_worker::{WorkerReply, WorkerRequest};

use runtime::Runtime;

/// The fewest points of memory pressure before the kernel picks this process:
/// the database and the server must outlive it.
const OOM_SCORE: &str = "800";

fn main() {
    // Best effort: without it the kernel only chooses on its own.
    let _ = std::fs::write("/proc/self/oom_score_adj", OOM_SCORE);
    let mut runtime = Runtime::start();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<WorkerRequest>(&line) {
            Ok(request) => handle(&mut runtime, request),
            Err(_) => failure("unknown request".into()),
        };
        let Ok(text) = serde_json::to_string(&reply) else {
            break;
        };
        if writeln!(stdout, "{text}")
            .and_then(|_| stdout.flush())
            .is_err()
        {
            break;
        }
    }
}

fn failure(message: String) -> WorkerReply {
    WorkerReply::Error { message }
}

/// A panic in a dependency answers like any other failure, and the sessions it
/// may have left half used go.
fn handle(runtime: &mut Result<Runtime, String>, request: WorkerRequest) -> WorkerReply {
    let rt = match runtime {
        Ok(rt) => rt,
        Err(reason) => return failure(reason.clone()),
    };
    match catch_unwind(AssertUnwindSafe(|| run(rt, request))) {
        Ok(Ok(reply)) => reply,
        Ok(Err(message)) => failure(message),
        Err(_) => {
            rt.unload();
            failure("the model failed unexpectedly".into())
        }
    }
}

fn run(rt: &mut Runtime, request: WorkerRequest) -> Result<WorkerReply, String> {
    match request {
        WorkerRequest::Hello => Ok(WorkerReply::Hello {
            gpu: calibrate::working_gpu(rt),
        }),
        WorkerRequest::Calibrate => calibrate::calibrate(rt),
        WorkerRequest::Measure { model } => calibrate::measure(rt, &model),
        WorkerRequest::Tag { model, gpu, jpeg } => {
            let image = picture::decode(&jpeg)?;
            let (tags, rating) = tag::tag(rt, &model, gpu, &image)?;
            Ok(WorkerReply::Tags { tags, rating })
        }
        WorkerRequest::Translate {
            model,
            gpu,
            from,
            to,
            texts,
        } => Ok(WorkerReply::Translated {
            texts: translate::translate(rt, &model, gpu, &from, &to, &texts)?,
        }),
        WorkerRequest::Detect {
            detector,
            gpu,
            jpeg,
        } => {
            let image = picture::decode(&jpeg)?;
            Ok(WorkerReply::Boxes {
                boxes: comics::detect(rt, &detector, gpu, &image)?,
            })
        }
        WorkerRequest::Read { gpu, jpeg, boxes } => {
            let image = picture::decode(&jpeg)?;
            Ok(WorkerReply::Texts {
                texts: comics::read(rt, gpu, &image, &boxes)?,
            })
        }
        WorkerRequest::Unload => {
            rt.unload();
            Ok(WorkerReply::Done)
        }
    }
}
