//! Server configuration, read from environment variables (see `.env.example`).

use std::net::SocketAddr;
use std::path::PathBuf;

use thiserror::Error;

use crate::fetch::Reach;
use crate::probe::Tools;

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
    /// Posts stay in the recycle bin this many days, then are purged. 0 keeps them.
    pub trash_days: u32,
    /// ffmpeg and ffprobe, for video, AVIF and JPEG XL thumbnails. `None` turns them off.
    pub tools: Option<Tools>,
    /// Which addresses URL import may fetch. `None` turns it off.
    pub url_import: Option<Reach>,
    /// Take the client address from the last `X-Forwarded-For` entry (set
    /// it when a reverse proxy is in front), for the sign-in limits.
    pub trust_proxy: bool,
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

        let trash_days: u32 = match get("STARBOORU_TRASH_DAYS") {
            None => 0,
            Some(v) => v.trim().parse().map_err(|_| ConfigError::Invalid {
                name: "STARBOORU_TRASH_DAYS",
                reason: format!("`{v}` is not a whole number of days"),
            })?,
        };

        // `STARBOORU_FFMPEG=off` turns thumbnails for video, AVIF and JPEG XL off.
        let ffmpeg = get("STARBOORU_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
        let tools = (!matches!(
            ffmpeg.trim().to_ascii_lowercase().as_str(),
            "" | "off" | "false" | "0"
        ))
        .then(|| Tools {
            ffmpeg: ffmpeg.trim().into(),
            ffprobe: get("STARBOORU_FFPROBE")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "ffprobe".into())
                .trim()
                .into(),
        });

        let url_import = match get("STARBOORU_URL_IMPORT")
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref()
        {
            None | Some("" | "on" | "true" | "1" | "public") => Some(Reach::Public),
            Some("off" | "false" | "0") => None,
            Some("private" | "lan") => Some(Reach::Any),
            Some(other) => {
                return Err(ConfigError::Invalid {
                    name: "STARBOORU_URL_IMPORT",
                    reason: format!("`{other}` is not on, off or private"),
                });
            }
        };

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
            allow_registration: flag(get("STARBOORU_ALLOW_REGISTRATION")),
            trust_proxy: flag(get("STARBOORU_TRUST_PROXY")),
            instance_name: get("STARBOORU_NAME").unwrap_or_else(|| "StarBooru".into()),
            trash_days,
            tools,
            url_import,
        })
    }

    #[cfg(test)]
    pub fn for_tests(data_dir: &std::path::Path) -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            database_url: String::new(),
            data_dir: data_dir.to_path_buf(),
            web_dir: None,
            secret: b"test-secret-test-secret-test-secret!".to_vec(),
            max_upload_bytes: 64 * 1024 * 1024,
            allow_registration: true,
            instance_name: "StarBooru".into(),
            trash_days: 0,
            tools: None,
            url_import: Some(Reach::Public),
            trust_proxy: false,
        }
    }
}

fn flag(value: Option<String>) -> bool {
    value.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
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
        assert_eq!(c.trash_days, 0);
        let tools = c.tools.unwrap();
        assert_eq!(tools.ffmpeg, std::path::Path::new("ffmpeg"));
        assert_eq!(tools.ffprobe, std::path::Path::new("ffprobe"));
        assert_eq!(c.url_import, Some(Reach::Public));
        assert!(!c.trust_proxy);
        let proxied = cfg(&[
            ("DATABASE_URL", "x"),
            ("STARBOORU_SECRET", SECRET),
            ("STARBOORU_TRUST_PROXY", " True"),
        ])
        .unwrap();
        assert!(proxied.trust_proxy);
    }

    #[test]
    fn url_import() {
        let with = |v: &str| {
            cfg(&[
                ("DATABASE_URL", "x"),
                ("STARBOORU_SECRET", SECRET),
                ("STARBOORU_URL_IMPORT", v),
            ])
            .map(|c| c.url_import)
        };
        assert_eq!(with("off").unwrap(), None);
        assert_eq!(with(" Private ").unwrap(), Some(Reach::Any));
        assert_eq!(with("on").unwrap(), Some(Reach::Public));
        assert!(with("sometimes").is_err());
    }

    #[test]
    fn media_tools() {
        let with = |pairs: &[(&str, &str)]| {
            let mut all = vec![("DATABASE_URL", "x"), ("STARBOORU_SECRET", SECRET)];
            all.extend_from_slice(pairs);
            cfg(&all).unwrap().tools
        };
        assert!(with(&[("STARBOORU_FFMPEG", "off")]).is_none());
        assert!(with(&[("STARBOORU_FFMPEG", " ")]).is_none());
        let t = with(&[
            ("STARBOORU_FFMPEG", "/opt/ff/ffmpeg"),
            ("STARBOORU_FFPROBE", "/opt/ff/ffprobe"),
        ])
        .unwrap();
        assert_eq!(t.ffmpeg, std::path::Path::new("/opt/ff/ffmpeg"));
        assert_eq!(t.ffprobe, std::path::Path::new("/opt/ff/ffprobe"));
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
        assert!(
            cfg(&[
                ("DATABASE_URL", "x"),
                ("STARBOORU_SECRET", SECRET),
                ("STARBOORU_TRASH_DAYS", "soon")
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
