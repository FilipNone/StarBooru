//! Database backups: `pg_dump` archives (custom format) in the backup
//! folder, made by hand from the app or on a schedule. Media files are not
//! in them: they are immutable files in the data folder, copied as they are.
//!
//! Archives hold every account's password hash, so only admins signed in
//! with a session list, make, download or remove them. Downloads use
//! short-lived signed links, like media, so a browser can save them.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use starbooru_core::Backup;
use tokio::process::Command;
use tokio::sync::{Mutex, OnceCell};
use tracing::{info, warn};

use crate::error::{AppError, AppResult};
use crate::settings;
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

const PREFIX: &str = "starbooru-";
const AUTO_PREFIX: &str = "starbooru-auto-";
const EXTENSION: &str = ".dump";
const PARTIAL: &str = ".partial";
const STAMP: &str = "%Y%m%d-%H%M%S";
/// Download links work this long.
const LINK_SECS: i64 = 3600;
/// A dump that takes longer is stopped.
const DUMP_TIMEOUT: Duration = Duration::from_secs(4 * 3600);
/// Kept from `pg_dump`'s error output for the log.
const MAX_ERROR_CHARS: usize = 500;

pub struct Backups {
    key: Vec<u8>,
    /// One backup at a time.
    running: Mutex<()>,
    /// `pg_dump` answered `--version`.
    available: OnceCell<bool>,
}

impl Backups {
    pub fn new(secret: &[u8]) -> Self {
        let mut key = b"starbooru-backup-v1:".to_vec();
        key.extend_from_slice(secret);
        Self {
            key,
            running: Mutex::new(()),
            available: OnceCell::new(),
        }
    }

    fn mac(&self, name: &str, exp: i64) -> Option<HmacSha256> {
        let mut mac = HmacSha256::new_from_slice(&self.key).ok()?;
        mac.update(format!("{name}:{exp}").as_bytes());
        Some(mac)
    }

    pub fn url(&self, name: &str, now: i64) -> String {
        let exp = now.saturating_add(LINK_SECS);
        let sig = self
            .mac(name, exp)
            .map(|m| hex::encode(m.finalize().into_bytes()))
            .unwrap_or_default();
        format!("/backups/{name}?exp={exp}&sig={sig}")
    }

    pub fn verify(&self, name: &str, exp: i64, sig: &str, now: i64) -> bool {
        if exp < now || exp > now.saturating_add(LINK_SECS) {
            return false;
        }
        let Ok(sig) = hex::decode(sig) else {
            return false;
        };
        self.mac(name, exp)
            .is_some_and(|m| m.verify_slice(&sig).is_ok())
    }

    pub fn is_running(&self) -> bool {
        self.running.try_lock().is_err()
    }
}

/// `pg_dump` is turned on and runs.
pub async fn available(state: &AppState) -> bool {
    let Some(tool) = &state.config.pg_dump else {
        return false;
    };
    *state
        .backups
        .available
        .get_or_init(|| async {
            let found = Command::new(tool)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status()
                .await
                .is_ok_and(|s| s.success());
            if !found {
                warn!(
                    "database backups are off: {} does not run (install the PostgreSQL client \
                     or set STARBOORU_PG_DUMP)",
                    tool.display()
                );
            }
            found
        })
        .await
}

/// When a backup named `name` was made, and whether the schedule made it.
/// Anything else (including path tricks) is not a backup.
pub fn parse_name(name: &str) -> Option<(DateTime<Utc>, bool)> {
    let stem = name.strip_suffix(EXTENSION)?;
    let (stamp, automatic) = match stem.strip_prefix(AUTO_PREFIX) {
        Some(stamp) => (stamp, true),
        None => (stem.strip_prefix(PREFIX)?, false),
    };
    let at = NaiveDateTime::parse_from_str(stamp, STAMP).ok()?.and_utc();
    // Only the exact form this module writes.
    (at.format(STAMP).to_string() == stamp).then_some((at, automatic))
}

fn file_name(at: DateTime<Utc>, automatic: bool) -> String {
    let prefix = if automatic { AUTO_PREFIX } else { PREFIX };
    format!("{prefix}{}{EXTENSION}", at.format(STAMP))
}

/// The path of the backup `name`, if it is one.
pub fn path_for(state: &AppState, name: &str) -> Option<PathBuf> {
    parse_name(name).map(|_| state.config.backup_dir.join(name))
}

/// The backups on disk, newest first.
pub async fn list(state: &AppState) -> AppResult<Vec<Backup>> {
    let mut entries = match tokio::fs::read_dir(&state.config.backup_dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(AppError::internal(e)),
    };
    let now = Utc::now().timestamp();
    let mut found = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(AppError::internal)? {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some((created_at, automatic)) = parse_name(&name) else {
            continue;
        };
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        found.push(Backup {
            url: state.backups.url(&name, now),
            name,
            bytes: meta.len(),
            created_at,
            automatic,
        });
    }
    found.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.name.cmp(&a.name)));
    Ok(found)
}

/// Makes a backup now. Refused while another one is being made.
pub async fn create(state: &AppState, automatic: bool) -> AppResult<Backup> {
    let Some(tool) = state.config.pg_dump.clone() else {
        return Err(AppError::Denied(
            "database backups are turned off on this server",
        ));
    };
    if !available(state).await {
        return Err(AppError::Denied(
            "this server can't make backups: the PostgreSQL client (pg_dump) is missing",
        ));
    }
    let Ok(_running) = state.backups.running.try_lock() else {
        return Err(AppError::Conflict {
            code: "backup_running",
            message: "a backup is being made already; wait for it to finish".into(),
        });
    };
    let dir = &state.config.backup_dir;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(AppError::internal)?;
    remove_partials(dir).await;

    let created_at = Utc::now();
    let name = file_name(created_at, automatic);
    let path = dir.join(&name);
    if tokio::fs::try_exists(&path).await.unwrap_or(true) {
        return Err(AppError::Conflict {
            code: "backup_running",
            message: "a backup was made this second; try again".into(),
        });
    }
    let partial = dir.join(format!("{name}{PARTIAL}"));
    let (url, password) = split_password(&state.config.database_url);
    let mut cmd = Command::new(&tool);
    cmd.args(["--format=custom", "--no-owner", "--no-privileges", "--file"])
        .arg(&partial)
        .arg("--dbname")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // In the environment, not the arguments, so other processes can't see it.
    if let Some(password) = password {
        cmd.env("PGPASSWORD", password);
    }
    let started = std::time::Instant::now();
    let failure = match tokio::time::timeout(DUMP_TIMEOUT, cmd.output()).await {
        Err(_) => Some("timed out".to_owned()),
        Ok(Err(e)) => Some(e.to_string()),
        Ok(Ok(out)) if !out.status.success() => {
            let text = String::from_utf8_lossy(&out.stderr);
            Some(text.trim().chars().take(MAX_ERROR_CHARS).collect())
        }
        Ok(Ok(_)) => None,
    };
    if let Some(reason) = failure {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(AppError::Internal(format!("pg_dump failed: {reason}")));
    }
    tokio::fs::rename(&partial, &path)
        .await
        .map_err(AppError::internal)?;
    let bytes = tokio::fs::metadata(&path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    info!(
        "database backup {name}: {bytes} bytes in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(Backup {
        url: state.backups.url(&name, created_at.timestamp()),
        name,
        bytes,
        created_at,
        automatic,
    })
}

/// Removes the backup `name`.
pub async fn remove(state: &AppState, name: &str) -> AppResult<()> {
    let path = path_for(state, name).ok_or(AppError::NotFound)?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::NotFound),
        Err(e) => Err(AppError::internal(e)),
    }
}

/// Removes automatic backups beyond the newest `keep`. Backups made by
/// hand stay. Returns how many were removed.
pub async fn prune(state: &AppState, keep: u32) -> AppResult<usize> {
    let keep = usize::try_from(keep).unwrap_or(usize::MAX);
    let mut removed = 0;
    for old in list(state)
        .await?
        .into_iter()
        .filter(|b| b.automatic)
        .skip(keep)
    {
        remove(state, &old.name).await?;
        removed += 1;
    }
    Ok(removed)
}

/// Left behind by a dump that was cut short.
async fn remove_partials(dir: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        if name
            .to_str()
            .and_then(|n| n.strip_suffix(PARTIAL))
            .is_some_and(|n| parse_name(n).is_some())
        {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

/// Takes the password out of a `postgres://` URL. Other connection strings
/// are passed on as they are.
fn split_password(database_url: &str) -> (String, Option<String>) {
    let Ok(mut url) = url::Url::parse(database_url) else {
        return (database_url.to_owned(), None);
    };
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return (database_url.to_owned(), None);
    }
    let password = url.password().map(|p| {
        percent_encoding::percent_decode_str(p)
            .decode_utf8_lossy()
            .into_owned()
    });
    if url.set_password(None).is_err() {
        return (database_url.to_owned(), None);
    }
    (url.to_string(), password)
}

/// Makes the scheduled backups. Checks hourly, starting a few minutes after
/// start-up.
pub async fn auto_backup(state: AppState) {
    let start = tokio::time::Instant::now() + Duration::from_secs(300);
    let mut tick = tokio::time::interval_at(start, Duration::from_secs(3600));
    loop {
        tick.tick().await;
        if let Err(e) = scheduled(&state).await {
            warn!("automatic database backup failed: {e}");
        }
    }
}

async fn scheduled(state: &AppState) -> AppResult<()> {
    let schedule = settings::backup_schedule(state).await?;
    if schedule.every_days == 0 || !available(state).await {
        return Ok(());
    }
    let latest = list(state)
        .await?
        .into_iter()
        .find(|b| b.automatic)
        .map(|b| b.created_at);
    if !is_due(latest, schedule.every_days, Utc::now()) {
        return Ok(());
    }
    create(state, true).await?;
    let removed = prune(state, schedule.keep).await?;
    if removed > 0 {
        info!("removed {removed} old automatic backups");
    }
    Ok(())
}

/// An automatic backup is due `every_days` after the last one. The hour of
/// slack keeps hourly checks from drifting it later each time.
fn is_due(latest: Option<DateTime<Utc>>, every_days: u32, now: DateTime<Utc>) -> bool {
    let Some(latest) = latest else {
        return true;
    };
    let gap = TimeDelta::days(i64::from(every_days)) - TimeDelta::hours(1);
    now.signed_duration_since(latest) >= gap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let at = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let manual = file_name(at, false);
        let auto = file_name(at, true);
        assert_eq!(manual, "starbooru-20270115-080000.dump");
        assert_eq!(auto, "starbooru-auto-20270115-080000.dump");
        assert_eq!(parse_name(&manual), Some((at, false)));
        assert_eq!(parse_name(&auto), Some((at, true)));
        for bad in [
            "starbooru-20270115-080000.dump.partial",
            "starbooru-2027115-80000.dump",
            "starbooru-20271315-080000.dump",
            "starbooru-20270115-080000",
            "other-20270115-080000.dump",
            "../starbooru-20270115-080000.dump",
            "starbooru-20270115-080000.dump/../x",
            "starbooru-auto-+20270115-080000.dump",
            "",
        ] {
            assert_eq!(parse_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn links_expire_and_bind_the_name() {
        let b = Backups::new(b"secret");
        let now = 1_800_000_000;
        let name = "starbooru-20270115-080000.dump";
        let url = b.url(name, now);
        let q = url.split_once('?').unwrap().1;
        let (exp, sig) = q.split_once('&').unwrap();
        let exp: i64 = exp.strip_prefix("exp=").unwrap().parse().unwrap();
        let sig = sig.strip_prefix("sig=").unwrap();
        assert!(url.starts_with(&format!("/backups/{name}?")));
        assert!(b.verify(name, exp, sig, now));
        assert!(b.verify(name, exp, sig, now + LINK_SECS));
        assert!(!b.verify(name, exp, sig, now + LINK_SECS + 1));
        assert!(!b.verify("starbooru-20270115-080001.dump", exp, sig, now));
        assert!(!b.verify(name, exp + 1, sig, now));
        assert!(!b.verify(name, exp, "zz", now));
        // A link can't be made to last longer.
        let far = now + 10 * LINK_SECS;
        let far_url = b.url(name, far - LINK_SECS);
        let far_sig = far_url.rsplit_once("sig=").unwrap().1;
        assert!(!b.verify(name, far, far_sig, now));
        assert!(!Backups::new(b"other").verify(name, exp, sig, now));
    }

    #[test]
    fn password_stays_out_of_the_arguments() {
        assert_eq!(
            split_password("postgres://booru:p%40ss%3Aw@db:5432/starbooru?sslmode=disable"),
            (
                "postgres://booru@db:5432/starbooru?sslmode=disable".to_owned(),
                Some("p@ss:w".to_owned())
            )
        );
        assert_eq!(
            split_password("postgresql://booru@db/starbooru"),
            ("postgresql://booru@db/starbooru".to_owned(), None)
        );
        assert_eq!(
            split_password("host=db user=booru"),
            ("host=db user=booru".to_owned(), None)
        );
    }

    #[test]
    fn schedule_falls_due() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        assert!(is_due(None, 1, now));
        assert!(!is_due(Some(now - TimeDelta::hours(22)), 1, now));
        assert!(is_due(Some(now - TimeDelta::minutes(23 * 60 + 10)), 1, now));
        assert!(!is_due(Some(now - TimeDelta::days(6)), 7, now));
        assert!(is_due(Some(now - TimeDelta::days(7)), 7, now));
    }
}
