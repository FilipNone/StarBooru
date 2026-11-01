//! szurubooru: `api/posts/` and `api/tags/`, walked by offset. Pools come on the
//! posts themselves, so there is no groups phase.

use serde_json::Value;
use starbooru_core::Rating;

use super::{
    Answer, Call, Item, PAGE, Page, Rule, Site, category_tag, clean_source, counter, group_tag,
    json, number, parse_date, tags, text,
};

const POST_FIELDS: &str =
    "id,creationTime,safety,source,type,contentUrl,tags,pools,ownScore,ownFavorite";

pub(super) fn call(site: &Site, step: &str, rest: &str) -> Option<Call> {
    let offset = counter(rest, 0).to_string();
    let (path, query, fields) = match step {
        "tags" => ("api/tags/", "sort:name,asc", "names,implications"),
        "posts" => ("api/posts/", "sort:id,asc", POST_FIELDS),
        _ => return None,
    };
    let url = site.api(
        path,
        [
            ("offset", offset),
            ("limit", PAGE.to_string()),
            ("query", query.to_owned()),
            ("fields", fields.to_owned()),
        ],
    )?;
    Some(Call { url, body: None })
}

pub(super) fn parse(site: &Site, step: &str, rest: &str, body: &[u8]) -> Option<Page> {
    let value = json(body)?;
    let list = value.get("results")?.as_array()?;
    let mut answer = Answer {
        total: value
            .get("total")
            .and_then(Value::as_u64)
            .filter(|_| step == "posts"),
        ..Answer::default()
    };
    match step {
        "tags" => list.iter().for_each(|tag| read_tag(tag, &mut answer)),
        "posts" => {
            for post in list {
                read_post(site, post, &mut answer);
            }
        }
        _ => return None,
    }
    let offset = counter(rest, 0).saturating_add(list.len() as u64);
    let more = (!list.is_empty()).then(|| offset.to_string());
    Some(Page { answer, more })
}

fn read_tag(tag: &Value, answer: &mut Answer) {
    let mut names = first_names(tag);
    let Some(main) = names.next() else { return };
    for alias in names {
        answer.rules.push(Rule::Alias {
            tag: main.to_owned(),
            alias: alias.to_owned(),
        });
    }
    let implied = tag.get("implications").and_then(Value::as_array);
    for implied in implied.into_iter().flatten() {
        if let Some(name) = first_names(implied).next() {
            answer.rules.push(Rule::Implies {
                tag: main.to_owned(),
                implied: name.to_owned(),
            });
        }
    }
}

fn first_names(tag: &Value) -> impl Iterator<Item = &str> {
    tag.get("names")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn read_post(site: &Site, post: &Value, answer: &mut Answer) {
    let Some(id) = post.get("id").and_then(number) else {
        return;
    };
    let remote_id = id.to_string();
    if text(post, "type") == Some("youtube") {
        let reason = "it is a YouTube video, not a file".to_owned();
        answer.skipped.push((remote_id, reason));
        return;
    }
    let Some(file_url) = text(post, "contentUrl").and_then(|raw| site.file(raw)) else {
        let reason = "the site gives no address for its file".to_owned();
        answer.skipped.push((remote_id, reason));
        return;
    };
    let mut raw: Vec<String> = Vec::new();
    for tag in post
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = first_names(tag).next() {
            raw.push(category_tag(
                text(tag, "category").unwrap_or_default(),
                name,
            ));
        }
    }
    for pool in post
        .get("pools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(tag) = first_names(pool)
            .next()
            .and_then(|name| group_tag("pool", name))
        {
            raw.push(tag);
        }
    }
    let liked = post.get("ownFavorite").and_then(Value::as_bool) == Some(true);
    let score = post.get("ownScore").and_then(number).unwrap_or(0);
    answer.items.push(Item {
        remote_id,
        file_url,
        tags: tags(raw.iter().map(String::as_str)),
        rating: text(post, "safety").and_then(rating),
        source: text(post, "source").and_then(clean_source),
        created_at: text(post, "creationTime").and_then(parse_date),
        vote: match (liked, score) {
            (true, _) => 1,
            (false, s) if s > 0 => 1,
            (false, s) if s < 0 => -1,
            _ => 0,
        },
    });
}

fn rating(safety: &str) -> Option<Rating> {
    match safety {
        "safe" => Some(Rating::Safe),
        "sketchy" => Some(Rating::Questionable),
        "unsafe" => Some(Rating::Explicit),
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
        test_site(LibrarySource::Szurubooru, Some("me"), Some("t"))
    }

    #[test]
    fn asks_for_posts_by_offset_oldest_first() {
        let call = site().call(LibraryPhase::Posts, "posts:200").unwrap();
        let url = call.url.as_str();
        assert!(
            url.starts_with("http://booru.test/sub/api/posts/?offset=200&limit=100"),
            "{url}"
        );
        assert!(url.contains("query=sort%3Aid%2Casc"), "{url}");
        assert!(call.body.is_none());
    }

    #[test]
    fn reads_a_page_of_posts() {
        let body = json!({"query": "", "offset": 0, "limit": 100, "total": 3, "results": [
            {
                "id": 7, "creationTime": "2020-01-02T03:04:05Z", "safety": "sketchy",
                "source": "\nhttps://a.b/1\nhttps://c.d/2", "type": "image",
                "contentUrl": "data/posts/7_abc.png",
                "tags": [
                    {"names": ["blue_sky", "sky_blue"], "category": "default"},
                    {"names": ["Some Artist"], "category": "artist"}
                ],
                "pools": [{"id": 1, "names": ["Summer Trip"]}],
                "ownScore": 0, "ownFavorite": true
            },
            {"id": 8, "type": "youtube", "contentUrl": "https://youtube.com/x"},
            {"id": 9, "safety": "unsafe", "type": "video", "contentUrl": "data/posts/9.mp4",
             "ownScore": -1, "ownFavorite": false}
        ]})
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "posts:0", body.as_bytes())
            .unwrap();
        assert_eq!(answer.total, Some(3));
        assert_eq!(answer.next.as_deref(), Some("posts:3"));
        assert_eq!(answer.skipped.len(), 1);
        let first = answer.items.first().unwrap();
        assert_eq!(first.remote_id, "7");
        assert_eq!(
            first.file_url.as_str(),
            "http://booru.test/sub/data/posts/7_abc.png"
        );
        assert_eq!(
            first.tags,
            ["blue_sky", "artist:some_artist", "pool_summer_trip"]
        );
        assert_eq!(first.rating, Some(Rating::Questionable));
        assert_eq!(first.source.as_deref(), Some("https://a.b/1"));
        assert_eq!(first.vote, 1);
        let last = answer.items.last().unwrap();
        assert_eq!((last.rating, last.vote), (Some(Rating::Explicit), -1));
    }

    #[test]
    fn reads_aliases_and_implications() {
        let body = json!({"total": 1, "results": [{
            "names": ["sky", "heavens"],
            "implications": [{"names": ["outdoors", "outside"], "category": "default"}]
        }]})
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Rules, "", body.as_bytes())
            .unwrap();
        assert_eq!(answer.total, None, "the tag count isn't the post count");
        assert_eq!(
            answer.rules,
            [
                Rule::Alias {
                    tag: "sky".into(),
                    alias: "heavens".into()
                },
                Rule::Implies {
                    tag: "sky".into(),
                    implied: "outdoors".into()
                },
            ]
        );
    }

    #[test]
    fn an_empty_page_ends_and_a_login_page_is_refused() {
        let body = json!({"total": 0, "results": []}).to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "posts:5", body.as_bytes())
            .unwrap();
        assert_eq!(answer.next, None);
        assert!(site().parse(LibraryPhase::Posts, "", b"<html>").is_none());
        assert!(site().parse(LibraryPhase::Posts, "", b"{}").is_none());
    }
}
