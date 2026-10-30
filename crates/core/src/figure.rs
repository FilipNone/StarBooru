//! The people in a picture ("figures"). Each figure has a kind (girl, boy
//! or other), maybe a character name and the character they cosplay, and
//! tags sorted into slots: clothing by where it is worn, then body traits.
//!
//! A figure's tags are also the post's tags, so ordinary searches find
//! them. The count tags (`1girl`, `2boys`, `solo`...) follow the figures:
//! while a post has figures they are worked out from them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::tag::{TagError, normalize_tag};
use crate::vocab::split_category;

/// Most figures on one post.
pub const MAX_FIGURES: usize = 20;
/// Most tags on one figure, over all its slots.
pub const MAX_FIGURE_TAGS: usize = 64;
/// Added to a post while any figure cosplays someone.
pub const COSPLAY_TAG: &str = "cosplay";
const COSPLAY_SUFFIX: &str = "_(cosplay)";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FigureKind {
    #[default]
    Girl,
    Boy,
    Other,
}

impl FigureKind {
    pub const ALL: [FigureKind; 3] = [FigureKind::Girl, FigureKind::Boy, FigureKind::Other];

    pub fn to_db(self) -> i16 {
        match self {
            FigureKind::Girl => 0,
            FigureKind::Boy => 1,
            FigureKind::Other => 2,
        }
    }

    pub fn from_db(v: i16) -> Option<Self> {
        match v {
            0 => Some(FigureKind::Girl),
            1 => Some(FigureKind::Boy),
            2 => Some(FigureKind::Other),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FigureKind::Girl => "girl",
            FigureKind::Boy => "boy",
            FigureKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| s.eq_ignore_ascii_case(k.as_str()))
    }

    /// The count tag for `n` figures of this kind: `1girl`, `2boys`, `6+others`.
    fn count_tag(self, n: usize) -> String {
        let one = self.as_str();
        match n {
            1 => format!("1{one}"),
            2..=5 => format!("{n}{one}s"),
            _ => format!("6+{one}s"),
        }
    }
}

/// Where on a figure a tag goes: clothing first, then the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot {
    /// Hats, hair ornaments, headphones.
    Head,
    /// Tops, dresses, uniforms.
    Body,
    /// Skirts, trousers and legwear.
    Bottom,
    Shoes,
    /// Other clothing and accessories.
    Wear,
    /// Build.
    Shape,
    Eyes,
    Skin,
    Hair,
    Pose,
    /// Expression and anything else about the figure.
    Other,
}

impl Slot {
    pub const ALL: [Slot; 11] = [
        Slot::Head,
        Slot::Body,
        Slot::Bottom,
        Slot::Shoes,
        Slot::Wear,
        Slot::Shape,
        Slot::Eyes,
        Slot::Skin,
        Slot::Hair,
        Slot::Pose,
        Slot::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Slot::Head => "head",
            Slot::Body => "body",
            Slot::Bottom => "bottom",
            Slot::Shoes => "shoes",
            Slot::Wear => "wear",
            Slot::Shape => "shape",
            Slot::Eyes => "eyes",
            Slot::Skin => "skin",
            Slot::Hair => "hair",
            Slot::Pose => "pose",
            Slot::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| s.eq_ignore_ascii_case(k.as_str()))
    }

    /// Clothing slots; the rest describe the body.
    pub fn is_clothing(self) -> bool {
        matches!(
            self,
            Slot::Head | Slot::Body | Slot::Bottom | Slot::Shoes | Slot::Wear
        )
    }

    /// Database value. 0 and 1 are the character and cosplay tags.
    pub fn to_db(self) -> i16 {
        match self {
            Slot::Head => 2,
            Slot::Body => 3,
            Slot::Bottom => 4,
            Slot::Shoes => 5,
            Slot::Wear => 6,
            Slot::Shape => 7,
            Slot::Eyes => 8,
            Slot::Skin => 9,
            Slot::Hair => 10,
            Slot::Pose => 11,
            Slot::Other => 12,
        }
    }

    pub fn from_db(v: i16) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.to_db() == v)
    }
}

/// One person (or creature) in a post.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Figure {
    pub kind: FigureKind,
    /// Who the figure is, as a character tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// The character the figure is dressed as, without `_(cosplay)`. The
    /// post gets the tag `name_(cosplay)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cosplay: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<Slot, Vec<String>>,
}

impl Figure {
    /// Every tag this figure puts on the post: the character, the cosplay
    /// tag and the slot tags.
    pub fn tag_names(&self) -> Vec<String> {
        self.character
            .iter()
            .cloned()
            .chain(self.cosplay.as_deref().map(cosplay_tag))
            .chain(self.tags.values().flatten().cloned())
            .collect()
    }

    fn tag_count(&self) -> usize {
        usize::from(self.character.is_some())
            + usize::from(self.cosplay.is_some())
            + self.tags.values().map(Vec::len).sum::<usize>()
    }
}

/// `PUT /posts/{id}/figures`: replaces the post's figures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiguresUpdate {
    pub figures: Vec<Figure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FigureError {
    #[error("too many people in one post (max {MAX_FIGURES})")]
    TooMany,
    #[error("too many tags on one person (max {MAX_FIGURE_TAGS})")]
    TooManyTags,
    #[error(transparent)]
    Tag(#[from] TagError),
}

/// The tag for cosplaying `name`: `name_(cosplay)`.
pub fn cosplay_tag(name: &str) -> String {
    format!("{}{COSPLAY_SUFFIX}", cosplay_name(name))
}

/// `name` without a `_(cosplay)` ending.
pub fn cosplay_name(tag: &str) -> &str {
    tag.strip_suffix(COSPLAY_SUFFIX)
        .filter(|n| !n.is_empty())
        .unwrap_or(tag)
}

/// The count tags for figures of these kinds, like `2girls`,
/// `multiple_girls`, `1boy`, or `solo` for a single figure.
pub fn count_tags(kinds: &[FigureKind]) -> Vec<String> {
    let mut out = Vec::new();
    for kind in FigureKind::ALL {
        let n = kinds.iter().filter(|k| **k == kind).count();
        if n > 0 {
            out.push(kind.count_tag(n));
        }
        if n > 1 {
            out.push(format!("multiple_{}s", kind.as_str()));
        }
    }
    if kinds.len() == 1 {
        out.push("solo".to_owned());
    }
    out
}

/// Whether `name` is one of the tags [`count_tags`] makes.
pub fn is_count_tag(name: &str) -> bool {
    if name == "solo" {
        return true;
    }
    FigureKind::ALL.into_iter().any(|kind| {
        let one = kind.as_str();
        name == format!("multiple_{one}s")
            || name == format!("1{one}")
            || name == format!("6+{one}s")
            || (2..=5).any(|n| name == format!("{n}{one}s"))
    })
}

/// The tags a post with these figures always has: the counts, and
/// `cosplay` when someone cosplays.
pub fn required_tags(figures: &[Figure]) -> Vec<String> {
    let kinds: Vec<FigureKind> = figures.iter().map(|f| f.kind).collect();
    let mut out = count_tags(&kinds);
    if figures.iter().any(|f| f.cosplay.is_some()) {
        out.push(COSPLAY_TAG.to_owned());
    }
    out
}

/// A name as a figure stores it: normalised, without a category prefix.
fn clean_name(raw: &str) -> Result<String, TagError> {
    let name = normalize_tag(raw)?;
    Ok(split_category(&name).1.to_owned())
}

/// Normalises figures for saving: names cleaned, empty values dropped, a
/// tag kept in its first slot only, count tags left out (they are made
/// from the kinds) and the limits checked.
pub fn clean_figures(figures: Vec<Figure>) -> Result<Vec<Figure>, FigureError> {
    if figures.len() > MAX_FIGURES {
        return Err(FigureError::TooMany);
    }
    let optional = |raw: Option<String>| -> Result<Option<String>, TagError> {
        match raw.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(raw) => clean_name(raw).map(Some),
        }
    };
    let mut out = Vec::with_capacity(figures.len());
    for figure in figures {
        let character = optional(figure.character)?;
        let cosplay = optional(figure.cosplay)?.map(|c| cosplay_name(&c).to_owned());
        let mut seen: Vec<String> = character
            .iter()
            .cloned()
            .chain(cosplay.as_deref().map(cosplay_tag))
            .collect();
        let mut tags = BTreeMap::new();
        for (slot, names) in figure.tags {
            let mut kept = Vec::new();
            for raw in &names {
                let name = clean_name(raw)?;
                if is_count_tag(&name) || seen.contains(&name) {
                    continue;
                }
                seen.push(name.clone());
                kept.push(name);
            }
            if !kept.is_empty() {
                tags.insert(slot, kept);
            }
        }
        let figure = Figure {
            kind: figure.kind,
            character,
            cosplay,
            tags,
        };
        if figure.tag_count() > MAX_FIGURE_TAGS {
            return Err(FigureError::TooManyTags);
        }
        out.push(figure);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<FigureKind> {
        s.chars()
            .map(|c| match c {
                'g' => FigureKind::Girl,
                'b' => FigureKind::Boy,
                _ => FigureKind::Other,
            })
            .collect()
    }

    #[test]
    fn counts_follow_the_kinds() {
        assert_eq!(count_tags(&kinds("g")), ["1girl", "solo"]);
        assert_eq!(
            count_tags(&kinds("gbg")),
            ["2girls", "multiple_girls", "1boy"]
        );
        assert_eq!(count_tags(&kinds("ooo")), ["3others", "multiple_others"]);
        assert_eq!(count_tags(&kinds("bbbbbbb")), ["6+boys", "multiple_boys"]);
        assert!(count_tags(&[]).is_empty());
    }

    #[test]
    fn every_count_tag_is_known_and_in_the_vocabulary() {
        let vocab: Vec<&str> = crate::VOCAB.iter().map(|(t, _)| *t).collect();
        for kind in FigureKind::ALL {
            for n in 1..=8 {
                for tag in count_tags(&vec![kind; n]) {
                    assert!(is_count_tag(&tag), "{tag}");
                    assert!(vocab.contains(&tag.as_str()), "{tag} is not seeded");
                }
            }
        }
        for tag in ["girl", "2girl", "multiple_girl", "7girls", "solo_focus"] {
            assert!(!is_count_tag(tag), "{tag}");
        }
    }

    #[test]
    fn cosplay_tags() {
        assert_eq!(cosplay_tag("hatsune_miku"), "hatsune_miku_(cosplay)");
        assert_eq!(cosplay_tag("saber_(cosplay)"), "saber_(cosplay)");
        assert_eq!(cosplay_name("saber_(cosplay)"), "saber");
        assert_eq!(cosplay_name("_(cosplay)"), "_(cosplay)");
        let figures = vec![Figure {
            cosplay: Some("saber".into()),
            ..Figure::default()
        }];
        assert_eq!(required_tags(&figures), ["1girl", "solo", "cosplay"]);
        assert_eq!(figures[0].tag_names(), ["saber_(cosplay)"]);
    }

    #[test]
    fn cleans_figures() {
        let figure = Figure {
            kind: FigureKind::Boy,
            character: Some("char:Link".into()),
            cosplay: Some("  ".into()),
            tags: BTreeMap::from([
                (Slot::Hair, vec!["Blonde Hair".into(), "link".into()]),
                (Slot::Pose, vec!["standing".into(), "1boy".into()]),
                (Slot::Other, vec!["blonde_hair".into()]),
                (Slot::Shoes, vec![]),
            ]),
        };
        let clean = clean_figures(vec![figure]).unwrap();
        assert_eq!(
            clean,
            vec![Figure {
                kind: FigureKind::Boy,
                character: Some("link".into()),
                cosplay: None,
                tags: BTreeMap::from([
                    (Slot::Hair, vec!["blonde_hair".into()]),
                    (Slot::Pose, vec!["standing".into()]),
                ]),
            }]
        );
        assert_eq!(clean[0].tag_names(), ["link", "blonde_hair", "standing"]);
    }

    #[test]
    fn limits() {
        assert_eq!(
            clean_figures(vec![Figure::default(); MAX_FIGURES + 1]),
            Err(FigureError::TooMany)
        );
        let many: Vec<String> = (0..=MAX_FIGURE_TAGS).map(|i| format!("t{i}")).collect();
        let figure = Figure {
            tags: BTreeMap::from([(Slot::Other, many)]),
            ..Figure::default()
        };
        assert_eq!(clean_figures(vec![figure]), Err(FigureError::TooManyTags));
        let bad = Figure {
            tags: BTreeMap::from([(Slot::Other, vec!["-x".into()])]),
            ..Figure::default()
        };
        assert!(matches!(clean_figures(vec![bad]), Err(FigureError::Tag(_))));
    }

    #[test]
    fn slots_and_kinds_round_trip() {
        for slot in Slot::ALL {
            assert_eq!(Slot::from_db(slot.to_db()), Some(slot));
            assert_eq!(Slot::parse(slot.as_str()), Some(slot));
        }
        for kind in FigureKind::ALL {
            assert_eq!(FigureKind::from_db(kind.to_db()), Some(kind));
            assert_eq!(FigureKind::parse(kind.as_str()), Some(kind));
        }
        let json = serde_json::to_string(&Figure {
            tags: BTreeMap::from([(Slot::Shoes, vec!["boots".into()])]),
            ..Figure::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"girl","tags":{"shoes":["boots"]}}"#);
    }
}
