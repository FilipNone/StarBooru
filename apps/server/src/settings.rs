//! Server settings an admin changes in the app. Each follows its
//! environment variable until it is changed, and again once it is set back
//! to the same value.

use starbooru_core::{BackupSchedule, ServerSettings};

use crate::error::AppResult;
use crate::state::AppState;

/// name, allow_registration, trash_days, auto_meta_tags; NULL follows the
/// environment.
type SettingsRow = (Option<String>, Option<bool>, Option<i32>, Option<bool>);

/// The settings in force.
pub async fn current(state: &AppState) -> AppResult<ServerSettings> {
    let row: Option<SettingsRow> = sqlx::query_as(
        "SELECT name, allow_registration, trash_days, auto_meta_tags FROM server_settings",
    )
    .fetch_optional(&state.db)
    .await?;
    let (name, allow_registration, trash_days, auto_meta_tags) = row.unwrap_or_default();
    let config = &state.config;
    Ok(ServerSettings {
        name: name.unwrap_or_else(|| config.instance_name.clone()),
        allow_registration: allow_registration.unwrap_or(config.allow_registration),
        trash_days: trash_days
            .and_then(|d| u32::try_from(d).ok())
            .unwrap_or(config.trash_days),
        auto_meta_tags: auto_meta_tags.unwrap_or(config.auto_meta_tags),
    })
}

/// Saves `settings` (already cleaned). Values equal to the environment's are
/// stored as unset, so a later change to the variable still applies.
pub async fn save(state: &AppState, settings: &ServerSettings) -> AppResult<()> {
    let config = &state.config;
    let trash_days = i32::try_from(settings.trash_days).unwrap_or(i32::MAX);
    sqlx::query(
        "INSERT INTO server_settings (id, name, allow_registration, trash_days, auto_meta_tags) \
         VALUES (true, $1, $2, $3, $4) \
         ON CONFLICT (id) DO UPDATE SET name = $1, allow_registration = $2, \
         trash_days = $3, auto_meta_tags = $4, updated_at = now()",
    )
    .bind((settings.name != config.instance_name).then_some(&settings.name))
    .bind(
        (settings.allow_registration != config.allow_registration)
            .then_some(settings.allow_registration),
    )
    .bind((settings.trash_days != config.trash_days).then_some(trash_days))
    .bind((settings.auto_meta_tags != config.auto_meta_tags).then_some(settings.auto_meta_tags))
    .execute(&state.db)
    .await?;
    Ok(())
}

/// The automatic backup schedule in force.
pub async fn backup_schedule(state: &AppState) -> AppResult<BackupSchedule> {
    let row: Option<(Option<i32>, Option<i32>)> =
        sqlx::query_as("SELECT backup_days, backup_keep FROM server_settings")
            .fetch_optional(&state.db)
            .await?;
    let (days, keep) = row.unwrap_or_default();
    let config = &state.config;
    Ok(BackupSchedule {
        every_days: days
            .and_then(|d| u32::try_from(d).ok())
            .unwrap_or(config.backup_days),
        keep: keep
            .and_then(|k| u32::try_from(k).ok())
            .unwrap_or(config.backup_keep),
    })
}

/// Saves `schedule` (already checked), like [`save`].
pub async fn save_backup_schedule(state: &AppState, schedule: BackupSchedule) -> AppResult<()> {
    let config = &state.config;
    let days = i32::try_from(schedule.every_days).unwrap_or(i32::MAX);
    let keep = i32::try_from(schedule.keep).unwrap_or(i32::MAX);
    sqlx::query(
        "INSERT INTO server_settings (id, backup_days, backup_keep) VALUES (true, $1, $2) \
         ON CONFLICT (id) DO UPDATE SET backup_days = $1, backup_keep = $2, updated_at = now()",
    )
    .bind((schedule.every_days != config.backup_days).then_some(days))
    .bind((schedule.keep != config.backup_keep).then_some(keep))
    .execute(&state.db)
    .await?;
    Ok(())
}
