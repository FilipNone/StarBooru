//! Blombooru: its own API (`api/media`), walked by page. Albums list their posts
//! apart from them, so they're read last, an album at a time. Aliases come from
//! the CSV backup of its tags, the one way to read them that every version has:
//! 1.40 has no tag list at all, and 1.41's leaves aliases out.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use starbooru_core::Rating;

use super::{
    Answer, Call, Group, Item, PAGE, Page, Rule, Site, category_tag, clean_source, counter,
    group_tag, json, names, number, parse_date, tags, text,
};

/// Where the albums step is: the next page of albums, the albums still to read
/// and the page of the first one's contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Albums {
    list_page: u64,
    queue: Vec<(i64, String)>,
    page: u64,
}

impl Albums {
    fn at(rest: &str) -> Self {
        serde_json::from_str(rest).unwrap_or(Albums {
            list_page: 1,
            queue: Vec::new(),
            page: 1,
        })
    }
}

pub(super) fn call(site: &Site, step: &str, rest: &str) -> Option<Call> {
    let url = match step {
        "tags" => site.api("api/admin/backup/tags", [])?,
        "implications" => site.api("api/tag-implications/", [])?,
        "media" => site.api(
            "api/media",
            [
                ("page", counter(rest, 1).to_string()),
                ("limit", PAGE.to_string()),
                ("sort", "uploaded_at".to_owned()),
                ("order", "asc".to_owned()),
            ],
        )?,
        "albums" => {
            let state = Albums::at(rest);
            match state.queue.first() {
                Some((id, _)) => site.api(
                    &format!("api/albums/{id}/contents"),
                    [
                        ("page", state.page.to_string()),
                        ("limit", PAGE.to_string()),
                    ],
                )?,
                None => site.api(
                    "api/albums",
                    [
                        ("page", state.list_page.to_string()),
                        ("limit", PAGE.to_string()),
                    ],
                )?,
            }
        }
        _ => return None,
    };
    Some(Call { url, body: None })
}

pub(super) fn parse(site: &Site, step: &str, rest: &str, body: &[u8]) -> Option<Page> {
    let mut answer = Answer::default();
    if step == "tags" {
        answer.rules = read_tag_backup(body)?;
        return Some(Page { answer, more: None });
    }
    let value = json(body)?;
    match step {
        "implications" => {
            for rule in value.as_array()? {
                read_implication(rule, &mut answer);
            }
            Some(Page { answer, more: None })
        }
        "media" => {
            let list = value.get("items")?.as_array()?;
            answer.total = value.get("total").and_then(Value::as_u64);
            for item in list {
                read_media(site, item, &mut answer);
            }
            Some(paged(answer, rest, list.len()))
        }
        "albums" => read_albums(rest, &value, answer),
        _ => None,
    }
}

fn paged(answer: Answer, rest: &str, read: usize) -> Page {
    Page {
        answer,
        more: (read > 0).then(|| counter(rest, 1).saturating_add(1).to_string()),
    }
}

fn read_albums(rest: &str, value: &Value, mut answer: Answer) -> Option<Page> {
    let mut state = Albums::at(rest);
    if let Some((id, name)) = state.queue.first().cloned() {
        answer.scope = format!("album {id}");
        let media = value.get("media")?.as_array()?;
        if media.is_empty() {
            state.queue.remove(0);
            state.page = 1;
        } else {
            state.page = state.page.saturating_add(1);
            if let Some(tag) = group_tag("album", &name) {
                let remote_ids = media
                    .iter()
                    .filter_map(|item| item.get("id").and_then(number))
                    .map(|id| id.to_string())
                    .collect();
                answer.groups.push(Group { tag, remote_ids });
            }
        }
    } else {
        answer.scope = "albums".to_owned();
        let albums = value.get("items")?.as_array()?;
        if albums.is_empty() {
            return Some(Page { answer, more: None });
        }
        state.list_page = state.list_page.saturating_add(1);
        state.queue = albums
            .iter()
            .filter_map(|album| {
                let id = album.get("id").and_then(number)?;
                Some((id, text(album, "name")?.to_owned()))
            })
            .collect();
    }
    Some(Page {
        answer,
        more: serde_json::to_string(&state).ok(),
    })
}

/// The aliases in a backup of the tags: name, category, post count and a
/// quoted list of aliases. A row in another shape refuses the whole thing, so a
/// login page isn't read as no aliases at all.
fn read_tag_backup(body: &[u8]) -> Option<Vec<Rule>> {
    let body = std::str::from_utf8(body).ok()?;
    let mut rules = Vec::new();
    for row in csv_rows(body.trim_start_matches('\u{feff}'))? {
        let name = row.first()?;
        row.get(1)?.parse::<i64>().ok()?;
        let aliases = row.get(3).map_or("", String::as_str);
        for alias in aliases.split(',').map(str::trim) {
            if !alias.is_empty() {
                rules.push(Rule::Alias {
                    tag: name.clone(),
                    alias: alias.to_owned(),
                });
            }
        }
    }
    Some(rules)
}

/// The rows of a CSV file, where a quoted field can hold commas, line breaks and
/// doubled quotes. Refuses a quote that's never closed.
fn csv_rows(body: &str) -> Option<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if quoted {
        return None;
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Some(rows)
}

fn read_implication(rule: &Value, answer: &mut Answer) {
    // Patterns (`target_tag_patterns`) match names; there is nothing like them here.
    let implied = names(rule, "implied_tags");
    for tag in names(rule, "target_tags") {
        for implied in &implied {
            answer.rules.push(Rule::Implies {
                tag: tag.clone(),
                implied: implied.clone(),
            });
        }
    }
}

fn read_media(site: &Site, item: &Value, answer: &mut Answer) {
    let Some(id) = item.get("id").and_then(number) else {
        return;
    };
    let Some(file_url) = site.api(
        &format!("api/media/{id}/file"),
        [("download", "true".to_owned())],
    ) else {
        return;
    };
    let mut raw = Vec::new();
    for tag in item
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = text(tag, "name") {
            raw.push(category_tag(
                text(tag, "category").unwrap_or_default(),
                name,
            ));
        }
    }
    let date = text(item, "custom_date").or_else(|| text(item, "uploaded_at"));
    answer.items.push(Item {
        remote_id: id.to_string(),
        file_url,
        tags: tags(raw.iter().map(String::as_str)),
        rating: text(item, "rating").and_then(rating),
        source: text(item, "source").and_then(clean_source),
        created_at: date.and_then(parse_date),
        vote: 0,
    });
}

fn rating(name: &str) -> Option<Rating> {
    match name {
        "safe" => Some(Rating::Safe),
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
        test_site(LibrarySource::Blombooru, None, Some("blom_k"))
    }

    #[test]
    fn asks_for_media_oldest_first() {
        let call = site().call(LibraryPhase::Posts, "media:3").unwrap();
        assert_eq!(
            call.url.as_str(),
            "http://booru.test/sub/api/media?page=3&limit=100&sort=uploaded_at&order=asc"
        );
    }

    #[test]
    fn reads_media_with_the_original_file() {
        let body = json!({"total": 1, "page": 1, "pages": 1, "items": [{
            "id": 4, "rating": "questionable", "source": "https://a.b/1",
            "uploaded_at": "2020-01-01T00:00:00", "custom_date": "2019-05-06T07:08:09Z",
            "tags": [
                {"name": "blue_sky", "category": "general", "implied": false},
                {"name": "someone", "category": "artist", "implied": true}
            ]
        }]})
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "", body.as_bytes())
            .unwrap();
        assert_eq!(answer.total, Some(1));
        assert_eq!(answer.next.as_deref(), Some("media:2"));
        let item = answer.items.first().unwrap();
        assert_eq!(
            item.file_url.as_str(),
            "http://booru.test/sub/api/media/4/file?download=true"
        );
        assert_eq!(item.tags, ["blue_sky", "artist:someone"]);
        assert_eq!(item.rating, Some(Rating::Questionable));
        assert_eq!(
            item.created_at.unwrap().to_rfc3339(),
            "2019-05-06T07:08:09+00:00"
        );
    }

    #[test]
    fn reads_aliases_from_the_tag_backup() {
        let call = site().call(LibraryPhase::Rules, "").unwrap();
        assert_eq!(
            call.url.as_str(),
            "http://booru.test/sub/api/admin/backup/tags"
        );
        let body = "sky,0,12,\"heavens, firmament\"\r\n\
                    outdoors,0,0,\r\n\
                    \"odd,\"\"name\"\"\",1,3,other\r\n";
        let answer = site()
            .parse(LibraryPhase::Rules, "", body.as_bytes())
            .unwrap();
        assert_eq!(
            answer.rules,
            [
                Rule::Alias {
                    tag: "sky".into(),
                    alias: "heavens".into()
                },
                Rule::Alias {
                    tag: "sky".into(),
                    alias: "firmament".into()
                },
                Rule::Alias {
                    tag: "odd,\"name\"".into(),
                    alias: "other".into()
                },
            ]
        );
        assert_eq!(answer.next.as_deref(), Some("implications:"));
    }

    #[test]
    fn an_empty_backup_is_fine_and_anything_else_is_refused() {
        let empty = site().parse(LibraryPhase::Rules, "", b"").unwrap();
        assert!(empty.rules.is_empty());
        for body in [
            &b"<!DOCTYPE html>\n<html lang=\"en\">"[..],
            b"{\"detail\":\"Not authenticated\"}",
            b"sky,0,1,\"never closed",
            b"\xff\xfe",
        ] {
            assert!(site().parse(LibraryPhase::Rules, "", body).is_none());
        }
    }

    #[test]
    fn reads_implications() {
        let body = json!([{
            "target_tags": [{"id": 1, "name": "a"}], "target_tag_patterns": ["b*"],
            "implied_tags": [{"id": 2, "name": "c"}]
        }])
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Rules, "implications:", body.as_bytes())
            .unwrap();
        assert_eq!(
            answer.rules,
            [Rule::Implies {
                tag: "a".into(),
                implied: "c".into()
            }]
        );
        assert_eq!(answer.next, None);
    }

    #[test]
    fn goes_through_albums_one_at_a_time() {
        let site = site();
        let phase = LibraryPhase::Groups;
        // The list of albums.
        let call = site.call(phase, "").unwrap();
        assert_eq!(
            call.url.as_str(),
            "http://booru.test/sub/api/albums?page=1&limit=100"
        );
        let body = json!({"items": [{"id": 3, "name": "A Trip"}], "total": 1}).to_string();
        let answer = site.parse(phase, "", body.as_bytes()).unwrap();
        let at_album = answer.next.unwrap();
        // That album's first page.
        let call = site.call(phase, &at_album).unwrap();
        assert_eq!(
            call.url.as_str(),
            "http://booru.test/sub/api/albums/3/contents?page=1&limit=100"
        );
        let body =
            json!({"media": [{"id": 8}, {"id": 9}], "albums": [], "total_media": 2}).to_string();
        let answer = site.parse(phase, &at_album, body.as_bytes()).unwrap();
        assert_eq!(answer.scope, "album 3");
        assert_eq!(
            answer.groups,
            [Group {
                tag: "album_a_trip".into(),
                remote_ids: vec!["8".into(), "9".into()]
            }]
        );
        let next_page = answer.next.unwrap();
        assert!(
            site.call(phase, &next_page)
                .unwrap()
                .url
                .as_str()
                .contains("contents?page=2")
        );
        // An empty page moves on to the next page of albums.
        let body = json!({"media": [], "albums": []}).to_string();
        let answer = site.parse(phase, &next_page, body.as_bytes()).unwrap();
        let more_albums = answer.next.unwrap();
        assert!(
            site.call(phase, &more_albums)
                .unwrap()
                .url
                .as_str()
                .contains("albums?page=2")
        );
        // No albums left ends it.
        let body = json!({"items": [], "total": 1}).to_string();
        let answer = site.parse(phase, &more_albums, body.as_bytes()).unwrap();
        assert_eq!(answer.next, None);
    }
}
