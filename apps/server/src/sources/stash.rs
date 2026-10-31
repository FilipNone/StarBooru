//! Stash: scenes and images through GraphQL. It has no safety ratings; a rating
//! of its own (1 to 100) of 60 or more counts as liking the post. Performers
//! become characters, studios, directors and photographers artists, groups
//! copyrights, and galleries a tag of their own.

use serde_json::Value;

use super::{
    Answer, Call, Item, PAGE, Page, Rule, Site, clean_source, counter, group_tag, json, names,
    number, parse_date, tags, text,
};

const SCENE_FIELDS: &str = "id urls date rating100 created_at director paths{stream} galleries{title} \
                            studio{name} groups{group{name}} tags{name} performers{name}";
const IMAGE_FIELDS: &str = "id urls date rating100 created_at photographer paths{image} galleries{title} \
                            studio{name} tags{name} performers{name}";
/// A rating at or above this counts as a favourite.
const LIKED: i64 = 60;

pub(super) fn call(site: &Site, step: &str, rest: &str) -> Option<Call> {
    let page = counter(rest, 1);
    let filter = format!("filter:{{page:{page},per_page:{PAGE},sort:\"id\",direction:ASC}}");
    let query = match step {
        "tags" => format!("{{findTags({filter}){{count tags{{name aliases parents{{name}}}}}}}}"),
        "performers" => {
            format!("{{findPerformers({filter}){{count performers{{name alias_list}}}}}}")
        }
        "scenes" => format!(
            "{{findScenes({filter}){{count scenes{{{SCENE_FIELDS}}}}} \
             imagesCount:findImages(filter:{{per_page:1}}){{count}}}}"
        ),
        "images" => format!("{{findImages({filter}){{count images{{{IMAGE_FIELDS}}}}}}}"),
        _ => return None,
    };
    let url = site.base.join("graphql").ok()?;
    let body = serde_json::json!({ "query": query }).to_string();
    Some(Call {
        url,
        body: Some(body),
    })
}

pub(super) fn parse(site: &Site, step: &str, rest: &str, body: &[u8]) -> Option<Page> {
    let value = json(body)?;
    let data = value.get("data")?;
    let mut answer = Answer::default();
    let list = match step {
        "tags" => {
            let list = data.get("findTags")?.get("tags")?.as_array()?;
            list.iter().for_each(|tag| read_tag(tag, &mut answer));
            list
        }
        "performers" => {
            let list = data.get("findPerformers")?.get("performers")?.as_array()?;
            list.iter().for_each(|one| read_performer(one, &mut answer));
            list
        }
        "scenes" => {
            let found = data.get("findScenes")?;
            let scenes = found.get("count").and_then(Value::as_u64)?;
            let images = data
                .get("imagesCount")?
                .get("count")
                .and_then(Value::as_u64)?;
            answer.total = Some(scenes.saturating_add(images));
            let list = found.get("scenes")?.as_array()?;
            for scene in list {
                read_post(site, "scene", scene, &mut answer);
            }
            list
        }
        "images" => {
            let list = data.get("findImages")?.get("images")?.as_array()?;
            for image in list {
                read_post(site, "image", image, &mut answer);
            }
            list
        }
        _ => return None,
    };
    let more = (!list.is_empty()).then(|| counter(rest, 1).saturating_add(1).to_string());
    Some(Page { answer, more })
}

fn read_tag(tag: &Value, answer: &mut Answer) {
    let Some(name) = text(tag, "name") else {
        return;
    };
    for alias in strings(tag, "aliases") {
        answer.rules.push(Rule::Alias {
            tag: name.to_owned(),
            alias: alias.to_owned(),
        });
    }
    // A tree has one parent; the first one stands for them.
    if let Some(parent) = names(tag, "parents").into_iter().next() {
        answer.rules.push(Rule::Parent {
            tag: name.to_owned(),
            parent,
        });
    }
}

fn read_performer(performer: &Value, answer: &mut Answer) {
    let Some(name) = text(performer, "name") else {
        return;
    };
    for alias in strings(performer, "alias_list") {
        answer.rules.push(Rule::Alias {
            tag: name.to_owned(),
            alias: alias.to_owned(),
        });
    }
}

fn strings<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn read_post(site: &Site, kind: &str, post: &Value, answer: &mut Answer) {
    let Some(id) = post.get("id").and_then(number) else {
        return;
    };
    let remote_id = format!("{kind}:{id}");
    let paths = post.get("paths");
    let raw_file = paths
        .and_then(|paths| paths.get("stream").or_else(|| paths.get("image")))
        .and_then(Value::as_str);
    let Some(file_url) = raw_file.and_then(|raw| site.file(raw)) else {
        let reason = "the site gives no address for its file".to_owned();
        answer.skipped.push((remote_id, reason));
        return;
    };
    let mut raw: Vec<String> = names(post, "tags");
    raw.extend(
        names(post, "performers")
            .iter()
            .map(|name| format!("character:{name}")),
    );
    let studio = post.get("studio").and_then(|studio| text(studio, "name"));
    let makers = [studio, text(post, "director"), text(post, "photographer")];
    for name in makers
        .into_iter()
        .flatten()
        .filter(|name| !name.trim().is_empty())
    {
        raw.push(format!("artist:{name}"));
    }
    for group in post
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = group.get("group").and_then(|group| text(group, "name")) {
            raw.push(format!("copyright:{name}"));
        }
    }
    for gallery in post
        .get("galleries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(tag) = text(gallery, "title").and_then(|title| group_tag("gallery", title)) {
            raw.push(tag);
        }
    }
    answer.items.push(Item {
        remote_id,
        file_url,
        tags: tags(raw.iter().map(String::as_str)),
        rating: None,
        source: strings(post, "urls").find_map(clean_source),
        created_at: text(post, "created_at").and_then(parse_date),
        vote: i16::from(
            post.get("rating100")
                .and_then(number)
                .is_some_and(|r| r >= LIKED),
        ),
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use starbooru_core::{LibraryPhase, LibrarySource};

    use super::*;
    use crate::sources::test_site;

    fn site() -> Site {
        test_site(LibrarySource::Stash, None, Some("k"))
    }

    #[test]
    fn walks_scenes_then_images() {
        let call = site().call(LibraryPhase::Posts, "").unwrap();
        assert_eq!(call.url.as_str(), "http://booru.test/sub/graphql");
        let body = call.body.unwrap();
        assert!(
            body.contains("findScenes(filter:{page:1,per_page:100,sort:\\\"id\\\",direction:ASC})"),
            "{body}"
        );
        assert!(body.contains("imagesCount"), "{body}");
        let body = site()
            .call(LibraryPhase::Posts, "images:3")
            .unwrap()
            .body
            .unwrap();
        assert!(body.contains("findImages(filter:{page:3,"), "{body}");
    }

    #[test]
    fn reads_scenes_with_everything_that_makes_a_tag() {
        let body = json!({"data": {
            "findScenes": {"count": 2, "scenes": [{
                "id": "12", "urls": ["", "https://a.b/1"], "rating100": 80,
                "created_at": "2020-01-02T03:04:05Z", "director": "A Director",
                "paths": {"stream": "http://booru.test/sub/scene/12/stream?apikey=k"},
                "galleries": [{"title": "Summer Set"}],
                "studio": {"name": "A Studio"},
                "groups": [{"group": {"name": "A Series"}}],
                "tags": [{"name": "Blue Sky"}],
                "performers": [{"name": "Some One"}]
            }, {"id": "13", "paths": {"stream": null}}]},
            "imagesCount": {"count": 5}
        }})
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "", body.as_bytes())
            .unwrap();
        assert_eq!(answer.total, Some(7));
        assert_eq!(answer.next.as_deref(), Some("scenes:2"));
        assert_eq!(
            answer.skipped.first().map(|s| s.0.as_str()),
            Some("scene:13")
        );
        let scene = answer.items.first().unwrap();
        assert_eq!(scene.remote_id, "scene:12");
        assert_eq!(scene.rating, None);
        assert_eq!(scene.vote, 1);
        assert_eq!(scene.source.as_deref(), Some("https://a.b/1"));
        assert_eq!(
            scene.tags,
            [
                "blue_sky",
                "character:some_one",
                "artist:a_studio",
                "artist:a_director",
                "copyright:a_series",
                "gallery_summer_set"
            ]
        );
    }

    #[test]
    fn scenes_end_and_images_follow_with_their_own_ids() {
        let body = json!({"data": {
            "findScenes": {"count": 0, "scenes": []}, "imagesCount": {"count": 1}
        }})
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "scenes:4", body.as_bytes())
            .unwrap();
        assert_eq!(answer.next.as_deref(), Some("images:"));

        let body = json!({"data": {"findImages": {"count": 1, "images": [{
            "id": "5", "rating100": 40, "photographer": "Some Body",
            "paths": {"image": "/image/5/image"}
        }]}}})
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Posts, "images:", body.as_bytes())
            .unwrap();
        let image = answer.items.first().unwrap();
        assert_eq!(image.remote_id, "image:5");
        assert_eq!(image.vote, 0);
        assert_eq!(image.tags, ["artist:some_body"]);
        assert_eq!(image.file_url.as_str(), "http://booru.test/image/5/image");
    }

    #[test]
    fn reads_tag_rules_and_performer_aliases() {
        let body = json!({"data": {"findTags": {"count": 1, "tags": [{
            "name": "sky", "aliases": ["heavens"], "parents": [{"name": "nature"}, {"name": "x"}]
        }]}}})
        .to_string();
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
                Rule::Parent {
                    tag: "sky".into(),
                    parent: "nature".into()
                },
            ]
        );
        let body = json!({"data": {"findPerformers": {"count": 1, "performers": [{
            "name": "Some One", "alias_list": ["S.O."]
        }]}}})
        .to_string();
        let answer = site()
            .parse(LibraryPhase::Rules, "performers:", body.as_bytes())
            .unwrap();
        assert_eq!(
            answer.rules,
            [Rule::Alias {
                tag: "Some One".into(),
                alias: "S.O.".into()
            }]
        );
    }

    #[test]
    fn graphql_errors_are_refused() {
        let body = br#"{"errors":[{"message":"no"}],"data":null}"#;
        assert!(site().parse(LibraryPhase::Posts, "", body).is_none());
    }
}
