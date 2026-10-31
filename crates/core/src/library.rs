//! Moving a whole library over from another self-hosted booru: which systems
//! StarBooru can read, the request that starts an import and how one is going.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::Rating;
use crate::remote::{MAX_REMOTE_FIELD, RemoteError, clean_base};

/// Most library imports one account may have waiting or running.
pub const MAX_ACTIVE_LIBRARY_IMPORTS: i64 = 5;

/// The systems a library can be imported from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibrarySource {
    Szurubooru,
    Shimmie2,
    Danbooru,
    Stash,
    Blombooru,
    Monbooru,
    Lanbooru,
}

impl LibrarySource {
    pub const ALL: [LibrarySource; 7] = [
        LibrarySource::Szurubooru,
        LibrarySource::Shimmie2,
        LibrarySource::Danbooru,
        LibrarySource::Stash,
        LibrarySource::Blombooru,
        LibrarySource::Monbooru,
        LibrarySource::Lanbooru,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LibrarySource::Szurubooru => "szurubooru",
            LibrarySource::Shimmie2 => "shimmie2",
            LibrarySource::Danbooru => "danbooru",
            LibrarySource::Stash => "stash",
            LibrarySource::Blombooru => "blombooru",
            LibrarySource::Monbooru => "monbooru",
            LibrarySource::Lanbooru => "lanbooru",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.as_str() == s)
    }

    /// The name people know it by.
    pub fn name(self) -> &'static str {
        match self {
            LibrarySource::Szurubooru => "szurubooru",
            LibrarySource::Shimmie2 => "Shimmie2",
            LibrarySource::Danbooru => "Danbooru",
            LibrarySource::Stash => "Stash",
            LibrarySource::Blombooru => "Blombooru",
            LibrarySource::Monbooru => "Monbooru",
            LibrarySource::Lanbooru => "LANBooru",
        }
    }

    /// Whether its accounts sign in with a user name next to the key.
    pub fn takes_login(self) -> bool {
        matches!(self, LibrarySource::Szurubooru | LibrarySource::Danbooru)
    }

    /// Whether it has API keys at all (LANBooru reads are open).
    pub fn takes_secret(self) -> bool {
        self != LibrarySource::Lanbooru
    }
}

fn default_unrated() -> Rating {
    Rating::Questionable
}

/// The site to read: the body of `POST /library-imports` and
/// `POST /library-imports/check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibrarySite {
    pub source: LibrarySource,
    /// The site's address, like `http://192.168.1.4:8080`.
    pub base: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// The rating of posts the site doesn't rate.
    #[serde(default = "default_unrated")]
    pub unrated: Rating,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LibraryError {
    #[error(transparent)]
    Base(#[from] RemoteError),
    #[error("{0} needs the user name along with the key")]
    MissingLogin(&'static str),
    #[error("{0} needs the key along with the user name")]
    MissingSecret(&'static str),
}

/// Empty credentials as none, and only those the system uses.
pub fn clean_credentials(
    source: LibrarySource,
    login: Option<String>,
    secret: Option<String>,
) -> Result<(Option<String>, Option<String>), LibraryError> {
    let filled = |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let login = filled(login).filter(|_| source.takes_login());
    let secret = filled(secret).filter(|_| source.takes_secret());
    if [&login, &secret]
        .into_iter()
        .flatten()
        .any(|field| field.len() > MAX_REMOTE_FIELD)
    {
        return Err(RemoteError::TooLong.into());
    }
    if source.takes_login() {
        match (&login, &secret) {
            (None, Some(_)) => return Err(LibraryError::MissingLogin(source.name())),
            (Some(_), None) => return Err(LibraryError::MissingSecret(source.name())),
            _ => {}
        }
    }
    Ok((login, secret))
}

impl LibrarySite {
    /// Checks the request and trims it.
    pub fn clean(mut self) -> Result<LibrarySite, LibraryError> {
        self.base = clean_base(&self.base)?;
        (self.login, self.secret) = clean_credentials(self.source, self.login, self.secret)?;
        Ok(self)
    }
}

/// The answer of `POST /library-imports/check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryCheck {
    /// Posts on the site; `None` when it doesn't say.
    pub total: Option<u64>,
    /// Whether this account's import would bring aliases and implications
    /// (shared by the whole server, so administrators only).
    pub rules: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryState {
    Queued,
    Running,
    Done,
    Stopped,
    Failed,
}

impl LibraryState {
    pub fn as_str(self) -> &'static str {
        match self {
            LibraryState::Queued => "queued",
            LibraryState::Running => "running",
            LibraryState::Done => "done",
            LibraryState::Stopped => "stopped",
            LibraryState::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            LibraryState::Queued,
            LibraryState::Running,
            LibraryState::Done,
            LibraryState::Stopped,
            LibraryState::Failed,
        ]
        .into_iter()
        .find(|state| state.as_str() == s)
    }

    pub fn is_active(self) -> bool {
        matches!(self, LibraryState::Queued | LibraryState::Running)
    }
}

/// What an import is reading, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryPhase {
    /// Aliases, implications and the tag tree.
    Rules,
    Posts,
    /// Pools, albums and favourites listed apart from the posts.
    Groups,
}

impl LibraryPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            LibraryPhase::Rules => "rules",
            LibraryPhase::Posts => "posts",
            LibraryPhase::Groups => "groups",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rules" => Some(LibraryPhase::Rules),
            "posts" => Some(LibraryPhase::Posts),
            "groups" => Some(LibraryPhase::Groups),
            _ => None,
        }
    }

    pub fn next(self) -> Option<Self> {
        match self {
            LibraryPhase::Rules => Some(LibraryPhase::Posts),
            LibraryPhase::Posts => Some(LibraryPhase::Groups),
            LibraryPhase::Groups => None,
        }
    }
}

/// An import of a whole library. Credentials are never sent back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryImport {
    pub id: i64,
    pub source: LibrarySource,
    pub base: String,
    pub state: LibraryState,
    pub phase: LibraryPhase,
    /// Posts on the site; `None` when it doesn't say.
    pub total: Option<u64>,
    pub imported: u64,
    /// Files already here: their tags were added to the existing post.
    pub merged: u64,
    pub failed: u64,
    /// Aliases, implications and parents added.
    pub rules: u64,
    /// Something left out that doesn't stop the import.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Why it stopped, when it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}

impl LibraryImport {
    /// Posts dealt with so far, failed ones included.
    pub fn handled(&self) -> u64 {
        self.imported
            .saturating_add(self.merged)
            .saturating_add(self.failed)
    }

    /// How far along the posts are, from 0 to 1, when the total is known.
    pub fn fraction(&self) -> Option<f64> {
        let total = self.total.filter(|t| *t > 0)?;
        // Precision only matters for the width of a progress bar.
        #[allow(clippy::cast_precision_loss)]
        Some((self.handled() as f64 / total as f64).clamp(0.0, 1.0))
    }
}

/// `POST /library-imports/{id}/retry`: credentials aren't kept once an import
/// ends, so going on asks for them again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryRetry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

/// A post that couldn't be imported, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryFailure {
    /// The post's id on the other site.
    pub remote_id: String,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(source: LibrarySource) -> LibrarySite {
        LibrarySite {
            source,
            base: " http://192.168.1.4:8080/ ".into(),
            login: None,
            secret: None,
            unrated: Rating::Explicit,
        }
    }

    #[test]
    fn every_source_and_state_round_trips() {
        for source in LibrarySource::ALL {
            let json = serde_json::to_string(&source).unwrap();
            assert_eq!(json, format!("\"{}\"", source.as_str()));
            assert_eq!(LibrarySource::parse(source.as_str()), Some(source));
        }
        for state in ["queued", "running", "done", "stopped", "failed"] {
            assert_eq!(LibraryState::parse(state).unwrap().as_str(), state);
        }
        for phase in [
            LibraryPhase::Rules,
            LibraryPhase::Posts,
            LibraryPhase::Groups,
        ] {
            assert_eq!(LibraryPhase::parse(phase.as_str()), Some(phase));
        }
        assert_eq!(LibrarySource::parse("gelbooru"), None);
    }

    #[test]
    fn cleans_the_address_and_credentials() {
        let mut s = site(LibrarySource::Stash);
        s.login = Some("ignored".into());
        s.secret = Some(" key ".into());
        let s = s.clean().unwrap();
        assert_eq!(s.base, "http://192.168.1.4:8080");
        assert_eq!(s.login, None, "Stash has no user names");
        assert_eq!(s.secret.as_deref(), Some("key"));

        let mut s = site(LibrarySource::Lanbooru);
        s.secret = Some("key".into());
        assert_eq!(s.clean().unwrap().secret, None, "LANBooru has no keys");

        let mut s = site(LibrarySource::Stash);
        s.base = "ftp://x".into();
        assert_eq!(s.clean(), Err(LibraryError::Base(RemoteError::BadBase)));
    }

    #[test]
    fn login_and_key_come_together() {
        let mut s = site(LibrarySource::Danbooru);
        s.secret = Some("key".into());
        assert_eq!(
            s.clone().clean(),
            Err(LibraryError::MissingLogin("Danbooru"))
        );
        s.secret = None;
        s.login = Some("me".into());
        assert_eq!(s.clean(), Err(LibraryError::MissingSecret("Danbooru")));
        // Anonymous is fine: public sites don't ask for an account.
        assert!(site(LibrarySource::Szurubooru).clean().is_ok());
    }

    #[test]
    fn refuses_long_credentials() {
        let mut s = site(LibrarySource::Stash);
        s.secret = Some("k".repeat(MAX_REMOTE_FIELD + 1));
        assert_eq!(s.clean(), Err(LibraryError::Base(RemoteError::TooLong)));
    }

    #[test]
    fn unrated_defaults_to_questionable() {
        let s: LibrarySite =
            serde_json::from_str(r#"{"source":"stash","base":"http://x"}"#).unwrap();
        assert_eq!(s.unrated, Rating::Questionable);
    }

    #[test]
    fn progress_counts_every_post_handled() {
        let import = LibraryImport {
            id: 1,
            source: LibrarySource::Stash,
            base: "http://x".into(),
            state: LibraryState::Running,
            phase: LibraryPhase::Posts,
            total: Some(10),
            imported: 2,
            merged: 2,
            failed: 1,
            rules: 0,
            note: None,
            error: None,
            created_at: Utc::now(),
            finished_at: None,
        };
        assert_eq!(import.handled(), 5);
        assert_eq!(import.fraction(), Some(0.5));
        let mut empty = import.clone();
        empty.total = Some(0);
        assert_eq!(empty.fraction(), None);
        let mut over = import;
        over.total = Some(1);
        assert_eq!(over.fraction(), Some(1.0));
    }
}
