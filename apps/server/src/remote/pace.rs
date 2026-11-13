//! Keeps the server within what other sites allow. Every request to a site's
//! API waits its turn, so however many searches and imports are going they
//! reach the site no faster than its limit (`starbooru_core::Limits`). When a
//! site says it is busy, nothing more goes to it until the time it gave has
//! passed. Kept in memory and shared by every account, as the site sees one
//! server.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, TimeDelta, Utc};
use starbooru_core::BooruApi;
use url::Url;

use crate::error::{AppError, AppResult};
use crate::fetch::{Answer, FetchError, Fetcher};

/// The longest a request waits for its turn before it is told to come back.
const MAX_WAIT: TimeDelta = TimeDelta::seconds(8);
/// How long to stay away after a site says it's busy, when it doesn't say.
pub const BUSY_WAIT_SECS: u64 = 15;
/// The longest a site is taken at its word to stay away.
const MAX_HOLD_SECS: u64 = 600;
/// How long a site that wanted an account is remembered as wanting one.
const LOCKED_FOR: TimeDelta = TimeDelta::hours(1);
/// Most sites remembered; past it, the ones with nothing pending go first.
const MAX_SITES: usize = 2000;

#[derive(Debug, Clone, Copy)]
struct Site {
    /// When the next request may go.
    next: DateTime<Utc>,
    /// Until when its API is known to refuse requests without an account.
    locked_until: Option<DateTime<Utc>>,
}

/// When each site may next be asked.
#[derive(Default)]
pub struct Pace(Mutex<HashMap<String, Site>>);

impl Pace {
    fn site<T>(&self, host: &str, now: DateTime<Utc>, with: impl FnOnce(&mut Site) -> T) -> T {
        let mut sites = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if sites.len() >= MAX_SITES && !sites.contains_key(host) {
            sites.retain(|_, site| {
                site.next > now || site.locked_until.is_some_and(|until| until > now)
            });
            if sites.len() >= MAX_SITES {
                sites.clear();
            }
        }
        let site = sites.entry(host.to_owned()).or_insert(Site {
            next: now,
            locked_until: None,
        });
        with(site)
    }

    /// Books the next turn with `host`, `interval` after the one before it.
    /// `Ok` is how long to wait before sending; `Err` is the seconds to come
    /// back in, when the turn is too far off to wait for.
    pub fn book(
        &self,
        host: &str,
        interval: TimeDelta,
        now: DateTime<Utc>,
    ) -> Result<TimeDelta, u64> {
        self.site(host, now, |site| {
            let turn = site.next.max(now);
            let wait = turn - now;
            if wait > MAX_WAIT {
                return Err(u64::try_from(wait.num_seconds()).unwrap_or(1).max(1));
            }
            site.next = turn + interval;
            Ok(wait)
        })
    }

    /// Keeps away from `host` for `secs`, as it said it is busy.
    pub fn hold(&self, host: &str, secs: u64, now: DateTime<Utc>) {
        let secs = i64::try_from(secs.clamp(1, MAX_HOLD_SECS)).unwrap_or(1);
        self.site(host, now, |site| {
            site.next = site.next.max(now + TimeDelta::seconds(secs));
        });
    }

    /// Forgets every site, as if the waits were over.
    #[cfg(test)]
    pub fn forget(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Remembers that `host`'s API refused a request without an account.
    pub fn lock(&self, host: &str, now: DateTime<Utc>) {
        self.site(host, now, |site| site.locked_until = Some(now + LOCKED_FOR));
    }

    /// Whether `host`'s API is known to want an account.
    pub fn locked(&self, host: &str, now: DateTime<Utc>) -> bool {
        self.site(host, now, |site| {
            site.locked_until.is_some_and(|until| until > now)
        })
    }
}

/// What a site is known by here: its host and port, as two servers on one
/// machine are two sites.
pub fn site_key(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port_or_known_default() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}

/// Whether an answer means "slow down": the usual 429, e621's 503 and
/// Moebooru's 421.
pub fn is_busy(status: u16) -> bool {
    matches!(status, 421 | 429 | 503)
}

/// Asks a site's API in its turn. Any answer comes back, whatever its status,
/// except "busy": that keeps everyone away from the site for as long as it
/// asks, and is an error saying when to come back.
pub async fn ask(pace: &Pace, fetcher: &Fetcher, api: BooruApi, url: &Url) -> AppResult<Answer> {
    let host = site_key(url);
    let interval = TimeDelta::milliseconds(i64::from(api.limits().interval_ms));
    wait_turn(pace, &host, interval).await?;
    // The address may carry a key: it never goes in an error or the log.
    let answer = fetcher.ask(url.as_str()).await.map_err(unreachable_site)?;
    if is_busy(answer.status) {
        let secs = answer.retry_after.unwrap_or(BUSY_WAIT_SECS).max(1);
        pace.hold(&host, secs, Utc::now());
        return Err(AppError::TooManyRequests {
            retry_after: secs.min(MAX_HOLD_SECS),
        });
    }
    Ok(answer)
}

/// Waits for the next turn with `host`, or says when to come back.
pub async fn wait_turn(pace: &Pace, host: &str, interval: TimeDelta) -> AppResult<()> {
    match pace.book(host, interval, Utc::now()) {
        Err(retry_after) => Err(AppError::TooManyRequests { retry_after }),
        Ok(wait) => {
            if let Ok(wait) = wait.to_std()
                && !wait.is_zero()
            {
                tokio::time::sleep(wait).await;
            }
            Ok(())
        }
    }
}

/// A site that couldn't be asked at all, in terms of what the user can change.
pub fn unreachable_site(e: FetchError) -> AppError {
    match e {
        FetchError::BadUrl | FetchError::Blocked => AppError::BadRequest(e.to_string()),
        FetchError::TooLarge => AppError::Upstream("that site's answer is too large".into()),
        _ => AppError::Upstream(e.to_string()),
    }
}

/// What a site says went wrong, from an answer that isn't what was asked for:
/// Danbooru and e621 write `message`, Moebooru `reason`, and Gelbooru's forks
/// an XML `reason="…"` under any status.
pub fn site_message(body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body.get(..body.len().min(4096)).unwrap_or(body));
    let found = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(value) => ["message", "reason", "error"]
            .into_iter()
            .find_map(|key| value.get(key)?.as_str().map(str::to_owned)),
        Err(_) => text
            .split_once("reason=\"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(reason, _)| reason.to_owned()),
    }?;
    let message: String = found
        .chars()
        .filter(|c| !c.is_control())
        .take(300)
        .collect();
    let message = message.trim();
    (!message.is_empty()).then(|| message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: i64) -> TimeDelta {
        TimeDelta::milliseconds(n)
    }

    #[test]
    fn requests_to_a_site_take_turns() {
        let pace = Pace::default();
        let now = Utc::now();
        // The first goes at once; the next ones queue up behind it.
        assert_eq!(pace.book("a.org", ms(500), now), Ok(ms(0)));
        assert_eq!(pace.book("a.org", ms(500), now), Ok(ms(500)));
        assert_eq!(pace.book("a.org", ms(500), now), Ok(ms(1000)));
        // Another site doesn't wait for this one.
        assert_eq!(pace.book("b.org", ms(500), now), Ok(ms(0)));
        // Time passing frees the turns.
        assert_eq!(pace.book("a.org", ms(500), now + ms(1400)), Ok(ms(100)));
        assert_eq!(pace.book("a.org", ms(500), now + ms(60_000)), Ok(ms(0)));
    }

    #[test]
    fn a_queue_too_long_sends_the_request_away() {
        let pace = Pace::default();
        let now = Utc::now();
        let mut waits = Vec::new();
        for _ in 0..20 {
            waits.push(pace.book("a.org", ms(1000), now));
        }
        assert_eq!(waits[8], Ok(ms(8000)));
        // The ninth second is past the longest wait, and books nothing.
        assert_eq!(waits[9], Err(9));
        assert_eq!(waits[19], Err(9));
        assert_eq!(pace.book("a.org", ms(1000), now + ms(1000)), Ok(ms(8000)));
    }

    #[test]
    fn a_busy_site_is_left_alone() {
        let pace = Pace::default();
        let now = Utc::now();
        pace.hold("a.org", 30, now);
        assert_eq!(pace.book("a.org", ms(500), now), Err(30));
        assert_eq!(pace.book("a.org", ms(500), now + ms(25_000)), Ok(ms(5000)));
        assert_eq!(pace.book("b.org", ms(500), now), Ok(ms(0)));
        // A site is taken at its word only so far, and a shorter hold doesn't
        // cut a longer one.
        pace.hold("c.org", 1_000_000, now);
        assert_eq!(pace.book("c.org", ms(0), now), Err(MAX_HOLD_SECS));
        pace.hold("c.org", 5, now);
        assert_eq!(pace.book("c.org", ms(0), now), Err(MAX_HOLD_SECS));
    }

    #[test]
    fn remembers_for_an_hour_that_a_site_wants_an_account() {
        let pace = Pace::default();
        let now = Utc::now();
        assert!(!pace.locked("a.org", now));
        pace.lock("a.org", now);
        assert!(pace.locked("a.org", now + TimeDelta::minutes(59)));
        assert!(!pace.locked("a.org", now + TimeDelta::minutes(61)));
        assert!(!pace.locked("b.org", now));
    }

    #[test]
    fn forgets_idle_sites_when_full() {
        let pace = Pace::default();
        let now = Utc::now();
        pace.hold("busy.org", 60, now);
        for i in 0..MAX_SITES + 10 {
            let _ = pace.book(&format!("{i}.org"), ms(0), now);
        }
        let later = now + ms(1000);
        let _ = pace.book("new.org", ms(0), later);
        let sites = pace.0.lock().unwrap();
        assert!(sites.len() <= MAX_SITES);
        assert!(sites.contains_key("busy.org"), "its hold still counts");
    }

    #[test]
    fn reads_what_a_site_says_went_wrong() {
        let said = |body: &str| site_message(body.as_bytes());
        assert_eq!(
            said(r#"{"success":false,"error":"PostQuery::TagLimitError","message":"You cannot search for more than 2 tags at a time.","backtrace":[]}"#).as_deref(),
            Some("You cannot search for more than 2 tags at a time.")
        );
        assert_eq!(
            said(r#"{"success":false,"reason":"Error: You cannot search for more than 6 tags at a time"}"#).as_deref(),
            Some("Error: You cannot search for more than 6 tags at a time")
        );
        assert_eq!(
            said(r#"<?xml version="1.0"?><response success="false" reason="Search error: API limited due to abuse."/>"#).as_deref(),
            Some("Search error: API limited due to abuse.")
        );
        assert_eq!(
            said(r#"{"error":"bad request"}"#).as_deref(),
            Some("bad request")
        );
        for nothing in [
            "",
            "<html>busy</html>",
            "[]",
            r#"{"message":"  "}"#,
            r#"{"message":7}"#,
        ] {
            assert_eq!(said(nothing), None, "{nothing}");
        }
        let long = format!(r#"{{"message":"{}"}}"#, "x".repeat(5000));
        assert_eq!(said(&long).map(|m| m.len()), Some(300));
    }

    #[test]
    fn a_site_is_a_host_and_a_port() {
        let key = |raw: &str| site_key(&Url::parse(raw).unwrap());
        assert_eq!(key("https://b.org/posts.json?tags=a"), "b.org:443");
        assert_eq!(key("http://B.org/x"), "b.org:80");
        assert_ne!(key("http://127.0.0.1:8000/"), key("http://127.0.0.1:8001/"));
    }

    #[test]
    fn knows_the_ways_sites_say_slow_down() {
        assert!(is_busy(429) && is_busy(503) && is_busy(421));
        assert!(!is_busy(500) && !is_busy(200) && !is_busy(403));
    }
}
