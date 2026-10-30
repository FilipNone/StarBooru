//! The starter tag hierarchy: sections (the top-level tags), the groups
//! inside them and common Danbooru-style tags under those, with parent
//! links. The server seeds it once per [`VOCAB_VERSION`]; after that the
//! tags are ordinary tags people can move, rename or delete.
//!
//! Searching a tag finds posts with any tag below it, so `footwear` finds
//! `boots`, and the viewer shows a post's general tags under their section.

use crate::model::TagCategory;

/// Bump when [`VOCAB`] gains entries, so servers seed the new ones.
pub const VOCAB_VERSION: i32 = 2;

/// The sections, in the order the viewer shows them.
pub const SECTIONS: &[&str] = &[
    "people",
    "hair",
    "eyes",
    "skin",
    "body",
    "expression",
    "pose",
    "clothing",
    "setting",
    "style",
];

/// `(tag, parent)`. Every parent is a section or appears earlier as a tag.
pub const VOCAB: &[(&str, &str)] = &[
    // people: who is in the picture
    ("1girl", "people"),
    ("2girls", "people"),
    ("3girls", "people"),
    ("4girls", "people"),
    ("5girls", "people"),
    ("6+girls", "people"),
    ("multiple_girls", "people"),
    ("1boy", "people"),
    ("2boys", "people"),
    ("3boys", "people"),
    ("4boys", "people"),
    ("5boys", "people"),
    ("6+boys", "people"),
    ("multiple_boys", "people"),
    ("1other", "people"),
    ("2others", "people"),
    ("3others", "people"),
    ("4others", "people"),
    ("5others", "people"),
    ("6+others", "people"),
    ("multiple_others", "people"),
    ("solo", "people"),
    ("no_humans", "people"),
    ("cosplay", "people"),
    // hair
    ("hair_color", "hair"),
    ("black_hair", "hair_color"),
    ("brown_hair", "hair_color"),
    ("blonde_hair", "hair_color"),
    ("red_hair", "hair_color"),
    ("orange_hair", "hair_color"),
    ("pink_hair", "hair_color"),
    ("purple_hair", "hair_color"),
    ("blue_hair", "hair_color"),
    ("aqua_hair", "hair_color"),
    ("green_hair", "hair_color"),
    ("white_hair", "hair_color"),
    ("grey_hair", "hair_color"),
    ("silver_hair", "hair_color"),
    ("multicolored_hair", "hair_color"),
    ("gradient_hair", "hair_color"),
    ("streaked_hair", "hair_color"),
    ("hair_length", "hair"),
    ("very_short_hair", "hair_length"),
    ("short_hair", "hair_length"),
    ("medium_hair", "hair_length"),
    ("long_hair", "hair_length"),
    ("very_long_hair", "hair_length"),
    ("absurdly_long_hair", "hair_length"),
    ("bald", "hair_length"),
    ("hairstyle", "hair"),
    ("ponytail", "hairstyle"),
    ("side_ponytail", "hairstyle"),
    ("twintails", "hairstyle"),
    ("braid", "hairstyle"),
    ("hair_bun", "hairstyle"),
    ("bob_cut", "hairstyle"),
    ("hime_cut", "hairstyle"),
    ("drill_hair", "hairstyle"),
    ("messy_hair", "hairstyle"),
    ("wavy_hair", "hairstyle"),
    ("curly_hair", "hairstyle"),
    ("straight_hair", "hairstyle"),
    ("ahoge", "hairstyle"),
    ("bangs", "hairstyle"),
    ("blunt_bangs", "hairstyle"),
    ("hair_over_one_eye", "hairstyle"),
    ("sidelocks", "hairstyle"),
    // eyes
    ("eye_color", "eyes"),
    ("black_eyes", "eye_color"),
    ("brown_eyes", "eye_color"),
    ("blue_eyes", "eye_color"),
    ("aqua_eyes", "eye_color"),
    ("green_eyes", "eye_color"),
    ("yellow_eyes", "eye_color"),
    ("orange_eyes", "eye_color"),
    ("red_eyes", "eye_color"),
    ("pink_eyes", "eye_color"),
    ("purple_eyes", "eye_color"),
    ("grey_eyes", "eye_color"),
    ("heterochromia", "eye_color"),
    ("closed_eyes", "eyes"),
    ("one_eye_closed", "eyes"),
    ("glowing_eyes", "eyes"),
    ("slit_pupils", "eyes"),
    ("eyelashes", "eyes"),
    // skin
    ("pale_skin", "skin"),
    ("dark_skin", "skin"),
    ("tan", "skin"),
    ("tanlines", "skin"),
    ("colored_skin", "skin"),
    ("freckles", "skin"),
    ("tattoo", "skin"),
    ("scar", "skin"),
    // body: build and features
    ("body_shape", "body"),
    ("petite", "body_shape"),
    ("slim", "body_shape"),
    ("curvy", "body_shape"),
    ("plump", "body_shape"),
    ("muscular", "body_shape"),
    ("tall", "body_shape"),
    ("short", "body_shape"),
    ("child", "body_shape"),
    ("mature_female", "body_shape"),
    ("mature_male", "body_shape"),
    ("body_features", "body"),
    ("animal_ears", "body_features"),
    ("cat_ears", "animal_ears"),
    ("fox_ears", "animal_ears"),
    ("rabbit_ears", "animal_ears"),
    ("dog_ears", "animal_ears"),
    ("tail", "body_features"),
    ("wings", "body_features"),
    ("horns", "body_features"),
    ("pointy_ears", "body_features"),
    ("fang", "body_features"),
    ("halo", "body_features"),
    // expression
    ("smile", "expression"),
    ("grin", "expression"),
    ("open_mouth", "expression"),
    ("closed_mouth", "expression"),
    ("blush", "expression"),
    ("crying", "expression"),
    ("tears", "expression"),
    ("angry", "expression"),
    ("surprised", "expression"),
    ("sad", "expression"),
    ("embarrassed", "expression"),
    ("expressionless", "expression"),
    ("wink", "expression"),
    ("tongue_out", "expression"),
    ("looking_at_viewer", "expression"),
    ("looking_away", "expression"),
    // pose
    ("standing", "pose"),
    ("sitting", "pose"),
    ("lying", "pose"),
    ("kneeling", "pose"),
    ("squatting", "pose"),
    ("walking", "pose"),
    ("running", "pose"),
    ("jumping", "pose"),
    ("leaning_forward", "pose"),
    ("arms_up", "pose"),
    ("arms_behind_back", "pose"),
    ("crossed_arms", "pose"),
    ("hand_on_hip", "pose"),
    ("hands_up", "pose"),
    ("peace_sign", "pose"),
    ("waving", "pose"),
    ("hug", "pose"),
    ("holding", "pose"),
    ("from_behind", "pose"),
    ("from_side", "pose"),
    ("from_above", "pose"),
    ("from_below", "pose"),
    ("full_body", "pose"),
    ("upper_body", "pose"),
    ("portrait", "pose"),
    ("close-up", "pose"),
    // clothing, by where it is worn
    ("headwear", "clothing"),
    ("hat", "headwear"),
    ("cap", "headwear"),
    ("beret", "headwear"),
    ("witch_hat", "headwear"),
    ("hood", "headwear"),
    ("helmet", "headwear"),
    ("crown", "headwear"),
    ("tiara", "headwear"),
    ("hairband", "headwear"),
    ("hair_ribbon", "headwear"),
    ("hair_bow", "headwear"),
    ("hair_ornament", "headwear"),
    ("hairclip", "headwear"),
    ("headphones", "headwear"),
    ("maid_headdress", "headwear"),
    ("bodywear", "clothing"),
    ("shirt", "bodywear"),
    ("t-shirt", "bodywear"),
    ("blouse", "bodywear"),
    ("sweater", "bodywear"),
    ("hoodie", "bodywear"),
    ("jacket", "bodywear"),
    ("coat", "bodywear"),
    ("vest", "bodywear"),
    ("crop_top", "bodywear"),
    ("tank_top", "bodywear"),
    ("dress", "bodywear"),
    ("school_uniform", "bodywear"),
    ("serafuku", "school_uniform"),
    ("uniform", "bodywear"),
    ("maid", "bodywear"),
    ("suit", "bodywear"),
    ("kimono", "bodywear"),
    ("armor", "bodywear"),
    ("bodysuit", "bodywear"),
    ("swimsuit", "bodywear"),
    ("bikini", "swimsuit"),
    ("one-piece_swimsuit", "swimsuit"),
    ("leotard", "bodywear"),
    ("apron", "bodywear"),
    ("bottomwear", "clothing"),
    ("skirt", "bottomwear"),
    ("miniskirt", "skirt"),
    ("pleated_skirt", "skirt"),
    ("long_skirt", "skirt"),
    ("shorts", "bottomwear"),
    ("pants", "bottomwear"),
    ("jeans", "pants"),
    ("legwear", "bottomwear"),
    ("thighhighs", "legwear"),
    ("pantyhose", "legwear"),
    ("socks", "legwear"),
    ("kneehighs", "legwear"),
    ("footwear", "clothing"),
    ("shoes", "footwear"),
    ("sneakers", "footwear"),
    ("boots", "footwear"),
    ("high_heels", "footwear"),
    ("sandals", "footwear"),
    ("loafers", "footwear"),
    ("slippers", "footwear"),
    ("barefoot", "footwear"),
    ("accessories", "clothing"),
    ("glasses", "accessories"),
    ("sunglasses", "glasses"),
    ("gloves", "accessories"),
    ("scarf", "accessories"),
    ("necktie", "accessories"),
    ("bowtie", "accessories"),
    ("choker", "accessories"),
    ("necklace", "accessories"),
    ("earrings", "accessories"),
    ("jewelry", "accessories"),
    ("belt", "accessories"),
    ("bag", "accessories"),
    ("wristwatch", "accessories"),
    ("mask", "accessories"),
    ("umbrella", "accessories"),
    ("nude", "clothing"),
    // setting
    ("outdoors", "setting"),
    ("indoors", "setting"),
    ("simple_background", "setting"),
    ("white_background", "simple_background"),
    ("black_background", "simple_background"),
    ("gradient_background", "simple_background"),
    ("transparent_background", "simple_background"),
    ("sky", "setting"),
    ("cloud", "sky"),
    ("night_sky", "sky"),
    ("starry_sky", "sky"),
    ("day", "setting"),
    ("night", "setting"),
    ("sunset", "setting"),
    ("city", "setting"),
    ("street", "setting"),
    ("building", "setting"),
    ("bedroom", "setting"),
    ("classroom", "setting"),
    ("forest", "setting"),
    ("tree", "setting"),
    ("flower", "setting"),
    ("beach", "setting"),
    ("ocean", "setting"),
    ("water", "setting"),
    ("snow", "setting"),
    ("rain", "setting"),
    ("scenery", "setting"),
    // style
    ("monochrome", "style"),
    ("greyscale", "style"),
    ("sketch", "style"),
    ("lineart", "style"),
    ("chibi", "style"),
    ("realistic", "style"),
    ("pixel_art", "style"),
    ("watercolor_(medium)", "style"),
    ("traditional_media", "style"),
    ("3d", "style"),
    ("photo_(medium)", "style"),
    ("comic", "style"),
    ("4koma", "comic"),
    ("text", "style"),
    ("english_text", "text"),
    ("japanese_text", "text"),
    ("signature", "style"),
    ("watermark", "style"),
];

/// Category prefixes accepted when tagging, e.g. `artist:some_name`.
const PREFIXES: &[(&str, TagCategory)] = &[
    ("artist:", TagCategory::Artist),
    ("character:", TagCategory::Character),
    ("char:", TagCategory::Character),
    ("copyright:", TagCategory::Copyright),
    ("copy:", TagCategory::Copyright),
    ("series:", TagCategory::Copyright),
    ("meta:", TagCategory::Meta),
    ("general:", TagCategory::General),
    ("gen:", TagCategory::General),
];

/// Splits a category prefix off a normalised tag: `artist:foo` is
/// `(Some(Artist), "foo")`. A bare prefix or any other name is left alone.
pub fn split_category(tag: &str) -> (Option<TagCategory>, &str) {
    for (prefix, category) in PREFIXES {
        if let Some(rest) = tag.strip_prefix(prefix)
            && !rest.is_empty()
        {
            return (Some(*category), rest);
        }
    }
    (None, tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::normalize_tag;

    #[test]
    fn every_entry_is_a_valid_tag_with_a_known_parent() {
        let mut known: Vec<&str> = SECTIONS.to_vec();
        for (tag, parent) in VOCAB {
            assert_eq!(normalize_tag(tag).as_deref(), Ok(*tag), "{tag}");
            assert!(
                known.contains(parent),
                "{tag}: parent {parent} comes later or is missing"
            );
            assert!(!known.contains(tag), "{tag} appears twice");
            assert_eq!(
                split_category(tag),
                (None, *tag),
                "{tag} looks like a prefix"
            );
            known.push(tag);
        }
    }

    #[test]
    fn splits_category_prefixes() {
        assert_eq!(
            split_category("artist:wlop"),
            (Some(TagCategory::Artist), "wlop")
        );
        assert_eq!(
            split_category("char:hatsune_miku"),
            (Some(TagCategory::Character), "hatsune_miku")
        );
        assert_eq!(
            split_category("series:re:zero"),
            (Some(TagCategory::Copyright), "re:zero")
        );
        // not prefixes: other names with colons, or nothing after it
        assert_eq!(split_category("re:zero"), (None, "re:zero"));
        assert_eq!(split_category("artist:"), (None, "artist:"));
        assert_eq!(split_category("artistic"), (None, "artistic"));
    }
}
