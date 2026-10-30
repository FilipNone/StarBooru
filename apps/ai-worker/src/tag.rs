//! Image taggers.

use std::rc::Rc;

use image::RgbImage;
use ort::inputs;
use starbooru_core::ai_catalog::{self, Engine};
use starbooru_core::ai_worker::{DAMAGED_MODEL, FoundTag};
use starbooru_core::model::{Rating, TagCategory};

use crate::onnx;
use crate::picture;
use crate::runtime::Runtime;

/// What the WD taggers were trained on.
const WD_SIZE: u32 = 448;
const WD_CHARACTER_THRESHOLD: f32 = 0.85;
const ML_DANBOORU_THRESHOLD: f32 = 0.5;
const RGB: [usize; 3] = [0, 1, 2];
const BGR: [usize; 3] = [2, 1, 0];
/// What JoyTag was trained on (the CLIP values).
const JOYTAG_SIZE: u32 = 448;
const JOYTAG_MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
const JOYTAG_STD: [f32; 3] = [0.268_629_54, 0.261_302_6, 0.275_777_1];
/// Tags kept per picture, best first.
const MAX_TAGS: usize = 40;

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    name: String,
    /// The WD list's category: 0 general, 4 character, 9 rating.
    category: u8,
}

pub type TagList = Vec<Row>;

/// One row of a CSV file that may have quoted fields.
fn fields(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(String::new()),
            c => {
                if let Some(last) = out.last_mut() {
                    last.push(c);
                }
            }
        }
    }
    out
}

/// Reads `tag,…` (ML-Danbooru) or `id,name,category,…` (WD) rows.
fn parse_tags(text: &str, wd: bool) -> TagList {
    text.lines()
        .skip(1)
        .map(|line| {
            let mut f = fields(line).into_iter();
            if wd {
                let name = f.nth(1).unwrap_or_default();
                let category = f.next().and_then(|c| c.parse().ok()).unwrap_or(0);
                Row { name, category }
            } else {
                Row {
                    name: f.next().unwrap_or_default(),
                    category: 0,
                }
            }
        })
        .collect()
}

/// Reads one tag per line (JoyTag).
fn parse_lines(text: &str) -> TagList {
    text.lines()
        .map(|name| Row {
            name: name.trim().to_string(),
            category: 0,
        })
        .collect()
}

fn tag_list(
    rt: &mut Runtime,
    component: &str,
    file: &str,
    parse: impl FnOnce(&str) -> TagList,
) -> Result<Rc<TagList>, String> {
    if let Some(list) = rt.tags.get(component) {
        return Ok(list.clone());
    }
    let path = rt.file(component, file)?;
    let text = std::fs::read_to_string(path).map_err(|_| DAMAGED_MODEL)?;
    let list = Rc::new(parse(&text));
    rt.tags.insert(component.to_string(), list.clone());
    Ok(list)
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Tags for the scores of one picture.
fn choose(
    list: &[Row],
    scores: &[f32],
    general: f32,
    character: f32,
) -> (Vec<FoundTag>, Option<Rating>) {
    let mut tags = Vec::new();
    let mut rating: Option<(f32, Rating)> = None;
    for (row, &score) in list.iter().zip(scores) {
        match row.category {
            9 => {
                let value = match row.name.as_str() {
                    "general" => Rating::General,
                    "sensitive" => Rating::Sensitive,
                    "questionable" => Rating::Questionable,
                    "explicit" => Rating::Explicit,
                    _ => continue,
                };
                if rating.is_none_or(|(best, _)| score > best) {
                    rating = Some((score, value));
                }
            }
            4 if score >= character => tags.push(FoundTag {
                name: row.name.clone(),
                category: Some(TagCategory::Character),
                score,
            }),
            0 if score >= general => tags.push(FoundTag {
                name: row.name.clone(),
                category: None,
                score,
            }),
            _ => {}
        }
    }
    tags.sort_by(|a, b| b.score.total_cmp(&a.score));
    tags.truncate(MAX_TAGS);
    (tags, rating.map(|(_, r)| r))
}

pub fn tag(
    rt: &mut Runtime,
    model: &str,
    gpu: bool,
    image: &RgbImage,
) -> Result<(Vec<FoundTag>, Option<Rating>), String> {
    let entry = ai_catalog::model(model).ok_or("unknown model")?;
    let component = entry.components.first().ok_or("unknown model")?;
    match entry.engine {
        Engine::MlDanbooru { size, file } => {
            let list = tag_list(rt, component, "tags.csv", |t| parse_tags(t, false))?;
            let path = rt.file(component, file)?;
            let (shape, data) = picture::stretched_chw(image, size);
            let scores: Vec<f32> = run(rt, &path, gpu, &shape, data)?
                .into_iter()
                .map(sigmoid)
                .collect();
            Ok(choose(
                &list,
                &scores,
                ML_DANBOORU_THRESHOLD,
                ML_DANBOORU_THRESHOLD,
            ))
        }
        Engine::Wd { threshold } => {
            let list = tag_list(rt, component, "selected_tags.csv", |t| parse_tags(t, true))?;
            let path = rt.file(component, "model.onnx")?;
            let (shape, data) = picture::padded_bgr_hwc(image, WD_SIZE);
            let scores = run(rt, &path, gpu, &shape, data)?;
            Ok(choose(&list, &scores, threshold, WD_CHARACTER_THRESHOLD))
        }
        Engine::DanbooruV1 { threshold } => {
            let list = tag_list(rt, component, "selected_tags.csv", |t| parse_tags(t, true))?;
            let path = rt.file(component, "model_prob.onnx")?;
            let (shape, data) = picture::padded_chw(image, WD_SIZE, BGR, [0.5; 3], [0.5; 3]);
            let scores = run(rt, &path, gpu, &shape, data)?;
            Ok(choose(&list, &scores, threshold, threshold))
        }
        Engine::JoyTag { threshold } => {
            let list = tag_list(rt, component, "top_tags.txt", parse_lines)?;
            let path = rt.file(component, "model.onnx")?;
            let (shape, data) =
                picture::padded_chw(image, JOYTAG_SIZE, RGB, JOYTAG_MEAN, JOYTAG_STD);
            let scores: Vec<f32> = run(rt, &path, gpu, &shape, data)?
                .into_iter()
                .map(sigmoid)
                .collect();
            Ok(choose(&list, &scores, threshold, threshold))
        }
        _ => Err("not a tagger".into()),
    }
}

fn run(
    rt: &mut Runtime,
    path: &std::path::Path,
    gpu: bool,
    shape: &[i64],
    data: Vec<f32>,
) -> Result<Vec<f32>, String> {
    let session = rt.session(path, gpu)?;
    let name = onnx::input_name(session, 0)?;
    let output = session
        .run(inputs![name.as_str() => onnx::floats(shape, data)?])
        .map_err(|e| e.to_string())?;
    let value = output.get(output.keys().next().ok_or("unexpected model output")?);
    let (_, scores) = onnx::read_floats(value.ok_or("unexpected model output")?)?;
    Ok(scores.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wd_list() -> TagList {
        parse_tags(
            "tag_id,name,category,count\n\
             1,general,9,10\n\
             2,explicit,9,10\n\
             3,1girl,0,10\n\
             4,\"odd,name\",0,10\n\
             5,hatsune_miku,4,10\n\
             6,low,0,10\n",
            true,
        )
    }

    #[test]
    fn reads_every_list() {
        let list = wd_list();
        assert_eq!(list.len(), 6);
        assert_eq!(list[3].name, "odd,name");
        assert_eq!(list[4].category, 4);
        let tres = parse_tags("tag,root,pos,count\nlong_hair,x,0,1\n", false);
        assert_eq!(tres[0].name, "long_hair");
        let joy = parse_lines("1girl\nsolo\n");
        assert_eq!(joy[1].name, "solo");
        assert_eq!(joy[1].category, 0);
        // Broken rows don't stop anything.
        assert_eq!(parse_tags("h\n\n,,,\n", true).len(), 2);
    }

    #[test]
    fn chooses_tags_and_one_rating() {
        let (tags, rating) = choose(&wd_list(), &[0.2, 0.7, 0.9, 0.6, 0.9, 0.1], 0.5, 0.85);
        assert_eq!(rating, Some(Rating::Explicit));
        let names: Vec<_> = tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["1girl", "hatsune_miku", "odd,name"]);
        assert_eq!(tags[1].category, Some(TagCategory::Character));
    }

    #[test]
    fn a_character_needs_more_certainty() {
        let (tags, _) = choose(&wd_list(), &[0.0, 0.0, 0.0, 0.0, 0.7, 0.0], 0.5, 0.85);
        assert!(tags.is_empty());
    }

    #[test]
    fn short_scores_are_harmless() {
        let (tags, rating) = choose(&wd_list(), &[0.9], 0.5, 0.85);
        assert!(tags.is_empty());
        assert_eq!(rating, Some(Rating::General));
    }

    #[test]
    fn logits_become_probabilities() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(10.0) > 0.99);
    }
}
