//! Server configuration, read from environment variables (see `.env.example`).

use std::net::SocketAddr;
use std::path::PathBuf;

use starbooru_core::admin::{MAX_BACKUP_DAYS, MAX_BACKUP_KEEP};
use thiserror::Error;
use url::Url;

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
    /// Make smaller copies of big images for viewing (`STARBOORU_SAMPLES`, on by default).
    pub samples: bool,
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
    /// Sign-in through an OpenID Connect provider. `None` turns it off.
    pub oidc: Option<OidcConfig>,
}

#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// The provider's issuer URL; its metadata is read from
    /// `<issuer>/.well-known/openid-configuration`.
    pub issuer: Url,
    pub client_id: String,
    pub client_secret: String,
    /// Shown on the sign-in button.
    pub name: String,
    /// Where the provider sends the browser back:
    /// `<STARBOORU_PUBLIC_URL>/api/v1/auth/oidc/callback`.
    pub redirect_uri: Url,
    /// The server's own address, so sign-ins can return to the web app.
    pub public_url: Url,
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

        let oidc = oidc_config(&get)?;

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
            samples: get("STARBOORU_SAMPLES").is_none_or(|v| flag(Some(v))),
            tools,
            url_import,
            backup_dir,
            pg_dump,
            backup_days,
            backup_keep,
            oidc,
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
            samples: true,
            tools: None,
            url_import: Some(Reach::Public),
            trust_proxy: false,
            backup_dir: data_dir.join("backups"),
            pg_dump: None,
            backup_days: 0,
            backup_keep: 7,
            oidc: None,
        }
    }
}

/// `STARBOORU_OIDC_*`: all of issuer, client id and secret, or none.
fn oidc_config(get: &impl Fn(&str) -> Option<String>) -> Result<Option<OidcConfig>, ConfigError> {
    let value = |name: &str| {
        get(name)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let (issuer, client_id, client_secret) = match (
        value("STARBOORU_OIDC_ISSUER"),
        value("STARBOORU_OIDC_CLIENT_ID"),
        value("STARBOORU_OIDC_CLIENT_SECRET"),
    ) {
        (None, None, None) => return Ok(None),
        (Some(i), Some(c), Some(s)) => (i, c, s),
        _ => {
            return Err(ConfigError::Invalid {
                name: "STARBOORU_OIDC_ISSUER",
                reason: "set STARBOORU_OIDC_ISSUER, STARBOORU_OIDC_CLIENT_ID and \
                         STARBOORU_OIDC_CLIENT_SECRET together"
                    .into(),
            });
        }
    };
    let issuer = web_url(&issuer).map_err(|reason| ConfigError::Invalid {
        name: "STARBOORU_OIDC_ISSUER",
        reason,
    })?;
    let public = value("STARBOORU_PUBLIC_URL").ok_or(ConfigError::Missing(
        "STARBOORU_PUBLIC_URL (needed for OpenID Connect sign-in)",
    ))?;
    let public_url = web_url(&public).map_err(|reason| ConfigError::Invalid {
        name: "STARBOORU_PUBLIC_URL",
        reason,
    })?;
    let redirect_uri = public_url
        .join(&format!(
            "{}/auth/oidc/callback",
            starbooru_core::API_PREFIX.trim_start_matches('/')
        ))
        .map_err(|e| ConfigError::Invalid {
            name: "STARBOORU_PUBLIC_URL",
            reason: e.to_string(),
        })?;
    Ok(Some(OidcConfig {
        issuer,
        client_id,
        client_secret,
        name: value("STARBOORU_OIDC_NAME").unwrap_or_else(|| "SSO".into()),
        redirect_uri,
        public_url,
    }))
}

/// An https URL (http only for this machine), with a trailing `/` so
/// relative paths join below it.
fn web_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|e| format!("`{raw}`: {e}"))?;
    let loopback = matches!(
        url.host(),
        Some(url::Host::Domain("localhost"))
            | Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
            | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
    );
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        _ => return Err(format!("`{raw}` must be an https address")),
    }
    if url.query().is_some() || url.fragment().is_some() || !url.username().is_empty() {
        return Err(format!("`{raw}` must be a plain address"));
    }
    let mut url = url;
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
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
        assert!(c.samples);
        let no_samples = cfg(&[
            ("DATABASE_URL", "x"),
            ("STARBOORU_SECRET", SECRET),
            ("STARBOORU_SAMPLES", "off"),
        ])
        .unwrap();
        assert!(!no_samples.samples);
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
    fn oidc() {
        let with = |pairs: &[(&str, &str)]| {
            let mut all = vec![("DATABASE_URL", "x"), ("STARBOORU_SECRET", SECRET)];
            all.extend_from_slice(pairs);
            cfg(&all).map(|c| c.oidc)
        };
        assert!(with(&[]).unwrap().is_none());
        let full = [
            (
                "STARBOORU_OIDC_ISSUER",
                "https://auth.example.com/application/o/booru",
            ),
            ("STARBOORU_OIDC_CLIENT_ID", "booru"),
            ("STARBOORU_OIDC_CLIENT_SECRET", "s3cret"),
            ("STARBOORU_PUBLIC_URL", "https://booru.example.com"),
        ];
        let o = with(&full).unwrap().unwrap();
        assert_eq!(
            o.issuer.as_str(),
            "https://auth.example.com/application/o/booru/"
        );
        assert_eq!(
            o.redirect_uri.as_str(),
            "https://booru.example.com/api/v1/auth/oidc/callback"
        );
        assert_eq!(o.name, "SSO");
        // Under a path, and plain http on this machine.
        let o = with(&[
            ("STARBOORU_OIDC_ISSUER", "http://127.0.0.1:9000"),
            ("STARBOORU_OIDC_CLIENT_ID", "booru"),
            ("STARBOORU_OIDC_CLIENT_SECRET", "s3cret"),
            ("STARBOORU_OIDC_NAME", "Authentik"),
            ("STARBOORU_PUBLIC_URL", "http://localhost:8080/booru"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            o.redirect_uri.as_str(),
            "http://localhost:8080/booru/api/v1/auth/oidc/callback"
        );
        assert_eq!(o.name, "Authentik");
        // Half set, no public URL, or plain http elsewhere.
        assert!(with(&full[..2]).is_err());
        assert!(with(&full[..3]).is_err());
        let mut http = full;
        http[0].1 = "http://auth.example.com";
        assert!(with(&http).is_err());
        let mut query = full;
        query[3].1 = "https://booru.example.com/?x=1";
        assert!(with(&query).is_err());
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
