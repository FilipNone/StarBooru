//! Monbooru: a server holds several galleries, each with its own posts, tags and
//! numbering, so everything is walked a gallery at a time. A post's rating is a
//! tag in the `rating` category. Implications are asked for tag by tag; aliases
//! can't be read, its API not saying which tag an alias stands for.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use starbooru_core::Rating;

use super::{
    Answer, Call, Item, PAGE, Page, Rule, Site, category_tag, clean_source, group_tag, json,
    number, parse_date, tags, text,
};

/// Tags asked for per page.
const TAG_PAGE: u32 = 500;

/// Where a walk is: the galleries left, the page of the first one and, for
/// implications, the tags whose answer is still to come.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Walk {
    galleries: Vec<String>,
    page: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    queue: Vec<(i64, String)>,
}

impl Walk {
    fn at(rest: &str) -> Option<Self> {
        serde_json::from_str(rest).ok()
    }

    /// The next gallery; `None` when that was the last.
    fn next_gallery(mut self) -> Option<String> {
        if self.galleries.is_empty() {
            return None;
        }
        self.galleries.remove(0);
        self.page = 1;
        self.queue.clear();
        if self.galleries.is_empty() {
            return None;
        }
        serde_json::to_string(&self).ok()
    }

    fn encoded(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

pub(super) fn call(site: &Site, step: &str, rest: &str) -> Option<Call> {
    let Some(walk) = Walk::at(rest) else {
        let url = site.api("api/v1/galleries", [])?;
        return Some(Call { url, body: None });
    };
    let gallery = walk.galleries.first()?.clone();
    let url = match step {
        "images" => site.api(
            "api/v1/images/search",
            [
                ("q", String::new()),
                ("sort", "newest".to_owned()),
                ("order", "asc".to_owned()),
                ("page", walk.page.to_string()),
                ("limit", PAGE.to_string()),
                ("gallery", gallery),
            ],
        )?,
        "implications" => match walk.queue.first() {
            Some((id, _)) => site.api(
                &format!("api/v1/tags/{id}/implications"),
                [("gallery", gallery)],
            )?,
            None => site.api(
                "api/v1/tags",
                [
                    ("page", walk.page.to_string()),
                    ("limit", TAG_PAGE.to_string()),
                    ("type", "tag".to_owned()),
                    ("gallery", gallery),
                ],
            )?,
        },
        _ => return None,
    };
    Some(Call { url, body: None })
}

pub(super) fn parse(site: &Site, step: &str, rest: &str, body: &[u8]) -> Option<Page> {
    let value = json(body)?;
    let mut answer = Answer::default();
    let Some(walk) = Walk::at(rest) else {
        return read_galleries(step, &value, answer);
    };
    let gallery = walk.galleries.first()?.clone();
    answer.scope = gallery.clone();
    match step {
        "images" => {
            let list = value.get("results")?.as_array()?;
            for image in list {
                read_image(site, &gallery, image, &mut answer);
            }
            let more = if list.is_empty() {
                walk.next_gallery()
            } else {
                let mut walk = walk;
                walk.page = walk.page.saturating_add(1);
                walk.encoded()
            };
            Some(Page { answer, more })
        }
        "implications" => read_implications(walk, &gallery, &value, answer),
        _ => None,
    }
}

fn read_galleries(step: &str, value: &Value, mut answer: Answer) -> Option<Page> {
    let list = value.as_array()?;
    let galleries: Vec<String> = list
        .iter()
        .filter_map(|gallery| text(gallery, "name").map(str::to_owned))
        .collect();
    if step == "images" {
        answer.total = Some(
            list.iter()
                .filter_map(|gallery| gallery.get("images")?.as_u64())
                .fold(0, u64::saturating_add),
        );
    }
    answer.scope = "galleries".to_owned();
    let more = (!galleries.is_empty())
        .then(|| {
            Walk {
                galleries,
                page: 1,
                queue: Vec::new(),
            }
            .encoded()
        })
        .flatten();
    Some(Page { answer, more })
}

fn read_implications(
    mut walk: Walk,
    gallery: &str,
    value: &Value,
    mut answer: Answer,
) -> Option<Page> {
    if let Some((id, tag)) = walk.queue.first().cloned() {
        answer.scope = format!("{gallery}/{id}");
        for rule in value.as_array()? {
            if let Some(implied) = text(rule, "implied_name") {
                answer.rules.push(Rule::Implies {
                    tag: tag.clone(),
                    implied: implied.to_owned(),
                });
            }
        }
        walk.queue.remove(0);
        return Some(Page {
            answer,
            more: walk.encoded(),
        });
    }
    let list = value.get("results")?.as_array()?;
    if list.is_empty() {
        let more = walk.next_gallery();
        return Some(Page { answer, more });
    }
    walk.page = walk.page.saturating_add(1);
    walk.queue = list
        .iter()
        .filter(|tag| text(tag, "category") != Some("rating"))
        .filter_map(|tag| {
            let id = tag.get("id").and_then(number)?;
            Some((id, text(tag, "name")?.to_owned()))
        })
        .collect();
    Some(Page {
        answer,
        more: walk.encoded(),
    })
}

fn read_image(site: &Site, gallery: &str, image: &Value, answer: &mut Answer) {
    let Some(id) = image.get("id").and_then(number) else {
        return;
    };
    let remote_id = format!("{gallery}/{id}");
    if image.get("is_missing").and_then(Value::as_bool) == Some(true) {
        let reason = "its file is missing on that server".to_owned();
        answer.skipped.push((remote_id, reason));
        return;
    }
    let Some(file_url) = site.api(
        &format!("api/v1/images/{id}/file"),
        [("gallery", gallery.to_owned())],
    ) else {
        return;
    };
    let mut raw = Vec::new();
    let mut found = None;
    for tag in image
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = text(tag, "name") else {
            continue;
        };
        let category = text(tag, "category").unwrap_or_default();
        if category == "rating" {
            found = found.or_else(|| rating(name));
        } else {
            raw.push(category_tag(category, name));
        }
    }
    let collections = image.get("collections").and_then(Value::as_array);
    for collection in collections.into_iter().flatten() {
        if let Some(tag) = text(collection, "name").and_then(|name| group_tag("collection", name)) {
            raw.push(tag);
        }
    }
    if let Some(tag) = text(image, "collection").and_then(|name| group_tag("collection", name)) {
        raw.push(tag);
    }
    answer.items.push(Item {
        remote_id,
        file_url,
        tags: tags(raw.iter().map(String::as_str)),
        rating: found,
        source: source(image),
        created_at: text(image, "ingested_at").and_then(parse_date),
        vote: i16::from(image.get("is_favorited").and_then(Value::as_bool) == Some(true)),
    });
}

/// Where the post came from: its `source` field only names a kind of origin.
fn source(image: &Value) -> Option<String> {
    let listed = image
        .get("sources")
        .and_then(Value::as_array)
        .and_then(|sources| sources.first())
        .and_then(|first| text(first, "url"));
    [text(image, "url"), listed, text(image, "original_source")]
        .into_iter()
        .flatten()
        .find_map(clean_source)
}

fn rating(name: &str) -> Option<Rating> {
    match name.to_ascii_lowercase().as_str() {
        "general" | "safe" => Some(Rating::Safe),
        "sensitive" => Some(Rating::Sensitive),
        "questionable" => Some(Rating::Questionable),
        "explicit" => Some(Rating::Explicit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use starbooru_core::{LibraryPhase, LibrarySource};

    use super::*;
    use crate::sources::test_site;

    fn site() -> Site {
        test_site(LibrarySource::Monbooru, None, Some("t"))
    }

    fn at(galleries: &[&str], page: u64) -> String {
        let walk = Walk {
            galleries: galleries.iter().map(|g| (*g).to_owned()).collect(),
            page,
            queue: Vec::new(),
        };
        format!("images:{}", walk.encoded().unwrap())
    }

    #[test]
    fn lists_galleries_first_and_adds_up_their_posts() {
        let site = site();
        let call = site.call(LibraryPhase::Posts, "").unwrap();
        assert_eq!(call.url.as_str(), "http://booru.test/sub/api/v1/galleries");
        let body = json!([
            {"name": "main", "images": 3, "tags": 1, "active": true},
            {"name": "other", "images": 4, "tags": 1, "active": false}
        ])
        .to_string();
        let answer = site
            .parse(LibraryPhase::Posts, "", body.as_bytes())
            .unwrap();
        assert_eq!(answer.total, Some(7));
        let next = answer.next.unwrap();
        assert_eq!(next, at(&["main", "other"], 1));
        let call = site.call(LibraryPhase::Posts, &next).unwrap();
        assert_eq!(
            call.url.as_str(),
            "http://booru.test/sub/api/v1/images/search?q=&sort=newest&order=asc&page=1&limit=100&gallery=main"
        );
    }

    #[test]
    fn reads_an_image_with_its_rating_tag() {
        let body = json!({"page": 1, "limit": 100, "total": 2, "results": [{
            "id": 5, "is_missing": false, "is_favorited": true, "source": "web",
            "url": "", "original_source": "https://a.b/1",
            "ingested_at": "2020-01-02T03:04:05Z", "collection": "A Set",
            "collections": [{"name": "Other Set", "order": 1}],
            "tags": [
                {"name": "explicit", "category": "rating"},
                {"name": "blue_sky", "category": "general"},
                {"name": "someone", "category": "artist"}
            ]
        }, {"id": 6, "is_missing": true}]})
        .to_string();
        let position = at(&["main"], 1);
        let answer = site()
            .parse(LibraryPhase::Posts, &position, body.as_bytes())
            .unwrap();
        assert_eq!(answer.scope, "main");
        assert_eq!(answer.skipped.first().map(|s| s.0.as_str()), Some("main/6"));
        let image = answer.items.first().unwrap();
        assert_eq!(image.remote_id, "main/5");
        assert_eq!(
            image.file_url.as_str(),
            "http://booru.test/sub/api/v1/images/5/file?gallery=main"
        );
        assert_eq!(image.rating, Some(Rating::Explicit));
        assert_eq!(image.vote, 1);
        assert_eq!(image.source.as_deref(), Some("https://a.b/1"));
        assert_eq!(
            image.tags,
            [
                "blue_sky",
                "artist:someone",
                "collection_other_set",
                "collection_a_set"
            ]
        );
        assert_eq!(answer.next.unwrap(), at(&["main"], 2));
    }

    #[test]
    fn an_empty_page_moves_to_the_next_gallery_and_then_ends() {
        let body = json!({"results": []}).to_string();
        let position = at(&["main", "other"], 4);
        let answer = site()
            .parse(LibraryPhase::Posts, &position, body.as_bytes())
            .unwrap();
        assert_eq!(answer.next.unwrap(), at(&["other"], 1));
        let answer = site()
            .parse(LibraryPhase::Posts, &at(&["other"], 2), body.as_bytes())
            .unwrap();
        assert_eq!(answer.next, None);
    }

    #[test]
    fn reads_implications_tag_by_tag() {
        let site = site();
        let phase = LibraryPhase::Rules;
        let walk = Walk {
            galleries: vec!["main".into()],
            page: 1,
            queue: Vec::new(),
        };
        let position = format!("implications:{}", walk.encoded().unwrap());
        let call = site.call(phase, &position).unwrap();
        assert!(
            call.url
                .as_str()
                .contains("api/v1/tags?page=1&limit=500&type=tag&gallery=main")
        );
        let body = json!({"results": [
            {"id": 7, "name": "sky", "category": "general"},
            {"id": 8, "name": "safe", "category": "rating"}
        ]})
        .to_string();
        let answer = site.parse(phase, &position, body.as_bytes()).unwrap();
        let at_tag = answer.next.unwrap();
        let call = site.call(phase, &at_tag).unwrap();
        assert!(
            call.url
                .as_str()
                .contains("api/v1/tags/7/implications?gallery=main")
        );
        let body = json!([{"parent_id": 7, "implied_id": 9, "implied_name": "outdoors", "implied_category": "general"}]).to_string();
        let answer = site.parse(phase, &at_tag, body.as_bytes()).unwrap();
        assert_eq!(
            answer.rules,
            [Rule::Implies {
                tag: "sky".into(),
                implied: "outdoors".into()
            }]
        );
        assert_eq!(answer.scope, "main/7");
        // The queue is done: on to the next page of tags.
        let next = answer.next.unwrap();
        assert!(
            site.call(phase, &next)
                .unwrap()
                .url
                .as_str()
                .contains("tags?page=2")
        );
    }

    #[test]
    fn refuses_what_is_not_its_api() {
        assert!(site().parse(LibraryPhase::Posts, "", b"<html>").is_none());
        assert!(
            site()
                .parse(LibraryPhase::Posts, "", br#"{"error":"x"}"#)
                .is_none()
        );
        assert!(
            site()
                .parse(LibraryPhase::Posts, &at(&["main"], 1), b"[]")
                .is_none()
        );
    }
}
