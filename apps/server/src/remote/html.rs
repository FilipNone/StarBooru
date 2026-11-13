//! Reading a web page for the picture it is about, for an address that turned
//! out to be a page and not a file. Any page that names its picture for link
//! previews (`og:image`, `og:video`) works; a Gelbooru-style post page also
//! gives its tags, rating and source. No I/O, and no HTML parser either: the
//! page is only scanned for the few tags that matter.

use starbooru_core::{Rating, RemotePost, TagCategory, TagGroups, remote_tags};
use url::Url;

/// Most of a page looked at; what matters is in its head or its sidebar.
const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;

/// What a page says about its picture.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    /// The file to download: the original where the page links one.
    pub file_url: Option<String>,
    pub tags: Vec<String>,
    pub groups: TagGroups,
    pub rating: Option<Rating>,
    pub source: Option<String>,
    /// The copy the page itself shows, when it isn't the file.
    pub sample_url: Option<String>,
    pub width: u32,
    pub height: u32,
    pub md5: Option<String>,
}

/// Whether a download is a web page, going by how it starts.
pub fn is_html(bytes: &[u8]) -> bool {
    let head = bytes.get(..bytes.len().min(1024)).unwrap_or_default();
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    let text = text.trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    text.starts_with("<!doctype html")
        || text.starts_with("<html")
        || text.starts_with("<head")
        || (text.starts_with("<!--") && text.contains("<html"))
        || (text.starts_with("<?xml") && text.contains("<html"))
}

/// An opening tag: its name and attributes, and where it ends in the page.
struct Tag<'a> {
    name: String,
    attrs: Vec<(String, String)>,
    /// What follows it.
    rest: &'a str,
}

impl Tag<'_> {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The text up to the next tag.
    fn text(&self) -> String {
        let end = self.rest.find('<').unwrap_or(self.rest.len());
        unescape(self.rest.get(..end).unwrap_or_default())
            .trim()
            .to_owned()
    }
}

/// The opening tags of a page, in order. Comments, closing tags and what is
/// inside a script or a style sheet are skipped, and so is anything that
/// looks like a tag and isn't one, such as a `<` in running text.
fn tags(html: &str) -> impl Iterator<Item = Tag<'_>> {
    let mut rest = html;
    std::iter::from_fn(move || {
        loop {
            let open = rest.find('<')?;
            let after = rest.get(open + 1..)?;
            if let Some(comment) = after.strip_prefix("!--") {
                rest = comment.split_once("-->").map_or("", |(_, tail)| tail);
                continue;
            }
            let name_len = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .unwrap_or(after.len());
            let Some((name, body)) = after.split_at_checked(name_len) else {
                rest = after;
                continue;
            };
            let Some((attrs, tail)) = attributes(body).filter(|_| !name.is_empty()) else {
                // A closing tag, a doctype or a stray `<`.
                rest = after;
                continue;
            };
            let name = name.to_ascii_lowercase();
            rest = match name.as_str() {
                "script" | "style" => after_closing(tail, &name),
                _ => tail,
            };
            return Some(Tag {
                name,
                attrs,
                rest: tail,
            });
        }
    })
}

/// What follows the closing tag of a script or a style sheet, whose text is
/// not markup.
fn after_closing<'a>(text: &'a str, name: &str) -> &'a str {
    let closing = format!("</{name}");
    let lower = text.to_ascii_lowercase();
    lower
        .find(&closing)
        .and_then(|at| text.get(at..))
        .unwrap_or_default()
}

/// A tag's attributes, and the page after its `>`.
fn attributes(mut body: &str) -> Option<(Vec<(String, String)>, &str)> {
    let mut attrs = Vec::new();
    loop {
        body = body.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if let Some(tail) = body.strip_prefix('>') {
            return Some((attrs, tail));
        }
        let name_len = body
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
            .unwrap_or(body.len());
        let (name, tail) = body.split_at_checked(name_len)?;
        if name.is_empty() {
            return None;
        }
        let tail = tail.trim_start();
        let (value, tail) = match tail.strip_prefix('=') {
            None => ("", tail),
            Some(after) => {
                let after = after.trim_start();
                match after.chars().next() {
                    Some(quote @ ('"' | '\'')) => {
                        let inner = after.get(1..)?;
                        let end = inner.find(quote)?;
                        (inner.get(..end)?, inner.get(end + 1..)?)
                    }
                    _ => {
                        let end = after
                            .find(|c: char| c.is_whitespace() || c == '>')
                            .unwrap_or(after.len());
                        after.split_at_checked(end)?
                    }
                }
            }
        };
        attrs.push((name.to_ascii_lowercase(), unescape(value)));
        body = tail;
    }
}

/// Text with the character references pages commonly use turned back.
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        let (before, tail) = rest.split_at(at);
        out.push_str(before);
        let end = tail.find(';').filter(|end| *end <= 10);
        let known = end.and_then(|end| {
            let name = tail.get(1..end)?;
            let c = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => ' ',
                _ => {
                    let code = name.strip_prefix('#')?;
                    let number = match code.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => code.parse().ok()?,
                    };
                    char::from_u32(number)?
                }
            };
            Some((c, end))
        });
        match known {
            Some((c, end)) => {
                out.push(c);
                rest = tail.get(end + 1..).unwrap_or_default();
            }
            None => {
                out.push('&');
                rest = tail.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

/// `raw` as a full http(s) address, from where the page is.
fn absolute(page: &Url, raw: &str) -> Option<String> {
    let url = page.join(raw.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

/// The tag in a link to a search for it: `…&tags=blue_sky`.
fn searched_tag(page: &Url, href: &str) -> Option<String> {
    let url = page.join(href).ok()?;
    let found = url
        .query_pairs()
        .find(|(key, _)| key == "tags")
        .map(|(_, value)| value.trim().to_owned());
    found.filter(|tag| !tag.is_empty() && !tag.contains(' '))
}

fn rating(word: &str) -> Option<Rating> {
    match word.trim().to_ascii_lowercase().as_str() {
        "general" | "safe" | "g" => Some(Rating::Safe),
        "sensitive" => Some(Rating::Sensitive),
        "questionable" | "q" => Some(Rating::Questionable),
        "explicit" | "e" => Some(Rating::Explicit),
        _ => None,
    }
}

/// Reads a page that came from `page`. `None` when it names no picture.
pub fn read(page: &Url, bytes: &[u8]) -> Option<Page> {
    let bytes = bytes
        .get(..bytes.len().min(MAX_PAGE_BYTES))
        .unwrap_or(bytes);
    let html = String::from_utf8_lossy(bytes);
    let mut og_image = None;
    let mut og_video = None;
    let mut original = None;
    let mut video = None;
    let mut source = None;
    let mut found = Page::default();
    let mut raw_tags: Vec<String> = Vec::new();
    let mut groups: Vec<(TagCategory, String)> = Vec::new();
    // The category of the list item the walk is in, while it is in one.
    let mut in_item: Option<TagCategory> = None;
    // Whether that item is the one that names the source.
    let mut in_source = false;
    let mut in_video = false;

    for tag in tags(&html) {
        // Danbooru-style pages say these on the picture's own element.
        if let Some(word) = tag.attr("data-rating") {
            found.rating = found.rating.or_else(|| rating(word));
        }
        if let Some(address) = tag.attr("data-source").filter(|a| !a.is_empty()) {
            source = source.or_else(|| absolute(page, address));
        }
        if let Some(md5) = tag.attr("data-md5").filter(|m| m.len() == 32) {
            found.md5 = found.md5.or_else(|| Some(md5.to_ascii_lowercase()));
            let size = |name: &str| tag.attr(name).and_then(|n| n.parse().ok()).unwrap_or(0);
            found.width = size("data-width");
            found.height = size("data-height");
        }
        if tag.name == "img" && tag.attr("id") == Some("image") {
            found.sample_url = tag.attr("src").and_then(|src| absolute(page, src));
        }
        match tag.name.as_str() {
            "meta" => {
                let key = tag.attr("property").or_else(|| tag.attr("name"));
                let content = tag.attr("content").and_then(|c| absolute(page, c));
                match key {
                    Some("og:image" | "og:image:url" | "og:image:secure_url" | "twitter:image") => {
                        og_image = og_image.or(content);
                    }
                    Some("og:video" | "og:video:url" | "og:video:secure_url") => {
                        og_video = og_video.or(content);
                    }
                    _ => {}
                }
            }
            "li" => {
                in_item = tag.attr("class").and_then(|all| {
                    all.split_whitespace().find_map(|class| {
                        match class.strip_prefix("tag-type-")? {
                            "artist" => Some(TagCategory::Artist),
                            "character" => Some(TagCategory::Character),
                            "copyright" => Some(TagCategory::Copyright),
                            "metadata" | "meta" => Some(TagCategory::Meta),
                            _ => Some(TagCategory::General),
                        }
                    })
                });
                let text = tag.text();
                if let Some(word) = text.strip_prefix("Rating:") {
                    found.rating = found.rating.or_else(|| rating(word));
                }
                in_source = text.starts_with("Source:");
            }
            "a" => {
                let Some(href) = tag.attr("href") else {
                    continue;
                };
                let text = tag.text();
                if text.eq_ignore_ascii_case("original image") {
                    original = original.or_else(|| absolute(page, href));
                } else if in_source {
                    source = source.or_else(|| absolute(page, href));
                    in_source = false;
                } else if let Some(category) = in_item
                    && let Some(name) = searched_tag(page, href)
                    // The item's other links: its wiki `?` and the `+` and `-`.
                    && text.chars().count() > 1
                {
                    if !raw_tags.contains(&name) {
                        raw_tags.push(name.clone());
                    }
                    groups.push((category, name));
                }
            }
            "video" => {
                in_video = true;
                video = video.or_else(|| tag.attr("src").and_then(|src| absolute(page, src)));
            }
            "source" if in_video => {
                video = video.or_else(|| tag.attr("src").and_then(|src| absolute(page, src)));
            }
            _ => {}
        }
    }

    // The original where the page links it; a player's own file may be a
    // smaller copy, and a preview picture a still of a video.
    found.file_url = original.or(og_video).or(video).or(og_image);
    found.file_url.as_ref()?;
    if found.sample_url == found.file_url {
        found.sample_url = None;
    }
    found.tags = remote_tags(raw_tags.iter().map(String::as_str));
    for (category, name) in groups {
        if let Some(tag) = remote_tags([name.as_str()]).first()
            && found.tags.contains(tag)
        {
            found.groups.put(category, tag);
        }
    }
    found.source = source;
    Some(found)
}

/// The posts on a Gelbooru list page (`page=post&s=list`), which is how the
/// site is searched without a key. A list says less than the API: no file, no
/// size and no categories, so `file_url` is empty and an import or a closer
/// look goes through the post's own page.
pub fn read_list(page: &Url, bytes: &[u8]) -> Vec<RemotePost> {
    let bytes = bytes
        .get(..bytes.len().min(MAX_PAGE_BYTES))
        .unwrap_or(bytes);
    let html = String::from_utf8_lossy(bytes);
    let site = page
        .join("/")
        .map(|u| u.as_str().trim_end_matches('/').to_owned());
    let mut posts: Vec<RemotePost> = Vec::new();
    // The post whose link the walk is inside, until its thumbnail comes.
    let mut linked: Option<u64> = None;
    for tag in tags(&html) {
        match tag.name.as_str() {
            "a" => {
                linked = tag
                    .attr("id")
                    .and_then(|id| id.strip_prefix('p'))
                    .and_then(|id| id.parse().ok())
                    .filter(|id| *id > 0);
            }
            "img" => {
                let Some(id) = linked.take() else { continue };
                let Some(preview) = tag.attr("src").and_then(|src| absolute(page, src)) else {
                    continue;
                };
                let Ok(page_url) = page.join(&format!("index.php?page=post&s=view&id={id}")) else {
                    continue;
                };
                // The title is the tags, then `score:N` and `rating:word`.
                let title = tag.attr("title").unwrap_or_default();
                let fact = |key: &str| {
                    title
                        .split_whitespace()
                        .rev()
                        .find_map(|word| word.strip_prefix(key))
                };
                let names = title
                    .split_whitespace()
                    .filter(|word| !word.starts_with("score:") && !word.starts_with("rating:"));
                if posts.iter().any(|p| p.id == id) {
                    continue;
                }
                posts.push(RemotePost {
                    id,
                    site: site.clone().unwrap_or_default(),
                    page_url: page_url.to_string(),
                    preview_url: preview,
                    sample_url: None,
                    file_url: String::new(),
                    tags: remote_tags(names),
                    groups: TagGroups::default(),
                    rating: fact("rating:")
                        .and_then(rating)
                        .unwrap_or(Rating::Questionable),
                    width: 0,
                    height: 0,
                    md5: None,
                    source: None,
                    score: fact("score:").and_then(|n| n.parse().ok()),
                    created_at: None,
                    file_size: None,
                    post_id: None,
                });
            }
            _ => {}
        }
    }
    posts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_gelbooru_list_page() {
        // As Gelbooru writes its lists (November 2026), with made-up values.
        let html = r#"<div class="thumbnail-container">
            <article class="thumbnail-preview">
              <a id="p15250367" href="https://b.example/index.php?page=post&amp;s=view&amp;id=15250367&tags=blue_eyes">
                <img src="https://img4.b.example/thumbnails/65/20/thumbnail_6520c64a8351872ae7d1164290ca4a3d.jpg" title=" 2boys blue_eyes :3 some_one  score:12 rating:general"  alt="Rule 34 | 2boys, blue eyes"/>
              </a></article>
            <article class="thumbnail-preview">
              <a id="p7" href="index.php?page=post&amp;s=view&amp;id=7"><img src="/thumbnails/1/thumbnail_a.jpg" title="solo score:-1 rating:explicit"/></a>
            </article>
            <a id="pool3" href="x"><img src="/pool.jpg" title="not a post"></a>
            <a id="p7" href="x"><img src="/again.jpg" title="twice"></a>
            <a href="index.php?page=post&s=list&pid=42"><img src="/next.png"></a></div>"#;
        let posts = read_list(
            &Url::parse("https://b.example/index.php?page=post&s=list&tags=blue_eyes").unwrap(),
            html.as_bytes(),
        );
        assert_eq!(posts.len(), 2);
        let p = &posts[0];
        assert_eq!((p.id, p.site.as_str()), (15_250_367, "https://b.example"));
        assert_eq!(
            p.page_url,
            "https://b.example/index.php?page=post&s=view&id=15250367"
        );
        assert!(
            p.preview_url
                .ends_with("thumbnail_6520c64a8351872ae7d1164290ca4a3d.jpg")
        );
        assert_eq!(p.tags, ["2boys", "blue_eyes", ":3", "some_one"]);
        assert_eq!((p.rating, p.score), (Rating::Safe, Some(12)));
        // What a list doesn't say.
        assert_eq!(
            (p.file_url.as_str(), p.width, p.md5.as_deref()),
            ("", 0, None)
        );
        assert_eq!(
            (posts[1].rating, posts[1].score),
            (Rating::Explicit, Some(-1))
        );
        assert_eq!(
            posts[1].preview_url,
            "https://b.example/thumbnails/1/thumbnail_a.jpg"
        );
        assert!(read_list(&Url::parse("https://b.example/").unwrap(), b"<html></html>").is_empty());
    }

    fn page(html: &str) -> Option<Page> {
        read(
            &Url::parse("https://b.example/index.php?page=post&s=view&id=7").unwrap(),
            html.as_bytes(),
        )
    }

    #[test]
    fn tells_pages_from_files() {
        for html in [
            "<!DOCTYPE html><html>",
            "\u{feff}\n  <html lang=\"en\">",
            "<!-- hi --><html>",
            "<head><title>x</title>",
        ] {
            assert!(is_html(html.as_bytes()), "{html}");
        }
        assert!(!is_html(b"\x89PNG\r\n\x1a\n"));
        assert!(!is_html(b"<svg xmlns=\"http://www.w3.org/2000/svg\">"));
        assert!(!is_html(b""));
    }

    #[test]
    fn reads_the_picture_a_page_is_about() {
        let found = page(
            r#"<html><head>
            <meta property="og:title" content="A post">
            <meta content='//img.b.example/images/a.jpg?x=1&amp;y=2' property="og:image" />
            <meta name="twitter:image" content="https://img.b.example/other.jpg">
            </head></html>"#,
        )
        .unwrap();
        assert_eq!(
            found.file_url.as_deref(),
            Some("https://img.b.example/images/a.jpg?x=1&y=2")
        );
        assert!(found.tags.is_empty() && found.rating.is_none());
        // A video comes before the still that stands for it.
        let found = page(
            r#"<meta property="og:image" content="/thumb.jpg">
               <meta property="og:video" content="/v.mp4">"#,
        )
        .unwrap();
        assert_eq!(found.file_url.as_deref(), Some("https://b.example/v.mp4"));
    }

    #[test]
    fn a_page_without_a_picture_is_nothing() {
        assert_eq!(page("<html><body><p>Hello</p></body></html>"), None);
        assert_eq!(
            page(r#"<meta property="og:image" content="javascript:alert(1)">"#),
            None
        );
        assert_eq!(page(r#"<meta property="og:image">"#), None);
        // Broken markup is walked past without a panic.
        for broken in [
            "<meta property=\"og:image",
            "<",
            "<a href=",
            "<<<>>>",
            "<!--",
        ] {
            assert_eq!(page(broken), None, "{broken}");
        }
    }

    #[test]
    fn scripts_and_stray_brackets_dont_end_the_walk() {
        // Regression: a `<` in a script ended the walk before the page's
        // picture was reached, on Gelbooru's real pages.
        let found = page(
            r#"<html><head><script>if (a<b && c>d) { x = "<meta property='og:image' content='/no.jpg'>"; }</script>
            <style>a<b { color: red }</style>
            <p>1 < 2 and 3 <4</p>
            <SCRIPT type="text/javascript">for (i=0;i<n;i++) {}</SCRIPT>
            <meta property="og:image" content="/yes.jpg">
            <script>unfinished < "#,
        )
        .unwrap();
        assert_eq!(found.file_url.as_deref(), Some("https://b.example/yes.jpg"));
    }

    /// A post page as Gelbooru writes one (November 2026), with made-up
    /// values: the quirks are the site's, such as `//images` and list items
    /// inside spans.
    const GELBOORU_PAGE: &str = r##"<!DOCTYPE html>
<html>
  <head>
    <title>some one, hatsune miku, vocaloid, highres, 1girl - Image View -  | Gelbooru</title>
    <meta name="rating" content="general" />
    <meta property="og:site_name" content="Gelbooru" />
    <meta property="og:image" content="https://img4.b.example//images/65/20/6520c64a8351872ae7d1164290ca4a3d.jpg" />
    <meta name="twitter:image" content="https://img4.b.example//images/65/20/6520c64a8351872ae7d1164290ca4a3d.jpg" />
  </head>
  <body>
<section class="image-container note-container" data-id="7" data-tags=" 1girl :3 hatsune_miku highres some_one vocaloid " data-rating="sensitive" data-width="1365" data-height="2048" data-source="https://art.example/works/1?a=1&amp;b=2" data-md5="6520c64a8351872ae7d1164290ca4a3d" data-file-url="" data-large-file-url="">
		<picture>
		<img width="850" height="1275" id="image" class="fit-width" src="https://img4.b.example//samples/65/20/sample_6520c64a8351872ae7d1164290ca4a3d.jpg">		</picture>
		</section>
<ul class="tag-list" id="tag-list" style="margin: 0px 10px 0px 10px;">
			<span class="sm-hidden"><li style="margin-top: 10px;"><b>Artist</b></li></span><li class="tag-type-artist"><span class="sm-hidden"><a href="index.php?page=wiki&amp;s=list&amp;search=some_one">?</a> </span><a href="index.php?page=post&amp;s=list&amp;tags=some_one">some one</a> <span style='color: #a0a0a0;'>3</span></li><span class="sm-hidden"><li style="margin-top: 10px;"><b>Character</b></li></span><li class="tag-type-character"><span class="sm-hidden"><a href="index.php?page=wiki&amp;s=list&amp;search=hatsune_miku">?</a> </span><a href="index.php?page=post&amp;s=list&amp;tags=hatsune_miku">hatsune miku</a> <span style='color: #a0a0a0;'>3511</span></li><li class="tag-type-copyright"><a href="index.php?page=post&amp;s=list&amp;tags=vocaloid">vocaloid</a> <span>9</span></li><li class="tag-type-metadata"><a href="index.php?page=post&amp;s=list&amp;tags=highres">highres</a></li><li class="tag-type-general"><a href="index.php?page=post&amp;s=list&amp;tags=1girl">1girl</a></li><li class="tag-type-general"><a href="index.php?page=post&amp;s=list&amp;tags=%3A3">:3</a></li>
					<li><h3>Statistics</h3></li>
					<li>Id: 7<br /></li>
					<li>Posted: 2026-11-13 11:01:02<br /> Uploader: <a href="index.php?page=account&amp;s=profile&amp;id=6498">someone</a><br /></li>
					<li>Size: 1365x2048<br /></li><li style="line-break: anywhere;">Source: <a href="https://art.example/works/1?a=1&amp;b=2" rel="nofollow">art.example/works/1</a><br /></li><li>Rating: Sensitive<br /></li>
					<li>Score: <span id="psc7">0</span> (vote <a href="#" onclick="Javascript:post_vote('7', 'up'); return false;">Up</a>)</li>
			<li><a href="javascript:;" class="showEditBox">Edit</a><br /></li>
		<li><a href="https://img4.b.example/images/65/20/6520c64a8351872ae7d1164290ca4a3d.png" target="_blank" rel="noopener" style="font-weight: bold;">Original image</a><br /></li>
</ul>
  </body>
</html>"##;

    #[test]
    fn reads_a_gelbooru_post_page() {
        let found = page(GELBOORU_PAGE).unwrap();
        assert_eq!(
            found.file_url.as_deref(),
            Some("https://img4.b.example/images/65/20/6520c64a8351872ae7d1164290ca4a3d.png"),
            "the original the page links, before its preview picture"
        );
        assert_eq!(
            found.tags,
            [
                "some_one",
                "hatsune_miku",
                "vocaloid",
                "highres",
                "1girl",
                ":3"
            ]
        );
        assert_eq!(found.groups.artist, ["some_one"]);
        assert_eq!(found.groups.character, ["hatsune_miku"]);
        assert_eq!(found.groups.copyright, ["vocaloid"]);
        assert_eq!(found.groups.meta, ["highres"]);
        assert_eq!(found.rating, Some(Rating::Sensitive));
        assert_eq!(
            found.source.as_deref(),
            Some("https://art.example/works/1?a=1&b=2")
        );
        // What the page says of the picture itself.
        assert_eq!((found.width, found.height), (1365, 2048));
        assert_eq!(
            found.md5.as_deref(),
            Some("6520c64a8351872ae7d1164290ca4a3d")
        );
        assert_eq!(
            found.sample_url.as_deref(),
            Some(
                "https://img4.b.example//samples/65/20/sample_6520c64a8351872ae7d1164290ca4a3d.jpg"
            )
        );
    }

    #[test]
    fn reads_a_video_page() {
        // Gelbooru's player plays an MP4 copy first; the original is linked.
        let found = page(
            r#"<meta property="og:image" content="https://img4.b.example//images/5d/ee/a.webm" />
               <video id="gelcomVideoPlayer" poster="https://img4.b.example/images/5d/ee/a.jpg" controls loop >
               <source src="https://img4.b.example/images/5d/ee/a.mp4" type="video/mp4" />
               <source src="https://img4.b.example/images/5d/ee/a.webm" type="video/webm" /></video>
               <li><a href="https://img4.b.example/images/5d/ee/a.webm" target="_blank">Original image</a></li>"#,
        )
        .unwrap();
        assert_eq!(
            found.file_url.as_deref(),
            Some("https://img4.b.example/images/5d/ee/a.webm")
        );
        // Without that link, the player's file is still better than a still.
        let found = page(
            r#"<meta property="og:image" content="/thumb.jpg">
               <video controls><source src="/v.mp4" type="video/mp4"></video>"#,
        )
        .unwrap();
        assert_eq!(found.file_url.as_deref(), Some("https://b.example/v.mp4"));
    }

    #[test]
    fn turns_character_references_back() {
        assert_eq!(
            unescape("a &amp; b &lt;c&gt; &#39;d&#x27; &quot;"),
            "a & b <c> 'd' \""
        );
        assert_eq!(unescape("AT&T &nope; &#xzz; &"), "AT&T &nope; &#xzz; &");
    }
}
