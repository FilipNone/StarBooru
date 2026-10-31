//! Danbooru: the `a<id>` pages walk every list by id, whatever its size. Those
//! pages come newest first within the page, so each is sorted here. Pools and
//! favourites are lists of their own, read after the posts.

use serde_json::Value;
use starbooru_core::Rating;

use super::{
    Answer, Call, Group, Item, PAGE, Page, Rule, Site, category_tag, clean_source, group_tag, json,
    number, parse_date, tags, text,
};

const TAG_FIELDS: [(&str, &str); 5] = [
    ("tag_string_general", ""),
    ("tag_string_artist", "artist"),
    ("tag_string_copyright", "copyright"),
    ("tag_string_character", "character"),
    ("tag_string_meta", "meta"),
];

pub(super) fn call(site: &Site, step: &str, rest: &str) -> Option<Call> {
    let after = rest.parse::<u64>().unwrap_or(0);
    let mut query = vec![("limit", PAGE.to_string()), ("page", format!("a{after}"))];
    let path = match step {
        "aliases" => "tag_aliases.json",
        "implications" => "tag_implications.json",
        "posts" => "posts.json",
        "pools" => "pools.json",
        "favs" => {
            let login = site.login.as_deref()?;
            query.push(("tags", format!("fav:{login}")));
            "posts.json"
        }
        _ => return None,
    };
    if matches!(step, "aliases" | "implications") {
        query.push(("search[status]", "active".to_owned()));
    }
    let url = site.api(path, query)?;
    Some(Call { url, body: None })
}

pub(super) fn parse(site: &Site, step: &str, _rest: &str, body: &[u8]) -> Option<Page> {
    let value = json(body)?;
    let list = value.as_array()?;
    let mut answer = Answer::default();
    let mut ordered: Vec<(i64, &Value)> = list
        .iter()
        .filter_map(|entry| Some((entry.get("id").and_then(number)?, entry)))
        .collect();
    ordered.sort_by_key(|(id, _)| *id);
    for (id, entry) in &ordered {
        match step {
            "aliases" => {
                if let Some((tag, alias)) = rule_names(entry) {
                    answer.rules.push(Rule::Alias {
                        tag: alias,
                        alias: tag,
                    });
                }
            }
            "implications" => {
                if let Some((tag, implied)) = rule_names(entry) {
                    answer.rules.push(Rule::Implies { tag, implied });
                }
            }
            "posts" => read_post(site, *id, entry, &mut answer),
            "pools" => read_pool(entry, &mut answer),
            "favs" => answer.likes.push(id.to_string()),
            _ => return None,
        }
    }
    let more = ordered.last().map(|(id, _)| id.to_string());
    Some(Page { answer, more })
}

pub(super) fn count_call(site: &Site) -> Option<Call> {
    let url = site.api("counts/posts.json", [])?;
    Some(Call { url, body: None })
}

pub(super) fn parse_count(body: &[u8]) -> Option<u64> {
    json(body)?.get("counts")?.get("posts")?.as_u64()
}

/// `(antecedent, consequent)` of an alias or implication.
fn rule_names(entry: &Value) -> Option<(String, String)> {
    Some((
        text(entry, "antecedent_name")?.to_owned(),
        text(entry, "consequent_name")?.to_owned(),
    ))
}

fn read_post(site: &Site, id: i64, post: &Value, answer: &mut Answer) {
    let remote_id = id.to_string();
    let Some(file_url) = text(post, "file_url").and_then(|raw| site.file(raw)) else {
        let reason = "the site doesn't show this file to this account".to_owned();
        answer.skipped.push((remote_id, reason));
        return;
    };
    let mut raw = Vec::new();
    for (field, category) in TAG_FIELDS {
        for name in text(post, field).unwrap_or_default().split_whitespace() {
            raw.push(category_tag(category, name));
        }
    }
    answer.items.push(Item {
        remote_id,
        file_url,
        tags: tags(raw.iter().map(String::as_str)),
        rating: text(post, "rating").and_then(rating),
        source: text(post, "source").and_then(clean_source),
        created_at: text(post, "created_at").and_then(parse_date),
        vote: 0,
    });
}

fn read_pool(pool: &Value, answer: &mut Answer) {
    if pool.get("is_deleted").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let Some(tag) = text(pool, "name").and_then(|name| group_tag("pool", name)) else {
        return;
    };
    let remote_ids = pool
        .get("post_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(number)
        .map(|id| id.to_string())
        .collect();
    answer.groups.push(Group { tag, remote_ids });
}

fn rating(code: &str) -> Option<Rating> {
    match code {
        "g" => Some(Rating::General),
        "s" => Some(Rating::Sensitive),
        "q" => Some(Rating::Questionable),
        "e" => Some(Rating::Explicit),
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
        test_site(LibrarySource::Danbooru, Some("me"), Some("key"))
    }

    #[test]
    fn walks_by_id_and_keeps_active_rules_only() {
        let posts = site().call(LibraryPhase::Posts, "posts:15").unwrap();
        assert_eq!(
            posts.url.as_str(),
            "http://booru.test/sub/posts.json?limit=100&page=a15"
        );
        let first = site().call(LibraryPhase::Posts, "").unwrap();
        assert!(first.url.as_str().ends_with("page=a0"));
        let rules = site().call(LibraryPhase::Rules, "").unwrap();
        assert!(rules.url.as_str().contains("tag_aliases.json"));
        assert!(rules.url.as_str().contains("search%5Bstatus%5D=active"));
        let favs = site().call(LibraryPhase::Groups, "favs:").unwrap();
        assert!(favs.url.as_str().contains("tags=fav%3Ame"));
    }

    #[test]
    fn sorts_each_page_and_goes_on_from_the_newest() {
        let body = json!([
            {
                "id": 20, "created_at": "2020-01-02T04:04:05.000+01:00", "rating": "s",
                "source": "https://a.b/1",
                "file_url": "https://cdn.donmai.test/original/a.png",
                "tag_string_general": "blue_sky 1girl",
                "tag_string_artist": "some_artist",
                "tag_string_copyright": "a_show",
                "tag_string_character": "a_hero",
                "tag_string_meta": "highres"
            },
            {"id": 10, "rating": "g"}
        ])
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "", body.as_bytes())
            .unwrap();
        assert_eq!(answer.next.as_deref(), Some("posts:20"));
        assert_eq!(answer.skipped.first().map(|s| s.0.as_str()), Some("10"));
        let post = answer.items.first().unwrap();
        assert_eq!(post.file_url.host_str(), Some("cdn.donmai.test"));
        assert_eq!(post.rating, Some(Rating::Sensitive));
        assert_eq!(
            post.tags,
            [
                "blue_sky",
                "1girl",
                "artist:some_artist",
                "copyright:a_show",
                "character:a_hero",
                "meta:highres"
            ]
        );
        assert_eq!(
            post.created_at.unwrap().to_rfc3339(),
            "2020-01-02T03:04:05+00:00"
        );
    }

    #[test]
    fn reads_rules_pools_and_favourites() {
        let rule = json!([{"id": 1, "antecedent_name": "old", "consequent_name": "new", "status": "active"}])
            .to_string();
        let aliases = site()
            .parse(LibraryPhase::Rules, "aliases:", rule.as_bytes())
            .unwrap();
        assert_eq!(
            aliases.rules,
            [Rule::Alias {
                tag: "new".into(),
                alias: "old".into()
            }]
        );
        let implications = site()
            .parse(LibraryPhase::Rules, "implications:", rule.as_bytes())
            .unwrap();
        assert_eq!(
            implications.rules,
            [Rule::Implies {
                tag: "old".into(),
                implied: "new".into()
            }]
        );

        let pools = json!([
            {"id": 3, "name": "A_Trip", "post_ids": [1, 2], "is_deleted": false},
            {"id": 4, "name": "gone", "post_ids": [3], "is_deleted": true}
        ])
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Groups, "pools:", pools.as_bytes())
            .unwrap();
        assert_eq!(
            answer.groups,
            [Group {
                tag: "pool_a_trip".into(),
                remote_ids: vec!["1".into(), "2".into()]
            }]
        );
        assert_eq!(answer.next.as_deref(), Some("pools:4"));

        let favs = json!([{"id": 9}, {"id": 5}]).to_string();
        let answer = site()
            .parse(LibraryPhase::Groups, "favs:", favs.as_bytes())
            .unwrap();
        assert_eq!(answer.likes, ["5", "9"]);
    }

    #[test]
    fn counts_and_refuses_what_is_not_a_list() {
        assert_eq!(parse_count(br#"{"counts":{"posts":12}}"#), Some(12));
        assert_eq!(parse_count(b"<html>"), None);
        assert!(
            site()
                .parse(LibraryPhase::Posts, "", br#"{"success":false}"#)
                .is_none()
        );
        let done = site().parse(LibraryPhase::Posts, "posts:9", b"[]").unwrap();
        assert_eq!(done.next, None);
        let done = site()
            .parse(LibraryPhase::Rules, "aliases:9", b"[]")
            .unwrap();
        assert_eq!(done.next.as_deref(), Some("implications:"));
    }
}
