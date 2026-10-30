//! Search query language.
//!
//! `blue_sky -cloud rating:g type:image order:score`
//!
//! * `tag`: post must have the tag; `-tag`: post must not have it.
//! * `rating:<g|s|q|e>`, `-rating:<..>`, `type:<image|animation|video>`.
//! * `order:<new|old|score|size|pixels|longside|tags|updated|random>`, with an
//!   optional `_asc` or `_desc` suffix.
//! * `status:deleted`: search the recycle bin instead of live posts.
//! * `text:word` or `text:"several words"`: the text OCR found in the image
//!   has those words; `-text:` excludes. Several `text:` terms all apply.
//! * `date:2024-03`, `date:2023..2024`, `date:>30d`: the file's date, or the
//!   upload date when it has none. `width:>1920`, `height:`, `mpixels:>8`,
//!   `ratio:16:9`: its size. The forms are in `crate::filter`; `-` excludes.
//! * `color:red`, `color:2060e0`: the picture has that colour, or the nearest
//!   one stored. `-color:` excludes. The forms are in `crate::color`.
//! * `{girl blonde_hair boots}`: one person in the picture has all of
//!   these tags (and is a girl; `boy` and `other` work too). `-{...}`: no
//!   one does.
//!
//! Any other `name:value` term is treated as a plain tag, so tags with
//! colons (`re:zero`) keep working.

use thiserror::Error;

use crate::color::{ColorTerm, parse_color};
use crate::figure::FigureKind;
use crate::filter::{DateTerm, SizeField, SizeTerm, parse_date_term, parse_size_term};
use crate::model::{MediaKind, Rating};
use crate::tag::{TagError, normalize_tag};

pub const MAX_QUERY_TERMS: usize = 40;
/// Longest `text:` value, in characters.
pub const MAX_TEXT_CHARS: usize = 200;

/// What results are sorted by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortKey {
    /// Upload order (the post id).
    #[default]
    Id,
    Score,
    /// File size in bytes.
    Size,
    /// Width × height; posts without dimensions sort last.
    Pixels,
    /// The longer of width and height; posts without dimensions sort last.
    LongSide,
    TagCount,
    /// Last edit of tags, rating or source.
    Updated,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    pub key: SortKey,
    pub descending: bool,
}

impl Default for Order {
    fn default() -> Self {
        Self {
            key: SortKey::Id,
            descending: true,
        }
    }
}

impl Order {
    /// Parses the value of an `order:` term, e.g. `score`, `size_asc`, `old`.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.to_ascii_lowercase();
        let (name, direction) = if let Some(n) = value.strip_suffix("_asc") {
            (n, Some(false))
        } else if let Some(n) = value.strip_suffix("_desc") {
            (n, Some(true))
        } else {
            (value.as_str(), None)
        };
        let (key, descending) = match name {
            "new" | "newest" => (SortKey::Id, true),
            "id" | "old" | "oldest" => (SortKey::Id, false),
            "score" | "top" => (SortKey::Score, true),
            "size" | "filesize" => (SortKey::Size, true),
            "pixels" | "resolution" | "dimensions" | "mpixels" | "megapixels" => {
                (SortKey::Pixels, true)
            }
            "longside" | "long_side" => (SortKey::LongSide, true),
            "tags" | "tagcount" => (SortKey::TagCount, true),
            "updated" | "edited" => (SortKey::Updated, true),
            "random" if direction.is_none() => (SortKey::Random, true),
            _ => return None,
        };
        Some(Self {
            key,
            descending: direction.unwrap_or(descending),
        })
    }

    /// The canonical `order:` value, e.g. `size_asc`. Round-trips through `parse`.
    pub fn as_value(self) -> &'static str {
        match (self.key, self.descending) {
            (SortKey::Id, true) => "new",
            (SortKey::Id, false) => "old",
            (SortKey::Score, true) => "score",
            (SortKey::Score, false) => "score_asc",
            (SortKey::Size, true) => "size",
            (SortKey::Size, false) => "size_asc",
            (SortKey::Pixels, true) => "pixels",
            (SortKey::Pixels, false) => "pixels_asc",
            (SortKey::LongSide, true) => "longside",
            (SortKey::LongSide, false) => "longside_asc",
            (SortKey::TagCount, true) => "tags",
            (SortKey::TagCount, false) => "tags_asc",
            (SortKey::Updated, true) => "updated",
            (SortKey::Updated, false) => "updated_asc",
            (SortKey::Random, _) => "random",
        }
    }
}

/// Replaces the `order:`/`sort:` terms of a query string with `order`
/// (or removes them for `None`), keeping every other term as typed.
pub fn with_order(query: &str, order: Option<Order>) -> String {
    let mut terms: Vec<String> = query
        .split_whitespace()
        .filter(|t| {
            let lower = t.to_ascii_lowercase();
            !(lower.starts_with("order:") || lower.starts_with("sort:"))
        })
        .map(str::to_owned)
        .collect();
    if let Some(order) = order.filter(|o| *o != Order::default()) {
        terms.push(format!("order:{}", order.as_value()));
    }
    terms.join(" ")
}

/// Replaces the `color:` terms of a query string with `color:<value>` (or
/// removes them for `None`), keeping every other term as typed.
pub fn with_color(query: &str, value: Option<&str>) -> String {
    let mut terms: Vec<String> = query
        .split_whitespace()
        .filter(|t| {
            let lower = t.to_ascii_lowercase();
            !(lower.starts_with("color:") || lower.starts_with("colour:"))
        })
        .map(str::to_owned)
        .collect();
    if let Some(value) = value {
        terms.push(format!("color:{value}"));
    }
    terms.join(" ")
}

/// A `{...}` term: one figure of the post has every tag, and the kind when given.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FigureTerm {
    pub kind: Option<FigureKind>,
    pub tags: Vec<String>,
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
    /// `text:` values: words the image's OCR text must contain.
    pub text: Vec<String>,
    /// `-text:` values: words it must not contain.
    pub exclude_text: Vec<String>,
    /// `{...}` terms: a figure matching each must be in the post.
    pub figures: Vec<FigureTerm>,
    /// `-{...}` terms: no figure may match.
    pub exclude_figures: Vec<FigureTerm>,
    /// `date:` terms: the post's date must be in each.
    pub dates: Vec<DateTerm>,
    /// `-date:` terms: it must be in none.
    pub exclude_dates: Vec<DateTerm>,
    /// `width:`, `height:`, `mpixels:` and `ratio:` terms.
    pub sizes: Vec<SizeTerm>,
    /// The same with `-`. A post of unknown size passes these.
    pub exclude_sizes: Vec<SizeTerm>,
    /// `color:` terms: the picture must have each colour.
    pub colors: Vec<ColorTerm>,
    /// `-color:` terms: it must have none.
    pub exclude_colors: Vec<ColorTerm>,
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
    #[error(
        "unknown order `{0}` (use new, old, score, size, pixels, longside, tags, updated or random; add _asc to reverse)"
    )]
    BadOrder(String),
    #[error("`text:` needs some words, e.g. text:hello or text:\"two words\"")]
    EmptyText,
    #[error("`text:` is too long (max {MAX_TEXT_CHARS} characters)")]
    TextTooLong,
    #[error("`{{}}` needs tags inside, e.g. {{girl blonde_hair boots}}")]
    EmptyFigure,
    #[error("`{{...}}` names two kinds (use one of girl, boy or other)")]
    TwoKinds,
    #[error("`date:{0}` isn't a date (try date:2024-03, date:2023..2024 or date:>30d)")]
    BadDate(String),
    #[error("`{0}:{1}` isn't a size (try width:>1920, mpixels:>8 or ratio:16:9)")]
    BadSize(String, String),
    #[error("`color:{0}` isn't a colour (try color:red or color:2060e0)")]
    BadColor(String),
    #[error(transparent)]
    Tag(#[from] TagError),
}

/// Splits a query into terms at whitespace, except inside the quotes of a
/// `text:"…"` value or the braces of a `{…}` term. An unclosed quote or
/// brace runs to the end.
fn terms(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut words = input.split_whitespace();
    while let Some(word) = words.next() {
        let lower = word.to_ascii_lowercase();
        let body = lower.strip_prefix('-').unwrap_or(&lower);
        let closing = if let Some(rest) = body.strip_prefix("text:\"") {
            Some((rest, '"'))
        } else {
            body.strip_prefix('{').map(|rest| (rest, '}'))
        };
        let mut term = word.to_owned();
        if let Some((rest, end)) = closing {
            let mut closed = !rest.is_empty() && rest.ends_with(end);
            while !closed {
                let Some(next) = words.next() else { break };
                term.push(' ');
                term.push_str(next);
                closed = next.ends_with(end);
            }
        }
        out.push(term);
    }
    out
}

/// The inside of a `{…}` term.
fn figure_term(inside: &str) -> Result<FigureTerm, QueryError> {
    let inside = inside.strip_suffix('}').unwrap_or(inside);
    let mut term = FigureTerm::default();
    for word in inside.split_whitespace() {
        if let Some(kind) = FigureKind::parse(word) {
            if term.kind.is_some_and(|k| k != kind) {
                return Err(QueryError::TwoKinds);
            }
            term.kind = Some(kind);
            continue;
        }
        let tag = normalize_tag(word)?;
        let tag = crate::vocab::split_category(&tag).1.to_owned();
        push_unique(&mut term.tags, tag);
    }
    if term.kind.is_none() && term.tags.is_empty() {
        return Err(QueryError::EmptyFigure);
    }
    if term.tags.len() > MAX_QUERY_TERMS {
        return Err(QueryError::TooManyTerms);
    }
    Ok(term)
}

/// A `text:` value without its quotes and with whitespace collapsed.
fn text_value(value: &str) -> Result<String, QueryError> {
    let value = value.strip_prefix('"').unwrap_or(value);
    let value = value.strip_suffix('"').unwrap_or(value);
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return Err(QueryError::EmptyText);
    }
    if value.chars().count() > MAX_TEXT_CHARS {
        return Err(QueryError::TextTooLong);
    }
    Ok(value)
}

pub fn parse_query(input: &str) -> Result<SearchQuery, QueryError> {
    let mut q = SearchQuery::default();
    for (i, term) in terms(input).iter().enumerate() {
        let term = term.as_str();
        if i >= MAX_QUERY_TERMS {
            return Err(QueryError::TooManyTerms);
        }
        let (negated, body) = match term.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (false, term),
        };
        if let Some(inside) = body.strip_prefix('{') {
            push_unique(
                if negated {
                    &mut q.exclude_figures
                } else {
                    &mut q.figures
                },
                figure_term(inside)?,
            );
            continue;
        }
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
                    q.order = Order::parse(value)
                        .ok_or_else(|| QueryError::BadOrder(value.to_owned()))?;
                    continue;
                }
                "text" => {
                    let value = text_value(value)?;
                    push_unique(
                        if negated {
                            &mut q.exclude_text
                        } else {
                            &mut q.text
                        },
                        value,
                    );
                    continue;
                }
                "date" => {
                    let date = parse_date_term(value)
                        .ok_or_else(|| QueryError::BadDate(value.to_owned()))?;
                    push_unique(
                        if negated {
                            &mut q.exclude_dates
                        } else {
                            &mut q.dates
                        },
                        date,
                    );
                    continue;
                }
                "color" | "colour" => {
                    let color =
                        parse_color(value).ok_or_else(|| QueryError::BadColor(value.to_owned()))?;
                    push_unique(
                        if negated {
                            &mut q.exclude_colors
                        } else {
                            &mut q.colors
                        },
                        color,
                    );
                    continue;
                }
                name @ ("width" | "height" | "mpixels" | "ratio") => {
                    let field = match name {
                        "width" => SizeField::Width,
                        "height" => SizeField::Height,
                        "mpixels" => SizeField::Pixels,
                        _ => SizeField::Ratio,
                    };
                    let size = parse_size_term(field, value)
                        .ok_or_else(|| QueryError::BadSize(name.to_owned(), value.to_owned()))?;
                    push_unique(
                        if negated {
                            &mut q.exclude_sizes
                        } else {
                            &mut q.sizes
                        },
                        size,
                    );
                    continue;
                }
                _ => {}
            }
        }
        let tag = normalize_tag(body)?;
        // `artist:name` searches the tag `name`
        let tag = match crate::vocab::split_category(&tag) {
            (Some(_), name) => name.to_owned(),
            (None, _) => tag,
        };
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
    fn parses_colors() {
        let q = parse_query("color:RED colour:#2060e0 -color:grey color:red").unwrap();
        assert_eq!(
            q.colors,
            vec![parse_color("red").unwrap(), parse_color("2060e0").unwrap()]
        );
        assert_eq!(q.exclude_colors, vec![parse_color("gray").unwrap()]);
        assert!(q.include.is_empty() && q.exclude.is_empty());
        assert_eq!(
            parse_query("color:bluish"),
            Err(QueryError::BadColor("bluish".into()))
        );
        assert_eq!(parse_query("color:"), Err(QueryError::BadColor("".into())));
    }

    #[test]
    fn with_color_replaces_the_color_terms() {
        assert_eq!(with_color("cat", Some("red")), "cat color:red");
        assert_eq!(
            with_color("color:red cat Colour:blue order:score", Some("2060e0")),
            "cat order:score color:2060e0"
        );
        assert_eq!(
            with_color("cat color:red -color:blue", None),
            "cat -color:blue"
        );
        assert_eq!(with_color("", None), "");
    }

    #[test]
    fn parses_mixed_query() {
        let q = parse_query("Blue_Sky -cloud rating:g -rating:e type:video order:score").unwrap();
        assert_eq!(q.include, vec!["blue_sky"]);
        assert_eq!(q.exclude, vec!["cloud"]);
        assert_eq!(q.ratings, vec![Rating::General]);
        assert_eq!(q.exclude_ratings, vec![Rating::Explicit]);
        assert_eq!(q.kind, Some(MediaKind::Video));
        assert_eq!(
            q.order,
            Order {
                key: SortKey::Score,
                descending: true
            }
        );
        assert!(!q.deleted);
        assert!(parse_query("status:deleted").unwrap().deleted);
    }

    #[test]
    fn category_prefixes_search_the_bare_tag() {
        let q = parse_query("artist:Some_One -char:miku re:zero").unwrap();
        assert_eq!(q.include, vec!["some_one", "re:zero"]);
        assert_eq!(q.exclude, vec!["miku"]);
    }

    #[test]
    fn parses_orders() {
        let o = |v: &str| Order::parse(v).map(|o| (o.key, o.descending));
        assert_eq!(o("old"), Some((SortKey::Id, false)));
        assert_eq!(o("ID_ASC"), Some((SortKey::Id, false)));
        assert_eq!(o("id_desc"), Some((SortKey::Id, true)));
        assert_eq!(o("size"), Some((SortKey::Size, true)));
        assert_eq!(o("size_asc"), Some((SortKey::Size, false)));
        assert_eq!(o("resolution"), Some((SortKey::Pixels, true)));
        assert_eq!(o("tagcount_asc"), Some((SortKey::TagCount, false)));
        assert_eq!(o("mpixels"), Some((SortKey::Pixels, true)));
        assert_eq!(o("megapixels_asc"), Some((SortKey::Pixels, false)));
        assert_eq!(o("longside"), Some((SortKey::LongSide, true)));
        assert_eq!(o("long_side_asc"), Some((SortKey::LongSide, false)));
        assert_eq!(o("random"), Some((SortKey::Random, true)));
        assert_eq!(o("random_asc"), None);
        assert_eq!(o("_asc"), None);
        assert_eq!(o(""), None);
        for key in [
            SortKey::Id,
            SortKey::Score,
            SortKey::Size,
            SortKey::Pixels,
            SortKey::LongSide,
            SortKey::TagCount,
            SortKey::Updated,
        ] {
            for descending in [true, false] {
                let order = Order { key, descending };
                assert_eq!(Order::parse(order.as_value()), Some(order));
            }
        }
    }

    #[test]
    fn rewrites_order_terms() {
        let size = Order::parse("size").unwrap();
        assert_eq!(
            with_order("cat order:score -dog", Some(size)),
            "cat -dog order:size"
        );
        assert_eq!(with_order("cat sort:old", None), "cat");
        assert_eq!(with_order("cat", Some(Order::default())), "cat");
        assert_eq!(with_order("", Some(size)), "order:size");
    }

    #[test]
    fn parses_text_terms() {
        let q = parse_query(r#"cat text:hello TEXT:"Two   words" -text:spam order:old"#).unwrap();
        assert_eq!(q.include, vec!["cat"]);
        assert_eq!(q.text, vec!["hello", "Two words"]);
        assert_eq!(q.exclude_text, vec!["spam"]);
        assert_eq!(q.order.key, SortKey::Id);
        assert!(!q.order.descending);

        let q = parse_query(r#"-text:"no thanks" dog"#).unwrap();
        assert_eq!(q.exclude_text, vec!["no thanks"]);
        assert_eq!(q.include, vec!["dog"]);

        // An unclosed quote runs to the end; a one-word quote works too.
        let q = parse_query(r#"text:"open ended"#).unwrap();
        assert_eq!(q.text, vec!["open ended"]);
        assert_eq!(parse_query(r#"text:"one""#).unwrap().text, vec!["one"]);
        // Duplicates collapse.
        assert_eq!(parse_query("text:a text:a").unwrap().text, vec!["a"]);
    }

    #[test]
    fn bad_text_terms() {
        assert_eq!(parse_query("text:"), Err(QueryError::EmptyText));
        assert_eq!(parse_query("text:\"\""), Err(QueryError::EmptyText));
        assert_eq!(parse_query(r#"text:" "#), Err(QueryError::EmptyText));
        let long = format!("text:{}", "a".repeat(MAX_TEXT_CHARS + 1));
        assert_eq!(parse_query(&long), Err(QueryError::TextTooLong));
    }

    #[test]
    fn parses_figure_terms() {
        let q = parse_query("{girl Blonde_Hair char:saber} -{boy boots} cat {other}").unwrap();
        assert_eq!(
            q.figures,
            vec![
                FigureTerm {
                    kind: Some(FigureKind::Girl),
                    tags: vec!["blonde_hair".into(), "saber".into()],
                },
                FigureTerm {
                    kind: Some(FigureKind::Other),
                    tags: vec![],
                },
            ]
        );
        assert_eq!(
            q.exclude_figures,
            vec![FigureTerm {
                kind: Some(FigureKind::Boy),
                tags: vec!["boots".into()],
            }]
        );
        assert_eq!(q.include, vec!["cat"]);
        // braces may touch the words or stand apart; an open one runs to the end
        let q = parse_query("{ hat } { skirt").unwrap();
        assert_eq!(q.figures[0].tags, vec!["hat"]);
        assert_eq!(q.figures[1].tags, vec!["skirt"]);
        assert_eq!(parse_query("{}"), Err(QueryError::EmptyFigure));
        assert_eq!(parse_query("{ }"), Err(QueryError::EmptyFigure));
        assert_eq!(parse_query("{girl boy}"), Err(QueryError::TwoKinds));
        assert!(parse_query("{girl -x}").is_err());
    }

    #[test]
    fn parses_date_and_size_terms() {
        let q = parse_query(
            "cat date:2024-03 -DATE:>30d width:>1920 -height:<100 mpixels:8 ratio:16:9 date:2024-03",
        )
        .unwrap();
        assert_eq!(q.include, vec!["cat"]);
        assert_eq!(q.dates, vec![parse_date_term("2024-03").unwrap()]);
        assert_eq!(q.exclude_dates, vec![parse_date_term(">30d").unwrap()]);
        assert_eq!(
            q.sizes,
            vec![
                parse_size_term(SizeField::Width, ">1920").unwrap(),
                parse_size_term(SizeField::Pixels, "8").unwrap(),
                parse_size_term(SizeField::Ratio, "16:9").unwrap(),
            ]
        );
        assert_eq!(
            q.exclude_sizes,
            vec![parse_size_term(SizeField::Height, "<100").unwrap()]
        );
    }

    #[test]
    fn bad_date_and_size_terms() {
        assert_eq!(
            parse_query("date:soon"),
            Err(QueryError::BadDate("soon".into()))
        );
        assert_eq!(parse_query("date:"), Err(QueryError::BadDate("".into())));
        assert_eq!(
            parse_query("width:wide"),
            Err(QueryError::BadSize("width".into(), "wide".into()))
        );
        assert_eq!(
            parse_query("-ratio:1:0"),
            Err(QueryError::BadSize("ratio".into(), "1:0".into()))
        );
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
