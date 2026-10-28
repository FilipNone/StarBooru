//! AI provider types and the tag-suggestion answer parser.
//!
//! The server talks to an OpenAI-compatible chat endpoint that the admin
//! configures. What the model answers is untrusted text: [`parse_suggestions`]
//! reads it leniently and keeps only valid tag names.

use serde::{Deserialize, Serialize};

use crate::model::{Rating, TagCategory};
use crate::tag::normalize_tag;

/// Most tags kept from one answer.
pub const MAX_SUGGESTIONS: usize = 40;
/// Longest base URL or model name an admin may enter.
pub const MAX_AI_FIELD_CHARS: usize = 512;

/// The prompt that asks a vision model for booru tags.
pub const TAG_PROMPT: &str = "You tag images for a booru (an image board). Look at the image and \
answer with JSON only, no prose, in this shape: \
{\"tags\": [{\"name\": \"long_hair\", \"category\": \"general\"}], \"rating\": \"general\"}. \
Use danbooru conventions: lowercase English, words joined by underscores, singular nouns, \
for example 1girl, blue_sky, holding_umbrella, outdoors. Category is one of general, artist, \
character, copyright, meta; only name characters, series or artists you are sure of. \
Give at most 30 tags, most important first. Rating is one of general, sensitive, \
questionable, explicit.";

/// `GET /ai`: whether AI features are available to clients.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiStatus {
    pub enabled: bool,
    /// The configured model, when enabled.
    pub model: Option<String>,
}

/// `GET /admin/ai`. The key itself is never returned.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSettings {
    /// Base URL of an OpenAI-compatible API, such as `https://api.openai.com/v1`
    /// (`STARBOORU_AI_URL`). Empty turns AI features off.
    pub url: String,
    /// Model name (`STARBOORU_AI_MODEL`).
    pub model: String,
    /// Whether an API key is stored or set in the environment.
    pub key_set: bool,
    /// Tag new uploads automatically (`STARBOORU_AI_AUTOTAG`).
    pub autotag: bool,
}

/// `PUT /admin/ai`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSettingsUpdate {
    pub url: String,
    pub model: String,
    /// `None` keeps the current key, `Some("")` clears it, anything else
    /// replaces it.
    #[serde(default)]
    pub key: Option<String>,
    pub autotag: bool,
}

/// Checks an admin's AI settings. Returns the trimmed URL and model.
pub fn clean_ai_settings(update: &AiSettingsUpdate) -> Result<(String, String), String> {
    let url = update.url.trim().trim_end_matches('/').to_owned();
    let model = update.model.trim().to_owned();
    for (what, value) in [("address", &url), ("model", &model)] {
        if value.chars().count() > MAX_AI_FIELD_CHARS {
            return Err(format!("the {what} is too long"));
        }
        if value.chars().any(char::is_control) {
            return Err(format!("the {what} contains a control character"));
        }
    }
    if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("the address must start with http:// or https://".into());
    }
    if !url.is_empty() && model.is_empty() {
        return Err("enter a model name".into());
    }
    if let Some(key) = &update.key
        && (key.chars().count() > MAX_AI_FIELD_CHARS || key.chars().any(char::is_control))
    {
        return Err("the key is not valid".into());
    }
    Ok((url, model))
}

/// One suggested tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSuggestion {
    pub name: String,
    pub category: TagCategory,
    /// The tag (or an alias of it) already exists on the server.
    pub exists: bool,
}

/// `POST /posts/{id}/suggest-tags`. Nothing is changed on the post.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSuggestions {
    pub tags: Vec<TagSuggestion>,
    pub rating: Option<Rating>,
}

/// What a model answered, cleaned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedSuggestions {
    /// Valid, normalised, de-duplicated names with the category the model
    /// gave, if any. At most [`MAX_SUGGESTIONS`].
    pub tags: Vec<(String, Option<TagCategory>)>,
    pub rating: Option<Rating>,
}

fn category(s: &str) -> Option<TagCategory> {
    match s.trim().to_ascii_lowercase().as_str() {
        "general" => Some(TagCategory::General),
        "artist" => Some(TagCategory::Artist),
        "character" => Some(TagCategory::Character),
        "copyright" | "series" => Some(TagCategory::Copyright),
        "meta" => Some(TagCategory::Meta),
        _ => None,
    }
}

/// The JSON object in `text`: code fences and prose around it are ignored.
fn json_object(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let slice = text.get(start..=end)?;
    serde_json::from_str(slice).ok()
}

/// Reads a model's answer leniently. Accepts the JSON shape the prompt asks
/// for, tags given as plain strings, or (as a fallback) a bare list of
/// names separated by commas or whitespace.
pub fn parse_suggestions(text: &str) -> ParsedSuggestions {
    let mut out = ParsedSuggestions::default();
    let push = |raw: &str, cat: Option<TagCategory>, out: &mut ParsedSuggestions| {
        if out.tags.len() >= MAX_SUGGESTIONS {
            return;
        }
        let raw = raw
            .trim()
            .trim_matches(|c| c == '"' || c == '\'' || c == '`');
        // A real tag never has more than a few words.
        if raw.split_whitespace().count() > 6 {
            return;
        }
        if let Ok(name) = normalize_tag(raw)
            && !name.contains(':')
            && !out.tags.iter().any(|(n, _)| *n == name)
        {
            out.tags.push((name, cat));
        }
    };
    if let Some(value) = json_object(text) {
        if let Some(tags) = value.get("tags").and_then(|t| t.as_array()) {
            for tag in tags {
                match tag {
                    serde_json::Value::String(s) => push(s, None, &mut out),
                    serde_json::Value::Object(o) => {
                        if let Some(name) = o.get("name").and_then(|n| n.as_str()) {
                            let cat = o
                                .get("category")
                                .and_then(|c| c.as_str())
                                .and_then(category);
                            push(name, cat, &mut out);
                        }
                    }
                    _ => {}
                }
            }
        }
        out.rating = value
            .get("rating")
            .and_then(|r| r.as_str())
            .and_then(|r| Rating::parse(r.trim()));
        return out;
    }
    let body: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join(",");
    for raw in body.split([',', '\n']) {
        push(raw, None, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(p: &ParsedSuggestions) -> Vec<&str> {
        p.tags.iter().map(|(n, _)| n.as_str()).collect()
    }

    #[test]
    fn reads_the_requested_shape() {
        let p = parse_suggestions(
            r#"{"tags":[{"name":"Long Hair","category":"general"},
                {"name":"hatsune_miku","category":"character"}],"rating":"sensitive"}"#,
        );
        assert_eq!(names(&p), vec!["long_hair", "hatsune_miku"]);
        assert_eq!(p.tags[1].1, Some(TagCategory::Character));
        assert_eq!(p.rating, Some(Rating::Sensitive));
    }

    #[test]
    fn strips_fences_and_prose() {
        let p = parse_suggestions(
            "Sure! Here you go:\n```json\n{\"tags\": [\"sky\", \"cloud\"], \"rating\": \"g\"}\n```\nEnjoy.",
        );
        assert_eq!(names(&p), vec!["sky", "cloud"]);
        assert_eq!(p.rating, Some(Rating::General));
    }

    #[test]
    fn drops_invalid_and_duplicates() {
        let p = parse_suggestions(
            r#"{"tags":["-bad","sky","SKY","", 5, {"name":"x"}, {"category":"meta"},
                "this is a whole sentence that is not a tag at all","rating:explicit"],
                "rating":"weird"}"#,
        );
        assert_eq!(names(&p), vec!["sky", "x"]);
        assert_eq!(p.rating, None);
        assert_eq!(p.tags[1].1, None);
    }

    #[test]
    fn caps_the_count() {
        let tags: Vec<String> = (0..100).map(|i| format!("\"t{i}\"")).collect();
        let p = parse_suggestions(&format!("{{\"tags\":[{}]}}", tags.join(",")));
        assert_eq!(p.tags.len(), MAX_SUGGESTIONS);
    }

    #[test]
    fn falls_back_to_a_plain_list() {
        let p = parse_suggestions("```\n1girl, blue sky, outdoors\n```");
        assert_eq!(names(&p), vec!["1girl", "blue_sky", "outdoors"]);
        assert_eq!(parse_suggestions("").tags.len(), 0);
        assert_eq!(parse_suggestions("{ not json").tags.len(), 1);
    }

    #[test]
    fn checks_settings() {
        let mut u = AiSettingsUpdate {
            url: " http://ollama:11434/v1/ ".into(),
            model: "llava".into(),
            key: None,
            autotag: false,
        };
        assert_eq!(
            clean_ai_settings(&u),
            Ok(("http://ollama:11434/v1".into(), "llava".into()))
        );
        u.url = "ftp://x".into();
        assert!(clean_ai_settings(&u).is_err());
        u.url = "https://x".into();
        u.model = " ".into();
        assert!(clean_ai_settings(&u).is_err());
        u.url = String::new();
        assert!(clean_ai_settings(&u).is_ok());
    }
}
