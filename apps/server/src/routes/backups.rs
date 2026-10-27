//! Database backups in the Administration page. Archives hold every
//! account's password hash, so all of this needs an admin signed in with a
//! session; API keys are refused.

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use starbooru_core::{Backup, BackupSchedule, Backups};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use super::admin::require_admin_session;
use crate::auth::AuthUser;
use crate::backup;
use crate::error::{AppError, AppResult};
use crate::settings;
use crate::state::AppState;

/// `GET /admin/backups`.
pub async fn list(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Backups>> {
    require_admin_session(&user)?;
    Ok(Json(Backups {
        available: backup::available(&state).await,
        running: state.backups.is_running(),
        schedule: settings::backup_schedule(&state).await?,
        items: backup::list(&state).await?,
    }))
}

/// `POST /admin/backups`: answers once the backup is written.
pub async fn create(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Backup>> {
    require_admin_session(&user)?;
    backup::create(&state, false).await.map(Json)
}

/// `PUT /admin/backups/schedule`: automatic backups beyond the new limit
/// are removed at once.
pub async fn set_schedule(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<BackupSchedule>,
) -> AppResult<Json<BackupSchedule>> {
    require_admin_session(&user)?;
    req.check()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    settings::save_backup_schedule(&state, req).await?;
    let schedule = settings::backup_schedule(&state).await?;
    backup::prune(&state, schedule.keep).await?;
    Ok(Json(schedule))
}

/// `DELETE /admin/backups/{name}`.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    require_admin_session(&user)?;
    backup::remove(&state, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SignedQuery {
    exp: i64,
    sig: String,
}

/// `GET /backups/{name}?exp=..&sig=..`: the link from the list.
pub async fn download(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<SignedQuery>,
    req: Request,
) -> AppResult<Response> {
    let path = backup::path_for(&state, &name).ok_or(AppError::NotFound)?;
    let now = chrono::Utc::now().timestamp();
    if !state.backups.verify(&name, q.exp, &q.sig, now) {
        return Err(AppError::Forbidden);
    }
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err(AppError::NotFound);
    }
    let resp = ServeFile::new(path)
        .oneshot(req)
        .await
        .map_err(AppError::internal)?;
    let mut resp = resp.map(Body::new).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    // The name is checked to be plain ASCII by `path_for`.
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use axum::http::Method;
    use serde_json::json;

    use crate::testing;

    fn has_pg_dump() -> bool {
        std::process::Command::new("pg_dump")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[tokio::test]
    async fn backups_off_without_pg_dump() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let r = app.get("/api/v1/admin/backups", &admin).await;
        assert_eq!(r.status, 200);
        assert_eq!(r.json()["available"], false);
        assert_eq!(r.json()["items"], json!([]));
        let r = app
            .call(Method::POST, "/api/v1/admin/backups", Some(&admin), None)
            .await;
        assert_eq!(r.status, 403);
        app.finish().await;
    }

    #[tokio::test]
    async fn admins_back_up_download_and_prune() {
        if !has_pg_dump() {
            eprintln!("skipped: pg_dump is not installed");
            return;
        }
        let Some(app) = testing::app_with(|c| c.pg_dump = Some("pg_dump".into())).await else {
            return;
        };
        let admin = app.register("admin").await;
        let user = app.register("someone").await;
        app.upload_png(&admin, 1, "sky").await;

        // Admins with a session only.
        let r = app.get("/api/v1/admin/backups", &user).await;
        assert_eq!(r.status, 403);
        let r = app
            .call(
                Method::POST,
                "/api/v1/keys",
                Some(&admin),
                Some(json!({ "name": "script" })),
            )
            .await;
        let key = r.json()["token"].as_str().unwrap().to_owned();
        for (method, path) in [
            (Method::GET, "/api/v1/admin/backups"),
            (Method::POST, "/api/v1/admin/backups"),
        ] {
            let r = app.call(method, path, Some(&key), None).await;
            assert_eq!(r.status, 403, "{path}");
        }

        let r = app.get("/api/v1/admin/backups", &admin).await;
        assert_eq!(r.json()["available"], true);
        assert_eq!(r.json()["running"], false);
        assert_eq!(r.json()["schedule"], json!({ "every_days": 0, "keep": 7 }));

        // A backup is a pg_dump archive, downloaded with its signed link.
        let r = app
            .call(Method::POST, "/api/v1/admin/backups", Some(&admin), None)
            .await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        let made = r.json();
        let name = made["name"].as_str().unwrap().to_owned();
        assert!(name.starts_with("starbooru-2"));
        assert_eq!(made["automatic"], false);
        assert!(made["bytes"].as_u64().unwrap() > 0);
        let url = made["url"].as_str().unwrap().to_owned();
        let r = app.call(Method::GET, &url, None, None).await;
        assert_eq!(r.status, 200);
        assert!(r.body.starts_with(b"PGDMP"));
        assert_eq!(r.body.len() as u64, made["bytes"].as_u64().unwrap());
        assert_eq!(
            r.headers[axum::http::header::CONTENT_DISPOSITION],
            format!("attachment; filename=\"{name}\"").as_str()
        );
        let tampered = url.replace("sig=", "sig=00");
        assert_eq!(
            app.call(Method::GET, &tampered, None, None).await.status,
            403
        );
        let (_, query) = url.split_once('?').unwrap();
        let other = format!("/backups/starbooru-20000101-000000.dump?{query}");
        assert_eq!(app.call(Method::GET, &other, None, None).await.status, 403);
        let bad = format!("/backups/..%2Fsecret.dump?{query}");
        assert_eq!(app.call(Method::GET, &bad, None, None).await.status, 404);

        // Leftovers and stray files aren't listed; automatic backups beyond
        // the limit go, ones made by hand stay.
        let dir = app.state.config.backup_dir.clone();
        for file in [
            "starbooru-auto-20260101-000000.dump",
            "starbooru-auto-20260102-000000.dump",
            "starbooru-auto-20260103-000000.dump",
            "starbooru-auto-20260104-000000.dump.partial",
            "notes.txt",
        ] {
            std::fs::write(dir.join(file), b"x").unwrap();
        }
        let r = app.get("/api/v1/admin/backups", &admin).await;
        let names: Vec<String> = r.json()["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            names,
            [
                name.as_str(),
                "starbooru-auto-20260103-000000.dump",
                "starbooru-auto-20260102-000000.dump",
                "starbooru-auto-20260101-000000.dump",
            ]
        );
        let schedule = |body: serde_json::Value| {
            let app = &app;
            let admin = admin.clone();
            async move {
                app.call(
                    Method::PUT,
                    "/api/v1/admin/backups/schedule",
                    Some(&admin),
                    Some(body),
                )
                .await
            }
        };
        assert_eq!(
            schedule(json!({ "every_days": 1, "keep": 0 })).await.status,
            400
        );
        assert_eq!(
            schedule(json!({ "every_days": 400, "keep": 3 }))
                .await
                .status,
            400
        );
        let r = schedule(json!({ "every_days": 2, "keep": 1 })).await;
        assert_eq!(r.status, 200);
        assert_eq!(r.json(), json!({ "every_days": 2, "keep": 1 }));
        let r = app.get("/api/v1/admin/backups", &admin).await;
        assert_eq!(r.json()["schedule"], json!({ "every_days": 2, "keep": 1 }));
        let items = r.json()["items"].as_array().unwrap().clone();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1]["name"], "starbooru-auto-20260103-000000.dump");
        assert!(dir.join("notes.txt").exists());

        // Removing.
        let path = format!("/api/v1/admin/backups/{name}");
        let r = app.call(Method::DELETE, &path, Some(&user), None).await;
        assert_eq!(r.status, 403);
        let r = app.call(Method::DELETE, &path, Some(&admin), None).await;
        assert_eq!(r.status, 204);
        let r = app.call(Method::DELETE, &path, Some(&admin), None).await;
        assert_eq!(r.status, 404);
        let r = app
            .call(
                Method::DELETE,
                "/api/v1/admin/backups/notes.txt",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, 404);
        assert_eq!(app.call(Method::GET, &url, None, None).await.status, 404);

        app.finish().await;
    }
}
