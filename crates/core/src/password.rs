//! How hard a password is to guess, as a score from 0 to 1.

/// Characters past this many add nothing to the score.
const MAX_SCANNED: usize = 128;

/// Bits of guessing work at and below which the score is 0.
const FLOOR_BITS: f64 = 14.0;
/// Bits of guessing work at and above which the score is 1.
const STRONG_BITS: f64 = 64.0;
/// Below this many bits a new account is refused.
const WEAK_BITS: f64 = 34.0;

/// Passwords and words that people pick most, roughly the most common first.
/// Matches are checked without case and after undoing letter swaps such as
/// `@` for `a`, so `P@ssw0rd` counts as `password`.
const COMMON: &[&str] = &[
    "password",
    "123456",
    "qwerty",
    "admin",
    "letmein",
    "welcome",
    "iloveyou",
    "monkey",
    "dragon",
    "master",
    "login",
    "princess",
    "sunshine",
    "football",
    "baseball",
    "shadow",
    "superman",
    "batman",
    "trustno",
    "starbooru",
    "booru",
    "secret",
    "hello",
    "freedom",
    "whatever",
    "summer",
    "winter",
    "spring",
    "autumn",
    "computer",
    "michael",
    "jordan",
    "charlie",
    "jessica",
    "ashley",
    "daniel",
    "thomas",
    "andrew",
    "hunter",
    "killer",
    "tigger",
    "soccer",
    "hockey",
    "ranger",
    "buster",
    "harley",
    "mustang",
    "matrix",
    "starwars",
    "pokemon",
    "naruto",
    "minecraft",
    "fortnite",
    "google",
    "facebook",
    "instagram",
    "anime",
    "manga",
    "hentai",
    "waifu",
    "senpai",
    "kawaii",
    "otaku",
    "user",
    "root",
    "test",
    "guest",
    "changeme",
    "default",
    "pass",
    "love",
    "lovely",
    "baby",
    "angel",
    "family",
    "flower",
    "cookie",
    "cheese",
    "pepper",
    "ginger",
    "maggie",
    "silver",
    "orange",
    "purple",
    "banana",
    "apple",
    "chocolate",
    "nicole",
    "access",
    "money",
    "mother",
    "father",
    "sister",
    "brother",
    "friend",
    "forever",
    "music",
    "photo",
    "image",
    "picture",
    "gallery",
    "private",
    "archive",
    "collection",
    "ciao",
    "amore",
    "tesoro",
    "estate",
    "inverno",
    "juventus",
    "napoli",
    "milano",
    "roma",
    "inter",
    "forza",
    "calcio",
    "italia",
    "azerty",
    "abcdef",
    "abc123",
    "qazwsx",
    "zxcvbnm",
    "asdfgh",
    "passwd",
    "pa55",
    "secure",
    "server",
    "home",
    "house",
    "cat",
    "dog",
    "kitty",
    "puppy",
    "tiger",
    "lion",
    "bear",
    "wolf",
    "fish",
    "red",
    "blue",
    "green",
    "black",
    "white",
    "yellow",
    "pink",
    "star",
    "moon",
    "sun",
    "sky",
    "fire",
    "water",
    "earth",
    "night",
    "magic",
    "power",
    "game",
    "gamer",
    "player",
    "super",
    "cool",
    "happy",
    "lucky",
    "sweet",
    "pretty",
    "cute",
    "crazy",
    "rock",
    "metal",
];

/// Keyboard rows and the number row, walked left to right or back.
const ROWS: &[&str] = &[
    "1234567890",
    "qwertyuiop",
    "asdfghjkl",
    "zxcvbnm",
    "azertyuiop",
    "qsdfghjklm",
    "wxcvbn",
    "qwertzuiop",
    "yxcvbnm",
];

/// How hard `password` is to guess, from 0 (very weak) to 1 (strong).
///
/// It is the estimated guessing work in bits, scaled so that `FLOOR_BITS`
/// gives 0 and `STRONG_BITS` gives 1.
/// `username` counts as known to an attacker; pass `""` when there is none.
pub fn password_strength(password: &str, username: &str) -> f32 {
    let bits = guess_bits(password, username);
    let score = (bits - FLOOR_BITS) / (STRONG_BITS - FLOOR_BITS);
    // f64 to f32 can only lose precision here: the value is in 0..=1.
    score.clamp(0.0, 1.0) as f32
}

/// Whether `password` is too easy to guess for a new account.
pub fn password_is_weak(password: &str, username: &str) -> bool {
    guess_bits(password, username) < WEAK_BITS
}

/// The colour hue (OKLCH degrees) for a strength score: red at 0, orange
/// around 0.5, green at 1, with every value in between.
pub fn strength_hue(score: f32) -> f32 {
    let s = score.clamp(0.0, 1.0);
    // The power keeps the hue in the reds and oranges for longer, so a
    // middling score looks orange and only a strong one turns green.
    25.0 + 120.0 * s.powf(2.2)
}

/// The estimated number of guesses, in bits, that `password` takes: the
/// cheapest way to spell it out as a chain of known patterns and single
/// characters.
fn guess_bits(password: &str, username: &str) -> f64 {
    let chars: Vec<char> = password.chars().take(MAX_SCANNED).collect();
    if chars.is_empty() {
        return 0.0;
    }
    let lower: Vec<char> = chars.iter().flat_map(|c| c.to_lowercase()).collect();
    // A few non-ASCII letters lowercase to more than one character. Then the
    // original characters are used, so positions still line up.
    let lower = if lower.len() == chars.len() {
        lower
    } else {
        chars.clone()
    };
    let unleet: Vec<char> = lower.iter().map(|&c| unleet(c)).collect();
    let name: Vec<char> = username.trim().to_lowercase().chars().collect();
    let per_char = pool_size(&chars).log2();

    let n = chars.len();
    // best[i]: the fewest bits that spell out the first i characters.
    let mut best = vec![f64::INFINITY; n + 1];
    if let Some(first) = best.first_mut() {
        *first = 0.0;
    }
    for i in 0..n {
        let Some(&here) = best.get(i) else { break };
        if !here.is_finite() {
            continue;
        }
        let mut relax = |len: usize, bits: f64| {
            if let Some(slot) = best.get_mut(i + len)
                && here + bits < *slot
            {
                *slot = here + bits;
            }
        };
        relax(1, per_char);
        for (len, bits) in patterns_at(i, &chars, &lower, &unleet, &name) {
            relax(len, bits);
        }
    }
    best.get(n).copied().unwrap_or(0.0)
}

/// Every known pattern that starts at `i`, as (length, bits).
fn patterns_at(
    i: usize,
    chars: &[char],
    lower: &[char],
    unleet: &[char],
    name: &[char],
) -> Vec<(usize, f64)> {
    let mut found = Vec::new();
    let rest = lower.get(i..).unwrap_or_default();
    let rest_unleet = unleet.get(i..).unwrap_or_default();
    let rest_chars = chars.get(i..).unwrap_or_default();

    // Common passwords and words, also with letters swapped for look-alikes.
    for (rank, word) in COMMON.iter().enumerate() {
        let word: Vec<char> = word.chars().collect();
        if word.len() < 3 || !rest_unleet.starts_with(&word) {
            continue;
        }
        let len = word.len();
        let base = log2(rank + 2);
        let swaps = rest
            .iter()
            .zip(rest_unleet)
            .take(len)
            .filter(|(a, b)| a != b)
            .count();
        let caps = rest_chars.get(..len).unwrap_or_default();
        found.push((len, base + case_bits(caps) + swaps.min(4) as f64));
    }

    // The whole username, in any case.
    if name.len() >= 3 && rest.starts_with(name) {
        let caps = rest_chars.get(..name.len()).unwrap_or_default();
        found.push((name.len(), 2.0 + case_bits(caps)));
    }

    // One character over and over: `aaaa`.
    if let Some(&first) = rest.first() {
        let run = rest.iter().take_while(|&&c| c == first).count();
        for len in 3..=run {
            found.push((len, pool_size(&[first]).log2() + log2(len)));
        }
    }

    // Letters or digits in order, up or down: `abcd`, `9876`.
    for step in [1i64, -1] {
        let mut len = 1;
        while let (Some(&a), Some(&b)) = (rest.get(len - 1), rest.get(len)) {
            if !same_class(a, b) || i64::from(u32::from(b)) - i64::from(u32::from(a)) != step {
                break;
            }
            len += 1;
            if len >= 3 {
                let start = rest.first().copied().unwrap_or('a');
                let obvious = matches!(start, 'a' | 'z' | '0' | '1' | '9');
                let bits = if obvious { 2.0 } else { 5.0 } + log2(len) + f64::from(step < 0);
                found.push((len, bits));
            }
        }
    }

    // Keys next to each other on a keyboard row: `qwerty`, `lkjh`.
    for row in ROWS {
        for reversed in [false, true] {
            let keys: Vec<char> = if reversed {
                row.chars().rev().collect()
            } else {
                row.chars().collect()
            };
            let Some(start) = rest.first().and_then(|c| keys.iter().position(|k| k == c)) else {
                continue;
            };
            let len = keys
                .get(start..)
                .unwrap_or_default()
                .iter()
                .zip(rest)
                .take_while(|(k, c)| k == c)
                .count();
            for l in 3..=len {
                found.push((l, 5.6 + log2(l) + f64::from(reversed)));
            }
        }
    }

    // A year from 1900 to 2099.
    if let Some(year) = rest.get(..4) {
        let year: String = year.iter().collect();
        if (year.starts_with("19") || year.starts_with("20"))
            && year.chars().all(|c| c.is_ascii_digit())
        {
            found.push((4, 6.7));
        }
    }

    // A piece that already came earlier: the second `abc` of `abcabc`.
    let before = lower.get(..i).unwrap_or_default();
    let mut len = 3;
    while let Some(piece) = rest.get(..len) {
        if !before.windows(len).any(|w| w == piece) {
            break;
        }
        found.push((len, log2(i + 1) + log2(len)));
        len += 1;
    }

    found
}

/// How many characters an attacker has to try at each position, judged from
/// the kinds of character the password uses.
fn pool_size(chars: &[char]) -> f64 {
    let has = |f: fn(&char) -> bool| chars.iter().any(f);
    let mut pool = 0.0;
    if has(char::is_ascii_lowercase) {
        pool += 26.0;
    }
    if has(char::is_ascii_uppercase) {
        pool += 26.0;
    }
    if has(char::is_ascii_digit) {
        pool += 10.0;
    }
    if has(|c| c.is_ascii() && !c.is_ascii_alphanumeric()) {
        pool += 33.0;
    }
    if has(|c| !c.is_ascii()) {
        pool += 100.0;
    }
    f64::max(pool, 2.0)
}

/// Extra bits for capitals in a matched word: one for a capital first
/// letter, more for capitals elsewhere.
fn case_bits(word: &[char]) -> f64 {
    let upper = word.iter().filter(|c| c.is_uppercase()).count();
    let first_only = upper == 1 && word.first().is_some_and(|c| c.is_uppercase());
    match upper {
        0 => 0.0,
        _ if first_only => 1.0,
        _ if upper == word.len() => 1.0,
        n => n.min(6) as f64 + 1.0,
    }
}

/// The letter a look-alike digit or symbol usually stands for.
fn unleet(c: char) -> char {
    match c {
        '4' | '@' => 'a',
        '3' => 'e',
        '1' | '!' => 'i',
        '0' => 'o',
        '5' | '$' => 's',
        '7' => 't',
        other => other,
    }
}

fn same_class(a: char, b: char) -> bool {
    (a.is_ascii_lowercase() && b.is_ascii_lowercase()) || (a.is_ascii_digit() && b.is_ascii_digit())
}

fn log2(n: usize) -> f64 {
    // usize to f64 is exact for every length this module sees (≤ 128).
    (n.max(1) as f64).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(p: &str) -> f64 {
        guess_bits(p, "")
    }

    #[test]
    fn common_passwords_are_weak() {
        for p in [
            "password",
            "password123",
            "P@ssw0rd!",
            "12345678",
            "qwertyuiop",
            "aaaaaaaaaa",
            "abcdefgh",
            "iloveyou2024",
            "Summer2024!",
            "starbooru1",
            "abcabcabcabc",
            "11111111",
            "zxcvbnm123",
            "9876543210",
        ] {
            assert!(
                password_is_weak(p, ""),
                "{p} should be weak ({:.1} bits)",
                bits(p)
            );
        }
    }

    #[test]
    fn the_username_does_not_count() {
        assert!(password_is_weak("filippo2024", "filippo"));
        assert!(password_is_weak("Filippo!!", "filippo"));
        assert!(!password_is_weak("filippo2024", "someone"));
    }

    #[test]
    fn long_or_random_passwords_are_strong() {
        for p in [
            "correct horse battery staple",
            "tR8#kq!Zp2@vW9",
            "gentle-otter-paints-lanterns",
            "Ἀθῆναι-σήμερα-βρέχει",
        ] {
            assert!(password_strength(p, "") > 0.99, "{p}: {:.1} bits", bits(p));
        }
    }

    #[test]
    fn middling_passwords_land_in_between() {
        for p in ["kfjdhsnw", "Tigerlily7", "bluecastle42"] {
            let s = password_strength(p, "");
            assert!(!password_is_weak(p, ""), "{p}: {:.1} bits", bits(p));
            assert!(s > 0.3 && s < 0.9, "{p}: {s}");
        }
    }

    #[test]
    fn longer_is_never_weaker() {
        let base = "mq7vz";
        let mut last = 0.0;
        for extra in ["", "r", "rk", "rk2", "rk2x", "rk2xw"] {
            let s = password_strength(&format!("{base}{extra}"), "");
            assert!(s >= last, "{base}{extra}");
            last = s;
        }
    }

    #[test]
    fn edge_cases_never_panic() {
        assert_eq!(password_strength("", ""), 0.0);
        assert_eq!(password_strength("", "someone"), 0.0);
        let huge = "ab".repeat(100_000);
        let s = password_strength(&huge, "ab");
        assert!((0.0..=1.0).contains(&s));
        // Characters whose lowercase is longer than one character.
        let s = password_strength("İİİİİİİİ", "İ");
        assert!((0.0..=1.0).contains(&s));
        // Digits and symbols only.
        assert!((0.0..=1.0).contains(&password_strength("!!!!", "")));
    }

    #[test]
    fn hue_runs_from_red_to_green() {
        assert_eq!(strength_hue(0.0), 25.0);
        assert_eq!(strength_hue(1.0), 145.0);
        assert_eq!(strength_hue(7.0), 145.0);
        assert_eq!(strength_hue(-1.0), 25.0);
        let mid = strength_hue(0.5);
        assert!((40.0..60.0).contains(&mid), "{mid}");
        assert!(strength_hue(0.4) < strength_hue(0.41));
    }
}
