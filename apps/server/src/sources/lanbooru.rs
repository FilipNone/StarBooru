//! LANBooru: an open search API, without aliases, implications or albums. Order
//! is asked for in the search itself.

use serde_json::Value;
use starbooru_core::Rating;

use super::{
    Answer, Call, Item, PAGE, Page, Site, clean_source, counter, json, number, parse_date, tags,
    text,
};

pub(super) fn call(site: &Site, step: &str, rest: &str) -> Option<Call> {
    if step != "posts" {
        return None;
    }
    let url = site.api(
        "api/search",
        [
            ("q", "sort:id_asc".to_owned()),
            ("page", counter(rest, 1).to_string()),
            ("limit", PAGE.to_string()),
        ],
    )?;
    Some(Call { url, body: None })
}

pub(super) fn parse(site: &Site, step: &str, rest: &str, body: &[u8]) -> Option<Page> {
    if step != "posts" {
        return None;
    }
    let value = json(body)?;
    let list = value.get("images")?.as_array()?;
    let mut answer = Answer {
        total: value.get("total").and_then(Value::as_u64),
        ..Answer::default()
    };
    for image in list {
        read_image(site, image, &mut answer);
    }
    let more = (!list.is_empty()).then(|| counter(rest, 1).saturating_add(1).to_string());
    Some(Page { answer, more })
}

fn read_image(site: &Site, image: &Value, answer: &mut Answer) {
    let Some(id) = image.get("id").and_then(number) else {
        return;
    };
    let Some(file_url) = site.api(&format!("api/image/{id}/file"), []) else {
        return;
    };
    let raw_tags = image.get("tags").and_then(Value::as_array);
    let mut raw: Vec<String> = raw_tags
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if let Some(artist) = text(image, "artist").filter(|name| !name.trim().is_empty()) {
        raw.push(format!("artist:{artist}"));
    }
    answer.items.push(Item {
        remote_id: id.to_string(),
        file_url,
        tags: tags(raw.iter().map(String::as_str)),
        rating: image.get("rating").and_then(number).and_then(rating),
        source: text(image, "source").and_then(clean_source),
        created_at: text(image, "created_at").and_then(parse_date),
        vote: 0,
    });
}

fn rating(code: i64) -> Option<Rating> {
    match code {
        1 => Some(Rating::Safe),
        2 => Some(Rating::Questionable),
        3 => Some(Rating::Explicit),
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
        test_site(LibrarySource::Lanbooru, None, None)
    }

    #[test]
    fn asks_by_id_through_the_search() {
        let call = site().call(LibraryPhase::Posts, "posts:2").unwrap();
        assert_eq!(
            call.url.as_str(),
            "http://booru.test/sub/api/search?q=sort%3Aid_asc&page=2&limit=100"
        );
    }

    #[test]
    fn reads_images() {
        let body = json!({"total": 2, "page": 1, "limit": 100, "total_pages": 1, "images": [
            {"id": 3, "rating": 3, "artist": "Some One", "source": "https://a.b/1",
             "created_at": "2020-01-02T03:04:05.000Z", "tags": ["blue_sky"]},
            {"id": 4, "rating": 0, "artist": null, "source": null, "tags": []}
        ]})
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "", body.as_bytes())
            .unwrap();
        assert_eq!(answer.total, Some(2));
        assert_eq!(answer.next.as_deref(), Some("posts:2"));
        let first = answer.items.first().unwrap();
        assert_eq!(
            first.file_url.as_str(),
            "http://booru.test/sub/api/image/3/file"
        );
        assert_eq!(first.tags, ["blue_sky", "artist:some_one"]);
        assert_eq!(first.rating, Some(Rating::Explicit));
        assert_eq!(answer.items.last().unwrap().rating, None);
    }

    #[test]
    fn ends_on_an_empty_page_and_refuses_other_pages() {
        let body = json!({"total": 0, "images": []}).to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "posts:9", body.as_bytes())
            .unwrap();
        assert_eq!(answer.next, None);
        assert!(site().parse(LibraryPhase::Posts, "", b"<html>").is_none());
    }
}
