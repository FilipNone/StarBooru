//! Text translators: encoder and decoder pairs (Opus-MT, M2M100, MADLAD-400)
//! and TranslateGemma.

use std::rc::Rc;

use ort::session::{Session, SessionInputValue};
use ort::value::DynValue;
use serde_json::Value;
use starbooru_core::ai_catalog::{self, Engine};
use starbooru_core::ai_worker::DAMAGED_MODEL;
use tokenizers::Tokenizer;

use crate::onnx::{self, Inputs};
use crate::runtime::Runtime;

const ENCODER: &str = "encoder_model_quantized.onnx";
const DECODER: &str = "decoder_model_merged_quantized.onnx";
const GEMMA: &str = "model_q4.onnx";
/// Longest piece of text sent to a model at once, in characters.
const MAX_PIECE_CHARS: usize = 400;
/// Words a model is given and may answer with, per piece.
const MAX_INPUT_TOKENS: usize = 400;
const MAX_OUTPUT_TOKENS: usize = 256;
/// `<eos>` and `<end_of_turn>`.
const GEMMA_STOP: [u32; 2] = [1, 106];

/// The tokenizer files these models ship with are made for transformers.js,
/// and the Rust library refuses or even panics on two things in them: a
/// normalizer with no data, and merges of tokens outside the vocabulary.
pub fn clean_tokenizer(text: &[u8]) -> Result<Vec<u8>, String> {
    let mut json: Value = serde_json::from_slice(text).map_err(|_| "bad tokenizer file")?;
    if let Some(normalizer) = json.get_mut("normalizer") {
        *normalizer = clean_normalizer(normalizer.take());
    }
    let vocab = json.pointer("/model/vocab").cloned();
    if let (Some(Value::Object(vocab)), Some(Value::Array(merges))) =
        (vocab, json.pointer_mut("/model/merges"))
    {
        merges.retain(|m| {
            let (a, b) = match m {
                Value::String(s) => match s.split_once(' ') {
                    Some(pair) => pair,
                    None => return false,
                },
                Value::Array(pair) => match (pair.first(), pair.get(1)) {
                    (Some(Value::String(a)), Some(Value::String(b))) => (a.as_str(), b.as_str()),
                    _ => return false,
                },
                _ => return false,
            };
            vocab.contains_key(a) && vocab.contains_key(b) && vocab.contains_key(&format!("{a}{b}"))
        });
    }
    serde_json::to_vec(&json).map_err(|_| "bad tokenizer file".to_string())
}

fn clean_normalizer(normalizer: Value) -> Value {
    match normalizer.get("type").and_then(Value::as_str) {
        Some("Precompiled")
            if normalizer
                .get("precompiled_charsmap")
                .is_none_or(Value::is_null) =>
        {
            Value::Null
        }
        Some("Sequence") => {
            let parts: Vec<Value> = normalizer
                .get("normalizers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .cloned()
                .map(clean_normalizer)
                .filter(|n| !n.is_null())
                .collect();
            if parts.is_empty() {
                Value::Null
            } else {
                serde_json::json!({ "type": "Sequence", "normalizers": parts })
            }
        }
        _ => normalizer,
    }
}

fn tokenizer(rt: &mut Runtime, component: &str) -> Result<Rc<Tokenizer>, String> {
    if let Some(found) = rt.tokenizers.get(component) {
        return Ok(found.clone());
    }
    let path = rt.file(component, "tokenizer.json")?;
    let raw = std::fs::read(path).map_err(|_| DAMAGED_MODEL)?;
    let clean = clean_tokenizer(&raw).map_err(|_| DAMAGED_MODEL)?;
    let loaded = Tokenizer::from_bytes(&clean).map_err(|_| DAMAGED_MODEL)?;
    let loaded = Rc::new(loaded);
    rt.tokenizers.insert(component.to_string(), loaded.clone());
    Ok(loaded)
}

/// Splits text into pieces a model handles well: lines, then sentences. Each
/// piece keeps what separated it from the next one.
pub fn pieces(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut count = 0;
    for c in text.chars() {
        if c == '\n' {
            if current.trim().is_empty() {
                current.clear();
                match out.last_mut() {
                    Some((_, sep)) => {
                        let sep: &mut String = sep;
                        // The end of a sentence that was also the end of a line.
                        if sep == " " {
                            sep.clear();
                        }
                        sep.push('\n');
                    }
                    None => out.push((String::new(), "\n".to_string())),
                }
            } else {
                out.push((current.trim().to_string(), "\n".to_string()));
                current.clear();
            }
            count = 0;
            continue;
        }
        current.push(c);
        count += 1;
        let sentence_end = matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '…');
        if (sentence_end && count > 12) || count >= MAX_PIECE_CHARS {
            out.push((current.trim().to_string(), " ".to_string()));
            current.clear();
            count = 0;
        }
    }
    if !current.trim().is_empty() {
        out.push((current.trim().to_string(), String::new()));
    }
    out
}

/// Translates each text piece by piece.
fn by_pieces(
    text: &str,
    mut one: impl FnMut(&str) -> Result<String, String>,
) -> Result<String, String> {
    let mut out = String::new();
    for (piece, sep) in pieces(text) {
        if !piece.is_empty() {
            out.push_str(one(&piece)?.trim());
        }
        out.push_str(&sep);
    }
    Ok(out.trim_end().to_string())
}

pub fn translate(
    rt: &mut Runtime,
    model: &str,
    gpu: bool,
    from: &str,
    to: &str,
    texts: &[String],
) -> Result<Vec<String>, String> {
    let entry = ai_catalog::model(model).ok_or("unknown model")?;
    let (from, to) = (
        ai_catalog::primary_language(from),
        ai_catalog::primary_language(to),
    );
    match entry.engine {
        Engine::Opus => {
            let route = ai_catalog::opus_route(&from, &to).ok_or("no model for these languages")?;
            let mut current = texts.to_vec();
            for step in route {
                current = current
                    .iter()
                    .map(|text| by_pieces(text, |p| seq2seq(rt, step.id, gpu, p, None)))
                    .collect::<Result<_, _>>()?;
            }
            Ok(current)
        }
        Engine::M2m => {
            if from == to {
                return Ok(texts.to_vec());
            }
            let langs = (m2m_token(&from), m2m_token(&to));
            texts
                .iter()
                .map(|text| {
                    by_pieces(text, |p| {
                        seq2seq(rt, "m2m", gpu, p, Some((&langs.0, &langs.1)))
                    })
                })
                .collect()
        }
        Engine::Madlad => {
            if from == to {
                return Ok(texts.to_vec());
            }
            let target = format!("<2{to}>");
            tokenizer(rt, "madlad")?
                .token_to_id(&target)
                .ok_or("no model for these languages")?;
            texts
                .iter()
                .map(|text| {
                    by_pieces(text, |p| {
                        seq2seq(rt, "madlad", gpu, &format!("{target} {p}"), None)
                    })
                })
                .collect()
        }
        Engine::Gemma => {
            if from == to {
                return Ok(texts.to_vec());
            }
            let source = ai_catalog::language_name(&from).ok_or("no model for these languages")?;
            let target = ai_catalog::language_name(&to).ok_or("no model for these languages")?;
            let start = gemma_start(source, &from, target, &to);
            texts
                .iter()
                .map(|text| by_pieces(text, |p| gemma(rt, gpu, &start, &gemma_text(p))))
                .collect()
        }
        _ => Err("not a translator".into()),
    }
}

fn m2m_token(code: &str) -> String {
    format!("__{code}__")
}

#[derive(Clone, Copy)]
struct Tokens {
    eos: u32,
    start: u32,
    pad: Option<u32>,
}

fn special_tokens(rt: &Runtime, component: &str) -> Result<Tokens, String> {
    let path = rt.file(component, "config.json")?;
    let raw = std::fs::read(path).map_err(|_| "the model settings don't read")?;
    let json: Value = serde_json::from_slice(&raw).map_err(|_| "bad model settings")?;
    let read = |key: &str| {
        json.get(key)
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
    };
    Ok(Tokens {
        eos: read("eos_token_id").ok_or("bad model settings")?,
        start: read("decoder_start_token_id").ok_or("bad model settings")?,
        pad: read("pad_token_id"),
    })
}

/// Greedy decoding of one piece. `langs` are M2M100's language tokens.
fn seq2seq(
    rt: &mut Runtime,
    component: &str,
    gpu: bool,
    text: &str,
    langs: Option<(&str, &str)>,
) -> Result<String, String> {
    let tk = tokenizer(rt, component)?;
    let tokens = special_tokens(rt, component)?;
    let encoding = tk
        .encode(text, langs.is_none())
        .map_err(|_| "the text doesn't tokenize")?;
    let mut ids: Vec<i64> = Vec::new();
    let mut forced = None;
    if let Some((from, to)) = langs {
        let lang = tk.token_to_id(from).ok_or("no model for these languages")?;
        forced = Some(tk.token_to_id(to).ok_or("no model for these languages")?);
        ids.push(i64::from(lang));
    }
    ids.extend(
        encoding
            .get_ids()
            .iter()
            .take(MAX_INPUT_TOKENS)
            .map(|&id| i64::from(id)),
    );
    if langs.is_some() {
        ids.push(i64::from(tokens.eos));
    }
    let len = i64::try_from(ids.len()).map_err(|e| e.to_string())?;
    let shape = [1, len];
    let mask = vec![1_i64; ids.len()];

    let encoder = rt.file(component, ENCODER)?;
    let decoder = rt.file(component, DECODER)?;
    let hidden = {
        let session = rt.session(&encoder, gpu)?;
        let outputs = onnx::output_names(session);
        let first = outputs.first().ok_or("unexpected model")?.clone();
        let mut result = session
            .run(ort::inputs![
                "input_ids" => onnx::ints(&shape, ids.clone())?,
                "attention_mask" => onnx::ints(&shape, mask.clone())?
            ])
            .map_err(|e| e.to_string())?;
        result.remove(&first).ok_or("unexpected model output")?
    };
    let attention = onnx::ints(&shape, mask)?;

    let session = rt.session(&decoder, gpu)?;
    let names = onnx::output_names(session);
    let has_branch = session
        .inputs()
        .iter()
        .any(|i| i.name() == "use_cache_branch");
    let mut past = onnx::empty_past(session)?;
    // Short input, short answer: stops the odd model that repeats itself.
    let limit = ids
        .len()
        .saturating_mul(3)
        .saturating_add(32)
        .min(MAX_OUTPUT_TOKENS);
    let mut produced: Vec<u32> = Vec::new();
    let mut last = tokens.start;
    for step in 0..limit {
        let mut inputs: Inputs = vec![
            (
                "encoder_attention_mask".to_string(),
                SessionInputValue::from(&attention),
            ),
            (
                "encoder_hidden_states".to_string(),
                SessionInputValue::from(&hidden),
            ),
            (
                "input_ids".to_string(),
                onnx::ints(&[1, 1], vec![i64::from(last)])?.into(),
            ),
        ];
        if has_branch {
            inputs.push(("use_cache_branch".to_string(), onnx::flag(step > 0)?.into()));
        }
        for (name, value) in &past {
            inputs.push((name.clone(), SessionInputValue::from(value)));
        }
        let mut result = session.run(inputs).map_err(|e| e.to_string())?;
        let logits_name = names.first().ok_or("unexpected model")?;
        let logits = result.get(logits_name).ok_or("unexpected model output")?;
        let next = match (step, forced) {
            (0, Some(token)) => token,
            _ => onnx::best(onnx::last_logits(logits)?, tokens.pad)
                .ok_or("unexpected model output")?,
        };
        if next == tokens.eos {
            break;
        }
        produced.push(next);
        last = next;
        // The cross-attention values only come out of the first step.
        let mut updated = Vec::with_capacity(past.len());
        for (name, value) in past {
            let output = name.replacen("past_key_values", "present", 1);
            let keep_old = step > 0 && name.contains(".encoder.");
            if keep_old {
                updated.push((name, value));
            } else {
                let fresh = result.remove(&output).ok_or("unexpected model output")?;
                updated.push((name, fresh));
            }
        }
        past = updated;
    }
    tk.decode(&produced, true)
        .map_err(|_| "the answer doesn't decode".to_string())
}

/// Every prompt for a pair of languages begins the same way.
fn gemma_start(source: &str, sc: &str, target: &str, tc: &str) -> String {
    format!(
        "<start_of_turn>user\nYou are a professional {source} ({sc}) to {target} ({tc}) translator. \
         Your goal is to accurately convey the meaning and nuances of the original {source} text \
         while adhering to {target} grammar, vocabulary, and cultural sensitivities.\n\
         Produce only the {target} translation, without any additional explanations or commentary. \
         Please translate the following {source} text into {target}:\n\n\n"
    )
}

fn gemma_text(text: &str) -> String {
    format!("{text}<end_of_turn>\n<start_of_turn>model\n")
}

/// What TranslateGemma made of the start of a prompt. It's longer than most
/// speech bubbles, so it's worked out once rather than for each of them.
pub struct GemmaStart {
    text: String,
    gpu: bool,
    tokens: usize,
    past: Past,
}

type Past = Vec<(String, DynValue)>;

/// Feeds tokens to the model: the token that comes next, and its memory now.
fn gemma_step(
    session: &mut Session,
    names: &[String],
    ids: Vec<i64>,
    seen: usize,
    past: &[(String, DynValue)],
) -> Result<(u32, Past), String> {
    let step = i64::try_from(ids.len()).map_err(|e| e.to_string())?;
    let total = i64::try_from(seen).map_err(|e| e.to_string())?;
    let mut inputs: Inputs = vec![
        ("input_ids".to_string(), onnx::ints(&[1, step], ids)?.into()),
        (
            "attention_mask".to_string(),
            onnx::ints(&[1, total], vec![1; seen])?.into(),
        ),
    ];
    for (name, value) in past {
        inputs.push((name.clone(), SessionInputValue::from(value)));
    }
    let mut result = session.run(inputs).map_err(|e| e.to_string())?;
    let logits_name = names.first().ok_or("unexpected model")?;
    let logits = result.get(logits_name).ok_or("unexpected model output")?;
    let next = onnx::best(onnx::last_logits(logits)?, None).ok_or("unexpected model output")?;
    let mut fresh = Vec::with_capacity(past.len());
    for (name, _) in past {
        let output = name.replacen("past_key_values", "present", 1);
        let value = result.remove(&output).ok_or("unexpected model output")?;
        fresh.push((name.clone(), value));
    }
    Ok((next, fresh))
}

fn gemma(rt: &mut Runtime, gpu: bool, start: &str, text: &str) -> Result<String, String> {
    let tk = tokenizer(rt, "gemma")?;
    let encode = |text: &str| -> Result<Vec<i64>, String> {
        let encoding = tk
            .encode(text, true)
            .map_err(|_| "the text doesn't tokenize")?;
        Ok(encoding.get_ids().iter().map(|&id| i64::from(id)).collect())
    };
    let start_ids = encode(start)?;
    let all = encode(&format!("{start}{text}"))?;
    let known = rt
        .gemma_start
        .take()
        .filter(|s| s.gpu == gpu && s.text == start);
    let path = rt.file("gemma", GEMMA)?;
    let session = rt.session(&path, gpu)?;
    let names = onnx::output_names(session);
    let empty = onnx::empty_past(session)?;
    // The tokenizer could join the text to the end of the start.
    let shared = all.len() > start_ids.len() && all.starts_with(&start_ids);
    let begun = match known {
        Some(known) if shared => Some(known),
        _ if shared => {
            let tokens = start_ids.len();
            let (_, past) = gemma_step(session, &names, start_ids, tokens, &empty)?;
            Some(GemmaStart {
                text: start.to_string(),
                gpu,
                tokens,
                past,
            })
        }
        _ => None,
    };
    let seen = all.len();
    let (rest, first_past) = match &begun {
        Some(begun) => (
            all.get(begun.tokens..).unwrap_or_default().to_vec(),
            begun.past.as_slice(),
        ),
        None => (all, empty.as_slice()),
    };
    let (mut next, mut past) = gemma_step(session, &names, rest, seen, first_past)?;
    let mut produced: Vec<u32> = Vec::new();
    for count in 1..MAX_OUTPUT_TOKENS {
        if GEMMA_STOP.contains(&next) {
            break;
        }
        produced.push(next);
        let ids = vec![i64::from(next)];
        (next, past) = gemma_step(session, &names, ids, seen.saturating_add(count), &past)?;
    }
    rt.gemma_start = begun;
    tk.decode(&produced, true)
        .map_err(|_| "the answer doesn't decode".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_lines_and_splits_sentences() {
        let got = pieces("One long sentence here. Two short ones!\n\nAnother line");
        let texts: Vec<_> = got.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            texts,
            ["One long sentence here.", "Two short ones!", "Another line"]
        );
        assert_eq!(got[0].1, " ");
        assert_eq!(got[1].1, "\n\n");
        assert_eq!(got[2].1, "");
    }

    #[test]
    fn short_exclamations_stay_with_their_sentence() {
        let got = pieces("Oh! That is fine.");
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn very_long_text_is_cut() {
        let text = "あ".repeat(MAX_PIECE_CHARS * 2 + 5);
        let got = pieces(&text);
        assert_eq!(got.len(), 3);
        assert!(
            got.iter()
                .all(|(t, _)| t.chars().count() <= MAX_PIECE_CHARS)
        );
    }

    #[test]
    fn empty_text_has_no_pieces_to_translate() {
        assert_eq!(by_pieces("", |_| Err("called".into())), Ok(String::new()));
        assert_eq!(
            by_pieces("  \n ", |_| Err("called".into())),
            Ok(String::new())
        );
    }

    #[test]
    fn rejoins_with_the_same_breaks() {
        let got = by_pieces("One long sentence here. Second.\nThird", |p| {
            Ok(p.to_uppercase())
        });
        assert_eq!(
            got,
            Ok("ONE LONG SENTENCE HERE. SECOND.\nTHIRD".to_string())
        );
    }

    #[test]
    fn cleans_tokenizers_the_library_refuses() {
        let raw = serde_json::json!({
            "normalizer": { "type": "Precompiled", "precompiled_charsmap": null },
            "model": { "type": "BPE", "vocab": { "a": 0, "b": 1, "ab": 2 },
                "merges": ["a b", "a c", ["a", "b"], ["c", "d"], "broken", 5] }
        });
        let clean = clean_tokenizer(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let json: Value = serde_json::from_slice(&clean).unwrap();
        assert!(json["normalizer"].is_null());
        assert_eq!(
            json["model"]["merges"],
            serde_json::json!(["a b", ["a", "b"]])
        );
    }

    #[test]
    fn cleans_a_sequence_of_normalizers() {
        let keep = serde_json::json!({ "type": "NFC" });
        let raw = serde_json::json!({ "type": "Sequence", "normalizers": [
            { "type": "Precompiled", "precompiled_charsmap": null }, keep ] });
        assert_eq!(
            clean_normalizer(raw),
            serde_json::json!({ "type": "Sequence", "normalizers": [keep] })
        );
        let empty = serde_json::json!({ "type": "Sequence", "normalizers": [
            { "type": "Precompiled" } ] });
        assert!(clean_normalizer(empty).is_null());
        assert!(clean_tokenizer(b"nope").is_err());
    }

    #[test]
    fn the_prompt_names_both_languages() {
        let start = gemma_start("Japanese", "ja", "Italian", "it");
        assert!(start.starts_with(
            "<start_of_turn>user\nYou are a professional Japanese (ja) to Italian (it)"
        ));
        assert!(start.ends_with("text into Italian:\n\n\n"));
        assert_eq!(
            gemma_text("こんにちは"),
            "こんにちは<end_of_turn>\n<start_of_turn>model\n"
        );
    }
}
