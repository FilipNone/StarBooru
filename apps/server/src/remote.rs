//! Searching other boorus (`POST /remote/search`): where each API's search
//! lives, and how its posts become `RemotePost`s. Parsing is lenient: a post
//! missing what an import needs (hidden files, odd ids) is skipped, not an
//! error.

use serde_json::Value;
use starbooru_core::remote::REMOTE_PAGE_SIZE;
use starbooru_core::{BooruApi, Rating, RemotePost, RemoteSearch, remote_tags};
use url::Url;

/// The site's address as a base to join paths onto: with a trailing slash,
/// so a site under a path (`https://host/booru`) keeps it.
pub fn base_url(search: &RemoteSearch) -> Option<Url> {
    let url = Url::parse(&format!("{}/", search.base)).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}

/// The search's address on the site, with the account, when given, in the
/// query as each API expects it.
pub fn search_url(search: &RemoteSearch) -> Option<Url> {
    let base = base_url(search)?;
    let limit = REMOTE_PAGE_SIZE.to_string();
    let (path, mut pairs) = match search.api {
        BooruApi::Danbooru | BooruApi::Blombooru | BooruApi::E621 | BooruApi::Moebooru => {
            let path = if search.api == BooruApi::Moebooru {
                "post.json"
            } else {
                "posts.json"
            };
            let pairs = vec![
                ("tags", search.tags.clone()),
                ("page", search.page.to_string()),
                ("limit", limit),
            ];
            (path, pairs)
        }
        BooruApi::Gelbooru => {
            let pairs = vec![
                ("page", "dapi".to_owned()),
                ("s", "post".to_owned()),
                ("q", "index".to_owned()),
                ("json", "1".to_owned()),
                ("tags", search.tags.clone()),
                ("pid", search.page.saturating_sub(1).to_string()),
                ("limit", limit),
            ];
            ("index.php", pairs)
        }
    };
    if let (Some(login), Some(key)) = (&search.login, &search.api_key) {
        pairs.push((search.api.login_field(), login.clone()));
        pairs.push((search.api.key_field(), key.clone()));
    }
    let mut url = base.join(path).ok()?;
    url.query_pairs_mut().extend_pairs(pairs);
    Some(url)
}

/// The posts in a search answer, and how many the site sent (some may be
/// skipped), which tells whether another page may follow.
pub fn parse(api: BooruApi, base: &Url, body: &[u8]) -> Option<(Vec<RemotePost>, usize)> {
    // Gelbooru forks answer an empty search with an empty body.
    if body.iter().all(u8::is_ascii_whitespace) {
        return Some((Vec::new(), 0));
    }
    let value: Value = serde_json::from_slice(body).ok()?;
    let list: &[Value] = match &value {
        Value::Array(list) => list,
        // e621 wraps them in `posts`, Gelbooru in `post` (absent when none).
        Value::Object(map) => map
            .get("posts")
            .or_else(|| map.get("post"))
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice),
        _ => return None,
    };
    let posts = list.iter().filter_map(|p| post(api, base, p)).collect();
    Some((posts, list.len()))
}

fn post(api: BooruApi, base: &Url, p: &Value) -> Option<RemotePost> {
    let id = id(p.get("id")?)?;
    let text = |key: &str| p.get(key).and_then(Value::as_str);
    let link = |raw: Option<&str>| raw.and_then(|raw| absolute(base, raw));
    let (file_url, preview_url, tags, width, height, page) = match api {
        BooruApi::Danbooru | BooruApi::Blombooru => {
            let preview = p
                .pointer("/media_asset/variants")
                .and_then(Value::as_array)
                .and_then(|variants| {
                    variants
                        .iter()
                        .find(|v| v.get("type").and_then(Value::as_str) == Some("360x360"))
                })
                .and_then(|v| v.get("url"))
                .and_then(Value::as_str)
                .or_else(|| text("preview_file_url"));
            (
                link(text("file_url")),
                link(preview),
                words(text("tag_string")),
                number(p.get("image_width")),
                number(p.get("image_height")),
                // Blombooru's own page; its `posts/{id}` is the JSON.
                if api == BooruApi::Blombooru {
                    format!("media/{id}")
                } else {
                    format!("posts/{id}")
                },
            )
        }
        BooruApi::E621 => {
            let groups = [
                "artist",
                "copyright",
                "character",
                "species",
                "general",
                "meta",
                "lore",
            ];
            let tags = groups
                .iter()
                .filter_map(|g| p.pointer(&format!("/tags/{g}")).and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            (
                link(p.pointer("/file/url").and_then(Value::as_str)),
                link(p.pointer("/preview/url").and_then(Value::as_str)),
                tags,
                number(p.pointer("/file/width")),
                number(p.pointer("/file/height")),
                format!("posts/{id}"),
            )
        }
        BooruApi::Moebooru => (
            link(text("file_url")),
            link(text("preview_url")),
            words(text("tags")),
            number(p.get("width")),
            number(p.get("height")),
            format!("post/show/{id}"),
        ),
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
            let file = link(text("file_url")).or_else(|| built("images", image.map(str::to_owned)));
            let thumb = link(text("preview_url")).or_else(|| {
                let stem = image.map(|i| i.rsplit_once('.').map_or(i, |(stem, _)| stem));
                built("thumbnails", stem.map(|s| format!("thumbnail_{s}.jpg")))
            });
            (
                file,
                thumb,
                words(text("tags")),
                number(p.get("width")),
                number(p.get("height")),
                format!("index.php?page=post&s=view&id={id}"),
            )
        }
    };
    let file_url = file_url?;
    Some(RemotePost {
        id,
        page_url: base.join(&page).ok()?.to_string(),
        preview_url: preview_url.unwrap_or_else(|| file_url.clone()),
        file_url,
        tags: remote_tags(tags),
        rating: rating(api, text("rating").unwrap_or_default()),
        width,
        height,
    })
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
            "https://m.org/sub/post.json?tags=blue_sky+rating%3Ag&page=3&limit=40"
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
