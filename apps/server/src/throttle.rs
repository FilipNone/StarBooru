//! Slows down password guessing: failed sign-ins are counted per account
//! and per client address in fixed windows, and a full window answers 429
//! with `Retry-After` until it ends. Kept in memory, so a restart forgets
//! it, which only ever helps a legitimate user.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Mutex, PoisonError};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use chrono::{DateTime, TimeDelta, Utc};

use crate::error::AppError;
use crate::state::AppState;

/// Most keys one throttle remembers; past it, expired windows are dropped
/// first, then the oldest.
const MAX_KEYS: usize = 10_000;
/// Longest account key kept (usernames are much shorter).
const MAX_KEY_CHARS: usize = 64;

/// Failed attempts allowed per key within one window.
pub struct Throttle {
    max: u32,
    window: TimeDelta,
    windows: Mutex<HashMap<String, Window>>,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    start: DateTime<Utc>,
    count: u32,
}

impl Throttle {
    pub fn new(max: u32, window: TimeDelta) -> Self {
        Self {
            max,
            window,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// `Err(seconds)` while `key` has no attempts left in its window.
    pub fn check(&self, key: &str, now: DateTime<Utc>) -> Result<(), u64> {
        let windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        match windows.get(key) {
            Some(w) if w.count >= self.max => {
                let left = (w.start + self.window - now).num_seconds();
                if left > 0 {
                    Err(u64::try_from(left).unwrap_or(1))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    /// Counts one attempt against `key`.
    pub fn hit(&self, key: &str, now: DateTime<Utc>) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(w) = windows.get_mut(key) {
            if now - w.start >= self.window {
                *w = Window {
                    start: now,
                    count: 1,
                };
            } else {
                w.count = w.count.saturating_add(1);
            }
            return;
        }
        if windows.len() >= MAX_KEYS {
            let window = self.window;
            windows.retain(|_, w| now - w.start < window);
            if windows.len() >= MAX_KEYS
                && let Some(oldest) = windows
                    .iter()
                    .min_by_key(|(_, w)| w.start)
                    .map(|(k, _)| k.clone())
            {
                windows.remove(&oldest);
            }
        }
        windows.insert(
            key.to_owned(),
            Window {
                start: now,
                count: 1,
            },
        );
    }

    pub fn clear(&self, key: &str) {
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key);
    }
}

/// The limits the auth routes use.
pub struct Guards {
    /// Wrong passwords per account (also the current password when
    /// changing it).
    pub account: Throttle,
    /// Wrong passwords per client address, across accounts.
    pub address: Throttle,
    /// Sign-up attempts per client address.
    pub signup: Throttle,
}

impl Default for Guards {
    fn default() -> Self {
        Self {
            account: Throttle::new(10, TimeDelta::minutes(15)),
            address: Throttle::new(50, TimeDelta::minutes(15)),
            signup: Throttle::new(10, TimeDelta::hours(1)),
        }
    }
}

impl Guards {
    /// Refuses a sign-in while the account or the address is locked out.
    pub fn check_login(
        &self,
        account: &str,
        address: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), AppError> {
        self.account.check(account, now).map_err(too_many)?;
        if let Some(a) = address {
            self.address.check(a, now).map_err(too_many)?;
        }
        Ok(())
    }

    pub fn failed_login(&self, account: &str, address: Option<&str>, now: DateTime<Utc>) {
        self.account.hit(account, now);
        if let Some(a) = address {
            self.address.hit(a, now);
        }
    }
}

fn too_many(retry_after: u64) -> AppError {
    AppError::TooManyRequests { retry_after }
}

/// The key for a sign-in name: case-insensitive, like usernames, and capped.
pub fn account_key(username: &str) -> String {
    username
        .trim()
        .to_lowercase()
        .chars()
        .take(MAX_KEY_CHARS)
        .collect()
}

/// Where a request came from, as a throttle key. IPv6 clients are keyed by
/// their /64, since one host usually holds the whole prefix. `None` when
/// unknown (no socket address, as in tests).
pub struct ClientAddr(pub Option<String>);

impl FromRequestParts<AppState> for ClientAddr {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Infallible> {
        let forwarded = state
            .config
            .trust_proxy
            .then(|| last_forwarded(&parts.headers))
            .flatten();
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip());
        Ok(ClientAddr(forwarded.or(peer).map(address_key)))
    }
}

/// The address the nearest proxy saw: the last `X-Forwarded-For` entry.
/// Earlier entries come from the client and can be forged.
fn last_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()?
        .to_str()
        .ok()?
        .rsplit(',')
        .next()?
        .trim()
        .parse()
        .ok()
}

fn address_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let [a, b, c, d, ..] = v6.segments();
                format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn locks_after_max_then_reopens() {
        let t = Throttle::new(3, TimeDelta::minutes(15));
        for i in 0..3 {
            assert_eq!(t.check("alice", at(i)), Ok(()));
            t.hit("alice", at(i));
        }
        assert_eq!(t.check("alice", at(60)), Err(15 * 60 - 60));
        assert_eq!(t.check("bob", at(60)), Ok(()));
        // The window ends 15 minutes after the first attempt.
        assert_eq!(t.check("alice", at(15 * 60)), Ok(()));
        t.hit("alice", at(15 * 60));
        assert_eq!(t.check("alice", at(15 * 60 + 1)), Ok(()));
        t.hit("alice", at(1));
        t.hit("alice", at(2));
        t.clear("alice");
        assert_eq!(t.check("alice", at(3)), Ok(()));
    }

    #[test]
    fn stays_bounded() {
        let t = Throttle::new(1, TimeDelta::minutes(1));
        for i in 0..MAX_KEYS + 50 {
            t.hit(&format!("k{i}"), at(0));
        }
        assert!(t.windows.lock().unwrap().len() <= MAX_KEYS);
        // Expired windows go first.
        t.hit("fresh", at(120));
        assert_eq!(t.windows.lock().unwrap().len(), 1);
    }

    #[test]
    fn keys() {
        assert_eq!(account_key("  Alice "), "alice");
        assert_eq!(account_key(&"x".repeat(500)).len(), MAX_KEY_CHARS);
        assert_eq!(address_key("203.0.113.9".parse().unwrap()), "203.0.113.9");
        assert_eq!(
            address_key("2001:db8:1:2:aaaa::1".parse().unwrap()),
            "2001:db8:1:2::/64"
        );
        assert_eq!(address_key("::ffff:10.0.0.1".parse().unwrap()), "10.0.0.1");

        let mut h = HeaderMap::new();
        assert_eq!(last_forwarded(&h), None);
        h.append(
            "x-forwarded-for",
            HeaderValue::from_static("1.1.1.1, 10.0.0.2"),
        );
        assert_eq!(last_forwarded(&h), "10.0.0.2".parse().ok());
        h.append("x-forwarded-for", HeaderValue::from_static(" 2001:db8::1 "));
        assert_eq!(last_forwarded(&h), "2001:db8::1".parse().ok());
        h.append("x-forwarded-for", HeaderValue::from_static("garbage"));
        assert_eq!(last_forwarded(&h), None);
    }
}
