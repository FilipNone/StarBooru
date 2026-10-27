//! Search query language.
//!
//! `blue_sky -cloud rating:g type:image order:score`
//!
//! * `tag`: post must have the tag; `-tag`: post must not have it.
//! * `rating:<g|s|q|e>`, `-rating:<..>`, `type:<image|animation|video>`.
//! * `order:<new|old|score|random>`.
//! * `status:deleted`: search the recycle bin instead of live posts.
//!
//! Any other `name:value` term is treated as a plain tag, so tags with
//! colons (`re:zero`) keep working.

use thiserror::Error;

use crate::model::{MediaKind, Rating};
use crate::tag::{TagError, normalize_tag};

pub const MAX_QUERY_TERMS: usize = 40;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Newest,
    Oldest,
    Score,
    Random,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub ratings: Vec<Rating>,
    pub exclude_ratings: Vec<Rating>,
    pub kind: Option<MediaKind>,
    pub order: Order,
    /// Search soft-deleted posts (the recycle bin).
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QueryError {
    #[error("too many search terms (max {MAX_QUERY_TERMS})")]
    TooManyTerms,
    #[error("unknown rating `{0}` (use g, s, q or e)")]
    BadRating(String),
    #[error("unknown type `{0}` (use image, animation or video)")]
    BadKind(String),
    #[error("unknown status `{0}` (use active or deleted)")]
    BadStatus(String),
    #[error("unknown order `{0}` (use new, old, score or random)")]
    BadOrder(String),
    #[error(transparent)]
    Tag(#[from] TagError),
}

pub fn parse_query(input: &str) -> Result<SearchQuery, QueryError> {
    let mut q = SearchQuery::default();
    for (i, term) in input.split_whitespace().enumerate() {
        if i >= MAX_QUERY_TERMS {
            return Err(QueryError::TooManyTerms);
        }
        let (negated, body) = match term.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (false, term),
        };
        if let Some((key, value)) = body.split_once(':') {
            match key.to_ascii_lowercase().as_str() {
                "rating" => {
                    let r = Rating::parse(value)
                        .ok_or_else(|| QueryError::BadRating(value.to_owned()))?;
                    push_unique(
                        if negated {
                            &mut q.exclude_ratings
                        } else {
                            &mut q.ratings
                        },
                        r,
                    );
                    continue;
                }
                "type" if !negated => {
                    q.kind = Some(
                        MediaKind::parse(value)
                            .ok_or_else(|| QueryError::BadKind(value.to_owned()))?,
                    );
                    continue;
                }
                "status" if !negated => {
                    q.deleted = match value.to_ascii_lowercase().as_str() {
                        "deleted" | "trash" => true,
                        "active" | "live" => false,
                        _ => return Err(QueryError::BadStatus(value.to_owned())),
                    };
                    continue;
                }
                "order" | "sort" if !negated => {
                    q.order = match value.to_ascii_lowercase().as_str() {
                        "new" | "newest" | "id_desc" => Order::Newest,
                        "old" | "oldest" | "id" | "id_asc" => Order::Oldest,
                        "score" => Order::Score,
                        "random" => Order::Random,
                        _ => return Err(QueryError::BadOrder(value.to_owned())),
                    };
                    continue;
                }
                _ => {}
            }
        }
        let tag = normalize_tag(body)?;
        push_unique(
            if negated {
                &mut q.exclude
            } else {
                &mut q.include
            },
            tag,
        );
    }
    Ok(q)
}

fn push_unique<T: PartialEq>(v: &mut Vec<T>, item: T) {
    if !v.contains(&item) {
        v.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_query() {
        let q = parse_query("Blue_Sky -cloud rating:g -rating:e type:video order:score").unwrap();
        assert_eq!(q.include, vec!["blue_sky"]);
        assert_eq!(q.exclude, vec!["cloud"]);
        assert_eq!(q.ratings, vec![Rating::General]);
        assert_eq!(q.exclude_ratings, vec![Rating::Explicit]);
        assert_eq!(q.kind, Some(MediaKind::Video));
        assert_eq!(q.order, Order::Score);
        assert!(!q.deleted);
        assert!(parse_query("status:deleted").unwrap().deleted);
    }

    #[test]
    fn unknown_meta_is_a_tag() {
        let q = parse_query("re:zero").unwrap();
        assert_eq!(q.include, vec!["re:zero"]);
    }

    #[test]
    fn empty_and_lone_dash() {
        assert_eq!(parse_query("").unwrap(), SearchQuery::default());
        assert!(parse_query("-").is_err()); // "-" alone is an invalid tag
    }

    #[test]
    fn errors_instead_of_panics() {
        assert!(matches!(
            parse_query("rating:x"),
            Err(QueryError::BadRating(_))
        ));
        assert!(matches!(
            parse_query("order:"),
            Err(QueryError::BadOrder(_))
        ));
        let long = "a ".repeat(MAX_QUERY_TERMS + 1);
        assert_eq!(parse_query(&long), Err(QueryError::TooManyTerms));
    }
}
