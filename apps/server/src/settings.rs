//! Server settings an admin changes in the app. Each follows its
//! environment variable until it is changed, and again once it is set back
//! to the same value.

use starbooru_core::ServerSettings;

use crate::error::AppResult;
use crate::state::AppState;

/// The settings in force.
pub async fn current(state: &AppState) -> AppResult<ServerSettings> {
    let row: Option<(Option<String>, Option<bool>, Option<i32>)> =
        sqlx::query_as("SELECT name, allow_registration, trash_days FROM server_settings")
            .fetch_optional(&state.db)
            .await?;
    let (name, allow_registration, trash_days) = row.unwrap_or_default();
    let config = &state.config;
    Ok(ServerSettings {
        name: name.unwrap_or_else(|| config.instance_name.clone()),
        allow_registration: allow_registration.unwrap_or(config.allow_registration),
        trash_days: trash_days
            .and_then(|d| u32::try_from(d).ok())
            .unwrap_or(config.trash_days),
    })
}

/// Saves `settings` (already cleaned). Values equal to the environment's are
/// stored as unset, so a later change to the variable still applies.
pub async fn save(state: &AppState, settings: &ServerSettings) -> AppResult<()> {
    let config = &state.config;
    let trash_days = i32::try_from(settings.trash_days).unwrap_or(i32::MAX);
    sqlx::query(
        "INSERT INTO server_settings (id, name, allow_registration, trash_days) \
         VALUES (true, $1, $2, $3) \
         ON CONFLICT (id) DO UPDATE SET name = $1, allow_registration = $2, \
         trash_days = $3, updated_at = now()",
    )
    .bind((settings.name != config.instance_name).then_some(&settings.name))
    .bind(
        (settings.allow_registration != config.allow_registration)
            .then_some(settings.allow_registration),
    )
    .bind((settings.trash_days != config.trash_days).then_some(trash_days))
    .execute(&state.db)
    .await?;
    Ok(())
}
