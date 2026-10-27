//! Browsing other boorus: the APIs StarBooru understands, the request a
//! client sends to search one through the server, and the shape every
//! site's posts are turned into.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::Rating;
use crate::tag::{MAX_TAGS_PER_POST, normalize_tag};

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

/// A post on another booru, in StarBooru's terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePost {
    pub id: u64,
    /// The post's page on that site, kept as the source when imported.
    pub page_url: String,
    pub preview_url: String,
    /// The original file; what an import downloads.
    pub file_url: String,
    /// Already valid StarBooru tags; the site's others are dropped.
    pub tags: Vec<String>,
    pub rating: Rating,
    pub width: u32,
    pub height: u32,
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
    #[error("the search or account details are too long")]
    TooLong,
}

impl RemoteSearch {
    /// Checks the request and trims it: the address without a trailing
    /// slash, and empty credentials as none.
    pub fn clean(mut self) -> Result<RemoteSearch, RemoteError> {
        let base = self.base.trim().trim_end_matches('/');
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
        self.base = base.to_owned();
        if !(1..=MAX_REMOTE_PAGE).contains(&self.page) {
            return Err(RemoteError::BadPage);
        }
        self.tags = self.tags.trim().to_owned();
        let filled = |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        self.login = filled(self.login);
        self.api_key = filled(self.api_key);
        let fields = [
            Some(&self.base),
            Some(&self.tags),
            self.login.as_ref(),
            self.api_key.as_ref(),
        ];
        if fields
            .into_iter()
            .flatten()
            .any(|f| f.len() > MAX_REMOTE_FIELD)
        {
            return Err(RemoteError::TooLong);
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
