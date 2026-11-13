//! What each kind of site allows, as its documentation says or as it answered
//! when asked (November 2026). The server keeps to these, so a search never
//! costs the user their welcome on a site; whatever a site refuses anyway is
//! passed on in the site's own words.

use super::{BooruApi, REMOTE_PAGE_SIZE};

/// The limits of one API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The deepest page of `REMOTE_PAGE_SIZE` posts a search may ask for.
    pub max_page: u32,
    /// How many tags a search takes without an account; `None` when the site
    /// sets no limit worth knowing. `rating:` doesn't count.
    pub anonymous_tags: Option<u32>,
    /// The least time between two requests to the site, in milliseconds.
    pub interval_ms: u32,
}

impl BooruApi {
    /// What this kind of site allows.
    pub fn limits(self) -> Limits {
        match self {
            // 10 requests a second for reads, about one a second sustained;
            // nothing beyond page 1000; two tags without an account.
            BooruApi::Danbooru => Limits {
                max_page: 1000,
                anonymous_tags: Some(2),
                interval_ms: 500,
            },
            // Undocumented; its file servers count 100 requests at a time.
            // Results stop 20,000 posts in.
            BooruApi::Gelbooru => Limits {
                max_page: 20_000 / REMOTE_PAGE_SIZE,
                anonymous_tags: None,
                interval_ms: 500,
            },
            // Undocumented, and it throttles (HTTP 421). Konachan takes six
            // tags and yande.re more, so the site is left to say.
            BooruApi::Moebooru => Limits {
                max_page: 1000,
                anonymous_tags: None,
                interval_ms: 500,
            },
            // A hard two requests a second, and one a second asked for;
            // nothing beyond page 750; forty tags.
            BooruApi::E621 => Limits {
                max_page: 750,
                anonymous_tags: Some(40),
                interval_ms: 1000,
            },
            // One's own server.
            BooruApi::Blombooru => Limits {
                max_page: 1000,
                anonymous_tags: None,
                interval_ms: 50,
            },
        }
    }
}

/// How many of a search's words count towards a site's tag limit.
pub fn counted_tags(search: &str) -> u32 {
    let counted = search
        .split_whitespace()
        .filter(|word| !word.trim_start_matches('-').starts_with("rating:"))
        .count();
    u32::try_from(counted).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::MAX_REMOTE_PAGE;

    #[test]
    fn every_api_has_limits_that_make_sense() {
        for api in BooruApi::ALL {
            let limits = api.limits();
            assert!((1..=MAX_REMOTE_PAGE).contains(&limits.max_page), "{api:?}");
            assert!(limits.interval_ms <= 5000, "{api:?}");
            assert_ne!(limits.anonymous_tags, Some(0), "{api:?}");
        }
        // Gelbooru stops 20,000 posts in, whatever the page size.
        let deepest = BooruApi::Gelbooru.limits().max_page;
        assert!(deepest * REMOTE_PAGE_SIZE <= 20_000);
        // e621 bans clients that go over two requests a second.
        assert!(BooruApi::E621.limits().interval_ms > 500);
    }

    #[test]
    fn a_rating_is_free() {
        assert_eq!(counted_tags(""), 0);
        assert_eq!(counted_tags(" blue_sky  cloud "), 2);
        assert_eq!(counted_tags("blue_sky rating:g -rating:e order:score"), 2);
    }
}
