//! The date and size terms of the search language, and reading the dates
//! files carry.
//!
//! * `date:2024`, `date:2024-03`, `date:2024-03-15`: that period.
//! * `date:2023..2024`, `date:2023-06..`, `date:..2024`: from the start of
//!   the first period to the end of the last.
//! * `date:>2023-06`, `date:>=2023-06`, `date:<2024`, `date:<=2024`: after
//!   or before a period, with or without it.
//! * `date:>30d`: more than 30 days old. `date:<7d` or `date:7d`: less than
//!   7 days old. The units are `h`, `d`, `w`, `mo` and `y`.
//! * `width:` and `height:` in pixels, `mpixels:` in megapixels, `ratio:` as
//!   width over height (`1.78` or `16:9`): a value, a comparison (`>1920`)
//!   or a range (`1000..2000`). A bare `mpixels:8` means about 8, to the
//!   digits given. A bare ratio has a margin of 0.01, so `ratio:1` is square
//!   and `ratio:1.78` finds 16:9.
//!
//! Comparisons use whole numbers: pixels, and thousandths for a ratio.

use chrono::{DateTime, Months, NaiveDate, NaiveDateTime, TimeDelta, Utc};

/// Most hours an age can go back: about 1000 years, well within what the
/// database can compare.
const MAX_AGE_HOURS: i64 = 1000 * 366 * 24;
const PIXELS_PER_MEGAPIXEL: f64 = 1_000_000.0;
/// Most megapixels `mpixels:` reads.
const MAX_MEGAPIXELS: f64 = 1_000_000.0;
/// `ratio:` is in these parts of a whole.
const RATIO_SCALE: f64 = 1000.0;
/// Most `ratio:` reads, and the margin of a bare value.
const MAX_RATIO: f64 = 1_000_000.0;
const RATIO_MARGIN: i64 = 10;
/// The first photograph; anything earlier is a camera with an unset clock.
const FIRST_PHOTO_YEAR: i32 = 1826;
/// A video's date before this is an unset clock too (1904 and 1970 are the
/// usual zeroes).
const FIRST_VIDEO_YEAR: i32 = 1990;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeField {
    Width,
    Height,
    /// In pixels: width × height.
    Pixels,
    /// In thousandths of width over height.
    Ratio,
}

/// A `width:`, `height:`, `mpixels:` or `ratio:` term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeTerm {
    pub field: SizeField,
    /// The value as typed.
    pub raw: String,
    /// Inclusive bounds, in the field's unit.
    pub min: Option<i64>,
    pub max: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgeUnit {
    Hours,
    Days,
    Weeks,
    Months,
    Years,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Age {
    amount: u32,
    unit: AgeUnit,
}

impl Age {
    /// `2w`, `30d`, `3mo`; `None` for anything else or too long ago.
    fn parse(s: &str) -> Option<Self> {
        let (digits, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit())?);
        let (unit, hours) = match unit {
            "h" => (AgeUnit::Hours, 1),
            "d" => (AgeUnit::Days, 24),
            "w" => (AgeUnit::Weeks, 24 * 7),
            "mo" => (AgeUnit::Months, 24 * 31),
            "y" => (AgeUnit::Years, 24 * 366),
            _ => return None,
        };
        let amount: u32 = digits.parse().ok()?;
        (i64::from(amount).checked_mul(hours)? <= MAX_AGE_HOURS).then_some(Self { amount, unit })
    }

    /// The moment this long before `now`.
    fn before(self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let n = i64::from(self.amount);
        match self.unit {
            AgeUnit::Hours => now.checked_sub_signed(TimeDelta::try_hours(n)?),
            AgeUnit::Days => now.checked_sub_signed(TimeDelta::try_days(n)?),
            AgeUnit::Weeks => now.checked_sub_signed(TimeDelta::try_weeks(n)?),
            AgeUnit::Months => now.checked_sub_months(Months::new(self.amount)),
            AgeUnit::Years => now.checked_sub_months(Months::new(self.amount.checked_mul(12)?)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateSpan {
    /// From the first moment (inclusive) to the second (exclusive).
    Calendar(Option<NaiveDateTime>, Option<NaiveDateTime>),
    OlderThan(Age),
    NewerThan(Age),
}

/// A `date:` term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTerm {
    /// The value as typed.
    pub raw: String,
    span: DateSpan,
}

impl DateTerm {
    /// The moments this term covers at `now`: from the first (inclusive) to
    /// the second (exclusive). Calendar dates are read as UTC.
    pub fn bounds(&self, now: DateTime<Utc>) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        match self.span {
            DateSpan::Calendar(from, to) => (from.map(|d| d.and_utc()), to.map(|d| d.and_utc())),
            DateSpan::OlderThan(age) => (None, age.before(now)),
            DateSpan::NewerThan(age) => (age.before(now), None),
        }
    }
}

/// A calendar year, month or day, as its first moment and the first moment
/// after it.
fn period(s: &str) -> Option<(NaiveDateTime, NaiveDateTime)> {
    let number = |part: &str, digits: usize| -> Option<u32> {
        if part.len() == digits && part.bytes().all(|b| b.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let parts: Vec<&str> = s.split('-').collect();
    let year = i32::try_from(number(parts.first()?, 4)?).ok()?;
    if year == 0 {
        return None;
    }
    let (start, end) = match parts.get(1..)? {
        [] => (
            NaiveDate::from_ymd_opt(year, 1, 1)?,
            NaiveDate::from_ymd_opt(year.checked_add(1)?, 1, 1)?,
        ),
        [month] => {
            let month = number(month, 2)?;
            let start = NaiveDate::from_ymd_opt(year, month, 1)?;
            (start, start.checked_add_months(Months::new(1))?)
        }
        [month, day] => {
            let start = NaiveDate::from_ymd_opt(year, number(month, 2)?, number(day, 2)?)?;
            (start, start.succ_opt()?)
        }
        _ => return None,
    };
    Some((start.and_hms_opt(0, 0, 0)?, end.and_hms_opt(0, 0, 0)?))
}

/// Reads the value of a `date:` term; `None` when it isn't one.
pub fn parse_date_term(value: &str) -> Option<DateTerm> {
    let span = date_span(value)?;
    Some(DateTerm {
        raw: value.to_owned(),
        span,
    })
}

fn date_span(value: &str) -> Option<DateSpan> {
    // `>=` before `>`: the longer operators first.
    if let Some(rest) = value.strip_prefix(">=") {
        return comparison(rest, true, true);
    }
    if let Some(rest) = value.strip_prefix("<=") {
        return comparison(rest, false, true);
    }
    if let Some(rest) = value.strip_prefix('>') {
        return comparison(rest, true, false);
    }
    if let Some(rest) = value.strip_prefix('<') {
        return comparison(rest, false, false);
    }
    if let Some((first, last)) = value.split_once("..") {
        let from = if first.is_empty() {
            None
        } else {
            Some(period(first)?)
        };
        let to = if last.is_empty() {
            None
        } else {
            Some(period(last)?)
        };
        if from.is_none() && to.is_none() {
            return None;
        }
        // Written the wrong way round: still the same stretch of time.
        return Some(match (from, to) {
            (Some(a), Some(b)) if a.0 > b.0 => DateSpan::Calendar(Some(b.0), Some(a.1)),
            _ => DateSpan::Calendar(from.map(|p| p.0), to.map(|p| p.1)),
        });
    }
    if let Some(age) = Age::parse(value) {
        return Some(DateSpan::NewerThan(age));
    }
    let (start, end) = period(value)?;
    Some(DateSpan::Calendar(Some(start), Some(end)))
}

fn comparison(value: &str, greater: bool, or_equal: bool) -> Option<DateSpan> {
    if let Some(age) = Age::parse(value) {
        return Some(if greater {
            DateSpan::OlderThan(age)
        } else {
            DateSpan::NewerThan(age)
        });
    }
    let (start, end) = period(value)?;
    Some(match (greater, or_equal) {
        (true, false) => DateSpan::Calendar(Some(end), None),
        (true, true) => DateSpan::Calendar(Some(start), None),
        (false, false) => DateSpan::Calendar(None, Some(start)),
        (false, true) => DateSpan::Calendar(None, Some(end)),
    })
}

/// A number of `field`'s unit, with the margin a bare value has around it.
fn size_value(field: SizeField, s: &str) -> Option<(i64, i64)> {
    match field {
        SizeField::Width | SizeField::Height => {
            let n: i64 = s.parse().ok()?;
            ((0..=i64::from(i32::MAX)).contains(&n)).then_some((n, 0))
        }
        SizeField::Pixels => {
            let n: f64 = s.parse().ok()?;
            if !s.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                || !(0.0..=MAX_MEGAPIXELS).contains(&n)
            {
                return None;
            }
            let decimals = s.split_once('.').map_or(0, |(_, d)| d.len());
            let margin = match decimals {
                0..=5 => 5 * 10_i64.pow(5 - u32::try_from(decimals).ok()?),
                _ => 0,
            };
            Some(((n * PIXELS_PER_MEGAPIXEL).round() as i64, margin))
        }
        SizeField::Ratio => {
            let number = |s: &str| -> Option<f64> {
                if !s.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
                    return None;
                }
                s.parse().ok()
            };
            let ratio = match s.split_once(':') {
                Some((w, h)) => {
                    let (w, h) = (number(w)?, number(h)?);
                    if h <= 0.0 {
                        return None;
                    }
                    w / h
                }
                None => number(s)?,
            };
            (0.0..=MAX_RATIO)
                .contains(&ratio)
                .then_some(((ratio * RATIO_SCALE).round() as i64, RATIO_MARGIN))
        }
    }
}

/// Reads the value of a size term; `None` when it isn't one.
pub fn parse_size_term(field: SizeField, value: &str) -> Option<SizeTerm> {
    let number = |s: &str| size_value(field, s).map(|(n, _)| n);
    let (min, max) = if let Some(rest) = value.strip_prefix(">=") {
        (Some(number(rest)?), None)
    } else if let Some(rest) = value.strip_prefix("<=") {
        (None, Some(number(rest)?))
    } else if let Some(rest) = value.strip_prefix('>') {
        (Some(number(rest)?.saturating_add(1)), None)
    } else if let Some(rest) = value.strip_prefix('<') {
        (None, Some(number(rest)?.saturating_sub(1)))
    } else if let Some((first, last)) = value.split_once("..") {
        let from = if first.is_empty() {
            None
        } else {
            Some(number(first)?)
        };
        let to = if last.is_empty() {
            None
        } else {
            Some(number(last)?)
        };
        if from.is_none() && to.is_none() {
            return None;
        }
        match (from, to) {
            (Some(a), Some(b)) if a > b => (Some(b), Some(a)),
            _ => (from, to),
        }
    } else {
        let (n, margin) = size_value(field, value)?;
        (
            Some(n.saturating_sub(margin).max(0)),
            Some(n.saturating_add(margin)),
        )
    };
    Some(SizeTerm {
        field,
        raw: value.to_owned(),
        min,
        max,
    })
}

/// The date in an EXIF value (`2024:03:15 10:20:30`), as its wall clock in
/// UTC. `None` for a blank or impossible one.
pub fn exif_date(value: &str) -> Option<DateTime<Utc>> {
    use chrono::Datelike;
    let value = value.trim_end_matches(['\0', ' ']);
    let date = NaiveDateTime::parse_from_str(value, "%Y:%m:%d %H:%M:%S").ok()?;
    (date.year() >= FIRST_PHOTO_YEAR).then(|| date.and_utc())
}

/// The date in a video's `creation_time` tag (`2024-03-15T10:20:30.000000Z`).
/// `None` for a blank or impossible one.
pub fn video_date(value: &str) -> Option<DateTime<Utc>> {
    use chrono::Datelike;
    let date = DateTime::parse_from_rfc3339(value.trim()).ok()?.to_utc();
    (date.year() >= FIRST_VIDEO_YEAR).then_some(date)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    fn now() -> DateTime<Utc> {
        at("2026-03-31T12:00:00Z")
    }

    fn bounds(value: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        parse_date_term(value).unwrap().bounds(now())
    }

    fn day(s: &str) -> Option<DateTime<Utc>> {
        Some(at(&format!("{s}T00:00:00Z")))
    }

    #[test]
    fn periods() {
        assert_eq!(bounds("2024"), (day("2024-01-01"), day("2025-01-01")));
        assert_eq!(bounds("2024-03"), (day("2024-03-01"), day("2024-04-01")));
        assert_eq!(bounds("2024-12"), (day("2024-12-01"), day("2025-01-01")));
        assert_eq!(bounds("2024-02-29"), (day("2024-02-29"), day("2024-03-01")));
    }

    #[test]
    fn ranges_cover_both_ends() {
        assert_eq!(bounds("2023..2024"), (day("2023-01-01"), day("2025-01-01")));
        assert_eq!(bounds("2023-06.."), (day("2023-06-01"), None));
        assert_eq!(bounds("..2024-03"), (None, day("2024-04-01")));
        // the wrong way round
        assert_eq!(bounds("2024..2023"), (day("2023-01-01"), day("2025-01-01")));
    }

    #[test]
    fn comparisons() {
        assert_eq!(bounds(">2023-06"), (day("2023-07-01"), None));
        assert_eq!(bounds(">=2023-06"), (day("2023-06-01"), None));
        assert_eq!(bounds("<2024"), (None, day("2024-01-01")));
        assert_eq!(bounds("<=2024"), (None, day("2025-01-01")));
    }

    #[test]
    fn ages() {
        let days = |n| Some(now() - TimeDelta::days(n));
        assert_eq!(bounds(">30d"), (None, days(30)));
        assert_eq!(bounds("<7d"), (days(7), None));
        assert_eq!(bounds("7d"), (days(7), None));
        assert_eq!(bounds(">2w"), (None, days(14)));
        assert_eq!(bounds("<48h"), (days(2), None));
        // Calendar months: a month before the 31st falls on a shorter month's end.
        assert_eq!(bounds(">1mo"), (None, Some(at("2026-02-28T12:00:00Z"))));
        assert_eq!(bounds(">2y"), (None, Some(at("2024-03-31T12:00:00Z"))));
    }

    #[test]
    fn refuses_bad_dates() {
        for bad in [
            "",
            "..",
            "soon",
            "2024-13",
            "2024-02-30",
            "24",
            "2024-3",
            "2024-03-15-01",
            "0000",
            "30",
            "30x",
            "30m",
            "d",
            ">",
            ">>2024",
            "a..2024",
            "2024..b",
            "7d..30d",
            "-5d",
            "99999999999999999999d",
            "4294967295y",
            "1001y",
            "99999mo",
        ] {
            assert_eq!(parse_date_term(bad), None, "{bad}");
        }
        // The furthest age is one the database can compare.
        let (_, oldest) = bounds(">1000y");
        assert!(oldest.is_some_and(|d| d > at("1000-01-01T00:00:00Z")));
    }

    fn range(field: SizeField, value: &str) -> (Option<i64>, Option<i64>) {
        let term = parse_size_term(field, value).unwrap();
        assert_eq!(term.raw, value);
        (term.min, term.max)
    }

    #[test]
    fn widths_and_heights() {
        use SizeField::Width;
        assert_eq!(range(Width, "1920"), (Some(1920), Some(1920)));
        assert_eq!(range(Width, ">1920"), (Some(1921), None));
        assert_eq!(range(Width, ">=1920"), (Some(1920), None));
        assert_eq!(range(Width, "<1920"), (None, Some(1919)));
        assert_eq!(range(Width, "<=1920"), (None, Some(1920)));
        assert_eq!(range(Width, "1000..2000"), (Some(1000), Some(2000)));
        assert_eq!(range(Width, "2000..1000"), (Some(1000), Some(2000)));
        assert_eq!(range(Width, "1000.."), (Some(1000), None));
        assert_eq!(range(Width, "..1000"), (None, Some(1000)));
    }

    #[test]
    fn megapixels_are_in_pixels() {
        use SizeField::Pixels;
        assert_eq!(range(Pixels, ">8"), (Some(8_000_001), None));
        assert_eq!(range(Pixels, "<=0.5"), (None, Some(500_000)));
        // about, to the digits given
        assert_eq!(range(Pixels, "8"), (Some(7_500_000), Some(8_500_000)));
        assert_eq!(range(Pixels, "8.3"), (Some(8_250_000), Some(8_350_000)));
        assert_eq!(range(Pixels, "0"), (Some(0), Some(500_000)));
        assert_eq!(range(Pixels, "2..4"), (Some(2_000_000), Some(4_000_000)));
    }

    #[test]
    fn ratios_are_in_thousandths() {
        use SizeField::Ratio;
        assert_eq!(range(Ratio, ">1"), (Some(1001), None));
        assert_eq!(range(Ratio, "<1"), (None, Some(999)));
        assert_eq!(range(Ratio, "1"), (Some(990), Some(1010)));
        // 16:9 is 1.7777..., which the bare 1.78 finds
        assert_eq!(range(Ratio, "16:9"), (Some(1768), Some(1788)));
        assert_eq!(range(Ratio, "1.78"), (Some(1770), Some(1790)));
        assert_eq!(range(Ratio, "4:3..16:9"), (Some(1333), Some(1778)));
    }

    #[test]
    fn refuses_bad_sizes() {
        use SizeField::{Height, Pixels, Ratio, Width};
        for (field, bad) in [
            (Width, ""),
            (Width, ".."),
            (Width, ">"),
            (Width, "wide"),
            (Width, "-5"),
            (Width, "1.5"),
            (Height, "99999999999"),
            (Height, "99999999999999999999999"),
            (Pixels, "lots"),
            (Pixels, "inf"),
            (Pixels, "NaN"),
            (Pixels, "1e9"),
            (Pixels, "-1"),
            (Pixels, "9999999"),
            (Ratio, "1:0"),
            (Ratio, "0:0"),
            (Ratio, "a:b"),
            (Ratio, "inf"),
            (Ratio, "NaN:1"),
            (Ratio, "16:9:1"),
            (Ratio, "1e999"),
            (Ratio, "9999999"),
        ] {
            assert_eq!(parse_size_term(field, bad), None, "{bad}");
        }
    }

    #[test]
    fn reads_file_dates() {
        assert_eq!(
            exif_date("2024:03:15 10:20:30"),
            Some(at("2024-03-15T10:20:30Z"))
        );
        assert_eq!(
            exif_date("2024:03:15 10:20:30\0"),
            Some(at("2024-03-15T10:20:30Z"))
        );
        for bad in [
            "",
            "    :  :     :  :  ",
            "0000:00:00 00:00:00",
            "2024:13:15 10:20:30",
            "1600:01:01 00:00:00",
            "yesterday",
        ] {
            assert_eq!(exif_date(bad), None, "{bad}");
        }
        assert_eq!(
            video_date("2024-03-15T10:20:30.000000Z"),
            Some(at("2024-03-15T10:20:30Z"))
        );
        assert_eq!(
            video_date("2024-03-15T10:20:30+02:00"),
            Some(at("2024-03-15T08:20:30Z"))
        );
        for bad in ["", "1904-01-01T00:00:00Z", "1970-01-01T00:00:00Z", "today"] {
            assert_eq!(video_date(bad), None, "{bad}");
        }
    }
}
