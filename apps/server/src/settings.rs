//! Server settings an admin changes in the app. Each follows its
//! environment variable until it is changed, and again once it is set back
//! to the same value.

use starbooru_core::{
    AiGeneralUpdate, AiMode, AiPreset, AiTool, AiToolUpdate, BackupSchedule, ServerSettings,
};

use crate::ai::{AiSetup, Provider, ToolSetup};
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

/// The AI setup in force: what the admin saved over `STARBOORU_AI_*`.
pub async fn ai(state: &AppState) -> AppResult<AiSetup> {
    #[derive(sqlx::FromRow)]
    struct Row {
        tool: String,
        mode: String,
        preset: String,
        model: String,
        api_url: String,
        api_key: String,
        api_model: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT tool, mode, preset, model, api_url, api_key, api_model FROM ai_tools",
    )
    .fetch_all(&state.db)
    .await?;
    let general: Option<(Option<bool>, Option<String>)> =
        sqlx::query_as("SELECT ai_autotag, ai_auto_translate FROM server_settings")
            .fetch_optional(&state.db)
            .await?;
    let (autotag, auto_translate) = general.unwrap_or_default();
    let env = &state.config.ai;
    let tools = AiTool::ALL
        .into_iter()
        .map(|tool| {
            let env_mode = match tool {
                AiTool::Tagging => env.tagging,
                AiTool::Translation => env.translation,
                AiTool::Comics => env.comics,
            };
            let default_mode = env_mode.unwrap_or(if !env.url.is_empty() {
                AiMode::Api
            } else if env.local_default {
                AiMode::Local
            } else {
                AiMode::Off
            });
            let Some(row) = rows.iter().find(|r| r.tool == tool.as_str()) else {
                return ToolSetup {
                    tool,
                    mode: default_mode,
                    preset: AiPreset::default(),
                    model: String::new(),
                    api: Provider::new(env.url.clone(), env.key.clone(), env.model.clone()),
                };
            };
            let follows_env = row.api_url.is_empty() || row.api_url == env.url;
            ToolSetup {
                tool,
                mode: AiMode::parse(&row.mode).unwrap_or(default_mode),
                preset: AiPreset::parse(&row.preset).unwrap_or_default(),
                model: row.model.clone(),
                api: Provider::new(
                    if row.api_url.is_empty() {
                        env.url.clone()
                    } else {
                        row.api_url.clone()
                    },
                    // The environment's key never goes to another address.
                    if row.api_key.is_empty() && follows_env {
                        env.key.clone()
                    } else {
                        row.api_key.clone()
                    },
                    if row.api_model.is_empty() {
                        env.model.clone()
                    } else {
                        row.api_model.clone()
                    },
                ),
            }
        })
        .collect();
    Ok(AiSetup {
        tools,
        autotag: autotag.unwrap_or(env.autotag),
        auto_translate: auto_translate.unwrap_or_else(|| env.translate_to.clone()),
    })
}

/// Saves one tool's setup (already cleaned). `api_key`: `None` keeps the
/// stored one.
pub async fn save_ai_tool(state: &AppState, tool: AiTool, update: &AiToolUpdate) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO ai_tools (tool, mode, preset, model, api_url, api_model, api_key) \
         VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, '')) \
         ON CONFLICT (tool) DO UPDATE SET mode = $2, preset = $3, model = $4, api_url = $5, \
         api_model = $6, api_key = COALESCE($7, ai_tools.api_key)",
    )
    .bind(tool.as_str())
    .bind(update.mode.as_str())
    .bind(update.preset.as_str())
    .bind(&update.model)
    .bind(&update.api_url)
    .bind(&update.api_model)
    .bind(&update.api_key)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Saves the settings all AI tools share (already cleaned), like [`save`].
pub async fn save_ai_general(state: &AppState, update: &AiGeneralUpdate) -> AppResult<()> {
    let env = &state.config.ai;
    sqlx::query(
        "INSERT INTO server_settings (id, ai_autotag, ai_auto_translate) VALUES (true, $1, $2) \
         ON CONFLICT (id) DO UPDATE SET ai_autotag = $1, ai_auto_translate = $2, \
         updated_at = now()",
    )
    .bind((update.autotag != env.autotag).then_some(update.autotag))
    .bind((update.auto_translate != env.translate_to).then_some(&update.auto_translate))
    .execute(&state.db)
    .await?;
    Ok(())
}
