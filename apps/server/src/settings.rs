//! Server settings an admin changes in the app. Each follows its
//! environment variable until it is changed, and again once it is set back
//! to the same value.

use starbooru_core::{AiRoute, AiSettingsUpdate, BackupSchedule, ServerSettings};

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

/// The AI setup in force: admin overrides over `STARBOORU_AI_*` and
/// `STARBOORU_LOCAL_AI_*`.
pub async fn ai(state: &AppState) -> AppResult<crate::ai::AiSetup> {
    #[derive(sqlx::FromRow, Default)]
    struct Row {
        ai_url: Option<String>,
        ai_key: Option<String>,
        ai_model: Option<String>,
        ai_autotag: Option<bool>,
        ai_local_enabled: Option<bool>,
        ai_local_url: Option<String>,
        ai_local_model: Option<String>,
        ai_tagging: Option<String>,
        ai_translation: Option<String>,
        ai_auto_translate: Option<String>,
    }
    let row: Option<Row> = sqlx::query_as(
        "SELECT ai_url, ai_key, ai_model, ai_autotag, ai_local_enabled, ai_local_url, \
         ai_local_model, ai_tagging, ai_translation, ai_auto_translate FROM server_settings",
    )
    .fetch_optional(&state.db)
    .await?;
    let row = row.unwrap_or_default();
    let env = &state.config.ai;
    let mut setup = crate::ai::AiSetup {
        hosted: crate::ai::Provider::hosted(
            row.ai_url.unwrap_or_else(|| env.url.clone()),
            row.ai_key.unwrap_or_else(|| env.key.clone()),
            row.ai_model.unwrap_or_else(|| env.model.clone()),
        ),
        autotag: row.ai_autotag.unwrap_or(env.autotag),
        local_enabled: row.ai_local_enabled.unwrap_or(env.local_enabled),
        local_url: row.ai_local_url.unwrap_or_else(|| env.local_url.clone()),
        local_model: row
            .ai_local_model
            .unwrap_or_else(|| env.local_model.clone()),
        tagging: AiRoute::Off,
        translation: AiRoute::Off,
        auto_translate: row
            .ai_auto_translate
            .unwrap_or_else(|| env.translate_to.clone()),
    };
    // Defaults follow what is set up after the overrides.
    let default = if !setup.hosted.url.is_empty() {
        AiRoute::Hosted
    } else if setup.local_enabled {
        AiRoute::Local
    } else {
        AiRoute::Off
    };
    let pick = |stored: Option<String>, env: Option<AiRoute>| {
        stored
            .as_deref()
            .and_then(AiRoute::parse)
            .or(env)
            .unwrap_or(default)
    };
    setup.tagging = pick(row.ai_tagging, env.tagging);
    setup.translation = pick(row.ai_translation, env.translation);
    Ok(setup)
}

/// Saves the AI settings (already cleaned). `key`: `None` keeps the stored
/// one. Values equal to the environment's are stored as unset, like [`save`].
pub async fn save_ai(state: &AppState, update: &AiSettingsUpdate) -> AppResult<()> {
    let env = &state.config.ai;
    let current = ai(state).await?;
    let key = update
        .key
        .as_deref()
        .map_or(current.hosted.key.as_str(), str::trim);
    let local = &update.local;
    sqlx::query(
        "INSERT INTO server_settings (id, ai_url, ai_model, ai_key, ai_autotag, \
         ai_local_enabled, ai_local_url, ai_local_model, ai_tagging, ai_translation, \
         ai_auto_translate) \
         VALUES (true, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT (id) DO UPDATE SET ai_url = $1, ai_model = $2, ai_key = $3, \
         ai_autotag = $4, ai_local_enabled = $5, ai_local_url = $6, ai_local_model = $7, \
         ai_tagging = $8, ai_translation = $9, ai_auto_translate = $10, updated_at = now()",
    )
    .bind((update.url != env.url).then_some(&update.url))
    .bind((update.model != env.model).then_some(&update.model))
    .bind((key != env.key).then_some(key))
    .bind((update.autotag != env.autotag).then_some(update.autotag))
    .bind((local.enabled != env.local_enabled).then_some(local.enabled))
    .bind((local.url != env.local_url).then_some(&local.url))
    .bind((local.model != env.local_model).then_some(&local.model))
    // Routes are always stored: their default depends on the other settings.
    .bind(update.tagging.as_str())
    .bind(update.translation.as_str())
    .bind((update.auto_translate != env.translate_to).then_some(&update.auto_translate))
    .execute(&state.db)
    .await?;
    Ok(())
}
