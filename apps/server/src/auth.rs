//! Accounts, password hashing, and bearer tokens: sign-in sessions and API
//! keys.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::Method;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use sha2::{Digest, Sha256};
use starbooru_core::Role;
use starbooru_core::keys::API_KEY_PREFIX;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const SESSION_DAYS: i64 = 90;
/// `last_used_at` of a session or key is refreshed when it is older than
/// this, so most requests don't write.
const TOUCH_AFTER_SECS: i64 = 300;
const MAX_PASSWORD_BYTES: usize = 1024;

/// The caller, resolved from `Authorization: Bearer <token>`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub role: Role,
    pub credential: Credential,
}

/// What the request signed in with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential {
    Session(i64),
    ApiKey(i64),
}

impl AuthUser {
    pub fn can_moderate(&self, owner: Option<i64>) -> bool {
        self.role == Role::Admin || owner == Some(self.id)
    }

    pub fn require_admin(&self) -> AppResult<()> {
        if self.role == Role::Admin {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// The sign-in session behind the request. API keys are refused, so a
    /// leaked key can't change the password, sign-ins or keys.
    pub fn session_id(&self) -> AppResult<i64> {
        match self.credential {
            Credential::Session(id) => Ok(id),
            Credential::ApiKey(_) => Err(AppError::Denied(
                "API keys can't do this; sign in to the app instead",
            )),
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or(AppError::Unauthorized)?;
        let token_hash = hash_token(token);
        let is_key = token.starts_with(API_KEY_PREFIX);
        let found: Option<(i64, String, i64, bool, bool)> = if is_key {
            sqlx::query_as(
                "SELECT u.id, u.role, k.id, k.read_only, \
                        coalesce(k.last_used_at < now() - make_interval(secs => $2), true) \
                 FROM api_keys k JOIN users u ON u.id = k.user_id \
                 WHERE k.token_hash = $1 AND (k.expires_at IS NULL OR k.expires_at > now()) \
                   AND u.disabled_at IS NULL",
            )
        } else {
            sqlx::query_as(
                "SELECT u.id, u.role, s.id, false, \
                        s.last_used_at < now() - make_interval(secs => $2) \
                 FROM sessions s JOIN users u ON u.id = s.user_id \
                 WHERE s.token_hash = $1 AND s.expires_at > now() AND u.disabled_at IS NULL",
            )
        }
        .bind(&token_hash)
        .bind(TOUCH_AFTER_SECS as f64)
        .fetch_optional(&state.db)
        .await?;
        let (id, role, credential_id, read_only, stale) = found.ok_or(AppError::Unauthorized)?;
        let role = Role::from_db(&role).ok_or_else(|| AppError::internal("bad role in db"))?;
        if read_only && !matches!(parts.method, Method::GET | Method::HEAD) {
            return Err(AppError::Denied("this API key is read-only"));
        }
        if stale {
            let touch = if is_key {
                "UPDATE api_keys SET last_used_at = now() WHERE id = $1"
            } else {
                "UPDATE sessions SET last_used_at = now() WHERE id = $1"
            };
            sqlx::query(touch)
                .bind(credential_id)
                .execute(&state.db)
                .await?;
        }
        let credential = if is_key {
            Credential::ApiKey(credential_id)
        } else {
            Credential::Session(credential_id)
        };
        Ok(Self {
            id,
            role,
            credential,
        })
    }
}

pub fn new_token() -> String {
    format!("sb_{}", random_hex())
}

pub fn new_api_key() -> String {
    format!("{API_KEY_PREFIX}{}", random_hex())
}

fn random_hex() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub fn validate_username(name: &str) -> AppResult<()> {
    let len = name.chars().count();
    if !(2..=32).contains(&len) {
        return Err(AppError::BadRequest(
            "username must be 2 to 32 characters".into(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(AppError::BadRequest(
            "username may only contain letters, digits, `_`, `-` and `.`".into(),
        ));
    }
    Ok(())
}

pub fn validate_password(pw: &str) -> AppResult<()> {
    if pw.chars().count() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    if pw.len() > MAX_PASSWORD_BYTES {
        return Err(AppError::BadRequest("password is too long".into()));
    }
    Ok(())
}

/// Argon2id with default parameters. Slow on purpose: run off the async runtime.
pub async fn hash_password(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let hash: PasswordHash = Argon2::default()
            .hash_password(password.as_bytes())
            .map_err(AppError::internal)?;
        Ok(hash.to_string())
    })
    .await?
}

/// Verifies `password` against a stored PHC string. With `stored = None` it
/// still does the work of one hash, so unknown usernames take as long as
/// wrong passwords.
pub async fn verify_password(password: String, stored: Option<String>) -> AppResult<bool> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Ok(false);
    }
    tokio::task::spawn_blocking(move || {
        let Some(stored) = stored else {
            let _: Result<PasswordHash, _> = Argon2::default().hash_password(password.as_bytes());
            return Ok(false);
        };
        let parsed = PasswordHash::new(&stored).map_err(AppError::internal)?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_round_trip() {
        let hash = hash_password("correct horse".into()).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(
            verify_password("correct horse".into(), Some(hash.clone()))
                .await
                .unwrap()
        );
        assert!(!verify_password("wrong".into(), Some(hash)).await.unwrap());
        assert!(!verify_password("x".into(), None).await.unwrap());
        assert!(
            verify_password("x".into(), Some("garbage".into()))
                .await
                .is_err()
        );
    }

    #[test]
    fn tokens_are_unique_and_hashed() {
        let (a, b) = (new_token(), new_token());
        assert_ne!(a, b);
        assert!(a.starts_with("sb_") && !a.starts_with(API_KEY_PREFIX));
        assert!(new_api_key().starts_with(API_KEY_PREFIX));
        assert_eq!(hash_token(&a).len(), 32);
    }

    #[test]
    fn validates_credentials() {
        assert!(validate_username("star_fan-1.0").is_ok());
        assert!(validate_username("a").is_err());
        assert!(validate_username("bad name").is_err());
        assert!(validate_username("ünïcode").is_err());
        assert!(validate_password("short").is_err());
        assert!(validate_password(&"x".repeat(2000)).is_err());
        assert!(validate_password("long enough").is_ok());
    }
}
