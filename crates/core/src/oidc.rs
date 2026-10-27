//! Signing in through an OpenID Connect provider (Authentik, Keycloak,
//! Google and the like).
//!
//! 1. The client calls `POST /auth/oidc/begin` (or, signed in,
//!    `POST /auth/oidc/begin-link`) and keeps the `handle`.
//! 2. It opens `url` in a browser; the provider sends the browser back to the
//!    server, which sends it on to `return_to` with `oidc_code` (or
//!    `oidc_error`) added to the query.
//! 3. The client trades the code and its handle for a session
//!    (`POST /auth/oidc/finish`), or links the sign-in to the account it is
//!    signed in to (`POST /auth/oidc/link`).
//!
//! The code reaches only `return_to`, and is worth nothing without the
//! handle, so a sign-in started by someone else can't be finished here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Query parameter carrying the one-time code back to `return_to`.
pub const CODE_PARAM: &str = "oidc_code";
/// Query parameter carrying a failure back to `return_to` instead.
pub const ERROR_PARAM: &str = "oidc_error";

/// `POST /auth/oidc/begin` and `POST /auth/oidc/begin-link`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcBegin {
    /// Where the browser goes at the end: a page of the server's own web
    /// app, or a loopback address (`http://127.0.0.1:<port>/...`) a native
    /// app listens on.
    pub return_to: String,
}

/// The answer to [`OidcBegin`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcStart {
    /// The provider's sign-in page.
    pub url: String,
    /// Secret; sent back with the code.
    pub handle: String,
}

/// `POST /auth/oidc/finish` and `POST /auth/oidc/link`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcFinish {
    pub code: String,
    pub handle: String,
}

/// Why a sign-in came back without a code (the `oidc_error` value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OidcError {
    /// The user said no at the provider.
    Denied,
    /// The provider answered with an error or something unexpected.
    Failed,
}

impl OidcError {
    pub fn as_str(self) -> &'static str {
        match self {
            OidcError::Denied => "denied",
            OidcError::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "denied" => OidcError::Denied,
            _ => OidcError::Failed,
        }
    }
}

/// One provider sign-in linked to an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub id: i64,
    /// What the provider calls the user (username or email), when it said.
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// `GET /auth/identities`: how the caller can sign in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInMethods {
    /// The account has a password.
    pub password: bool,
    pub identities: Vec<Identity>,
}

/// A username for an account made by a provider sign-in: `preferred` (or
/// the part of an email before `@`) cut down to the characters usernames
/// allow, 2 to 32 of them. `None` if nothing usable is left.
pub fn username_from(preferred: &str) -> Option<String> {
    let base = preferred.split('@').next().unwrap_or_default();
    let mut name: String = base
        .chars()
        .map(|c| if c == ' ' { '_' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(32)
        .collect();
    while name.ends_with('.') {
        name.pop();
    }
    (name.chars().count() >= 2).then_some(name)
}

/// What came back to `return_to`, read from its query string (with or
/// without the leading `?`): the code, the error, or `None` if neither is
/// there. The server only ever sends plain ASCII values, so nothing is
/// percent-decoded.
pub fn read_return(query: &str) -> Option<Result<String, OidcError>> {
    let query = query.strip_prefix('?').unwrap_or(query);
    let mut error = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some((CODE_PARAM, code)) if !code.is_empty() => return Some(Ok(code.to_owned())),
            Some((ERROR_PARAM, e)) => error = Some(OidcError::parse(e)),
            _ => {}
        }
    }
    error.map(Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_from_claims() {
        assert_eq!(username_from("alice").as_deref(), Some("alice"));
        assert_eq!(
            username_from("bob.smith@example.com").as_deref(),
            Some("bob.smith")
        );
        assert_eq!(username_from("Zoë Ann").as_deref(), Some("Zo_Ann"));
        assert_eq!(username_from("a").as_deref(), None);
        assert_eq!(username_from("日本").as_deref(), None);
        assert_eq!(username_from("").as_deref(), None);
        assert_eq!(username_from("x.").as_deref(), None);
        assert_eq!(username_from(&"n".repeat(40)).map(|n| n.len()), Some(32));
    }

    #[test]
    fn errors_round_trip() {
        for e in [OidcError::Denied, OidcError::Failed] {
            assert_eq!(OidcError::parse(e.as_str()), e);
        }
        assert_eq!(OidcError::parse("whatever"), OidcError::Failed);
    }

    #[test]
    fn reads_the_return() {
        assert_eq!(read_return("?oidc_code=ab12"), Some(Ok("ab12".into())));
        assert_eq!(read_return("x=1&oidc_code=ab12"), Some(Ok("ab12".into())));
        assert_eq!(
            read_return("oidc_error=denied"),
            Some(Err(OidcError::Denied))
        );
        assert_eq!(read_return("oidc_error=boom"), Some(Err(OidcError::Failed)));
        assert_eq!(read_return(""), None);
        assert_eq!(read_return("oidc_code="), None);
        assert_eq!(read_return("favicon"), None);
    }
}
