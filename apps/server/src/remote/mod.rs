//! Other boorus: where each API's search and its posts live, and how its
//! answers become `RemotePost`s. Parsing is lenient: a post missing what an
//! import needs (hidden files, odd ids) is skipped, not an error. Nothing here
//! does any I/O; `lookup` and `routes::remote` do the asking.

pub mod accounts;
pub mod html;
pub mod lookup;
pub mod pace;

use chrono::{DateTime, Utc};
use serde_json::Value;
use starbooru_core::remote::REMOTE_PAGE_SIZE;
use starbooru_core::{
    BooruApi, PostRef, Rating, RemotePost, RemoteSearch, TagCategory, TagGroups, remote_tags,
};
use url::Url;

/// The site's address as a base to join paths onto: with a trailing slash,
/// so a site under a path (`https://host/booru`) keeps it.
pub fn base_url(base: &str) -> Option<Url> {
    let url = Url::parse(&format!("{}/", base.trim_end_matches('/'))).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}

/// An account on a site: the login (a user id on Gelbooru) and the key.
pub type Account<'a> = Option<(&'a str, &'a str)>;

/// Where an API lists posts, and the query that every listing starts with.
fn listing(api: BooruApi) -> (&'static str, Vec<(&'static str, String)>) {
    match api {
        BooruApi::Danbooru | BooruApi::Blombooru | BooruApi::E621 => ("posts.json", Vec::new()),
        // The second version of the answer names each tag's category.
        BooruApi::Moebooru => (
            "post.json",
            vec![
                ("api_version", "2".to_owned()),
                ("include_tags", "1".to_owned()),
            ],
        ),
        BooruApi::Gelbooru => (
            "index.php",
            vec![
                ("page", "dapi".to_owned()),
                ("s", "post".to_owned()),
                ("q", "index".to_owned()),
                ("json", "1".to_owned()),
            ],
        ),
    }
}

fn with_query(
    base: &Url,
    api: BooruApi,
    path: &str,
    mut pairs: Vec<(&'static str, String)>,
    account: Account<'_>,
) -> Option<Url> {
    if let Some((login, key)) = account {
        pairs.push((api.login_field(), login.to_owned()));
        pairs.push((api.key_field(), key.to_owned()));
    }
    let mut url = base.join(path).ok()?;
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    Some(url)
}

/// The address that answers with one post, named by its id or its file's MD5.
pub fn post_url(api: BooruApi, base: &Url, post: &PostRef, account: Account<'_>) -> Option<Url> {
    let (path, mut pairs) = listing(api);
    let by_tag = |tag: String, mut pairs: Vec<(&'static str, String)>| {
        pairs.push(("tags", tag));
        pairs.push(("limit", "1".to_owned()));
        pairs
    };
    match (api, post) {
        (BooruApi::Danbooru | BooruApi::Blombooru | BooruApi::E621, PostRef::Id(id)) => {
            with_query(base, api, &format!("posts/{id}.json"), pairs, account)
        }
        (BooruApi::Gelbooru, PostRef::Id(id)) => {
            pairs.push(("id", id.to_string()));
            with_query(base, api, path, pairs, account)
        }
        (BooruApi::Moebooru, PostRef::Id(id)) => {
            with_query(base, api, path, by_tag(format!("id:{id}"), pairs), account)
        }
        (_, PostRef::Md5(md5)) => with_query(
            base,
            api,
            path,
            by_tag(format!("md5:{md5}"), pairs),
            account,
        ),
    }
}

/// The search's address on the site, with the account, when given, in the
/// query as each API expects it.
pub fn search_url(search: &RemoteSearch) -> Option<Url> {
    let base = base_url(&search.base)?;
    let (path, mut pairs) = listing(search.api);
    pairs.push(("tags", search.tags.clone()));
    match search.api {
        BooruApi::Gelbooru => pairs.push(("pid", search.page.saturating_sub(1).to_string())),
        _ => pairs.push(("page", search.page.to_string())),
    }
    pairs.push(("limit", REMOTE_PAGE_SIZE.to_string()));
    let account = search.login.as_deref().zip(search.api_key.as_deref());
    with_query(&base, search.api, path, pairs, account)
}

/// The posts in an answer, and how many the site sent (some may be skipped),
/// which tells whether another page may follow. An answer about one post
/// reads the same way.
pub fn parse(api: BooruApi, base: &Url, body: &[u8]) -> Option<(Vec<RemotePost>, usize)> {
    // Gelbooru forks answer an empty search with an empty body.
    if body.iter().all(u8::is_ascii_whitespace) {
        return Some((Vec::new(), 0));
    }
    let value: Value = serde_json::from_slice(body).ok()?;
    let one;
    let list: &[Value] = match &value {
        Value::Array(list) => list,
        Value::Object(map) => {
            // e621 wraps them in `posts`, Gelbooru in `post` (absent when
            // none); one post comes bare, or as e621's `post`.
            match map.get("posts").or_else(|| map.get("post")) {
                Some(Value::Array(list)) => list,
                Some(post @ Value::Object(_)) => {
                    one = [post.clone()];
                    &one
                }
                Some(_) => return None,
                None if map.contains_key("id") => {
                    one = [value.clone()];
                    &one
                }
                None => &[],
            }
        }
        _ => return None,
    };
    let mut posts: Vec<RemotePost> = list.iter().filter_map(|p| post(api, base, p)).collect();
    // Moebooru lists the categories of a page's tags beside its posts.
    if let Some(kinds) = value.get("tags").and_then(Value::as_object) {
        for post in &mut posts {
            for (name, kind) in kinds {
                let category = match kind.as_str() {
                    Some("artist" | "circle") => TagCategory::Artist,
                    Some("copyright") => TagCategory::Copyright,
                    Some("character") => TagCategory::Character,
                    Some("faults") => TagCategory::Meta,
                    _ => continue,
                };
                if let Some(tag) = remote_tags([name.as_str()]).first()
                    && post.tags.contains(tag)
                {
                    post.groups.put(category, tag);
                }
            }
        }
    }
    Some((posts, list.len()))
}

/// What each API calls the parts of a post that don't need working out.
struct Fields {
    file: Option<String>,
    preview: Option<String>,
    sample: Option<String>,
    /// Every tag, then the ones with a category.
    tags: Vec<String>,
    grouped: Vec<(TagCategory, String)>,
    width: u32,
    height: u32,
    page: String,
    md5: Option<String>,
    source: Option<String>,
    score: Option<i64>,
    created: Option<DateTime<Utc>>,
    size: Option<u64>,
}

fn post(api: BooruApi, base: &Url, p: &Value) -> Option<RemotePost> {
    let id = id(p.get("id")?)?;
    let text = |key: &str| p.get(key).and_then(Value::as_str);
    let at = |path: &str| p.pointer(path).and_then(Value::as_str);
    let link = |raw: Option<&str>| raw.and_then(|raw| absolute(base, raw));
    let owned = |list: Vec<&str>| list.into_iter().map(str::to_owned).collect::<Vec<_>>();
    let fields = match api {
        BooruApi::Danbooru | BooruApi::Blombooru => {
            let variant = |kind: &str| {
                p.pointer("/media_asset/variants")
                    .and_then(Value::as_array)
                    .and_then(|variants| {
                        variants
                            .iter()
                            .find(|v| v.get("type").and_then(Value::as_str) == Some(kind))
                    })
                    .and_then(|v| v.get("url"))
                    .and_then(Value::as_str)
            };
            let groups = [
                (TagCategory::Artist, "tag_string_artist"),
                (TagCategory::Copyright, "tag_string_copyright"),
                (TagCategory::Character, "tag_string_character"),
                (TagCategory::Meta, "tag_string_meta"),
            ];
            Fields {
                file: link(text("file_url")),
                preview: link(variant("360x360").or_else(|| text("preview_file_url"))),
                sample: link(text("large_file_url")),
                tags: owned(words(text("tag_string"))),
                grouped: groups
                    .into_iter()
                    .flat_map(|(category, key)| {
                        words(text(key))
                            .into_iter()
                            .map(move |t| (category, t.to_owned()))
                    })
                    .collect(),
                width: number(p.get("image_width")),
                height: number(p.get("image_height")),
                // Blombooru's own page; its `posts/{id}` is the JSON.
                page: if api == BooruApi::Blombooru {
                    format!("media/{id}")
                } else {
                    format!("posts/{id}")
                },
                md5: text("md5").map(str::to_owned),
                source: text("source").map(str::to_owned),
                score: p.get("score").and_then(Value::as_i64),
                created: text("created_at").and_then(date),
                size: p.get("file_size").and_then(Value::as_u64),
            }
        }
        BooruApi::E621 => {
            let groups = [
                ("artist", TagCategory::Artist),
                ("contributor", TagCategory::Artist),
                ("copyright", TagCategory::Copyright),
                ("character", TagCategory::Character),
                ("species", TagCategory::General),
                ("general", TagCategory::General),
                ("meta", TagCategory::Meta),
                ("lore", TagCategory::Meta),
            ];
            let grouped: Vec<(TagCategory, String)> = groups
                .iter()
                .flat_map(|(group, category)| {
                    p.pointer(&format!("/tags/{group}"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(|t| (*category, t.to_owned()))
                })
                .collect();
            Fields {
                file: link(at("/file/url")),
                preview: link(at("/preview/url")),
                sample: link(at("/sample/url")),
                tags: grouped.iter().map(|(_, t)| t.clone()).collect(),
                grouped,
                width: number(p.pointer("/file/width")),
                height: number(p.pointer("/file/height")),
                page: format!("posts/{id}"),
                md5: at("/file/md5").map(str::to_owned),
                source: p
                    .pointer("/sources/0")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                score: p.pointer("/score/total").and_then(Value::as_i64),
                created: text("created_at").and_then(date),
                size: p.pointer("/file/size").and_then(Value::as_u64),
            }
        }
        BooruApi::Moebooru => Fields {
            file: link(text("file_url")),
            preview: link(text("preview_url")),
            sample: link(text("sample_url")),
            tags: owned(words(text("tags"))),
            grouped: Vec::new(),
            width: number(p.get("width")),
            height: number(p.get("height")),
            page: format!("post/show/{id}"),
            md5: text("md5").map(str::to_owned),
            source: text("source").map(str::to_owned),
            score: p.get("score").and_then(Value::as_i64),
            created: p.get("created_at").and_then(date_value),
            size: p.get("file_size").and_then(Value::as_u64),
        },
        BooruApi::Gelbooru => {
            // Older forks leave the URLs out; they follow from the folder
            // and file name.
            let folder = p.get("directory").map(|d| match d {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
            let image = text("image");
            let built = |prefix: &str, name: Option<String>| {
                folder
                    .as_ref()
                    .zip(name)
                    .and_then(|(dir, name)| absolute(base, &format!("{prefix}/{dir}/{name}")))
            };
            let stem = image.map(|i| i.rsplit_once('.').map_or(i, |(stem, _)| stem));
            let has_sample = p.get("sample").is_some_and(|s| match s {
                Value::Bool(yes) => *yes,
                other => number(Some(other)) > 0,
            });
            Fields {
                file: link(text("file_url")).or_else(|| built("images", image.map(str::to_owned))),
                preview: link(text("preview_url"))
                    .or_else(|| built("thumbnails", stem.map(|s| format!("thumbnail_{s}.jpg")))),
                sample: link(text("sample_url").filter(|_| has_sample)).or_else(|| {
                    built("samples", stem.map(|s| format!("sample_{s}.jpg"))).filter(|_| has_sample)
                }),
                tags: owned(words(text("tags"))),
                grouped: Vec::new(),
                width: number(p.get("width")),
                height: number(p.get("height")),
                page: format!("index.php?page=post&s=view&id={id}"),
                md5: text("md5").or_else(|| text("hash")).map(str::to_owned),
                source: text("source").map(str::to_owned),
                score: p.get("score").and_then(Value::as_i64),
                created: p
                    .get("created_at")
                    .or_else(|| p.get("change"))
                    .and_then(date_value),
                size: None,
            }
        }
    };
    let file_url = fields.file?;
    let tags = remote_tags(fields.tags.iter().map(String::as_str));
    let mut groups = TagGroups::default();
    for (category, raw) in &fields.grouped {
        if let Some(tag) = remote_tags([raw.as_str()]).first()
            && tags.contains(tag)
        {
            groups.put(*category, tag);
        }
    }
    Some(RemotePost {
        id,
        site: base.as_str().trim_end_matches('/').to_owned(),
        page_url: base.join(&fields.page).ok()?.to_string(),
        preview_url: fields.preview.unwrap_or_else(|| file_url.clone()),
        // A sample that is the file itself says nothing.
        sample_url: fields.sample.filter(|sample| *sample != file_url),
        file_url,
        tags,
        groups,
        rating: rating(api, text("rating").unwrap_or_default()),
        width: fields.width,
        height: fields.height,
        md5: fields
            .md5
            .map(|md5| md5.to_ascii_lowercase())
            .filter(|md5| md5.len() == 32 && md5.bytes().all(|b| b.is_ascii_hexdigit())),
        source: fields.source.and_then(|source| clean_source(&source)),
        score: fields.score,
        created_at: fields.created,
        file_size: fields.size,
    })
}

/// A post's source as the site gives it, when it is short enough to keep.
fn clean_source(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    (line.chars().count() <= 2048 && !line.chars().any(char::is_control)).then(|| line.to_owned())
}

/// A date as these sites write them: ISO 8601, or Gelbooru's
/// `Sat Nov 08 12:34:56 -0600 2025`.
fn date(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    DateTime::parse_from_rfc3339(raw)
        .or_else(|_| DateTime::parse_from_str(raw, "%a %b %d %H:%M:%S %z %Y"))
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

/// A date that may also be seconds since 1970, as a number or as text.
fn date_value(v: &Value) -> Option<DateTime<Utc>> {
    let seconds = v
        .as_i64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()));
    match seconds {
        Some(seconds) => DateTime::from_timestamp(seconds, 0),
        None => v.as_str().and_then(date),
    }
}

fn id(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn number(v: Option<&Value>) -> u32 {
    v.and_then(id)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

fn words(s: Option<&str>) -> Vec<&str> {
    s.map(|s| s.split_whitespace().collect())
        .unwrap_or_default()
}

/// `raw` resolved against the site (sites send absolute, `//host/…` and
/// relative addresses), when it is http or https.
fn absolute(base: &Url, raw: &str) -> Option<String> {
    let url = base.join(raw.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

/// Danbooru's `s` is *sensitive*; on the older scale (Moebooru, e621,
/// Blombooru) it means *safe*. Anything unknown is treated as questionable.
fn rating(api: BooruApi, raw: &str) -> Rating {
    match raw {
        "g" | "general" | "safe" => Rating::Safe,
        "s" if api != BooruApi::Danbooru => Rating::Safe,
        "s" | "sensitive" => Rating::Sensitive,
        "e" | "explicit" => Rating::Explicit,
        _ => Rating::Questionable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn search(api: BooruApi, base: &str) -> RemoteSearch {
        RemoteSearch {
            api,
            base: base.into(),
            tags: "blue_sky rating:g".into(),
            page: 3,
            login: None,
            api_key: None,
        }
    }

    fn base(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn builds_each_search_url() {
        let url = |api, b| search_url(&search(api, b)).unwrap().to_string();
        assert_eq!(
            url(BooruApi::Danbooru, "https://d.org"),
            "https://d.org/posts.json?tags=blue_sky+rating%3Ag&page=3&limit=40"
        );
        assert_eq!(
            url(BooruApi::Moebooru, "https://m.org/sub"),
            "https://m.org/sub/post.json?api_version=2&include_tags=1&tags=blue_sky+rating%3Ag&page=3&limit=40"
        );
        assert_eq!(
            url(BooruApi::Gelbooru, "https://g.org"),
            "https://g.org/index.php?page=dapi&s=post&q=index&json=1&tags=blue_sky+rating%3Ag&pid=2&limit=40"
        );
        let mut s = search(BooruApi::Gelbooru, "https://g.org");
        s.login = Some("42".into());
        s.api_key = Some("k&y".into());
        assert!(
            search_url(&s)
                .unwrap()
                .as_str()
                .ends_with("&user_id=42&api_key=k%26y")
        );
        // A key without a login is not sent.
        s.login = None;
        assert!(!search_url(&s).unwrap().as_str().contains("api_key"));
    }

    #[test]
    fn asks_for_one_post() {
        let url = |api, post: PostRef, account| {
            post_url(api, &base("https://b.org/sub/"), &post, account)
                .unwrap()
                .to_string()
        };
        let md5 = || PostRef::Md5("0123456789abcdef0123456789abcdef".into());
        assert_eq!(
            url(BooruApi::Danbooru, PostRef::Id(7), None),
            "https://b.org/sub/posts/7.json"
        );
        assert_eq!(
            url(BooruApi::E621, PostRef::Id(7), Some(("me", "k"))),
            "https://b.org/sub/posts/7.json?login=me&api_key=k"
        );
        assert_eq!(
            url(BooruApi::Gelbooru, PostRef::Id(7), Some(("42", "k"))),
            "https://b.org/sub/index.php?page=dapi&s=post&q=index&json=1&id=7&user_id=42&api_key=k"
        );
        assert_eq!(
            url(BooruApi::Moebooru, PostRef::Id(7), None),
            "https://b.org/sub/post.json?api_version=2&include_tags=1&tags=id%3A7&limit=1"
        );
        assert_eq!(
            url(BooruApi::Danbooru, md5(), None),
            "https://b.org/sub/posts.json?tags=md5%3A0123456789abcdef0123456789abcdef&limit=1"
        );
        assert!(
            url(BooruApi::Gelbooru, md5(), None)
                .ends_with("&json=1&tags=md5%3A0123456789abcdef0123456789abcdef&limit=1")
        );
    }

    #[test]
    fn reads_what_else_a_site_says_about_a_post() {
        const MD5: &str = "CC0663336404B0F5FE38EB0CD0E67E59";
        // Danbooru, one post: a bare object.
        let body = json!({"id": 7, "created_at": "2026-11-13T12:01:24.059-05:00", "score": 12,
            "source": "https://art.example/1\nsecond line", "md5": MD5, "rating": "g",
            "image_width": 2709, "image_height": 3696, "file_size": 4_509_857,
            "tag_string": "2girls some_one miku vocaloid highres",
            "tag_string_general": "2girls", "tag_string_artist": "some_one",
            "tag_string_character": "miku", "tag_string_copyright": "vocaloid",
            "tag_string_meta": "highres",
            "file_url": "https://cdn.d.org/original/a.jpg",
            "large_file_url": "https://cdn.d.org/sample/sample-a.jpg",
            "preview_file_url": "https://cdn.d.org/180x180/a.jpg"});
        let d = base("https://d.org/");
        let (posts, sent) = parse(BooruApi::Danbooru, &d, body.to_string().as_bytes()).unwrap();
        assert_eq!((posts.len(), sent), (1, 1));
        let p = &posts[0];
        assert_eq!(p.site, "https://d.org");
        assert_eq!(p.md5.as_deref(), Some(MD5.to_lowercase().as_str()));
        assert_eq!(
            p.sample_url.as_deref(),
            Some("https://cdn.d.org/sample/sample-a.jpg")
        );
        assert_eq!(p.source.as_deref(), Some("https://art.example/1"));
        assert_eq!((p.score, p.file_size), (Some(12), Some(4_509_857)));
        assert_eq!(
            p.created_at.unwrap().to_rfc3339(),
            "2026-11-13T17:01:24.059+00:00"
        );
        assert_eq!(
            p.import_tags(),
            [
                "artist:some_one",
                "copyright:vocaloid",
                "character:miku",
                "2girls",
                "meta:highres"
            ]
        );

        // e621, one post: wrapped in `post`.
        let body = json!({"post": {"id": 5, "created_at": "2026-01-02T03:04:05.000Z",
            "file": {"url": "https://s.e.net/data/a.png", "width": 10, "height": 20,
                     "md5": MD5, "size": 99},
            "preview": {"url": "https://s.e.net/data/preview/a.jpg"},
            "sample": {"has": false, "url": "https://s.e.net/data/a.png"},
            "score": {"up": 3, "down": -1, "total": 2}, "sources": ["https://a.example/x"],
            "tags": {"general": ["smile"], "artist": ["someone"], "species": ["wolf"],
                     "lore": ["old_story"], "invalid": ["bad"]},
            "rating": "s"}});
        let e = base("https://e.net/");
        let (posts, _) = parse(BooruApi::E621, &e, body.to_string().as_bytes()).unwrap();
        let p = &posts[0];
        assert_eq!(p.sample_url, None, "the file itself isn't a sample");
        assert_eq!((p.score, p.file_size), (Some(2), Some(99)));
        assert_eq!(p.source.as_deref(), Some("https://a.example/x"));
        assert_eq!(
            p.import_tags(),
            ["artist:someone", "wolf", "smile", "meta:old_story"]
        );

        // Moebooru: the tags' categories come beside the posts.
        let body = json!({"posts": [{"id": 9, "tags": "neo-porte dress someone", "md5": MD5,
            "created_at": 1_794_584_768, "score": 3, "file_size": 5, "source": "",
            "file_url": "https://files.m.org/image/x/a.png",
            "sample_url": "https://files.m.org/sample/x/a.jpg",
            "preview_url": "https://assets.m.org/data/preview/a.jpg",
            "rating": "s", "width": 1, "height": 2}],
            "tags": {"neo-porte": "copyright", "dress": "general", "someone": "artist",
                     "elsewhere": "character"}});
        let m = base("https://m.org/");
        let (posts, sent) = parse(BooruApi::Moebooru, &m, body.to_string().as_bytes()).unwrap();
        assert_eq!(sent, 1);
        let p = &posts[0];
        assert_eq!(
            p.import_tags(),
            ["artist:someone", "copyright:neo-porte", "dress"]
        );
        assert_eq!(p.source, None);
        assert_eq!(p.created_at.unwrap().timestamp(), 1_794_584_768);

        // Safebooru: the MD5 is `hash`, and the date `change`.
        let body = json!([{"id": 11, "directory": 1112, "image": "3b8c.jpg", "hash": MD5,
            "file_url": "https://s.org/images/1112/3b8c.jpg",
            "sample_url": "https://s.org/samples/1112/sample_3b8c.jpg", "sample": true,
            "preview_url": "https://s.org/thumbnails/1112/thumbnail_3b8c.jpg",
            "change": 1_794_589_237, "score": null, "tags": "a", "rating": "general",
            "width": 3, "height": 4}]);
        let g = base("https://s.org/");
        let (posts, _) = parse(BooruApi::Gelbooru, &g, body.to_string().as_bytes()).unwrap();
        let p = &posts[0];
        assert_eq!(p.md5.as_deref(), Some(MD5.to_lowercase().as_str()));
        assert_eq!(
            p.sample_url.as_deref(),
            Some("https://s.org/samples/1112/sample_3b8c.jpg")
        );
        assert_eq!(
            (p.score, p.created_at.unwrap().timestamp()),
            (None, 1_794_589_237)
        );
        // Gelbooru: its own way of writing a date, and no sample.
        let body = json!({"post": [{"id": 12, "md5": "nope", "sample": 0,
            "created_at": "Sat Nov 08 12:34:56 -0600 2025",
            "file_url": "https://img.g.com/a.mp4", "sample_url": "",
            "preview_url": "https://img.g.com/t.jpg", "tags": "x", "rating": "general",
            "width": 1, "height": 1}]});
        let (posts, _) = parse(BooruApi::Gelbooru, &g, body.to_string().as_bytes()).unwrap();
        let p = &posts[0];
        assert_eq!((p.md5.as_deref(), p.sample_url.as_deref()), (None, None));
        assert_eq!(
            p.created_at.unwrap().to_rfc3339(),
            "2025-11-08T18:34:56+00:00"
        );
        assert!(p.is_video());
    }

    #[test]
    fn reads_danbooru() {
        let body = json!([
            {"id": 7, "file_url": "https://cdn.d.org/original/a.jpg",
             "preview_file_url": "https://cdn.d.org/180x180/a.jpg",
             "media_asset": {"variants": [{"type": "180x180", "url": "https://cdn.d.org/180x180/a.jpg"},
                                          {"type": "360x360", "url": "https://cdn.d.org/360x360/a.jpg"}]},
             "tag_string": "1girl Blue_Sky -_- 1girl", "rating": "s",
             "image_width": 2500, "image_height": 3000},
            {"id": 8, "rating": "g", "tag_string": "hidden"},
        ]);
        let (posts, sent) = parse(
            BooruApi::Danbooru,
            &base("https://d.org/"),
            body.to_string().as_bytes(),
        )
        .unwrap();
        assert_eq!(sent, 2);
        assert_eq!(posts.len(), 1, "the post without a file is skipped");
        let p = &posts[0];
        assert_eq!(p.page_url, "https://d.org/posts/7");
        assert_eq!(p.preview_url, "https://cdn.d.org/360x360/a.jpg");
        assert_eq!(p.tags, ["1girl", "blue_sky"]);
        assert_eq!(p.rating, Rating::Sensitive);
        assert_eq!((p.width, p.height), (2500, 3000));
    }

    #[test]
    fn reads_blombooru() {
        // As Blombooru's Danbooru layer writes it, with auth on: the key the
        // user searched with rides along in the media links.
        let body = json!([
            {"id": 12, "rating": "s", "tag_string": "sunset sea",
             "file_url": "https://b.home/api/media/12/file?api_key=k",
             "preview_file_url": "https://b.home/api/media/12/thumbnail?api_key=k",
             "media_asset": {"variants": [{"type": "180x180", "url": "https://b.home/api/media/12/thumbnail?api_key=k"}]},
             "image_width": 800, "image_height": 600},
        ]);
        let (posts, _) = parse(
            BooruApi::Blombooru,
            &base("https://b.home/"),
            body.to_string().as_bytes(),
        )
        .unwrap();
        let p = &posts[0];
        assert_eq!(p.page_url, "https://b.home/media/12");
        assert_eq!(
            p.preview_url,
            "https://b.home/api/media/12/thumbnail?api_key=k"
        );
        assert_eq!(p.rating, Rating::Safe);
        assert_eq!(p.tags, ["sunset", "sea"]);
        let mut s = search(BooruApi::Blombooru, "https://b.home");
        s.login = Some("me".into());
        s.api_key = Some("k".into());
        let url = search_url(&s).unwrap();
        assert_eq!(url.path(), "/posts.json");
        assert!(url.query().unwrap().contains("login=me&api_key=k"));
    }

    #[test]
    fn reads_e621() {
        let body = json!({"posts": [
            {"id": 5, "file": {"url": "https://s.e.net/data/a.png", "width": 10, "height": 20},
             "preview": {"url": "https://s.e.net/data/preview/a.jpg"},
             "tags": {"general": ["smile"], "artist": ["someone"], "invalid": ["bad"]},
             "rating": "s"},
            {"id": 6, "file": {"url": null}, "rating": "q"},
        ]});
        let (posts, sent) = parse(
            BooruApi::E621,
            &base("https://e.net/"),
            body.to_string().as_bytes(),
        )
        .unwrap();
        assert_eq!((posts.len(), sent), (1, 2));
        assert_eq!(posts[0].tags, ["someone", "smile"]);
        assert_eq!(posts[0].rating, Rating::Safe);
        assert_eq!(posts[0].page_url, "https://e.net/posts/5");
        assert_eq!((posts[0].width, posts[0].height), (10, 20));
    }

    #[test]
    fn reads_moebooru() {
        let body = json!([{"id": 9, "file_url": "//files.m.org/image/a.jpg",
            "preview_url": "/data/preview/a.jpg", "tags": "tagme wallpaper",
            "rating": "e", "width": 1, "height": 2}]);
        let (posts, _) = parse(
            BooruApi::Moebooru,
            &base("https://m.org/"),
            body.to_string().as_bytes(),
        )
        .unwrap();
        let p = &posts[0];
        assert_eq!(p.file_url, "https://files.m.org/image/a.jpg");
        assert_eq!(p.preview_url, "https://m.org/data/preview/a.jpg");
        assert_eq!(p.page_url, "https://m.org/post/show/9");
        assert_eq!(p.rating, Rating::Explicit);
    }

    #[test]
    fn reads_gelbooru_and_its_forks() {
        // Safebooru: a bare list; this one without URLs.
        let body = json!([{"id": "11", "directory": 3926, "image": "abc.png",
            "tags": " a  b ", "rating": "general", "width": 3, "height": 4}]);
        let (posts, _) = parse(
            BooruApi::Gelbooru,
            &base("https://s.org/"),
            body.to_string().as_bytes(),
        )
        .unwrap();
        let p = &posts[0];
        assert_eq!(p.id, 11);
        assert_eq!(p.file_url, "https://s.org/images/3926/abc.png");
        assert_eq!(
            p.preview_url,
            "https://s.org/thumbnails/3926/thumbnail_abc.jpg"
        );
        assert_eq!(p.page_url, "https://s.org/index.php?page=post&s=view&id=11");
        assert_eq!(p.tags, ["a", "b"]);

        // Gelbooru: wrapped in `post`, and none at all when nothing matched.
        let body = json!({"@attributes": {"count": 1}, "post": [{"id": 12,
            "file_url": "https://img.g.com/a.mp4", "preview_url": "https://img.g.com/t.jpg",
            "tags": "x", "rating": "questionable", "width": 1, "height": 1}]});
        let (posts, _) = parse(
            BooruApi::Gelbooru,
            &base("https://g.com/"),
            body.to_string().as_bytes(),
        )
        .unwrap();
        assert_eq!(posts[0].rating, Rating::Questionable);
        let empty = json!({"@attributes": {"count": 0}}).to_string();
        let g = base("https://g.com/");
        assert_eq!(
            parse(BooruApi::Gelbooru, &g, empty.as_bytes()),
            Some((vec![], 0))
        );
        assert_eq!(parse(BooruApi::Gelbooru, &g, b"  \n"), Some((vec![], 0)));
    }

    #[test]
    fn refuses_what_is_not_a_search_answer() {
        let b = base("https://d.org/");
        assert_eq!(parse(BooruApi::Danbooru, &b, b"<html>"), None);
        assert_eq!(parse(BooruApi::Danbooru, &b, b"42"), None);
        // Only web addresses come back, so nothing else reaches an <img>.
        let body = json!([{"id": 1, "file_url": "javascript:alert(1)", "rating": "g"},
                          {"id": 2, "file_url": "https://d.org/a.jpg",
                           "preview_file_url": "data:image/png;base64,AA", "rating": "g"}]);
        let (posts, _) = parse(BooruApi::Danbooru, &b, body.to_string().as_bytes()).unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].preview_url, "https://d.org/a.jpg");
    }

    #[test]
    fn maps_ratings_per_scale() {
        assert_eq!(rating(BooruApi::Danbooru, "s"), Rating::Sensitive);
        assert_eq!(rating(BooruApi::Moebooru, "s"), Rating::Safe);
        assert_eq!(rating(BooruApi::Gelbooru, "sensitive"), Rating::Sensitive);
        assert_eq!(rating(BooruApi::E621, "e"), Rating::Explicit);
        assert_eq!(rating(BooruApi::Blombooru, "s"), Rating::Safe);
        assert_eq!(rating(BooruApi::Danbooru, ""), Rating::Questionable);
    }
}
