//! How far a download or an upload has got, and how fast it is going. The
//! server measures its imports with this and the apps their uploads.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A transfer under way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    /// Bytes moved so far.
    pub done: u64,
    /// The size of the whole, when it is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Bytes per second over the last moments; 0 until it has been measured.
    #[serde(default)]
    pub speed: u64,
}

impl Transfer {
    /// How much is done, from 0 to 100. `None` without a total.
    pub fn percent(&self) -> Option<u8> {
        let total = self.total.filter(|total| *total > 0)?;
        let percent = u128::from(self.done.min(total)) * 100 / u128::from(total);
        u8::try_from(percent).ok()
    }
}

/// The least time between two readings of the speed, so that one quick piece
/// doesn't make the number jump.
const SAMPLE_MS: i64 = 500;

/// Works out a transfer's speed from the totals it is told.
#[derive(Debug, Clone, Copy)]
pub struct Meter {
    since: DateTime<Utc>,
    bytes: u64,
    speed: u64,
}

impl Meter {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            since: now,
            bytes: 0,
            speed: 0,
        }
    }

    /// Takes the bytes moved so far and gives the speed in bytes per second.
    /// Each reading counts for a third, so the number settles without
    /// lagging far behind.
    pub fn seen(&mut self, bytes: u64, now: DateTime<Utc>) -> u64 {
        let ms = (now - self.since).num_milliseconds();
        // A transfer that started over, or a clock set back: measure afresh.
        if bytes < self.bytes || ms < 0 {
            *self = Self {
                speed: self.speed,
                ..Self::new(now)
            };
            self.bytes = bytes;
            return self.speed;
        }
        if ms >= SAMPLE_MS {
            let moved = bytes.saturating_sub(self.bytes);
            let latest = moved.saturating_mul(1000) / u64::try_from(ms).unwrap_or(1).max(1);
            self.speed = if self.speed == 0 {
                latest
            } else {
                self.speed.saturating_mul(2).saturating_add(latest) / 3
            };
            self.since = now;
            self.bytes = bytes;
        }
        self.speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn percent_needs_a_total() {
        let t = |done, total| Transfer {
            done,
            total,
            speed: 0,
        };
        assert_eq!(t(50, Some(200)).percent(), Some(25));
        assert_eq!(t(0, Some(200)).percent(), Some(0));
        assert_eq!(t(199, Some(200)).percent(), Some(99));
        assert_eq!(t(300, Some(200)).percent(), Some(100));
        assert_eq!(t(u64::MAX, Some(u64::MAX)).percent(), Some(100));
        assert_eq!(t(5, None).percent(), None);
        assert_eq!(t(5, Some(0)).percent(), None);
    }

    #[test]
    fn older_answers_have_no_speed() {
        let t: Transfer = serde_json::from_str(r#"{"done":3}"#).unwrap();
        assert_eq!((t.done, t.total, t.speed), (3, None, 0));
    }

    #[test]
    fn measures_speed_in_steps() {
        let start = Utc::now();
        let at = |ms| start + TimeDelta::milliseconds(ms);
        let mut meter = Meter::new(start);
        // Too soon to say.
        assert_eq!(meter.seen(10_000, at(100)), 0);
        // 1 MB in the first second.
        assert_eq!(meter.seen(1_000_000, at(1000)), 1_000_000);
        // Between readings the number stays.
        assert_eq!(meter.seen(1_200_000, at(1200)), 1_000_000);
        // Then 4 MB/s: a third of the way there.
        assert_eq!(meter.seen(5_000_000, at(2000)), 2_000_000);
        // Stalled: it comes down, not to nothing at once.
        assert_eq!(meter.seen(5_000_000, at(3000)), 1_333_333);
    }

    #[test]
    fn starts_over_without_a_wild_number() {
        let start = Utc::now();
        let at = |ms| start + TimeDelta::milliseconds(ms);
        let mut meter = Meter::new(start);
        assert_eq!(meter.seen(2_000_000, at(1000)), 2_000_000);
        // A second download: the count goes back to the start.
        assert_eq!(meter.seen(1000, at(1100)), 2_000_000);
        assert_eq!(meter.seen(1_001_000, at(2100)), 1_666_666);
        // A clock set back doesn't divide by a negative time.
        assert_eq!(meter.seen(1_002_000, at(-5000)), 1_666_666);
        assert_eq!(
            meter.seen(u64::MAX, at(-4000)),
            meter.seen(u64::MAX, at(-3999))
        );
    }
}
