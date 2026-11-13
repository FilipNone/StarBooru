//! One search for several sites. Tags are the same nearly everywhere, but each
//! kind of site writes ratings and sorting its own way; a search written the
//! StarBooru way is turned into each site's before it goes out. Anything in a
//! site's own short forms (`rating:s`, which is *sensitive* on Danbooru and
//! *safe* elsewhere) is left exactly as typed.

use super::BooruApi;

/// A rating by its full name, as the site spells it.
fn rating(api: BooruApi, word: &str) -> Option<&'static str> {
    // Danbooru's scale has four steps; the older one has no "sensitive", and
    // what Danbooru calls that is "questionable" there.
    let (danbooru, gelbooru, older) = match word {
        "safe" | "general" => ("g", "general", "s"),
        "sensitive" => ("s", "sensitive", "q"),
        "questionable" => ("q", "questionable", "q"),
        "explicit" => ("e", "explicit", "e"),
        _ => return None,
    };
    Some(match api {
        BooruApi::Danbooru => danbooru,
        BooruApi::Gelbooru => gelbooru,
        BooruApi::Moebooru | BooruApi::E621 | BooruApi::Blombooru => older,
    })
}

/// An ordering as the site spells it: `order:score` nearly everywhere,
/// `sort:score:desc` on Gelbooru.
fn ordering(api: BooruApi, key: &str, value: &str) -> Option<String> {
    let (field, way) = match key {
        "order" => match value.strip_suffix("_asc") {
            Some(field) => (field, "asc"),
            None => (value.strip_suffix("_desc").unwrap_or(value), "desc"),
        },
        "sort" => value.split_once(':').unwrap_or((value, "desc")),
        _ => return None,
    };
    if field.is_empty() || !matches!(way, "asc" | "desc") {
        return None;
    }
    Some(match (api, way) {
        (BooruApi::Gelbooru, _) => format!("sort:{field}:{way}"),
        (_, "asc") => format!("order:{field}_asc"),
        _ => format!("order:{field}"),
    })
}

/// `search` as the site of this kind writes it.
pub fn for_site(api: BooruApi, search: &str) -> String {
    let words = search.split_whitespace().map(|word| {
        let (sign, term) = match word.strip_prefix('-') {
            Some(term) => ("-", term),
            None => ("", word),
        };
        let Some((key, value)) = term.split_once(':') else {
            return word.to_owned();
        };
        let key = key.to_ascii_lowercase();
        let value = value.to_ascii_lowercase();
        let own = match key.as_str() {
            "rating" => rating(api, &value).map(|own| format!("rating:{own}")),
            "order" | "sort" => ordering(api, &key, &value),
            _ => None,
        };
        own.map_or_else(|| word.to_owned(), |own| format!("{sign}{own}"))
    });
    words.collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_ratings_each_sites_way() {
        let q = "blue_sky rating:safe -rating:Explicit";
        assert_eq!(
            for_site(BooruApi::Danbooru, q),
            "blue_sky rating:g -rating:e"
        );
        assert_eq!(
            for_site(BooruApi::Gelbooru, q),
            "blue_sky rating:general -rating:explicit"
        );
        assert_eq!(
            for_site(BooruApi::Moebooru, q),
            "blue_sky rating:s -rating:e"
        );
        assert_eq!(for_site(BooruApi::E621, "rating:sensitive"), "rating:q");
        assert_eq!(for_site(BooruApi::Danbooru, "rating:sensitive"), "rating:s");
        assert_eq!(for_site(BooruApi::Blombooru, "rating:general"), "rating:s");
    }

    #[test]
    fn leaves_a_sites_own_words_alone() {
        for q in [
            "rating:s",
            "rating:g score:>10",
            "id:123 user:someone",
            "{a ~ b} -c *_hair",
            "rating:unknown order: sort::",
            "",
        ] {
            for api in BooruApi::ALL {
                assert_eq!(for_site(api, q), q, "{api:?}");
            }
        }
        assert_eq!(for_site(BooruApi::Danbooru, "  a   b "), "a b");
    }

    #[test]
    fn writes_sorting_each_sites_way() {
        assert_eq!(
            for_site(BooruApi::Gelbooru, "a order:score"),
            "a sort:score:desc"
        );
        assert_eq!(for_site(BooruApi::Gelbooru, "order:id_asc"), "sort:id:asc");
        assert_eq!(
            for_site(BooruApi::Gelbooru, "sort:score:asc"),
            "sort:score:asc"
        );
        assert_eq!(
            for_site(BooruApi::Danbooru, "sort:score:desc"),
            "order:score"
        );
        assert_eq!(for_site(BooruApi::Danbooru, "sort:score"), "order:score");
        assert_eq!(for_site(BooruApi::E621, "sort:id:asc"), "order:id_asc");
        assert_eq!(
            for_site(BooruApi::Moebooru, "order:score_desc"),
            "order:score"
        );
        assert_eq!(
            for_site(BooruApi::Danbooru, "sort:score:sideways"),
            "sort:score:sideways"
        );
    }
}
