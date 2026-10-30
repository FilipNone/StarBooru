//! Finding and reading the text of a comic page.

use image::RgbImage;
use starbooru_core::ai_worker::{DAMAGED_MODEL, TextBox};

use crate::onnx;
use crate::picture;
use crate::runtime::Runtime;

const DETECTOR_SIZE: u32 = 640;
const MIN_SCORE: f32 = 0.45;
/// The detector's classes: 0 a bubble, 1 text in a bubble, 2 text outside one.
const TEXT_LABELS: [i64; 2] = [1, 2];
const MAX_BOXES: usize = 64;
const OCR_SIZE: u32 = 224;
const MAX_OCR_TOKENS: usize = 64;

const OCR_COMPONENT: &str = "manga-ocr";

fn detector_file(component: &str) -> Result<&'static str, String> {
    match component {
        "det-small" => Ok("detector-v4-s_int8.onnx"),
        "det-large" => Ok("detector_int8.onnx"),
        _ => Err("unknown model".into()),
    }
}

/// The boxes worth reading, in pixels, inside the page and top to bottom.
fn keep_boxes(
    labels: &[i64],
    boxes: &[f32],
    scores: &[f32],
    width: u32,
    height: u32,
) -> Vec<TextBox> {
    let (max_x, max_y) = (width as f32, height as f32);
    let mut found: Vec<(f32, TextBox)> = labels
        .iter()
        .zip(scores)
        .zip(boxes.as_chunks::<4>().0)
        .filter(|((label, score), _)| TEXT_LABELS.contains(label) && **score >= MIN_SCORE)
        .filter_map(|((_, score), &[x0, y0, x1, y1])| {
            if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
                return None;
            }
            let (x0, y0) = (x0.clamp(0.0, max_x), y0.clamp(0.0, max_y));
            let (x1, y1) = (x1.clamp(0.0, max_x), y1.clamp(0.0, max_y));
            let (w, h) = (x1 - x0, y1 - y0);
            (w >= 1.0 && h >= 1.0).then_some((
                *score,
                TextBox {
                    x: x0 as u32,
                    y: y0 as u32,
                    w: w as u32,
                    h: h as u32,
                },
            ))
        })
        .collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.truncate(MAX_BOXES);
    let mut boxes: Vec<TextBox> = found.into_iter().map(|(_, b)| b).collect();
    boxes.sort_by_key(|b| (b.y, std::cmp::Reverse(b.x)));
    boxes
}

pub fn detect(
    rt: &mut Runtime,
    detector: &str,
    gpu: bool,
    image: &RgbImage,
) -> Result<Vec<TextBox>, String> {
    let path = rt.file(detector, detector_file(detector)?)?;
    let (shape, data) = picture::stretched_chw(image, DETECTOR_SIZE);
    let (width, height) = image.dimensions();
    let session = rt.session(&path, gpu)?;
    let outputs = session
        .run(ort::inputs![
            "images" => onnx::floats(&shape, data)?,
            "orig_target_sizes" => onnx::ints(&[1, 2], vec![i64::from(width), i64::from(height)])?
        ])
        .map_err(|e| e.to_string())?;
    let get = |name: &str| outputs.get(name).ok_or("unexpected model output");
    let labels = onnx::read_ints(get("labels")?)?;
    let (_, boxes) = onnx::read_floats(get("boxes")?)?;
    let (_, scores) = onnx::read_floats(get("scores")?)?;
    Ok(keep_boxes(labels, boxes, scores, width, height))
}

/// Turns Manga OCR's tokens into text.
fn join_tokens(tokens: &[String], ids: &[u32]) -> String {
    ids.iter()
        .filter_map(|&id| tokens.get(usize::try_from(id).ok()?))
        .filter(|t| !t.starts_with('['))
        .map(|t| t.trim_start_matches("##"))
        .collect()
}

pub fn read(
    rt: &mut Runtime,
    gpu: bool,
    image: &RgbImage,
    boxes: &[TextBox],
) -> Result<Vec<String>, String> {
    let vocab = rt.file(OCR_COMPONENT, "vocab.txt")?;
    let vocab = std::fs::read_to_string(vocab).map_err(|_| DAMAGED_MODEL)?;
    let tokens: Vec<String> = vocab.lines().map(str::to_string).collect();
    let id_of = |name: &str| {
        tokens
            .iter()
            .position(|t| t == name)
            .and_then(|i| u32::try_from(i).ok())
            .ok_or("unexpected vocabulary")
    };
    let (start, stop) = (id_of("[CLS]")?, id_of("[SEP]")?);
    let encoder = rt.file(OCR_COMPONENT, "encoder_model.onnx")?;
    let decoder = rt.file(OCR_COMPONENT, "decoder_model.onnx")?;
    let mut texts = Vec::with_capacity(boxes.len());
    for b in boxes.iter().take(MAX_BOXES) {
        let (shape, data) = picture::ocr_crop(image, b.x, b.y, b.w, b.h, OCR_SIZE);
        let hidden = {
            let session = rt.session(&encoder, gpu)?;
            let first = onnx::output_names(session)
                .into_iter()
                .next()
                .ok_or("unexpected model")?;
            let mut result = session
                .run(ort::inputs!["pixel_values" => onnx::floats(&shape, data)?])
                .map_err(|e| e.to_string())?;
            result.remove(&first).ok_or("unexpected model output")?
        };
        let session = rt.session(&decoder, gpu)?;
        let first = onnx::output_names(session)
            .into_iter()
            .next()
            .ok_or("unexpected model")?;
        let mut ids = vec![start];
        for _ in 0..MAX_OCR_TOKENS {
            let input: Vec<i64> = ids.iter().map(|&id| i64::from(id)).collect();
            let len = i64::try_from(input.len()).map_err(|e| e.to_string())?;
            let result = session
                .run(ort::inputs![
                    "input_ids" => onnx::ints(&[1, len], input)?,
                    "encoder_hidden_states" => &hidden
                ])
                .map_err(|e| e.to_string())?;
            let logits = result.get(&first).ok_or("unexpected model output")?;
            let next =
                onnx::best(onnx::last_logits(logits)?, None).ok_or("unexpected model output")?;
            if next == stop {
                break;
            }
            ids.push(next);
        }
        texts.push(join_tokens(&tokens, ids.get(1..).unwrap_or_default()));
    }
    Ok(texts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_confident_text_only() {
        let labels = [0, 1, 2, 1];
        let boxes = [
            0., 0., 10., 10., // a bubble, not text
            5., 50., 25., 90., // text
            40., 10., 60., 30., // free text, higher up
            5., 5., 9., 9., // not sure enough
        ];
        let scores = [0.9, 0.9, 0.5, 0.2];
        let got = keep_boxes(&labels, &boxes, &scores, 100, 100);
        assert_eq!(
            got,
            [
                TextBox {
                    x: 40,
                    y: 10,
                    w: 20,
                    h: 20
                },
                TextBox {
                    x: 5,
                    y: 50,
                    w: 20,
                    h: 40
                },
            ]
        );
    }

    #[test]
    fn hostile_boxes_stay_on_the_page() {
        let labels = [1, 1, 1, 1];
        let boxes = [
            -50.,
            -50.,
            5000.,
            5000., // bigger than the page
            f32::NAN,
            0.,
            10.,
            10., // not a number
            60.,
            60.,
            10.,
            10., // upside down
            90.,
            90.,
            90.5,
            90.5, // too small
        ];
        let scores = [0.9; 4];
        let got = keep_boxes(&labels, &boxes, &scores, 100, 100);
        assert_eq!(
            got,
            [TextBox {
                x: 0,
                y: 0,
                w: 100,
                h: 100
            }]
        );
        assert!(keep_boxes(&[1], &[1.0], &[0.9], 10, 10).is_empty());
    }

    #[test]
    fn caps_the_boxes_per_page() {
        let n = MAX_BOXES + 10;
        let boxes: Vec<f32> = (0..n).flat_map(|_| [0.0, 0.0, 10.0, 10.0]).collect();
        let got = keep_boxes(&vec![1; n], &boxes, &vec![0.9; n], 100, 100);
        assert_eq!(got.len(), MAX_BOXES);
    }

    #[test]
    fn joins_tokens_without_markers() {
        let tokens: Vec<String> = ["[CLS]", "今", "##日", "は"].map(String::from).to_vec();
        assert_eq!(join_tokens(&tokens, &[0, 1, 2, 3, 99]), "今日は");
    }
}
