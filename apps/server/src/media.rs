//! Signed media URLs.
//!
//! `<img>` and `<video>` tags cannot send an `Authorization` header, so post
//! JSON carries short-lived HMAC-signed URLs instead. Expiry is rounded to a
//! 6-hour bucket so the same URL is reused (and cached) within a window.

use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::Variant;

type HmacSha256 = Hmac<Sha256>;

const BUCKET_SECS: i64 = 6 * 3600;

pub struct Signer {
    key: Vec<u8>,
}

impl Signer {
    pub fn new(secret: &[u8]) -> Self {
        // Domain-separate the media key from any other use of the secret.
        let mut key = b"starbooru-media-v1:".to_vec();
        key.extend_from_slice(secret);
        Self { key }
    }

    fn mac(&self, variant: Variant, file: &str, exp: i64) -> Option<HmacSha256> {
        let mut mac = HmacSha256::new_from_slice(&self.key).ok()?;
        mac.update(format!("{}/{file}:{exp}", variant.as_str()).as_bytes());
        Some(mac)
    }

    pub fn url(&self, variant: Variant, file: &str, now: i64) -> String {
        let exp = (now.div_euclid(BUCKET_SECS) + 2).saturating_mul(BUCKET_SECS);
        let sig = self
            .mac(variant, file, exp)
            .map(|m| hex::encode(m.finalize().into_bytes()))
            .unwrap_or_default();
        format!("/media/{}/{file}?exp={exp}&sig={sig}", variant.as_str())
    }

    pub fn verify(&self, variant: Variant, file: &str, exp: i64, sig: &str, now: i64) -> bool {
        if exp < now || exp > now.saturating_add(3 * BUCKET_SECS) {
            return false;
        }
        let Ok(sig) = hex::decode(sig) else {
            return false;
        };
        self.mac(variant, file, exp)
            .is_some_and(|m| m.verify_slice(&sig).is_ok())
    }
}

#[derive(Deserialize)]
pub struct SignedQuery {
    exp: i64,
    sig: String,
}

/// `GET /media/{variant}/{file}?exp=..&sig=..`, with Range support for video.
pub async fn serve(
    State(state): State<AppState>,
    Path((variant, file)): Path<(String, String)>,
    Query(q): Query<SignedQuery>,
    req: Request,
) -> AppResult<Response> {
    let variant = Variant::parse(&variant).ok_or(AppError::NotFound)?;
    let path = state
        .storage
        .path_for_file(variant, &file)
        .ok_or(AppError::NotFound)?;
    let now = chrono::Utc::now().timestamp();
    if !state.signer.verify(variant, &file, q.exp, &q.sig, now) {
        return Err(AppError::Forbidden);
    }
    let resp = ServeFile::new(path)
        .oneshot(req)
        .await
        .map_err(AppError::internal)?;
    let mut resp = resp.map(Body::new).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=21600, immutable"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "ab.jpg";

    fn parts(url: &str) -> (i64, String) {
        let q = url.split_once('?').unwrap().1;
        let mut exp = 0;
        let mut sig = String::new();
        for kv in q.split('&') {
            match kv.split_once('=').unwrap() {
                ("exp", v) => exp = v.parse().unwrap(),
                ("sig", v) => sig = v.to_owned(),
                _ => {}
            }
        }
        (exp, sig)
    }

    #[test]
    fn signs_and_verifies() {
        let s = Signer::new(b"secret");
        let now = 1_700_000_000;
        let url = s.url(Variant::Thumb, FILE, now);
        let (exp, sig) = parts(&url);
        assert!(s.verify(Variant::Thumb, FILE, exp, &sig, now));
        // Stable within a bucket, so browsers can cache.
        assert_eq!(url, s.url(Variant::Thumb, FILE, now + 60));
    }

    #[test]
    fn rejects_tampering_and_expiry() {
        let s = Signer::new(b"secret");
        let now = 1_700_000_000;
        let (exp, sig) = parts(&s.url(Variant::Thumb, FILE, now));
        assert!(!s.verify(Variant::Original, FILE, exp, &sig, now));
        assert!(!s.verify(Variant::Thumb, "cd.jpg", exp, &sig, now));
        assert!(!s.verify(Variant::Thumb, FILE, exp, "zz", now));
        assert!(!s.verify(Variant::Thumb, FILE, exp, &sig, exp + 1));
        assert!(!Signer::new(b"other").verify(Variant::Thumb, FILE, exp, &sig, now));
        assert!(!s.verify(Variant::Thumb, FILE, i64::MAX, &sig, now));
    }
}
