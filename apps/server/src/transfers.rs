//! How far each running import's download has got. Kept in memory: a restart
//! starts those downloads over anyway.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, Utc};
use starbooru_core::{Meter, Transfer};

/// The downloads under way, by import job.
#[derive(Default)]
pub struct Transfers(Mutex<HashMap<i64, (Meter, Transfer)>>);

impl Transfers {
    /// Notes how far job `id` has got.
    pub fn seen(&self, id: i64, done: u64, total: Option<u64>, now: DateTime<Utc>) {
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let (meter, transfer) = all
            .entry(id)
            .or_insert_with(|| (Meter::new(now), Transfer::default()));
        *transfer = Transfer {
            done,
            total,
            speed: meter.seen(done, now),
        };
    }

    /// How far job `id` has got, while it downloads.
    pub fn get(&self, id: i64) -> Option<Transfer> {
        let all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        all.get(&id).map(|(_, transfer)| *transfer)
    }

    /// Forgets a job that has finished.
    pub fn end(&self, id: i64) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn follows_each_job_until_it_ends() {
        let start = Utc::now();
        let all = Transfers::default();
        assert_eq!(all.get(1), None);
        all.seen(1, 0, Some(4_000_000), start);
        all.seen(2, 10, None, start);
        all.seen(1, 1_000_000, Some(4_000_000), start + TimeDelta::seconds(1));
        let one = all.get(1).unwrap();
        assert_eq!(
            (one.done, one.percent(), one.speed),
            (1_000_000, Some(25), 1_000_000)
        );
        assert_eq!(all.get(2).map(|t| (t.done, t.total)), Some((10, None)));
        all.end(1);
        assert_eq!(all.get(1), None);
        assert!(all.get(2).is_some());
    }
}
