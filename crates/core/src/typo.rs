//! Which tag a misspelled name probably meant. The server finds the candidates
//! (`pg_trgm`); this picks one, or none when none is close enough.

/// How many edits a name of this many characters can be off by and still be a
/// typo. Under three characters any change makes a different word.
fn max_edits(chars: usize) -> usize {
    match chars {
        0..=2 => 0,
        3..=5 => 1,
        6..=11 => 2,
        _ => 3,
    }
}

/// Edits between two names, counting an insert, a delete, a replaced character
/// and two swapped neighbours as one each (optimal string alignment distance).
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut before: Vec<usize> = Vec::new();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut row = Vec::with_capacity(b.len().saturating_add(1));
        row.push(i.saturating_add(1));
        for (j, cb) in b.iter().enumerate() {
            let at = |list: &[usize], k: usize| list.get(k).copied().unwrap_or(usize::MAX);
            let mut edits = at(&previous, j)
                .saturating_add(usize::from(ca != cb))
                .min(at(&previous, j.saturating_add(1)).saturating_add(1))
                .min(at(&row, j).saturating_add(1));
            let swapped = i > 0
                && j > 0
                && a.get(i.saturating_sub(1)) == Some(cb)
                && b.get(j.saturating_sub(1)) == Some(ca);
            if swapped {
                edits = edits.min(at(&before, j.saturating_sub(1)).saturating_add(1));
            }
            row.push(edits);
        }
        before = std::mem::replace(&mut previous, row);
    }
    previous.last().copied().unwrap_or(usize::MAX)
}

/// The candidate (name, post count) that `name` most likely meant: the fewest
/// edits, then the most posts, then the first by name. Digits must match
/// exactly, so `4k` never becomes `8k` and `tag_1` never becomes `tag_2`.
pub fn closest_tag<'a>(
    name: &str,
    candidates: impl IntoIterator<Item = (&'a str, i32)>,
) -> Option<&'a str> {
    let typed: Vec<char> = name.chars().collect();
    let max = max_edits(typed.len());
    if max == 0 {
        return None;
    }
    let digits = |s: &'a str| s.chars().filter(char::is_ascii_digit);
    let typed_digits: Vec<char> = name.chars().filter(char::is_ascii_digit).collect();
    candidates
        .into_iter()
        .filter(|(candidate, _)| {
            *candidate != name && digits(candidate).eq(typed_digits.iter().copied())
        })
        .filter_map(|(candidate, count)| {
            let chars: Vec<char> = candidate.chars().collect();
            let edits = edit_distance(&typed, &chars);
            (edits <= max).then_some((edits, std::cmp::Reverse(count), candidate))
        })
        .min()
        .map(|(_, _, candidate)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distance(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        edit_distance(&a, &b)
    }

    #[test]
    fn counts_edits() {
        assert_eq!(distance("cat", "cat"), 0);
        assert_eq!(distance("cat", "cats"), 1);
        assert_eq!(distance("cat", "at"), 1);
        assert_eq!(distance("cat", "cut"), 1);
        assert_eq!(distance("cta", "cat"), 1);
        assert_eq!(distance("blnde_hiar", "blonde_hair"), 2);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("abc", ""), 3);
        assert_eq!(distance("héllo", "hello"), 1);
    }

    #[test]
    fn picks_the_closest() {
        let tags = [("blonde_hair", 9), ("blue_hair", 500), ("blonde", 40)];
        assert_eq!(closest_tag("blnde_hair", tags), Some("blonde_hair"));
        assert_eq!(closest_tag("blu_hair", tags), Some("blue_hair"));
    }

    #[test]
    fn ties_go_to_the_most_used_then_by_name() {
        assert_eq!(closest_tag("bat", [("cat", 2), ("hat", 9)]), Some("hat"));
        assert_eq!(closest_tag("bat", [("hat", 2), ("cat", 2)]), Some("cat"));
    }

    #[test]
    fn leaves_what_is_too_far_or_too_short() {
        assert_eq!(closest_tag("ab", [("ac", 9)]), None);
        assert_eq!(closest_tag("dog", [("cat", 9)]), None);
        // two edits are too many for a short word, fine for a longer one
        assert_eq!(closest_tag("hello", [("halo", 9)]), None);
        assert_eq!(closest_tag("sneekars", [("sneakers", 9)]), Some("sneakers"));
        assert_eq!(closest_tag("cat", []), None);
    }

    #[test]
    fn numbers_are_never_changed() {
        assert_eq!(closest_tag("4k", [("8k", 9)]), None);
        assert_eq!(closest_tag("4kk", [("8k", 9)]), None);
        assert_eq!(closest_tag("tag_1", [("tag_2", 9), ("tag_12", 9)]), None);
        assert_eq!(
            closest_tag("2girl", [("1girl", 9), ("2girls", 9)]),
            Some("2girls")
        );
        assert_eq!(
            closest_tag("tgag_300", [("tag_300", 9), ("tag_301", 99)]),
            Some("tag_300")
        );
    }

    #[test]
    fn the_name_itself_is_not_a_correction() {
        assert_eq!(closest_tag("cat", [("cat", 9)]), None);
    }
}
