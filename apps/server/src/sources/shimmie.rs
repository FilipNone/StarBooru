//! Shimmie2: posts through its Ouroboros API extension, aliases and auto-tags
//! through their CSV exports. GraphQL can't be used: its `posts` query is only
//! in the schema once the search class has been loaded, which a GraphQL request
//! doesn't do, and it has no field for the rating anyway.
//!
//! `rating=*` gets past the account's default filter, but with ratings turned
//! off it searches for a tag of that name and finds nothing. An empty first page
//! so goes on with `plain:`, the same walk without that term.

use chrono::DateTime;
use serde_json::Value;
use starbooru_core::Rating;

use super::{
    Answer, Call, Item, PAGE, Page, Rule, Site, clean_source, counter, json, number, tags, text,
};

/// The walk without the rating term, where ratings can't be trusted: with them
/// off, the API still says every post is `q`.
const PLAIN: &str = "plain:";

pub(super) fn call(site: &Site, step: &str, rest: &str) -> Option<Call> {
    let key = site
        .secret
        .as_deref()
        .map(|key| ("api_key", key.to_owned()));
    match step {
        "aliases" => export(site, "alias/export/aliases.csv", key),
        "autotag" => export(site, "auto_tag/export/auto_tag.csv", key),
        "posts" => {
            let (plain, after) = walk_of(rest);
            let term = if plain { "" } else { " rating=*" };
            let mut pairs = vec![
                ("q", "post/index.json".to_owned()),
                ("limit", PAGE.to_string()),
                ("page", "1".to_owned()),
                ("tags", format!("id>{after} order=id_asc{term}")),
            ];
            pairs.extend(key);
            let url = site.api("index.php", pairs)?;
            Some(Call { url, body: None })
        }
        _ => None,
    }
}

/// Whether this is the walk without ratings, and the last post read.
fn walk_of(rest: &str) -> (bool, u64) {
    match rest.strip_prefix(PLAIN) {
        Some(after) => (true, counter(after, 0)),
        None => (false, counter(rest, 0)),
    }
}

fn export(site: &Site, page: &str, key: Option<(&'static str, String)>) -> Option<Call> {
    let mut pairs = vec![("q", page.to_owned())];
    pairs.extend(key);
    let url = site.api("index.php", pairs)?;
    Some(Call { url, body: None })
}

pub(super) fn parse(site: &Site, step: &str, rest: &str, body: &[u8]) -> Option<Page> {
    let mut answer = Answer::default();
    match step {
        "aliases" => {
            for (old, new) in csv_pairs(body)? {
                // One old tag can stand for several here; an alias can't.
                if !old.contains(' ') && !new.contains(' ') {
                    answer.rules.push(Rule::Alias {
                        tag: new,
                        alias: old,
                    });
                }
            }
            Some(Page { answer, more: None })
        }
        "autotag" => {
            for (tag, implied) in csv_pairs(body)? {
                if tag.contains(' ') {
                    continue;
                }
                for implied in implied.split_whitespace() {
                    answer.rules.push(Rule::Implies {
                        tag: tag.clone(),
                        implied: implied.to_owned(),
                    });
                }
            }
            Some(Page { answer, more: None })
        }
        "posts" => read_posts(site, rest, body),
        _ => None,
    }
}

fn read_posts(site: &Site, rest: &str, body: &[u8]) -> Option<Page> {
    let value = json(body)?;
    let list = value.as_array()?;
    let (plain, _) = walk_of(rest);
    let mut answer = Answer::default();
    let mut last = None;
    for post in list {
        let Some(id) = post.get("id").and_then(number) else {
            continue;
        };
        last = Some(id);
        read_post(site, id, post, plain, &mut answer);
    }
    let prefix = if plain { PLAIN } else { "" };
    let more = match last {
        Some(id) => Some(format!("{prefix}{id}")),
        // Nothing with ratings at all: they may be turned off.
        None if rest.is_empty() => Some(format!("{PLAIN}0")),
        None => None,
    };
    Some(Page { answer, more })
}

fn read_post(site: &Site, id: i64, post: &Value, plain: bool, answer: &mut Answer) {
    let remote_id = id.to_string();
    let Some(file_url) = text(post, "file_url").and_then(|raw| file_of(site, raw)) else {
        let reason = "the site gives no address for its file".to_owned();
        answer.skipped.push((remote_id, reason));
        return;
    };
    let raw = text(post, "tags").unwrap_or_default();
    answer.items.push(Item {
        remote_id,
        file_url,
        // Categories are part of the name there too (`artist:someone`).
        tags: tags(raw.split_whitespace()),
        rating: if plain { None } else { rating(post) },
        source: text(post, "source").and_then(clean_source),
        created_at: post
            .get("created_at")
            .and_then(|at| at.get("s"))
            .and_then(number)
            .and_then(|seconds| DateTime::from_timestamp(seconds, 0)),
        vote: 0,
    });
}

/// The API builds whole addresses from what the web server told PHP, which can
/// lack the port or name an inside host, so its own files go by the address
/// the site was reached at.
fn file_of(site: &Site, raw: &str) -> Option<url::Url> {
    let url = site.file(raw)?;
    let path = url.path();
    match path.find("_images/") {
        Some(at) => site.base.join(path.get(at..)?).ok(),
        None => Some(url),
    }
}

fn rating(post: &Value) -> Option<Rating> {
    match text(post, "rating")? {
        "s" => Some(Rating::General),
        "q" => Some(Rating::Questionable),
        "e" => Some(Rating::Explicit),
        _ => None,
    }
}

/// The rows of an export (`"one","two"`). An export can be empty, but a page in
/// any other format is refused, so a login page isn't read as no rules at all.
fn csv_pairs(body: &[u8]) -> Option<Vec<(String, String)>> {
    let body = std::str::from_utf8(body).ok()?;
    let mut rows = Vec::new();
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let inner = line.strip_prefix('"')?.strip_suffix('"')?;
        let (first, second) = inner.split_once("\",\"")?;
        rows.push((first.to_owned(), second.to_owned()));
    }
    Some(rows)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use starbooru_core::{LibraryPhase, LibrarySource};

    use super::*;
    use crate::sources::test_site;

    fn site() -> Site {
        test_site(LibrarySource::Shimmie2, None, Some("k"))
    }

    fn post(id: i64, rating: &str) -> Value {
        json!({
            "id": id, "rating": rating, "tags": "blue_sky artist:someone",
            "source": "https://a.b/1", "created_at": {"n": 1, "s": 1577934245, "json_class": "Time"},
            "file_url": format!("http://inside/_images/ab{id}/{id}%20-%20blue_sky.png")
        })
    }

    #[test]
    fn asks_for_every_rating_past_the_last_post() {
        let call = site().call(LibraryPhase::Posts, "posts:41").unwrap();
        assert_eq!(
            call.url.as_str(),
            "http://booru.test/sub/index.php?q=post%2Findex.json&limit=100&page=1\
             &tags=id%3E41+order%3Did_asc+rating%3D*&api_key=k"
        );
        assert!(call.body.is_none());
    }

    #[test]
    fn reads_a_page_in_order_with_files_at_the_address_it_was_reached_at() {
        let body = json!([post(2, "q"), post(5, "s"), post(9, "?")]).to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "", body.as_bytes())
            .unwrap();
        let seen: Vec<(&str, Option<Rating>)> = answer
            .items
            .iter()
            .map(|item| (item.remote_id.as_str(), item.rating))
            .collect();
        assert_eq!(
            seen,
            [
                ("2", Some(Rating::Questionable)),
                ("5", Some(Rating::General)),
                ("9", None)
            ]
        );
        assert_eq!(answer.next.as_deref(), Some("posts:9"));
        let first = answer.items.first().unwrap();
        assert_eq!(first.tags, ["blue_sky", "artist:someone"]);
        assert_eq!(
            first.file_url.as_str(),
            "http://booru.test/sub/_images/ab2/2%20-%20blue_sky.png"
        );
        assert_eq!(
            first.created_at.unwrap().to_rfc3339(),
            "2020-01-02T03:04:05+00:00"
        );
    }

    #[test]
    fn a_file_elsewhere_stays_there() {
        let mut one = post(1, "s");
        one["file_url"] = json!("https://cdn.test/files/1.png");
        let body = json!([one]).to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "", body.as_bytes())
            .unwrap();
        assert_eq!(
            answer.items.first().unwrap().file_url.as_str(),
            "https://cdn.test/files/1.png"
        );
    }

    #[test]
    fn an_empty_first_page_tries_again_without_ratings() {
        let answer = site().parse(LibraryPhase::Posts, "", b"[]").unwrap();
        assert_eq!(answer.next.as_deref(), Some("posts:plain:0"));
        let call = site().call(LibraryPhase::Posts, "posts:plain:0").unwrap();
        assert!(!call.url.as_str().contains("rating"), "{}", call.url);

        // With ratings off, the API says `q` for everything.
        let body = json!([post(4, "q")]).to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "posts:plain:0", body.as_bytes())
            .unwrap();
        assert_eq!(answer.items.first().unwrap().rating, None);
        assert_eq!(answer.next.as_deref(), Some("posts:plain:4"));

        let answer = site()
            .parse(LibraryPhase::Posts, "posts:plain:4", b"[]")
            .unwrap();
        assert_eq!(answer.next, None);
    }

    #[test]
    fn an_empty_later_page_ends_the_walk() {
        let answer = site().parse(LibraryPhase::Posts, "posts:9", b"[]").unwrap();
        assert_eq!(answer.next, None);
    }

    #[test]
    fn reads_the_exports_and_refuses_other_pages() {
        let aliases = b"\"old\",\"new\"\n\"multi\",\"two words\"\n";
        let answer = site()
            .parse(LibraryPhase::Rules, "aliases:", aliases)
            .unwrap();
        assert_eq!(
            answer.rules,
            [Rule::Alias {
                tag: "new".into(),
                alias: "old".into()
            }]
        );
        assert_eq!(answer.next.as_deref(), Some("autotag:"));

        let auto = b"\"tag\",\"one two\"\n";
        let answer = site().parse(LibraryPhase::Rules, "autotag:", auto).unwrap();
        assert_eq!(answer.rules.len(), 2);
        assert_eq!(answer.next, None);

        assert!(
            site()
                .parse(LibraryPhase::Rules, "aliases:", b"")
                .unwrap()
                .rules
                .is_empty()
        );
        assert!(
            site()
                .parse(LibraryPhase::Rules, "aliases:", b"<html></html>")
                .is_none()
        );
        assert!(
            site()
                .parse(LibraryPhase::Posts, "", b"<html></html>")
                .is_none()
        );
    }

    #[test]
    fn the_export_asks_for_the_csv_page() {
        let call = site().call(LibraryPhase::Rules, "").unwrap();
        assert_eq!(
            call.url.as_str(),
            "http://booru.test/sub/index.php?q=alias%2Fexport%2Faliases.csv&api_key=k"
        );
    }
}
