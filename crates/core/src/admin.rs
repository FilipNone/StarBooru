//! Administration: server settings, statistics, accounts and storage
//! limits. Only admins call these endpoints.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::Role;

pub const MAX_INSTANCE_NAME_CHARS: usize = 64;
pub const MAX_TRASH_DAYS: u32 = 3650;
/// Days of uploads in [`ServerStats::uploads_by_day`].
pub const STATS_DAYS: u32 = 30;
pub const MAX_BACKUP_DAYS: u32 = 365;
pub const MAX_BACKUP_KEEP: u32 = 100;

/// `GET` and `PUT /admin/settings`. Each value starts from its environment
/// variable until an admin changes it here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerSettings {
    /// Shown at sign-in (`STARBOORU_NAME`).
    pub name: String,
    /// New accounts may sign up (`STARBOORU_ALLOW_REGISTRATION`).
    pub allow_registration: bool,
    /// Posts in the recycle bin are purged after this many days; 0 keeps
    /// them (`STARBOORU_TRASH_DAYS`).
    pub trash_days: u32,
    /// New posts get automatic metadata tags such as `highres` and `video`
    /// (`STARBOORU_AUTO_TAGS`). Missing from older clients, it means on.
    #[serde(default = "default_true")]
    pub auto_meta_tags: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsError {
    #[error("give the server a name")]
    EmptyName,
    #[error("the name is longer than {MAX_INSTANCE_NAME_CHARS} characters")]
    NameTooLong,
    #[error("the name contains a control character")]
    ControlChar,
    #[error("the recycle bin keeps posts for 0 to {MAX_TRASH_DAYS} days")]
    BadTrashDays,
    #[error("automatic backups run every 0 to {MAX_BACKUP_DAYS} days")]
    BadBackupDays,
    #[error("keep 1 to {MAX_BACKUP_KEEP} automatic backups")]
    BadBackupKeep,
}

/// Trims and checks settings before they are saved.
pub fn clean_settings(settings: &ServerSettings) -> Result<ServerSettings, SettingsError> {
    let name = settings
        .name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return Err(SettingsError::EmptyName);
    }
    if name.chars().count() > MAX_INSTANCE_NAME_CHARS {
        return Err(SettingsError::NameTooLong);
    }
    if name.chars().any(char::is_control) {
        return Err(SettingsError::ControlChar);
    }
    if settings.trash_days > MAX_TRASH_DAYS {
        return Err(SettingsError::BadTrashDays);
    }
    Ok(ServerSettings {
        name,
        allow_registration: settings.allow_registration,
        trash_days: settings.trash_days,
        auto_meta_tags: settings.auto_meta_tags,
    })
}

/// `GET /admin/stats`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerStats {
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub users: u64,
    pub disabled_users: u64,
    /// Live posts, by kind.
    pub images: u64,
    pub animations: u64,
    pub videos: u64,
    pub bin_posts: u64,
    pub tags: u64,
    /// Originals on disk, the recycle bin included (thumbnails not counted).
    pub media_bytes: u64,
    pub bin_bytes: u64,
    pub database_bytes: u64,
    /// The last [`STATS_DAYS`] days in UTC, oldest first, with every day present.
    pub uploads_by_day: Vec<DayCount>,
    pub max_upload_bytes: u64,
    /// ffmpeg is set up: thumbnails for video, AVIF and JPEG XL.
    pub video_thumbnails: bool,
    pub url_import: bool,
}

impl ServerStats {
    pub fn posts(&self) -> u64 {
        self.images
            .saturating_add(self.animations)
            .saturating_add(self.videos)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayCount {
    pub day: NaiveDate,
    pub posts: u64,
}

/// One account, as admins see it (`GET /admin/users`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminUser {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    /// Can't sign in; its sessions and API keys are refused.
    pub disabled: bool,
    /// Posts this account uploaded, the recycle bin included.
    pub posts: u64,
    /// Bytes of those posts' originals.
    pub bytes: u64,
    /// Most bytes the account may upload; `None` is unlimited.
    pub quota_bytes: Option<u64>,
    /// The latest use of any of its sessions or keys.
    pub last_active: Option<DateTime<Utc>>,
}

/// `PATCH /admin/users/{id}`: fields left out stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// `Some(None)` (JSON `null`) removes the limit.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub quota_bytes: Option<Option<u64>>,
}

/// Tells a `null` field (`Some(None)`) from a missing one (`None`).
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// `POST /admin/users/{id}/password`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PasswordReset {
    pub new: String,
}

/// `GET /auth/usage`: what the caller has uploaded, against their limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub posts: u64,
    pub bytes: u64,
    pub quota_bytes: Option<u64>,
}

impl Usage {
    /// Share of the quota in use, 0 to 1; `None` without a limit.
    pub fn fraction(&self) -> Option<f64> {
        let quota = self.quota_bytes?;
        if quota == 0 {
            return Some(1.0);
        }
        Some((self.bytes as f64 / quota as f64).clamp(0.0, 1.0))
    }
}

/// `GET /admin/backups`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backups {
    /// `pg_dump` is installed and turned on, so backups can be made.
    pub available: bool,
    /// A backup is being made now.
    pub running: bool,
    pub schedule: BackupSchedule,
    /// Newest first.
    pub items: Vec<Backup>,
}

/// `PUT /admin/backups/schedule`. Each value starts from its environment
/// variable until an admin changes it here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupSchedule {
    /// Days between automatic backups; 0 turns them off
    /// (`STARBOORU_BACKUP_DAYS`).
    pub every_days: u32,
    /// Automatic backups kept; older ones are removed
    /// (`STARBOORU_BACKUP_KEEP`). Backups made by hand are kept until removed.
    pub keep: u32,
}

impl BackupSchedule {
    pub fn check(&self) -> Result<(), SettingsError> {
        if self.every_days > MAX_BACKUP_DAYS {
            return Err(SettingsError::BadBackupDays);
        }
        if self.keep == 0 || self.keep > MAX_BACKUP_KEEP {
            return Err(SettingsError::BadBackupKeep);
        }
        Ok(())
    }
}

/// One database backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backup {
    pub name: String,
    pub bytes: u64,
    pub created_at: DateTime<Utc>,
    /// Made by the schedule rather than by hand.
    pub automatic: bool,
    /// A short-lived signed download link (relative to the server).
    pub url: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn settings(name: &str, trash_days: u32) -> ServerSettings {
        ServerSettings {
            name: name.into(),
            allow_registration: true,
            trash_days,
            auto_meta_tags: true,
        }
    }

    #[test]
    fn cleans_settings() {
        assert_eq!(
            clean_settings(&settings("  My   booru ", 30)),
            Ok(settings("My booru", 30))
        );
        assert!(clean_settings(&settings("x", MAX_TRASH_DAYS)).is_ok());
        assert_eq!(
            clean_settings(&settings(" ", 0)),
            Err(SettingsError::EmptyName)
        );
        assert_eq!(
            clean_settings(&settings(&"n".repeat(MAX_INSTANCE_NAME_CHARS + 1), 0)),
            Err(SettingsError::NameTooLong)
        );
        assert_eq!(
            clean_settings(&settings("a\u{0}b", 0)),
            Err(SettingsError::ControlChar)
        );
        assert_eq!(
            clean_settings(&settings("x", MAX_TRASH_DAYS + 1)),
            Err(SettingsError::BadTrashDays)
        );
    }

    #[test]
    fn user_update_tells_null_from_missing() {
        let parse = |v: serde_json::Value| serde_json::from_value::<UserUpdate>(v).unwrap();
        assert_eq!(parse(json!({})), UserUpdate::default());
        assert_eq!(
            parse(json!({ "quota_bytes": null })).quota_bytes,
            Some(None)
        );
        assert_eq!(
            parse(json!({ "quota_bytes": 5, "disabled": true })),
            UserUpdate {
                role: None,
                disabled: Some(true),
                quota_bytes: Some(Some(5)),
            }
        );
        assert_eq!(parse(json!({ "role": "admin" })).role, Some(Role::Admin));
        assert!(serde_json::from_value::<UserUpdate>(json!({ "quota_bytes": -1 })).is_err());
        let round = UserUpdate {
            quota_bytes: Some(None),
            ..UserUpdate::default()
        };
        assert_eq!(
            serde_json::to_value(&round).unwrap(),
            json!({ "quota_bytes": null })
        );
        assert_eq!(parse(serde_json::to_value(&round).unwrap()), round);
    }

    #[test]
    fn checks_backup_schedule() {
        let schedule = |every_days, keep| BackupSchedule { every_days, keep };
        assert_eq!(schedule(0, 1).check(), Ok(()));
        assert_eq!(schedule(MAX_BACKUP_DAYS, MAX_BACKUP_KEEP).check(), Ok(()));
        assert_eq!(
            schedule(MAX_BACKUP_DAYS + 1, 7).check(),
            Err(SettingsError::BadBackupDays)
        );
        assert_eq!(schedule(1, 0).check(), Err(SettingsError::BadBackupKeep));
        assert_eq!(
            schedule(1, MAX_BACKUP_KEEP + 1).check(),
            Err(SettingsError::BadBackupKeep)
        );
    }

    #[test]
    fn usage_fraction() {
        let usage = |bytes, quota_bytes| Usage {
            posts: 1,
            bytes,
            quota_bytes,
        };
        assert_eq!(usage(5, None).fraction(), None);
        assert_eq!(usage(5, Some(10)).fraction(), Some(0.5));
        assert_eq!(usage(50, Some(10)).fraction(), Some(1.0));
        assert_eq!(usage(0, Some(0)).fraction(), Some(1.0));
    }
}
