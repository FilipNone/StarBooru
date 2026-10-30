//! How fast this machine is: tiny built-in models for a first estimate, and
//! real timings of each installed model.

use std::time::{Duration, Instant};

use ort::session::Session;
use starbooru_core::ai_catalog::{self, AiTool, Engine};
use starbooru_core::ai_worker::{TextBox, WorkerReply};

use crate::onnx::{self, Message};
use crate::picture;
use crate::runtime::{self, Runtime};
use crate::{comics, tag, translate};

const SIDE: usize = 256;
const LAYERS: usize = 32;
const RUNS: u32 = 6;
/// Longer than the slices a limited container or a shared server is given, or
/// short runs would all fit in one and hide the limit.
const MIN_SPAN: Duration = Duration::from_millis(1000);
/// Past this, even without the time to load it the model is over every budget,
/// and the machine is spared a second run.
const LONG_RUN_MS: u128 = 120_000;
const SENTENCE: &str = "今日はとても天気がいいので、友達と一緒に公園へ散歩に行きました。";
/// What the catalog's comic times count.
const PAGE_BLOCKS: u32 = 8;
/// What a speech bubble says, more or less.
const BUBBLE: &str = "今日はいい天気ですね。";

/// A repeatable random sequence.
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }

    /// Between -1 and 1.
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

fn tensor(name: &str, data_type: u64, dims: &[u64], raw: &[u8]) -> Message {
    let mut t = Message::default();
    for &d in dims {
        t = t.int(1, d);
    }
    t.int(2, data_type).text(8, name).bytes(9, raw)
}

fn value_info(name: &str, elem_type: u64, dims: &[u64]) -> Message {
    let mut shape = Message::default();
    for &d in dims {
        shape = shape.nested(1, Message::default().int(1, d));
    }
    let ty = Message::default().int(1, elem_type).nested(2, shape);
    Message::default()
        .text(1, name)
        .nested(2, Message::default().nested(1, ty))
}

fn node(op: &str, inputs: &[&str], outputs: &[&str]) -> Message {
    let mut n = Message::default();
    for i in inputs {
        n = n.text(1, i);
    }
    for o in outputs {
        n = n.text(2, o);
    }
    n.text(4, op)
}

fn model(graph: Message) -> Vec<u8> {
    Message::default()
        .int(1, 8)
        .nested(7, graph)
        .nested(8, Message::default().text(1, "").int(2, 17))
        .into_bytes()
}

const FLOAT: u64 = 1;
const INT8: u64 = 3;
const SIDE64: u64 = SIDE as u64;

fn input_output(graph: Message, last: &str) -> Message {
    graph
        .nested(11, value_info("x", FLOAT, &[1, SIDE64, SIDE64]))
        .nested(12, value_info(last, FLOAT, &[1, SIDE64, SIDE64]))
}

/// Matrix products, what most of a vision transformer's time goes to.
fn float_model() -> Vec<u8> {
    float_layers(LAYERS)
}

fn float_layers(layers: usize) -> Vec<u8> {
    let mut rng = Lcg(1);
    let scale = 1.0 / (SIDE as f32).sqrt();
    let mut graph = Message::default().text(2, "float");
    let mut last = "x".to_string();
    for layer in 0..layers {
        let weights: Vec<u8> = (0..SIDE * SIDE)
            .flat_map(|_| (rng.unit() * scale).to_le_bytes())
            .collect();
        let (w, out) = (format!("w{layer}"), format!("m{layer}"));
        graph = graph
            .nested(5, tensor(&w, FLOAT, &[SIDE64, SIDE64], &weights))
            .nested(1, node("MatMul", &[&last, &w], &[&out]));
        last = out;
    }
    model(input_output(graph, &last))
}

/// Dynamically quantized products, what the quantized text models do.
fn int8_model() -> Vec<u8> {
    let mut rng = Lcg(2);
    let mut graph = Message::default().text(2, "int8");
    let mut last = "x".to_string();
    for layer in 0..LAYERS {
        let weights: Vec<u8> = (0..SIDE * SIDE).map(|_| (rng.next() >> 24) as u8).collect();
        let (w, q, scale, zero, product, out) = (
            format!("w{layer}"),
            format!("q{layer}"),
            format!("s{layer}"),
            format!("z{layer}"),
            format!("p{layer}"),
            format!("f{layer}"),
        );
        let cast = Message::default().text(1, "to").int(3, FLOAT).int(20, 2);
        graph = graph
            .nested(5, tensor(&w, INT8, &[SIDE64, SIDE64], &weights))
            .nested(
                1,
                node("DynamicQuantizeLinear", &[&last], &[&q, &scale, &zero]),
            )
            .nested(1, node("MatMulInteger", &[&q, &w, &zero], &[&product]))
            .nested(1, node("Cast", &[&product], &[&out]).nested(5, cast));
        last = out;
    }
    model(input_output(graph, &last))
}

/// The average time of one run, after a first one that sets things up.
fn time_runs(session: &mut Session) -> Result<f64, String> {
    let mut rng = Lcg(3);
    let data: Vec<f32> = (0..SIDE * SIDE).map(|_| rng.unit()).collect();
    let side = SIDE as i64;
    let name = onnx::input_name(session, 0)?;
    let mut run = |data: Vec<f32>| -> Result<(), String> {
        let input = onnx::floats(&[1, side, side], data)?;
        session
            .run(ort::inputs![name.as_str() => input])
            .map_err(|e| e.to_string())?;
        Ok(())
    };
    run(data.clone())?;
    let start = Instant::now();
    let mut runs = 0u32;
    while runs < RUNS || start.elapsed() < MIN_SPAN {
        run(data.clone())?;
        runs = runs.saturating_add(1);
    }
    Ok(start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs))
}

/// The graphics card interface this build has, when a card really runs a
/// model through it: the runtime only says whether the build can talk to one.
pub fn working_gpu(rt: &Runtime) -> Option<String> {
    let gpu = runtime::gpu_name()?;
    let mut session = rt.session_from_memory(&float_layers(1), true).ok()?;
    let name = onnx::input_name(&session, 0).ok()?;
    let side = SIDE as i64;
    let input = onnx::floats(&[1, side, side], vec![0.0; SIDE * SIDE]).ok()?;
    session.run(ort::inputs![name.as_str() => input]).ok()?;
    Some(gpu)
}

pub fn calibrate(rt: &mut Runtime) -> Result<WorkerReply, String> {
    let float = float_model();
    let float_ms = time_runs(&mut rt.session_from_memory(&float, false)?)?;
    let int8_ms = time_runs(&mut rt.session_from_memory(&int8_model(), false)?)?;
    // A card that fails here just doesn't count.
    let gpu_ms = runtime::gpu_name()
        .and_then(|_| rt.session_from_memory(&float, true).ok())
        .and_then(|mut session| time_runs(&mut session).ok());
    Ok(WorkerReply::Calibrated {
        float_ms,
        int8_ms,
        gpu_ms,
    })
}

/// What a run of a model produces, for comparing devices.
fn run_once(rt: &mut Runtime, id: &str, gpu: bool) -> Result<Vec<String>, String> {
    let entry = ai_catalog::model(id).ok_or("unknown model")?;
    match (entry.tool, entry.engine) {
        (AiTool::Tagging, _) => {
            let image = picture::generated(512, 512);
            let (tags, _) = tag::tag(rt, id, gpu, &image)?;
            // Devices round differently: the strongest tags must agree.
            let mut names: Vec<String> = tags.into_iter().take(5).map(|t| t.name).collect();
            names.sort();
            Ok(names)
        }
        (AiTool::Translation, engine) => {
            let (from, to) = sample_languages(engine);
            translate::translate(rt, id, gpu, from, to, &[SENTENCE.to_string()])
        }
        (
            AiTool::Comics,
            Engine::Comics {
                detector,
                translator,
            },
        ) => {
            let page = picture::generated(1100, 1536);
            let found = comics::detect(rt, detector, gpu, &page)?;
            let boxes: Vec<TextBox> = (0..PAGE_BLOCKS)
                .map(|i| TextBox {
                    x: 40 + i * 120,
                    y: 60 + i * 150,
                    w: 110,
                    h: 220,
                })
                .collect();
            let mut out = comics::read(rt, gpu, &page, &boxes)?;
            let translator_entry = ai_catalog::model(translator).ok_or("unknown model")?;
            let (from, to) = sample_languages(translator_entry.engine);
            let texts = vec![BUBBLE.to_string(); PAGE_BLOCKS as usize];
            out.extend(translate::translate(rt, translator, gpu, from, to, &texts)?);
            // Opus reaches most languages through English: two steps.
            if matches!(translator_entry.engine, Engine::Opus) {
                translate::translate(rt, translator, gpu, from, to, &texts)?;
            }
            out.push(found.len().to_string());
            Ok(out)
        }
        _ => Err("unknown model".into()),
    }
}

/// A pair every install of the translator has.
fn sample_languages(engine: Engine) -> (&'static str, &'static str) {
    match engine {
        Engine::Opus => ("ja", "en"),
        _ => ("ja", "it"),
    }
}

/// How long a model takes once it's loaded, which is what the budgets count: a
/// first run includes reading it from disk, which can take far longer.
fn timed(rt: &mut Runtime, id: &str, gpu: bool) -> Result<(u32, Vec<String>), String> {
    let start = Instant::now();
    let mut result = run_once(rt, id, gpu)?;
    let mut elapsed = start.elapsed().as_millis();
    if elapsed < LONG_RUN_MS {
        let start = Instant::now();
        result = run_once(rt, id, gpu)?;
        elapsed = start.elapsed().as_millis();
    }
    Ok((u32::try_from(elapsed).unwrap_or(u32::MAX), result))
}

/// Times a model with nothing else loaded: models left in memory would crowd
/// it out and slow it down, and the time is kept for good.
pub fn measure(rt: &mut Runtime, id: &str) -> Result<WorkerReply, String> {
    rt.unload();
    let reply = measure_alone(rt, id);
    rt.unload();
    reply
}

fn measure_alone(rt: &mut Runtime, id: &str) -> Result<WorkerReply, String> {
    let entry = ai_catalog::model(id).ok_or("unknown model")?;
    let (mut cpu_ms, cpu_output) = timed(rt, id, false)?;
    rt.unload();
    // A translation through English takes two turns. Only the first language
    // pair is sure to be installed.
    if entry.engine == Engine::Opus {
        cpu_ms = cpu_ms.saturating_mul(2);
    }
    let on_gpu = runtime::gpu_name().and_then(|_| timed(rt, id, true).ok());
    let (gpu_ms, gpu_matches) = match on_gpu {
        Some((ms, output)) => (
            Some(if entry.engine == Engine::Opus {
                ms.saturating_mul(2)
            } else {
                ms
            }),
            output == cpu_output,
        ),
        None => (None, false),
    };
    Ok(WorkerReply::Measured {
        cpu_ms,
        gpu_ms,
        gpu_matches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_protobuf_varints() {
        let bytes = Message::default().int(1, 300).into_bytes();
        assert_eq!(bytes, [0x08, 0xac, 0x02]);
        let bytes = Message::default().text(2, "hi").into_bytes();
        assert_eq!(bytes, [0x12, 2, b'h', b'i']);
    }

    #[test]
    fn calibration_models_are_repeatable() {
        assert_eq!(float_model(), float_model());
        assert_eq!(int8_model(), int8_model());
        assert_ne!(float_model(), int8_model());
    }
}
