//! Which post a link points at: a post's page on a booru, or one of its files
//! on the site's file servers. The server looks the post up from that; the
//! app uses it to tell a dropped link to a post from any other link.

use super::{BooruApi, clean_base};

/// How a link names its post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostRef {
    Id(u64),
    /// The file's MD5, as file addresses carry it.
    Md5(String),
}

/// A post on another booru, as a link names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostLink {
    pub api: BooruApi,
    /// The site's address, as `RemoteSearch::base` wants it.
    pub base: String,
    pub post: PostRef,
}

/// A host StarBooru knows: a site or the servers its files come from.
struct Host {
    /// The host, or what it ends with after a dot.
    name: &'static str,
    api: BooruApi,
    base: &'static str,
    /// Whether it serves files only, so it has no post pages.
    files: bool,
}

/// Sites whose file servers only answer their own pages: a picture from one
/// shows in a page of ours only when the server fetches it, naming the site.
const GUARDED: [(&str, &str); 1] = [("gelbooru.com", "https://gelbooru.com/")];

/// The page a file wants to be asked from (as a `Referer`), when its site
/// refuses any other.
pub fn guarded_by(raw: &str) -> Option<&'static str> {
    let host = parts(raw)?.host;
    GUARDED
        .iter()
        .find(|(name, _)| under(&host, name))
        .map(|(_, referer)| *referer)
}

const fn site(name: &'static str, api: BooruApi, base: &'static str) -> Host {
    Host {
        name,
        api,
        base,
        files: false,
    }
}

const fn files(name: &'static str, api: BooruApi, base: &'static str) -> Host {
    Host {
        name,
        api,
        base,
        files: true,
    }
}

/// File servers first: `cdn.donmai.us` is also under `donmai.us`.
const HOSTS: [Host; 13] = [
    files(
        "cdn.donmai.us",
        BooruApi::Danbooru,
        "https://danbooru.donmai.us",
    ),
    site(
        "donmai.us",
        BooruApi::Danbooru,
        "https://danbooru.donmai.us",
    ),
    site("gelbooru.com", BooruApi::Gelbooru, "https://gelbooru.com"),
    site("safebooru.org", BooruApi::Gelbooru, "https://safebooru.org"),
    site("yande.re", BooruApi::Moebooru, "https://yande.re"),
    site("konachan.com", BooruApi::Moebooru, "https://konachan.com"),
    site("konachan.net", BooruApi::Moebooru, "https://konachan.net"),
    files("static1.e621.net", BooruApi::E621, "https://e621.net"),
    files("static1.e926.net", BooruApi::E621, "https://e926.net"),
    site("e621.net", BooruApi::E621, "https://e621.net"),
    site("e926.net", BooruApi::E621, "https://e926.net"),
    site("rule34.xxx", BooruApi::Gelbooru, "https://rule34.xxx"),
    site("tbib.org", BooruApi::Gelbooru, "https://tbib.org"),
];

/// A web address, taken apart just enough to read a post out of it.
struct Parts<'a> {
    /// `https://host:port`.
    origin: String,
    host: String,
    path: &'a str,
    query: &'a str,
}

fn parts(raw: &str) -> Option<Parts<'_>> {
    let raw = raw.trim();
    let (scheme, rest) = raw.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return None;
    }
    let rest = rest.split('#').next().unwrap_or_default();
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = match rest.find('/') {
        Some(at) => rest.split_at(at),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let authority = authority.to_ascii_lowercase();
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority.as_str(),
    };
    Some(Parts {
        origin: format!("{scheme}://{authority}"),
        host: host.to_owned(),
        path,
        query,
    })
}

fn under(host: &str, name: &str) -> bool {
    host == name
        || host
            .strip_suffix(name)
            .is_some_and(|rest| rest.ends_with('.'))
}

/// The value of `key` in a query string, as written.
fn param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

/// The number a path segment starts with: `123`, `123.json`, `123-a-slug`.
fn leading_id(segment: &str) -> Option<u64> {
    let digits = segment
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count()
        .min(segment.len());
    let (number, rest) = segment.split_at_checked(digits)?;
    if rest.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return None;
    }
    number.parse().ok().filter(|id| *id > 0)
}

fn is_md5(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The MD5 in a file's address: a folder named by it (Moebooru), or the end
/// of the file's name (`<md5>.jpg`, `sample_<md5>.jpg`, `__tags__<md5>.png`).
fn md5_in(path: &str) -> Option<String> {
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    let last = segments.next_back()?;
    if let Some(folder) = segments.find(|s| is_md5(s)) {
        return Some(folder.to_ascii_lowercase());
    }
    let stem = last.rsplit_once('.').map_or(last, |(stem, _)| stem);
    let at = stem.len().checked_sub(32)?;
    let (lead, tail) = stem.split_at_checked(at)?;
    (is_md5(tail) && (lead.is_empty() || lead.ends_with(['_', '-'])))
        .then(|| tail.to_ascii_lowercase())
}

/// The post id in a page's address, as sites of this kind write them.
/// `strict` leaves out the shapes other websites use too.
fn page_id(api: BooruApi, path: &str, query: &str, strict: bool) -> Option<u64> {
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    match api {
        BooruApi::Gelbooru => {
            let viewing = param(query, "page") == Some("post") && param(query, "s") == Some("view");
            viewing
                .then(|| param(query, "id"))
                .flatten()
                .and_then(leading_id)
        }
        BooruApi::Moebooru => {
            let found = segments.by_ref().skip_while(|s| *s != "post").nth(1)?;
            (found == "show")
                .then(|| segments.next())
                .flatten()
                .and_then(leading_id)
        }
        BooruApi::Danbooru | BooruApi::E621 | BooruApi::Blombooru if !strict => {
            let folder = if api == BooruApi::Blombooru {
                "media"
            } else {
                "posts"
            };
            segments
                .by_ref()
                .skip_while(|s| *s != folder)
                .nth(1)
                .and_then(leading_id)
        }
        _ => None,
    }
}

/// The post a link points at, if it is one. `sites` are other sites the
/// caller knows (the user's own, say), as their kind and address; they come
/// before the built-in ones. A host nobody knows still counts when its
/// address has a shape only boorus use.
pub fn post_link(raw: &str, sites: &[(BooruApi, String)]) -> Option<PostLink> {
    let url = parts(raw)?;
    let link = |api, base: String, post| Some(PostLink { api, base, post });

    for (api, base) in sites {
        let Ok(base) = clean_base(base) else {
            continue;
        };
        let Some((origin, folder)) = parts(&base).map(|p| (p.origin, p.path.to_owned())) else {
            continue;
        };
        let folder = folder.trim_end_matches('/');
        let inside = url.path == folder
            || url
                .path
                .strip_prefix(folder)
                .is_some_and(|rest| rest.starts_with('/'));
        if origin != url.origin || !inside {
            continue;
        }
        if let Some(id) = page_id(*api, url.path, url.query, false) {
            return link(*api, base, PostRef::Id(id));
        }
        if let Some(md5) = md5_in(url.path) {
            return link(*api, base, PostRef::Md5(md5));
        }
    }

    if let Some(host) = HOSTS.iter().find(|h| under(&url.host, h.name)) {
        let id = (!host.files)
            .then(|| page_id(host.api, url.path, url.query, false))
            .flatten();
        let post = match id {
            Some(id) => PostRef::Id(id),
            None => PostRef::Md5(md5_in(url.path)?),
        };
        return link(host.api, host.base.to_owned(), post);
    }

    [BooruApi::Gelbooru, BooruApi::Moebooru]
        .into_iter()
        .find_map(|api| Some((api, page_id(api, url.path, url.query, true)?)))
        .and_then(|(api, id)| link(api, url.origin.clone(), PostRef::Id(id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(url: &str) -> Option<(BooruApi, String, PostRef)> {
        post_link(url, &[]).map(|l| (l.api, l.base, l.post))
    }

    fn id(api: BooruApi, base: &str, id: u64) -> Option<(BooruApi, String, PostRef)> {
        Some((api, base.to_owned(), PostRef::Id(id)))
    }

    fn md5(api: BooruApi, base: &str, md5: &str) -> Option<(BooruApi, String, PostRef)> {
        Some((api, base.to_owned(), PostRef::Md5(md5.to_owned())))
    }

    const HASH: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn reads_post_pages_of_known_sites() {
        assert_eq!(
            found("https://gelbooru.com/index.php?page=post&s=view&id=15250239"),
            id(BooruApi::Gelbooru, "https://gelbooru.com", 15_250_239)
        );
        assert_eq!(
            found("https://www.gelbooru.com/index.php?id=7&s=view&page=post&tags=a+b#top"),
            id(BooruApi::Gelbooru, "https://gelbooru.com", 7)
        );
        assert_eq!(
            found("https://safebooru.org/index.php?page=post&s=view&id=42"),
            id(BooruApi::Gelbooru, "https://safebooru.org", 42)
        );
        assert_eq!(
            found("https://danbooru.donmai.us/posts/123?q=blue_sky"),
            id(BooruApi::Danbooru, "https://danbooru.donmai.us", 123)
        );
        assert_eq!(
            found("https://safebooru.donmai.us/posts/123.json"),
            id(BooruApi::Danbooru, "https://danbooru.donmai.us", 123)
        );
        assert_eq!(
            found("https://yande.re/post/show/99/some-tags-here"),
            id(BooruApi::Moebooru, "https://yande.re", 99)
        );
        assert_eq!(
            found("HTTPS://Konachan.NET/post/show/5"),
            id(BooruApi::Moebooru, "https://konachan.net", 5)
        );
        assert_eq!(
            found("https://e926.net/posts/31?q=wolf"),
            id(BooruApi::E621, "https://e926.net", 31)
        );
    }

    #[test]
    fn reads_the_md5_out_of_file_addresses() {
        assert_eq!(
            found(&format!("https://cdn.donmai.us/original/01/23/{HASH}.jpg")),
            md5(BooruApi::Danbooru, "https://danbooru.donmai.us", HASH)
        );
        assert_eq!(
            found(&format!(
                "https://cdn.donmai.us/original/01/23/__a_b_drawn_by_c__{HASH}.png"
            )),
            md5(BooruApi::Danbooru, "https://danbooru.donmai.us", HASH)
        );
        assert_eq!(
            found(&format!(
                "https://cdn.donmai.us/sample/01/23/sample-{HASH}.jpg"
            )),
            md5(BooruApi::Danbooru, "https://danbooru.donmai.us", HASH)
        );
        assert_eq!(
            found(&format!(
                "https://img3.gelbooru.com//samples/01/23/sample_{}.jpg",
                HASH.to_uppercase()
            )),
            md5(BooruApi::Gelbooru, "https://gelbooru.com", HASH)
        );
        assert_eq!(
            found(&format!(
                "https://files.yande.re/image/{HASH}/yande.re%20123%20tag.jpg"
            )),
            md5(BooruApi::Moebooru, "https://yande.re", HASH)
        );
        assert_eq!(
            found(&format!("https://static1.e621.net/data/01/23/{HASH}.png")),
            md5(BooruApi::E621, "https://e621.net", HASH)
        );
        // A file server has no post pages, whatever its address looks like.
        assert_eq!(found("https://cdn.donmai.us/posts/12"), None);
        // Close, but not an MD5.
        for name in [
            format!("x{HASH}.jpg"),
            format!("{}.jpg", &HASH[1..]),
            format!("{HASH}0.jpg"),
        ] {
            assert_eq!(
                found(&format!("https://cdn.donmai.us/original/{name}")),
                None
            );
        }
    }

    #[test]
    fn guesses_unknown_sites_only_by_shapes_boorus_use() {
        assert_eq!(
            found("https://booru.example/index.php?page=post&s=view&id=8"),
            id(BooruApi::Gelbooru, "https://booru.example", 8)
        );
        assert_eq!(
            found("http://moe.example:8080/post/show/8/tags"),
            id(BooruApi::Moebooru, "http://moe.example:8080", 8)
        );
        for other in [
            "https://blog.example/posts/8",
            &format!("https://pics.example/{HASH}.jpg"),
            "https://booru.example/index.php?page=post&s=list&tags=a",
            "https://booru.example/index.php?page=post&s=view&id=abc",
            "https://booru.example/index.php?page=post&s=view&id=0",
            "https://gelbooru.com/index.php?page=post&s=list",
            "https://danbooru.donmai.us/posts?tags=a",
            "https://danbooru.donmai.us/posts/12abc",
            "https://yande.re/post?tags=a",
            "ftp://gelbooru.com/index.php?page=post&s=view&id=8",
            "https://me:pw@gelbooru.com/index.php?page=post&s=view&id=8",
            "gelbooru.com/index.php?page=post&s=view&id=8",
            "",
        ] {
            assert_eq!(found(other), None, "{other}");
        }
    }

    #[test]
    fn knows_which_files_want_their_site_named() {
        assert_eq!(
            guarded_by("https://img4.gelbooru.com/thumbnails/65/20/thumbnail_x.jpg"),
            Some("https://gelbooru.com/")
        );
        assert_eq!(
            guarded_by("https://video-cdn3.gelbooru.com/a.mp4"),
            Some("https://gelbooru.com/")
        );
        assert_eq!(guarded_by("https://cdn.donmai.us/original/a.jpg"), None);
        assert_eq!(guarded_by("https://notgelbooru.com/a.jpg"), None);
        assert_eq!(guarded_by("/api/v1/remote/media?u=x"), None);
    }

    #[test]
    fn the_callers_sites_come_first() {
        let sites = [
            (BooruApi::Blombooru, "http://nas:8000/booru/".to_owned()),
            (BooruApi::Danbooru, "https://dan.example".to_owned()),
            (BooruApi::Danbooru, "not an address".to_owned()),
        ];
        let link = post_link("http://nas:8000/booru/media/4", &sites).unwrap();
        assert_eq!(
            (link.api, link.base.as_str(), link.post),
            (BooruApi::Blombooru, "http://nas:8000/booru", PostRef::Id(4))
        );
        let link = post_link("https://dan.example/posts/9", &sites).unwrap();
        assert_eq!((link.api, link.post), (BooruApi::Danbooru, PostRef::Id(9)));
        let link = post_link(&format!("https://dan.example/data/{HASH}.webm"), &sites).unwrap();
        assert_eq!(link.post, PostRef::Md5(HASH.to_owned()));
        // Another folder on the same host isn't that site.
        assert_eq!(post_link("http://nas:8000/other/media/4", &sites), None);
        assert_eq!(post_link("https://dan.example/about", &sites), None);
    }
}
