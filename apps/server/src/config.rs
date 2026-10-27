//! Server configuration, read from environment variables (see `.env.example`).

use std::net::SocketAddr;
use std::path::PathBuf;

use starbooru_core::admin::{MAX_BACKUP_DAYS, MAX_BACKUP_KEEP};
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
    /// Database backups are written here.
    pub backup_dir: PathBuf,
    /// `pg_dump`, for database backups. `None` turns backups off.
    pub pg_dump: Option<PathBuf>,
    /// Days between automatic backups; 0 turns them off.
    pub backup_days: u32,
    /// Automatic backups kept.
    pub backup_keep: u32,
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

        let backup_days = whole(&get, "STARBOORU_BACKUP_DAYS", 1)?;
        if backup_days > MAX_BACKUP_DAYS {
            return Err(ConfigError::Invalid {
                name: "STARBOORU_BACKUP_DAYS",
                reason: format!("must be between 0 and {MAX_BACKUP_DAYS}"),
            });
        }
        let backup_keep = whole(&get, "STARBOORU_BACKUP_KEEP", 7)?;
        if backup_keep == 0 || backup_keep > MAX_BACKUP_KEEP {
            return Err(ConfigError::Invalid {
                name: "STARBOORU_BACKUP_KEEP",
                reason: format!("must be between 1 and {MAX_BACKUP_KEEP}"),
            });
        }
        // `STARBOORU_PG_DUMP=off` turns database backups off.
        let pg_dump = get("STARBOORU_PG_DUMP").unwrap_or_else(|| "pg_dump".into());
        let pg_dump = (!matches!(
            pg_dump.trim().to_ascii_lowercase().as_str(),
            "" | "off" | "false" | "0"
        ))
        .then(|| PathBuf::from(pg_dump.trim()));

        let web_dir = get("STARBOORU_WEB_DIR")
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from);

        let data_dir: PathBuf = get("STARBOORU_DATA_DIR")
            .unwrap_or_else(|| "./data".into())
            .into();
        let backup_dir = get("STARBOORU_BACKUP_DIR")
            .filter(|s| !s.trim().is_empty())
            .map_or_else(|| data_dir.join("backups"), PathBuf::from);

        Ok(Self {
            bind,
            database_url,
            data_dir,
            web_dir,
            secret: secret.into_bytes(),
            max_upload_bytes: max_upload_mb.saturating_mul(1024 * 1024),
            allow_registration: flag(get("STARBOORU_ALLOW_REGISTRATION")),
            trust_proxy: flag(get("STARBOORU_TRUST_PROXY")),
            instance_name: get("STARBOORU_NAME").unwrap_or_else(|| "StarBooru".into()),
            trash_days,
            tools,
            url_import,
            backup_dir,
            pg_dump,
            backup_days,
            backup_keep,
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
            backup_dir: data_dir.join("backups"),
            pg_dump: None,
            backup_days: 0,
            backup_keep: 7,
        }
    }
}

/// A whole number from `name`, or `default` when it is unset.
fn whole(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: u32,
) -> Result<u32, ConfigError> {
    match get(name) {
        None => Ok(default),
        Some(v) => v.trim().parse().map_err(|_| ConfigError::Invalid {
            name,
            reason: format!("`{v}` is not a whole number"),
        }),
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
        assert_eq!(c.backup_dir, std::path::Path::new("./data/backups"));
        assert_eq!(c.pg_dump.as_deref(), Some(std::path::Path::new("pg_dump")));
        assert_eq!((c.backup_days, c.backup_keep), (1, 7));
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
    fn backups() {
        let with = |pairs: &[(&str, &str)]| {
            let mut all = vec![("DATABASE_URL", "x"), ("STARBOORU_SECRET", SECRET)];
            all.extend_from_slice(pairs);
            cfg(&all)
        };
        let c = with(&[
            ("STARBOORU_DATA_DIR", "/srv/data"),
            ("STARBOORU_BACKUP_DAYS", " 0 "),
            ("STARBOORU_BACKUP_KEEP", "30"),
            ("STARBOORU_PG_DUMP", "off"),
        ])
        .unwrap();
        assert_eq!(c.backup_dir, std::path::Path::new("/srv/data/backups"));
        assert_eq!((c.backup_days, c.backup_keep), (0, 30));
        assert!(c.pg_dump.is_none());
        let c = with(&[
            ("STARBOORU_BACKUP_DIR", "/backups"),
            ("STARBOORU_PG_DUMP", "/usr/lib/postgresql/17/bin/pg_dump"),
        ])
        .unwrap();
        assert_eq!(c.backup_dir, std::path::Path::new("/backups"));
        assert!(c.pg_dump.is_some());
        assert!(with(&[("STARBOORU_BACKUP_KEEP", "0")]).is_err());
        assert!(with(&[("STARBOORU_BACKUP_DAYS", "366")]).is_err());
        assert!(with(&[("STARBOORU_BACKUP_DAYS", "daily")]).is_err());
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
