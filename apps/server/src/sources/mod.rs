//! Reading a whole library from another self-hosted booru. Each system gets a
//! module that says what to ask for next and turns the answers into StarBooru's
//! terms; nothing here does any I/O, so all of it is tested against recorded
//! answers. `routes::library` does the walking.
//!
//! A walk has phases (rules, posts, groups), each made of named steps, and a
//! position (`"step:rest"`) that survives a restart. What `rest` holds is up to
//! the step: usually a page or an offset. A page with nothing on it ends a step.

mod blombooru;
mod danbooru;
mod lanbooru;
mod monbooru;
mod shimmie;
mod stash;
mod szurubooru;

use base64::Engine as _;
use chrono::{DateTime, NaiveDateTime, Utc};
use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;
use starbooru_core::tag::MAX_TAG_CHARS;
use starbooru_core::{LibraryPhase, LibrarySource, Rating, normalize_tag, remote_tags};
use url::Url;

/// Posts asked for per page.
pub const PAGE: u32 = 100;
/// Largest API answer read; files have their own limit.
pub const MAX_ANSWER: usize = 32 * 1024 * 1024;
/// Longest source kept (a longer one is dropped, the post is still imported).
const MAX_SOURCE_CHARS: usize = 2048;

/// A site to read, with its account.
#[derive(Debug, Clone)]
pub struct Site {
    pub source: LibrarySource,
    pub base: Url,
    login: Option<String>,
    secret: Option<String>,
    headers: Vec<(HeaderName, HeaderValue)>,
}

/// A request to send to the site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub url: Url,
    /// A JSON body, which makes it a POST.
    pub body: Option<String>,
}

/// A post in StarBooru's terms, ready to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Its id on the other site, unique within that site.
    pub remote_id: String,
    pub file_url: Url,
    /// Already valid StarBooru tags.
    pub tags: Vec<String>,
    /// `None` when the site doesn't rate it.
    pub rating: Option<Rating>,
    pub source: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    /// The account's own opinion: 1 for a favourite or upvote, -1 for a downvote.
    pub vote: i16,
}

/// A relation between tags on the other site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    Alias { tag: String, alias: String },
    Implies { tag: String, implied: String },
    Parent { tag: String, parent: String },
}

/// Posts that belong together, listed apart from the posts themselves: they get
/// a tag here, StarBooru having no pools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub tag: String,
    pub remote_ids: Vec<String>,
}

/// One answer, read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    pub items: Vec<Item>,
    /// Posts that can't come across, with the reason.
    pub skipped: Vec<(String, String)>,
    pub rules: Vec<Rule>,
    pub groups: Vec<Group>,
    /// Remote ids of posts the account likes.
    pub likes: Vec<String>,
    /// Where to go on; `None` ends the phase.
    pub next: Option<String>,
    /// Posts on the site, when this answer says.
    pub total: Option<u64>,
    /// What this page is a page of, when one step walks several lists: two
    /// albums may well have the same contents, which isn't the site stuck.
    pub scope: String,
}

impl Answer {
    /// Whether the page had nothing useful on it.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
            && self.skipped.is_empty()
            && self.rules.is_empty()
            && self.groups.is_empty()
            && self.likes.is_empty()
    }
}

/// What a source's parser says about the step it's on.
struct Page {
    answer: Answer,
    /// The next `rest` within this step; `None` moves on to the next step.
    more: Option<String>,
}

impl Site {
    /// `None` when the address is unusable or a credential can't go in a header.
    pub fn new(
        source: LibrarySource,
        base: &str,
        login: Option<String>,
        secret: Option<String>,
    ) -> Option<Site> {
        let base = Url::parse(&format!("{base}/")).ok()?;
        if !matches!(base.scheme(), "http" | "https") {
            return None;
        }
        let headers = auth_headers(source, login.as_deref(), secret.as_deref())?;
        Some(Site {
            source,
            base,
            login,
            secret,
            headers,
        })
    }

    pub fn headers(&self) -> &[(HeaderName, HeaderValue)] {
        &self.headers
    }

    pub fn has_login(&self) -> bool {
        self.login.is_some()
    }

    /// A path on the site with a query.
    fn api<'a>(
        &self,
        path: &str,
        query: impl IntoIterator<Item = (&'a str, String)>,
    ) -> Option<Url> {
        let mut url = self.base.join(path).ok()?;
        let pairs: Vec<(&str, String)> = query.into_iter().collect();
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
        Some(url)
    }

    /// A file's address from what the site sent, which may be relative.
    fn file(&self, raw: &str) -> Option<Url> {
        let url = self.base.join(raw.trim()).ok()?;
        matches!(url.scheme(), "http" | "https").then_some(url)
    }

    /// Shimmie takes its key in the address, so files on the site need it too.
    pub fn file_address(&self, url: &Url) -> Url {
        let mut url = url.clone();
        if self.source == LibrarySource::Shimmie2
            && let Some(secret) = &self.secret
            && crate::fetch::same_site(&url, &self.base)
        {
            url.query_pairs_mut().append_pair("api_key", secret);
        }
        url
    }

    /// The steps of a phase, in order.
    fn steps(&self, phase: LibraryPhase) -> &'static [&'static str] {
        match (self.source, phase) {
            (LibrarySource::Szurubooru, LibraryPhase::Rules) => &["tags"],
            (LibrarySource::Szurubooru, LibraryPhase::Posts) => &["posts"],
            (LibrarySource::Shimmie2, LibraryPhase::Rules) => &["aliases", "autotag"],
            (LibrarySource::Shimmie2, LibraryPhase::Posts) => &["posts"],
            (LibrarySource::Danbooru, LibraryPhase::Rules) => &["aliases", "implications"],
            (LibrarySource::Danbooru, LibraryPhase::Posts) => &["posts"],
            (LibrarySource::Danbooru, LibraryPhase::Groups) if self.has_login() => {
                &["pools", "favs"]
            }
            (LibrarySource::Danbooru, LibraryPhase::Groups) => &["pools"],
            (LibrarySource::Stash, LibraryPhase::Rules) => &["tags", "performers"],
            (LibrarySource::Stash, LibraryPhase::Posts) => &["scenes", "images"],
            (LibrarySource::Blombooru, LibraryPhase::Rules) => &["tags", "implications"],
            (LibrarySource::Blombooru, LibraryPhase::Posts) => &["media"],
            (LibrarySource::Blombooru, LibraryPhase::Groups) => &["albums"],
            (LibrarySource::Monbooru, LibraryPhase::Rules) => &["implications"],
            (LibrarySource::Monbooru, LibraryPhase::Posts) => &["images"],
            (LibrarySource::Lanbooru, LibraryPhase::Posts) => &["posts"],
            _ => &[],
        }
    }

    /// The step a position is on, and the rest of it.
    fn at<'a>(&self, phase: LibraryPhase, position: &'a str) -> Option<(&'static str, &'a str)> {
        let steps = self.steps(phase);
        if position.is_empty() {
            return steps.first().map(|step| (*step, ""));
        }
        let (step, rest) = position.split_once(':')?;
        steps
            .iter()
            .find(|candidate| **candidate == step)
            .map(|step| (*step, rest))
    }

    /// The request for this position; `None` when the phase is over.
    pub fn call(&self, phase: LibraryPhase, position: &str) -> Option<Call> {
        let (step, rest) = self.at(phase, position)?;
        match self.source {
            LibrarySource::Szurubooru => szurubooru::call(self, step, rest),
            LibrarySource::Shimmie2 => shimmie::call(self, step, rest),
            LibrarySource::Danbooru => danbooru::call(self, step, rest),
            LibrarySource::Stash => stash::call(self, step, rest),
            LibrarySource::Blombooru => blombooru::call(self, step, rest),
            LibrarySource::Monbooru => monbooru::call(self, step, rest),
            LibrarySource::Lanbooru => lanbooru::call(self, step, rest),
        }
    }

    /// Reads the answer to `call(phase, position)`; `None` when it isn't what
    /// that system would send, such as a login page.
    pub fn parse(&self, phase: LibraryPhase, position: &str, body: &[u8]) -> Option<Answer> {
        let (step, rest) = self.at(phase, position)?;
        let page = match self.source {
            LibrarySource::Szurubooru => szurubooru::parse(self, step, rest, body),
            LibrarySource::Shimmie2 => shimmie::parse(self, step, rest, body),
            LibrarySource::Danbooru => danbooru::parse(self, step, rest, body),
            LibrarySource::Stash => stash::parse(self, step, rest, body),
            LibrarySource::Blombooru => blombooru::parse(self, step, rest, body),
            LibrarySource::Monbooru => monbooru::parse(self, step, rest, body),
            LibrarySource::Lanbooru => lanbooru::parse(self, step, rest, body),
        }?;
        let mut answer = page.answer;
        answer.next = match page.more {
            Some(more) => Some(format!("{step}:{more}")),
            None => self.step_after(phase, step),
        };
        Some(answer)
    }

    fn step_after(&self, phase: LibraryPhase, step: &str) -> Option<String> {
        let steps = self.steps(phase);
        let here = steps.iter().position(|candidate| *candidate == step)?;
        steps.get(here + 1).map(|next| format!("{next}:"))
    }

    /// Where to go on when this position's step can't be read.
    pub fn skip_step(&self, phase: LibraryPhase, position: &str) -> Option<String> {
        let (step, _) = self.at(phase, position)?;
        self.step_after(phase, step)
    }

    /// A separate request for the number of posts, for systems whose lists
    /// don't say.
    pub fn count_call(&self) -> Option<Call> {
        match self.source {
            LibrarySource::Danbooru => danbooru::count_call(self),
            _ => None,
        }
    }

    pub fn parse_count(&self, body: &[u8]) -> Option<u64> {
        match self.source {
            LibrarySource::Danbooru => danbooru::parse_count(body),
            _ => None,
        }
    }
}

fn auth_headers(
    source: LibrarySource,
    login: Option<&str>,
    secret: Option<&str>,
) -> Option<Vec<(HeaderName, HeaderValue)>> {
    let pair = || {
        let joined = format!("{}:{}", login?, secret?);
        Some(base64::engine::general_purpose::STANDARD.encode(joined))
    };
    let (name, value) = match source {
        LibrarySource::Szurubooru => match pair() {
            Some(token) => (AUTHORIZATION, format!("Token {token}")),
            None => return Some(Vec::new()),
        },
        LibrarySource::Danbooru => match pair() {
            Some(token) => (AUTHORIZATION, format!("Basic {token}")),
            None => return Some(Vec::new()),
        },
        LibrarySource::Stash => match secret {
            Some(key) => (HeaderName::from_static("apikey"), key.to_owned()),
            None => return Some(Vec::new()),
        },
        LibrarySource::Blombooru | LibrarySource::Monbooru => match secret {
            Some(key) => (AUTHORIZATION, format!("Bearer {key}")),
            None => return Some(Vec::new()),
        },
        // Shimmie's key goes in the address; LANBooru has none.
        LibrarySource::Shimmie2 | LibrarySource::Lanbooru => return Some(Vec::new()),
    };
    let value = HeaderValue::from_str(&value).ok()?;
    Some(vec![(name, value)])
}

/// A JSON answer, or `None` for anything else.
fn json(body: &[u8]) -> Option<Value> {
    serde_json::from_slice(body).ok()
}

/// A number, which some systems send as a string.
fn number(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.trim().parse().ok())
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

/// A list of objects' `name` fields, or the first of their `names`.
fn names(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|entry| text(entry, "name").map(str::to_owned))
        .collect()
}

/// The rest of a position as a number, starting at `first`.
fn counter(rest: &str, first: u64) -> u64 {
    rest.parse().unwrap_or(first)
}

/// `name` with the prefix that sets its category here, from what a system calls
/// that category; unknown ones stay general.
fn category_tag(category: &str, name: &str) -> String {
    let category = category.to_ascii_lowercase();
    let prefix = match category.as_str() {
        "artist" | "creator" | "author" => "artist:",
        "character" | "characters" => "character:",
        "copyright" | "series" | "franchise" => "copyright:",
        "meta" | "metadata" => "meta:",
        _ => "",
    };
    format!("{prefix}{name}")
}

/// The tag standing in for a pool, album or collection: `pool_summer_trip`.
/// Cut short rather than lost when the name is very long.
fn group_tag(prefix: &str, name: &str) -> Option<String> {
    let joined: String = format!("{prefix}_{name}")
        .chars()
        .take(MAX_TAG_CHARS)
        .collect();
    // A nameless group would leave the bare prefix as a tag on every post.
    normalize_tag(&joined).ok().filter(|tag| tag != prefix)
}

/// A post's source: the first line that isn't empty.
fn clean_source(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    (line.chars().count() <= MAX_SOURCE_CHARS).then(|| line.to_owned())
}

/// A date as these systems write them; without a zone, as UTC.
fn parse_date(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(date) = DateTime::parse_from_rfc3339(raw) {
        return Some(date.with_timezone(&Utc));
    }
    ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"]
        .into_iter()
        .find_map(|format| NaiveDateTime::parse_from_str(raw, format).ok())
        .map(|date| date.and_utc())
}

/// Valid tags from a list of raw ones.
fn tags<'a>(raw: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    remote_tags(raw)
}

#[cfg(test)]
pub(crate) fn test_site(source: LibrarySource, login: Option<&str>, secret: Option<&str>) -> Site {
    Site::new(
        source,
        "http://booru.test/sub",
        login.map(str::to_owned),
        secret.map(str::to_owned),
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_become_the_right_header() {
        let header = |source, login, secret| {
            let site = test_site(source, login, secret);
            let list = site.headers();
            list.first()
                .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_owned()))
        };
        // base64("me:key") = bWU6a2V5
        assert_eq!(
            header(LibrarySource::Szurubooru, Some("me"), Some("key")),
            Some(("authorization".into(), "Token bWU6a2V5".into()))
        );
        assert_eq!(
            header(LibrarySource::Danbooru, Some("me"), Some("key")),
            Some(("authorization".into(), "Basic bWU6a2V5".into()))
        );
        assert_eq!(
            header(LibrarySource::Stash, None, Some("key")),
            Some(("apikey".into(), "key".into()))
        );
        assert_eq!(
            header(LibrarySource::Blombooru, None, Some("key")),
            Some(("authorization".into(), "Bearer key".into()))
        );
        assert_eq!(header(LibrarySource::Monbooru, None, None), None);
        assert_eq!(header(LibrarySource::Lanbooru, None, Some("key")), None);
        assert_eq!(header(LibrarySource::Shimmie2, None, Some("key")), None);
    }

    #[test]
    fn a_key_that_cannot_be_sent_is_refused() {
        let bad = Site::new(
            LibrarySource::Stash,
            "http://x",
            None,
            Some("line\nbreak".into()),
        );
        assert!(bad.is_none());
        assert!(Site::new(LibrarySource::Stash, "ftp://x", None, None).is_none());
    }

    #[test]
    fn shimmie_keys_stay_on_the_site() {
        let site = test_site(LibrarySource::Shimmie2, None, Some("k"));
        let home = Url::parse("http://booru.test/_images/a.png").unwrap();
        assert_eq!(
            site.file_address(&home).as_str(),
            "http://booru.test/_images/a.png?api_key=k"
        );
        let cdn = Url::parse("http://cdn.test/a.png").unwrap();
        assert_eq!(site.file_address(&cdn), cdn);
    }

    #[test]
    fn steps_follow_each_other_and_end_the_phase() {
        let site = test_site(LibrarySource::Danbooru, None, None);
        assert_eq!(
            site.skip_step(LibraryPhase::Rules, ""),
            Some("implications:".into())
        );
        assert_eq!(site.skip_step(LibraryPhase::Rules, "implications:a5"), None);
        assert!(
            site.call(LibraryPhase::Groups, "favs:").is_none(),
            "no login"
        );
        let lan = test_site(LibrarySource::Lanbooru, None, None);
        assert!(lan.call(LibraryPhase::Rules, "").is_none());
        assert!(lan.call(LibraryPhase::Groups, "").is_none());
        assert!(lan.call(LibraryPhase::Posts, "nonsense:1").is_none());
    }

    #[test]
    fn a_group_without_a_name_makes_no_tag() {
        assert_eq!(
            group_tag("pool", "Summer Trip").as_deref(),
            Some("pool_summer_trip")
        );
        for name in ["", "  ", "___"] {
            assert_eq!(group_tag("collection", name), None, "{name:?}");
        }
    }

    #[test]
    fn helpers_read_what_systems_send() {
        assert_eq!(category_tag("Artist", "x"), "artist:x");
        assert_eq!(category_tag("series", "x"), "copyright:x");
        assert_eq!(category_tag("costumes", "x"), "x");
        assert_eq!(
            group_tag("pool", "Summer Trip").as_deref(),
            Some("pool_summer_trip")
        );
        let long = group_tag("pool", &"x".repeat(500)).unwrap();
        assert_eq!(long.chars().count(), MAX_TAG_CHARS);
        assert_eq!(
            clean_source("\n  https://a.b/1 \nhttps://c.d/2").as_deref(),
            Some("https://a.b/1")
        );
        assert_eq!(clean_source(" \n "), None);
        assert_eq!(clean_source(&"x".repeat(MAX_SOURCE_CHARS + 1)), None);
        for date in [
            "2020-01-02T03:04:05Z",
            "2020-01-02T04:04:05.000+01:00",
            "2020-01-02 03:04:05",
            "2020-01-02T03:04:05.000",
        ] {
            assert_eq!(
                parse_date(date).unwrap().to_rfc3339(),
                "2020-01-02T03:04:05+00:00",
                "{date}"
            );
        }
        assert_eq!(parse_date("yesterday"), None);
        assert_eq!(number(&serde_json::json!("12")), Some(12));
        assert_eq!(number(&serde_json::json!(12)), Some(12));
        assert_eq!(number(&serde_json::json!("x")), None);
    }
}
