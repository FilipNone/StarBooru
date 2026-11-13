//! Accounts on other boorus that a user saved (`booru_accounts`). The key is
//! sealed before it reaches the database and opened only to be sent to its
//! site, so a copy of the database alone gives none of them away, and no
//! answer of the API ever carries one.

use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use sha2::Sha256;
use sqlx::FromRow;
use starbooru_core::{BooruAccount, BooruApi};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Seals and opens saved keys.
pub struct Vault {
    /// `None` only if the cipher refused its key, which it doesn't.
    key: Option<LessSafeKey>,
}

impl Vault {
    /// Derives the vault's key from the server's secret.
    pub fn new(secret: &[u8]) -> Self {
        // A key of its own, apart from every other use of the secret.
        let derived = Hmac::<Sha256>::new_from_slice(secret).ok().map(|mut mac| {
            mac.update(b"starbooru-booru-accounts-v1");
            mac.finalize().into_bytes()
        });
        let key = derived
            .and_then(|bytes| UnboundKey::new(&CHACHA20_POLY1305, &bytes).ok())
            .map(LessSafeKey::new);
        Self { key }
    }

    /// What a sealed key is bound to: it opens only for the same account and
    /// site.
    fn bound_to(user_id: i64, base: &str) -> Vec<u8> {
        format!("{user_id}|{base}").into_bytes()
    }

    /// Encrypts `secret` for `user_id`'s account on `base`.
    pub fn seal(&self, user_id: i64, base: &str, secret: &str) -> AppResult<Vec<u8>> {
        let key = self
            .key
            .as_ref()
            .ok_or_else(|| AppError::internal("the vault has no key"))?;
        let mut nonce = [0u8; NONCE_LEN];
        rand::fill(&mut nonce);
        let mut sealed = secret.as_bytes().to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(Self::bound_to(user_id, base)),
            &mut sealed,
        )
        .map_err(|_| AppError::internal("cannot seal a key"))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// The key back, or `None` when it was sealed for another account or
    /// site, with another server secret, or was damaged.
    pub fn open(&self, user_id: i64, base: &str, sealed: &[u8]) -> Option<String> {
        let (nonce, body) = sealed.split_at_checked(NONCE_LEN)?;
        let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
        let mut body = body.to_vec();
        let plain = self
            .key
            .as_ref()?
            .open_in_place(nonce, Aad::from(Self::bound_to(user_id, base)), &mut body)
            .ok()?;
        String::from_utf8(plain.to_vec()).ok()
    }
}

/// A saved account with its key, ready to send to its site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub api: BooruApi,
    pub base: String,
    pub login: String,
    pub key: String,
}

/// A user's saved accounts, for one request or one import.
#[derive(Debug, Clone, Default)]
pub struct Accounts(pub Vec<Account>);

impl Accounts {
    /// The login and key for the site at `base`.
    pub fn of(&self, base: &str) -> Option<(&str, &str)> {
        self.0
            .iter()
            .find(|account| account.base == base)
            .map(|account| (account.login.as_str(), account.key.as_str()))
    }

    /// The sites with an account, for telling whose a link is.
    pub fn sites(&self) -> Vec<(BooruApi, String)> {
        self.0
            .iter()
            .map(|account| (account.api, account.base.clone()))
            .collect()
    }
}

/// A row of `booru_accounts`.
#[derive(FromRow)]
struct Row {
    id: i64,
    api: String,
    base: String,
    login: String,
    sealed_key: Vec<u8>,
    updated_at: DateTime<Utc>,
}

/// The API a stored name stands for. `None` for one this version doesn't
/// know, which a newer one may have written.
fn api_of(name: &str) -> Option<BooruApi> {
    BooruApi::ALL.into_iter().find(|api| api.as_str() == name)
}

/// Every account `user_id` saved, with the keys opened. One that no longer
/// opens (the server's secret changed) is left out, as if never saved.
pub async fn load(state: &AppState, user_id: i64) -> AppResult<Accounts> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, api, base, login, sealed_key, updated_at FROM booru_accounts \
         WHERE user_id = $1 ORDER BY id",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let accounts = rows
        .into_iter()
        .filter_map(|row| {
            Some(Account {
                api: api_of(&row.api)?,
                key: state.vault.open(user_id, &row.base, &row.sealed_key)?,
                base: row.base,
                login: row.login,
            })
        })
        .collect();
    Ok(Accounts(accounts))
}

/// The saved accounts as the API lists them: never with a key.
pub async fn list(state: &AppState, user_id: i64) -> AppResult<Vec<BooruAccount>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, api, base, login, sealed_key, updated_at FROM booru_accounts \
         WHERE user_id = $1 ORDER BY base",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(BooruAccount {
                id: row.id,
                api: api_of(&row.api)?,
                // One that can't be opened any more is shown, so it can be
                // saved again or forgotten.
                usable: state
                    .vault
                    .open(user_id, &row.base, &row.sealed_key)
                    .is_some(),
                base: row.base,
                login: row.login,
                updated_at: row.updated_at,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_opens_only_for_its_account_and_site() {
        let vault = Vault::new(b"test-secret-test-secret-test-secret!");
        let sealed = vault.seal(7, "https://b.org", "hunter2").unwrap();
        assert_eq!(
            vault.open(7, "https://b.org", &sealed).as_deref(),
            Some("hunter2")
        );
        assert_eq!(vault.open(8, "https://b.org", &sealed), None);
        assert_eq!(vault.open(7, "https://c.org", &sealed), None);
        let other = Vault::new(b"another-secret-another-secret-another");
        assert_eq!(other.open(7, "https://b.org", &sealed), None);
    }

    #[test]
    fn what_is_stored_gives_nothing_away() {
        let vault = Vault::new(b"test-secret-test-secret-test-secret!");
        let a = vault.seal(7, "https://b.org", "hunter2").unwrap();
        let b = vault.seal(7, "https://b.org", "hunter2").unwrap();
        // The same key twice doesn't seal the same way.
        assert_ne!(a, b);
        assert!(!a.windows(7).any(|window| window == b"hunter2"));
        // The nonce, the key and the tag.
        assert_eq!(a.len(), NONCE_LEN + 7 + 16);
    }

    #[test]
    fn damaged_or_cut_short_doesnt_open() {
        let vault = Vault::new(b"test-secret-test-secret-test-secret!");
        let sealed = vault.seal(7, "https://b.org", "hunter2").unwrap();
        for cut in [0, 5, NONCE_LEN, sealed.len() - 1] {
            assert_eq!(
                vault.open(7, "https://b.org", &sealed[..cut]),
                None,
                "{cut}"
            );
        }
        let mut flipped = sealed.clone();
        flipped[NONCE_LEN + 2] ^= 1;
        assert_eq!(vault.open(7, "https://b.org", &flipped), None);
        // An empty key seals and opens too.
        let empty = vault.seal(7, "https://b.org", "").unwrap();
        assert_eq!(vault.open(7, "https://b.org", &empty).as_deref(), Some(""));
    }

    #[test]
    fn finds_the_account_of_a_site() {
        let accounts = Accounts(vec![Account {
            api: BooruApi::Gelbooru,
            base: "https://g.org".into(),
            login: "42".into(),
            key: "k".into(),
        }]);
        assert_eq!(accounts.of("https://g.org"), Some(("42", "k")));
        assert_eq!(accounts.of("https://g.org/"), None);
        assert_eq!(
            accounts.sites(),
            [(BooruApi::Gelbooru, "https://g.org".to_owned())]
        );
    }
}
