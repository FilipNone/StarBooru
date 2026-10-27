//! Server configuration, read from environment variables (see `.env.example`).

use std::net::SocketAddr;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0} is not set")]
    Missing(&'static str),
    #[error("{name} is invalid: {reason}")]
    Invalid { name: &'static str, reason: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub data_dir: PathBuf,
    /// Built web app (`dx bundle --web` output). `None` disables hosting it.
    pub web_dir: Option<PathBuf>,
    pub secret: Vec<u8>,
    pub max_upload_bytes: usize,
    pub allow_registration: bool,
    pub instance_name: String,
}

const MIN_SECRET_LEN: usize = 32;

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let bind_raw = get("STARBOORU_BIND").unwrap_or_else(|| "0.0.0.0:8080".into());
        let bind = bind_raw.parse().map_err(|e| ConfigError::Invalid {
            name: "STARBOORU_BIND",
            reason: format!("{bind_raw}: {e}"),
        })?;

        let database_url = get("DATABASE_URL").ok_or(ConfigError::Missing("DATABASE_URL"))?;

        let secret = get("STARBOORU_SECRET").ok_or(ConfigError::Missing("STARBOORU_SECRET"))?;
        if secret.len() < MIN_SECRET_LEN {
            return Err(ConfigError::Invalid {
                name: "STARBOORU_SECRET",
                reason: format!(
                    "must be at least {MIN_SECRET_LEN} characters (try `openssl rand -hex 32`)"
                ),
            });
        }

        let max_upload_mb: usize = match get("STARBOORU_MAX_UPLOAD_MB") {
            None => 512,
            Some(v) => v.trim().parse().map_err(|_| ConfigError::Invalid {
                name: "STARBOORU_MAX_UPLOAD_MB",
                reason: format!("`{v}` is not a whole number"),
            })?,
        };
        if max_upload_mb == 0 || max_upload_mb > 16 * 1024 {
            return Err(ConfigError::Invalid {
                name: "STARBOORU_MAX_UPLOAD_MB",
                reason: "must be between 1 and 16384".into(),
            });
        }

        let web_dir = get("STARBOORU_WEB_DIR")
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from);

        Ok(Self {
            bind,
            database_url,
            data_dir: get("STARBOORU_DATA_DIR")
                .unwrap_or_else(|| "./data".into())
                .into(),
            web_dir,
            secret: secret.into_bytes(),
            max_upload_bytes: max_upload_mb.saturating_mul(1024 * 1024),
            allow_registration: get("STARBOORU_ALLOW_REGISTRATION").is_some_and(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            }),
            instance_name: get("STARBOORU_NAME").unwrap_or_else(|| "StarBooru".into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_lookup(|k| map.get(k).cloned())
    }

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn defaults() {
        let c = cfg(&[
            ("DATABASE_URL", "postgres://x"),
            ("STARBOORU_SECRET", SECRET),
        ])
        .unwrap();
        assert_eq!(c.bind.port(), 8080);
        assert_eq!(c.max_upload_bytes, 512 * 1024 * 1024);
        assert!(!c.allow_registration);
        assert!(c.web_dir.is_none());
    }

    #[test]
    fn rejects_bad_values() {
        assert!(cfg(&[("STARBOORU_SECRET", SECRET)]).is_err());
        assert!(cfg(&[("DATABASE_URL", "x"), ("STARBOORU_SECRET", "short")]).is_err());
        assert!(
            cfg(&[
                ("DATABASE_URL", "x"),
                ("STARBOORU_SECRET", SECRET),
                ("STARBOORU_MAX_UPLOAD_MB", "-3")
            ])
            .is_err()
        );
        assert!(
            cfg(&[
                ("DATABASE_URL", "x"),
                ("STARBOORU_SECRET", SECRET),
                ("STARBOORU_BIND", "nope")
            ])
            .is_err()
        );
    }

    #[test]
    fn registration_flag() {
        let open = |v: &str| {
            cfg(&[
                ("DATABASE_URL", "x"),
                ("STARBOORU_SECRET", SECRET),
                ("STARBOORU_ALLOW_REGISTRATION", v),
            ])
            .unwrap()
            .allow_registration
        };
        assert!(open("true"));
        assert!(open(" TRUE "));
        assert!(open("Yes"));
        assert!(open("1"));
        assert!(!open("false"));
        assert!(!open("0"));
        assert!(!open(""));
    }
}
