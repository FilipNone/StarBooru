//! Browsing other boorus: the APIs StarBooru understands, the request a
//! client sends to search one through the server, and the shape every
//! site's posts are turned into.

mod limits;
mod link;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{Rating, TagCategory};
use crate::tag::{MAX_TAGS_PER_POST, normalize_tag};

pub use limits::{Limits, counted_tags};
pub use link::{PostLink, PostRef, post_link};

/// Posts asked for per page of a remote search.
pub const REMOTE_PAGE_SIZE: u32 = 40;
/// Deepest page a remote search may ask for (Danbooru's limit without an
/// account).
pub const MAX_REMOTE_PAGE: u32 = 1000;
/// Longest search, credentials included, sent to another site.
pub const MAX_REMOTE_FIELD: usize = 512;

/// The booru APIs StarBooru speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BooruApi {
    /// Danbooru 2 (`/posts.json`).
    Danbooru,
    /// Gelbooru 0.2 and its forks such as Safebooru (`/index.php?page=dapi`).
    Gelbooru,
    /// Moebooru: yande.re, Konachan (`/post.json`).
    Moebooru,
    /// e621 and e926 (`/posts.json` with grouped tags).
    E621,
    /// Blombooru, self-hosted: Danbooru's API, but `s` means safe.
    Blombooru,
}

impl BooruApi {
    pub const ALL: [BooruApi; 5] = [
        BooruApi::Danbooru,
        BooruApi::Gelbooru,
        BooruApi::Moebooru,
        BooruApi::E621,
        BooruApi::Blombooru,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BooruApi::Danbooru => "danbooru",
            BooruApi::Gelbooru => "gelbooru",
            BooruApi::Moebooru => "moebooru",
            BooruApi::E621 => "e621",
            BooruApi::Blombooru => "blombooru",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BooruApi::Danbooru => "Danbooru",
            BooruApi::Gelbooru => "Gelbooru",
            BooruApi::Moebooru => "Moebooru",
            BooruApi::E621 => "e621",
            BooruApi::Blombooru => "Blombooru",
        }
    }

    /// What the site calls the account field sent with the API key.
    pub fn login_field(self) -> &'static str {
        match self {
            BooruApi::Gelbooru => "user_id",
            BooruApi::Danbooru | BooruApi::Moebooru | BooruApi::E621 | BooruApi::Blombooru => {
                "login"
            }
        }
    }

    /// What the site calls the key sent with the login.
    pub fn key_field(self) -> &'static str {
        match self {
            BooruApi::Moebooru => "password_hash",
            BooruApi::Danbooru | BooruApi::Gelbooru | BooruApi::E621 | BooruApi::Blombooru => {
                "api_key"
            }
        }
    }
}

/// A well-known site, offered as a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooruSite {
    pub name: &'static str,
    pub api: BooruApi,
    pub base: &'static str,
}

/// Presets; any other site running one of the APIs works too.
pub const SITES: [BooruSite; 6] = [
    BooruSite {
        name: "Danbooru",
        api: BooruApi::Danbooru,
        base: "https://danbooru.donmai.us",
    },
    BooruSite {
        name: "Safebooru",
        api: BooruApi::Gelbooru,
        base: "https://safebooru.org",
    },
    BooruSite {
        name: "Gelbooru",
        api: BooruApi::Gelbooru,
        base: "https://gelbooru.com",
    },
    BooruSite {
        name: "yande.re",
        api: BooruApi::Moebooru,
        base: "https://yande.re",
    },
    BooruSite {
        name: "Konachan",
        api: BooruApi::Moebooru,
        base: "https://konachan.net",
    },
    BooruSite {
        name: "e926",
        api: BooruApi::E621,
        base: "https://e926.net",
    },
];

/// `POST /remote/search`: one page of a search on another booru, fetched by
/// the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSearch {
    pub api: BooruApi,
    /// The site's address, like `https://danbooru.donmai.us`.
    pub base: String,
    /// The search in the site's own syntax.
    #[serde(default)]
    pub tags: String,
    /// 1-based.
    #[serde(default = "first_page")]
    pub page: u32,
    /// Optional account on that site; sent only to it, never stored by the
    /// server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

fn first_page() -> u32 {
    1
}

/// Which of a post's tags aren't general ones. Every name here is in the
/// post's `tags` too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagGroups {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub artist: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub copyright: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub character: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub meta: Vec<String>,
}

impl TagGroups {
    pub fn is_empty(&self) -> bool {
        self.artist.is_empty()
            && self.copyright.is_empty()
            && self.character.is_empty()
            && self.meta.is_empty()
    }

    /// The category of one of the post's tags.
    pub fn category(&self, tag: &str) -> TagCategory {
        let has = |list: &[String]| list.iter().any(|t| t == tag);
        if has(&self.artist) {
            TagCategory::Artist
        } else if has(&self.copyright) {
            TagCategory::Copyright
        } else if has(&self.character) {
            TagCategory::Character
        } else if has(&self.meta) {
            TagCategory::Meta
        } else {
            TagCategory::General
        }
    }

    /// `tags` as an import sends them: artists, copyrights and characters
    /// first, each with the prefix that sets its category here.
    pub fn import_tags(&self, tags: &[String]) -> Vec<String> {
        let prefixed = |list: &[String], prefix: &str| -> Vec<String> {
            list.iter()
                .filter(|tag| tags.contains(tag))
                .map(|tag| format!("{prefix}{tag}"))
                .collect()
        };
        let mut out = prefixed(&self.artist, "artist:");
        out.extend(prefixed(&self.copyright, "copyright:"));
        out.extend(prefixed(&self.character, "character:"));
        let general = tags
            .iter()
            .filter(|tag| self.category(tag) == TagCategory::General);
        out.extend(general.cloned());
        out.extend(prefixed(&self.meta, "meta:"));
        out
    }

    /// Files `tag` under `category`, once.
    pub fn put(&mut self, category: TagCategory, tag: &str) {
        let list = match category {
            TagCategory::Artist => &mut self.artist,
            TagCategory::Copyright => &mut self.copyright,
            TagCategory::Character => &mut self.character,
            TagCategory::Meta => &mut self.meta,
            TagCategory::General => return,
        };
        if !list.iter().any(|t| t == tag) {
            list.push(tag.to_owned());
        }
    }
}

/// A post on another booru, in StarBooru's terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePost {
    pub id: u64,
    /// The site it is on, as its address.
    #[serde(default)]
    pub site: String,
    /// The post's page on that site, kept as the source when imported.
    pub page_url: String,
    pub preview_url: String,
    /// A copy sized for a screen, when the site makes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_url: Option<String>,
    /// The original file; what an import downloads.
    pub file_url: String,
    /// Already valid StarBooru tags; the site's others are dropped.
    pub tags: Vec<String>,
    /// Which of them are artists, characters and so on, when the site says.
    #[serde(default, skip_serializing_if = "TagGroups::is_empty")]
    pub groups: TagGroups,
    pub rating: Rating,
    pub width: u32,
    pub height: u32,
    /// The file's MD5, which is how the same file is known on two sites.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    /// Where the site says the picture comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// The original's size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
}

impl RemotePost {
    /// The tags to import it with (`TagGroups::import_tags`).
    pub fn import_tags(&self) -> Vec<String> {
        self.groups.import_tags(&self.tags)
    }

    /// Whether the file is a video, going by its address.
    pub fn is_video(&self) -> bool {
        let path = self.file_url.split(['?', '#']).next().unwrap_or_default();
        let ext = path
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        matches!(ext.as_str(), "mp4" | "webm" | "mkv" | "mov" | "m4v")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePage {
    pub posts: Vec<RemotePost>,
    /// Whether a next page may have more.
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteError {
    #[error("enter the site's http or https address")]
    BadBase,
    #[error("pages go from 1 to {MAX_REMOTE_PAGE}")]
    BadPage,
    #[error("{site} doesn't show results past page {max}; narrow the search")]
    TooDeep { site: &'static str, max: u32 },
    #[error(
        "{site} takes {max} tags in a search without an account (rating: doesn't count); add your account or search for fewer"
    )]
    TooManyTags { site: &'static str, max: u32 },
    #[error("the search or account details are too long")]
    TooLong,
}

/// A site's address without a trailing slash, when it is http or https and
/// has neither credentials nor a query.
pub fn clean_base(raw: &str) -> Result<String, RemoteError> {
    let base = raw.trim().trim_end_matches('/');
    let rest = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .ok_or(RemoteError::BadBase)?;
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty()
        || host.contains('@')
        || rest.contains(['?', '#'])
        || rest.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(RemoteError::BadBase);
    }
    if base.len() > MAX_REMOTE_FIELD {
        return Err(RemoteError::TooLong);
    }
    Ok(base.to_owned())
}

impl RemoteSearch {
    /// Checks the request and trims it: the address without a trailing
    /// slash, and empty credentials as none.
    pub fn clean(mut self) -> Result<RemoteSearch, RemoteError> {
        self.base = clean_base(&self.base)?;
        if !(1..=MAX_REMOTE_PAGE).contains(&self.page) {
            return Err(RemoteError::BadPage);
        }
        self.tags = self.tags.trim().to_owned();
        let filled = |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        self.login = filled(self.login);
        self.api_key = filled(self.api_key);
        let fields = [Some(&self.tags), self.login.as_ref(), self.api_key.as_ref()];
        if fields
            .into_iter()
            .flatten()
            .any(|f| f.len() > MAX_REMOTE_FIELD)
        {
            return Err(RemoteError::TooLong);
        }
        let limits = self.api.limits();
        let site = self.api.name();
        if self.page > limits.max_page {
            return Err(RemoteError::TooDeep {
                site,
                max: limits.max_page,
            });
        }
        let signed_in = self.login.is_some() && self.api_key.is_some();
        if let Some(max) = limits.anonymous_tags
            && !signed_in
            && counted_tags(&self.tags) > max
        {
            return Err(RemoteError::TooManyTags { site, max });
        }
        Ok(self)
    }
}

/// The tags of a remote post that are valid here, normalised and without
/// repeats. Others (a site's `-_-`, say) are dropped instead of refusing
/// the whole post.
pub fn remote_tags<'a>(raw: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in raw.into_iter().filter_map(|t| normalize_tag(t).ok()) {
        if out.len() >= MAX_TAGS_PER_POST {
            break;
        }
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(base: &str) -> RemoteSearch {
        RemoteSearch {
            api: BooruApi::Danbooru,
            base: base.into(),
            tags: " blue_sky ".into(),
            page: 1,
            login: Some("  ".into()),
            api_key: Some(" key ".into()),
        }
    }

    #[test]
    fn cleans_a_search() {
        let s = search("https://danbooru.donmai.us/ ").clean().unwrap();
        assert_eq!(s.base, "https://danbooru.donmai.us");
        assert_eq!(s.tags, "blue_sky");
        assert_eq!(s.login, None);
        assert_eq!(s.api_key.as_deref(), Some("key"));
        assert!(search("http://192.168.1.4:8080/booru").clean().is_ok());
    }

    #[test]
    fn refuses_bad_addresses() {
        for base in [
            "",
            "danbooru.donmai.us",
            "ftp://x.org",
            "https://",
            "https:///path",
            "https://user:pw@x.org",
            "https://x.org/?a=1",
            "https://x.org/#top",
            "https://x .org",
        ] {
            assert_eq!(search(base).clean(), Err(RemoteError::BadBase), "{base}");
        }
    }

    #[test]
    fn refuses_bad_pages_and_long_fields() {
        let mut s = search("https://x.org");
        s.page = 0;
        assert_eq!(s.clone().clean(), Err(RemoteError::BadPage));
        s.page = MAX_REMOTE_PAGE + 1;
        assert_eq!(s.clone().clean(), Err(RemoteError::BadPage));
        s.page = MAX_REMOTE_PAGE;
        s.tags = "a".repeat(MAX_REMOTE_FIELD + 1);
        assert_eq!(s.clean(), Err(RemoteError::TooLong));
    }

    #[test]
    fn keeps_to_each_sites_limits() {
        let search = |api, tags: &str, page, account: bool| RemoteSearch {
            api,
            base: "https://x.org".into(),
            tags: tags.into(),
            page,
            login: account.then(|| "me".to_owned()),
            api_key: account.then(|| "key".to_owned()),
        };
        // Danbooru: two tags without an account, and `rating:` is free.
        let three = "a b c";
        assert_eq!(
            search(BooruApi::Danbooru, three, 1, false).clean(),
            Err(RemoteError::TooManyTags {
                site: "Danbooru",
                max: 2
            })
        );
        assert!(
            search(BooruApi::Danbooru, "a b rating:g", 1, false)
                .clean()
                .is_ok()
        );
        // With an account the site decides: a paid one takes more.
        assert!(search(BooruApi::Danbooru, three, 1, true).clean().is_ok());
        assert!(search(BooruApi::Gelbooru, three, 1, false).clean().is_ok());
        // How deep each goes.
        assert!(search(BooruApi::Danbooru, "", 1000, false).clean().is_ok());
        assert_eq!(
            search(BooruApi::E621, "", 751, false).clean(),
            Err(RemoteError::TooDeep {
                site: "e621",
                max: 750
            })
        );
        assert!(search(BooruApi::Gelbooru, "", 500, false).clean().is_ok());
        assert!(search(BooruApi::Gelbooru, "", 501, true).clean().is_err());
        let message = search(BooruApi::Danbooru, three, 1, false)
            .clean()
            .unwrap_err()
            .to_string();
        assert!(message.contains("Danbooru takes 2 tags"), "{message}");
    }

    #[test]
    fn page_defaults_to_one() {
        let s: RemoteSearch =
            serde_json::from_str(r#"{"api":"e621","base":"https://e926.net"}"#).unwrap();
        assert_eq!((s.page, s.tags.as_str(), s.api), (1, "", BooruApi::E621));
    }

    #[test]
    fn keeps_only_valid_tags() {
        let tags = remote_tags(["Blue_Sky", "-_-", "~x", "blue_sky", "1girl", "\u{7}bell"]);
        assert_eq!(tags, ["blue_sky", "1girl"]);
        let many: Vec<String> = (0..MAX_TAGS_PER_POST + 10)
            .map(|i| format!("t{i}"))
            .collect();
        assert_eq!(
            remote_tags(many.iter().map(String::as_str)).len(),
            MAX_TAGS_PER_POST
        );
    }

    fn post(tags: &[&str]) -> RemotePost {
        RemotePost {
            id: 1,
            site: "https://d.org".into(),
            page_url: "https://d.org/posts/1".into(),
            preview_url: "https://d.org/p.jpg".into(),
            sample_url: None,
            file_url: "https://d.org/a.jpg".into(),
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            groups: TagGroups::default(),
            rating: Rating::default(),
            width: 1,
            height: 1,
            md5: None,
            source: None,
            score: None,
            created_at: None,
            file_size: None,
        }
    }

    #[test]
    fn import_tags_carry_their_category() {
        let mut p = post(&["sky", "someone", "miku", "vocaloid", "highres"]);
        assert_eq!(p.import_tags(), p.tags);
        p.groups.put(TagCategory::Artist, "someone");
        p.groups.put(TagCategory::Artist, "someone");
        p.groups.put(TagCategory::Character, "miku");
        p.groups.put(TagCategory::Copyright, "vocaloid");
        p.groups.put(TagCategory::Meta, "highres");
        p.groups.put(TagCategory::General, "sky");
        // A name the post doesn't carry is left out.
        p.groups.put(TagCategory::Artist, "nobody");
        assert_eq!(
            p.import_tags(),
            [
                "artist:someone",
                "copyright:vocaloid",
                "character:miku",
                "sky",
                "meta:highres"
            ]
        );
        assert_eq!(p.groups.category("miku"), TagCategory::Character);
        assert_eq!(p.groups.category("sky"), TagCategory::General);
        // Every one of them is a tag StarBooru takes.
        for tag in p.import_tags() {
            assert_eq!(normalize_tag(&tag).as_deref(), Ok(tag.as_str()));
        }
    }

    #[test]
    fn older_answers_still_read() {
        let p: RemotePost = serde_json::from_str(
            r#"{"id":1,"page_url":"a","preview_url":"b","file_url":"c","tags":["x"],
                "rating":"safe","width":1,"height":2}"#,
        )
        .unwrap();
        assert_eq!((&p.md5, &p.sample_url, p.site.as_str()), (&None, &None, ""));
        assert!(p.groups.is_empty());
        let json = serde_json::to_value(&p).unwrap();
        assert!(json.get("groups").is_none() && json.get("md5").is_none());
    }

    #[test]
    fn tells_videos_by_their_address() {
        let mut p = post(&[]);
        assert!(!p.is_video());
        p.file_url = "https://d.org/a.WEBM?x=1.jpg".into();
        assert!(p.is_video());
    }

    #[test]
    fn presets_are_clean() {
        for site in SITES {
            let s = RemoteSearch {
                api: site.api,
                base: site.base.into(),
                tags: String::new(),
                page: 1,
                login: None,
                api_key: None,
            };
            assert_eq!(s.clean().unwrap().base, site.base);
        }
    }

    #[test]
    fn every_api_round_trips() {
        for api in BooruApi::ALL {
            let json = serde_json::to_string(&api).unwrap();
            assert_eq!(json, format!("\"{}\"", api.as_str()));
            assert_eq!(serde_json::from_str::<BooruApi>(&json).unwrap(), api);
        }
    }
}
