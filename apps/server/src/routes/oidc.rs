//! Signing in through an OpenID Connect provider, and the sign-ins linked
//! to an account. The flow is described in `starbooru_core::oidc`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use starbooru_core::oidc::{CODE_PARAM, ERROR_PARAM, username_from};
use starbooru_core::{
    AuthResponse, Identity, OidcBegin, OidcError, OidcFinish, OidcStart, Role, SignInMethods,
};
use tracing::warn;

use super::auth::{UserRow, start_session, to_user};
use crate::auth::{AuthUser, hash_token, random_hex};
use crate::error::{AppError, AppResult};
use crate::oidc::{Oidc, check_return_to, with_param};
use crate::settings;
use crate::state::AppState;

/// A sign-in must be finished within this many minutes of starting.
const LOGIN_MINUTES: i32 = 10;
/// Sign-ins in progress at once, so nobody can fill the table.
const MAX_PENDING: i64 = 1000;

fn provider(state: &AppState) -> AppResult<&Oidc> {
    state.oidc.as_ref().ok_or(AppError::NotFound)
}

/// `POST /auth/oidc/begin`.
pub async fn begin(
    State(state): State<AppState>,
    Json(req): Json<OidcBegin>,
) -> AppResult<Json<OidcStart>> {
    start(&state, &req, None).await.map(Json)
}

/// `POST /auth/oidc/begin-link`: the sign-in will be linked to the caller.
pub async fn begin_link(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<OidcBegin>,
) -> AppResult<Json<OidcStart>> {
    user.session_id()?;
    start(&state, &req, Some(user.id)).await.map(Json)
}

async fn start(state: &AppState, req: &OidcBegin, link: Option<i64>) -> AppResult<OidcStart> {
    let oidc = provider(state)?;
    let return_to = check_return_to(&req.return_to, oidc.public_url()).ok_or_else(|| {
        AppError::BadRequest(
            "return_to must be a page of this server's web app or a loopback address".into(),
        )
    })?;
    let meta = oidc.metadata().await?;
    sqlx::query("DELETE FROM oidc_logins WHERE created_at < now() - make_interval(mins => $1)")
        .bind(LOGIN_MINUTES)
        .execute(&state.db)
        .await?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM oidc_logins")
        .fetch_one(&state.db)
        .await?;
    if pending >= MAX_PENDING {
        return Err(AppError::TooManyRequests { retry_after: 60 });
    }
    let (sign_state, nonce, verifier, handle) =
        (random_hex(), random_hex(), random_hex(), random_hex());
    sqlx::query(
        "INSERT INTO oidc_logins (state_hash, handle_hash, nonce, verifier, return_to, \
         link_user_id) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(hash_token(&sign_state))
    .bind(hash_token(&handle))
    .bind(&nonce)
    .bind(&verifier)
    .bind(return_to.as_str())
    .bind(link)
    .execute(&state.db)
    .await?;
    Ok(OidcStart {
        url: oidc
            .authorize_url(&meta, &sign_state, &nonce, &verifier)
            .into(),
        handle,
    })
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

/// `GET /auth/oidc/callback`: the provider sends the browser here, and it
/// goes on to the `return_to` given at the start.
pub async fn callback(State(state): State<AppState>, Query(q): Query<CallbackQuery>) -> Response {
    match finish_callback(&state, q).await {
        Ok(location) => redirect(&location),
        Err(page) => *page,
    }
}

/// The address to send the browser on to, or the page to show instead.
async fn finish_callback(state: &AppState, q: CallbackQuery) -> Result<String, Box<Response>> {
    let oidc = provider(state).map_err(|e| Box::new(e.into_response()))?;
    let state_hash = hash_token(q.state.as_deref().unwrap_or_default());
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT return_to, nonce, verifier FROM oidc_logins \
         WHERE state_hash = $1 AND code_hash IS NULL \
           AND created_at >= now() - make_interval(mins => $2)",
    )
    .bind(&state_hash)
    .bind(LOGIN_MINUTES)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| Box::new(AppError::from(e).into_response()))?;
    let Some((return_to, nonce, verifier)) = row else {
        return Err(Box::new(expired_page(oidc)));
    };
    if let Some(error) = q.error.as_deref() {
        let error = if error == "access_denied" {
            OidcError::Denied
        } else {
            OidcError::Failed
        };
        return Ok(fail(state, &state_hash, &return_to, error).await);
    }
    let Some(code) = q.code.filter(|c| !c.is_empty()) else {
        return Ok(fail(state, &state_hash, &return_to, OidcError::Failed).await);
    };
    let claims = match oidc.redeem(&code, &verifier, &nonce).await {
        Ok(claims) => claims,
        Err(reason) => {
            warn!("OpenID Connect sign-in failed: {reason}");
            return Ok(fail(state, &state_hash, &return_to, OidcError::Failed).await);
        }
    };
    let result = random_hex();
    let done = sqlx::query(
        "UPDATE oidc_logins SET code_hash = $2, issuer = $3, subject = $4, label = $5 \
         WHERE state_hash = $1 AND code_hash IS NULL",
    )
    .bind(&state_hash)
    .bind(hash_token(&result))
    .bind(&claims.issuer)
    .bind(&claims.subject)
    .bind(&claims.label)
    .execute(&state.db)
    .await
    .map_err(|e| Box::new(AppError::from(e).into_response()))?;
    if done.rows_affected() == 0 {
        return Err(Box::new(expired_page(oidc)));
    }
    Ok(with_param(&return_to, CODE_PARAM, &result))
}

/// Ends the sign-in and sends the browser back with `error`.
async fn fail(state: &AppState, state_hash: &[u8], return_to: &str, error: OidcError) -> String {
    let _ = sqlx::query("DELETE FROM oidc_logins WHERE state_hash = $1")
        .bind(state_hash)
        .execute(&state.db)
        .await;
    with_param(return_to, ERROR_PARAM, error.as_str())
}

fn redirect(location: &str) -> Response {
    let mut resp = StatusCode::SEE_OTHER.into_response();
    let headers = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(location) {
        headers.insert(header::LOCATION, v);
    }
    no_store(headers);
    resp
}

fn no_store(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
}

/// Shown when the provider sends back a sign-in this server doesn't know
/// (expired, used, or never started here), since there is nowhere to send
/// the browser.
fn expired_page(oidc: &Oidc) -> Response {
    let home = escape_html(oidc.public_url().as_str());
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width\">\
         <title>StarBooru</title></head>\
         <body style=\"font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem\">\
         <h1>This sign-in has expired</h1>\
         <p>It took too long, or was already used. Go back to StarBooru and sign in again.</p>\
         <p><a href=\"{home}\">Open StarBooru</a></p></body></html>"
    );
    let mut resp = (StatusCode::BAD_REQUEST, body).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
    );
    no_store(headers);
    resp
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A finished sign-in, taken out of the table so it can't be used twice.
struct Finished {
    issuer: String,
    subject: String,
    label: Option<String>,
}

async fn take(state: &AppState, req: &OidcFinish, link: Option<i64>) -> AppResult<Finished> {
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "DELETE FROM oidc_logins \
         WHERE code_hash = $1 AND handle_hash = $2 AND link_user_id IS NOT DISTINCT FROM $3 \
           AND created_at >= now() - make_interval(mins => $4) \
         RETURNING issuer, subject, label",
    )
    .bind(hash_token(&req.code))
    .bind(hash_token(&req.handle))
    .bind(link)
    .bind(LOGIN_MINUTES)
    .fetch_optional(&state.db)
    .await?;
    let (issuer, subject, label) = row.ok_or_else(|| {
        AppError::BadRequest("this sign-in has expired or was already used; try again".into())
    })?;
    Ok(Finished {
        issuer,
        subject,
        label,
    })
}

/// `POST /auth/oidc/finish`: signs in to the account linked to the
/// provider sign-in, or makes one when sign-ups are open.
pub async fn finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OidcFinish>,
) -> AppResult<Json<AuthResponse>> {
    provider(&state)?;
    let found = take(&state, &req, None).await?;
    let linked: Option<(i64, String, String, DateTime<Utc>, bool)> = sqlx::query_as(
        "UPDATE user_identities i SET last_used_at = now() FROM users u \
         WHERE i.issuer = $1 AND i.subject = $2 AND u.id = i.user_id \
         RETURNING u.id, u.username, u.role, u.created_at, u.disabled_at IS NOT NULL",
    )
    .bind(&found.issuer)
    .bind(&found.subject)
    .fetch_optional(&state.db)
    .await?;
    let user = match linked {
        Some((_, _, _, _, true)) => {
            return Err(AppError::Denied(
                "this account is disabled; ask the server's admin",
            ));
        }
        Some((id, username, role, created_at, false)) => (id, username, role, created_at),
        None => sign_up(&state, &found).await?,
    };
    start_session(&state, to_user(user)?, &headers).await
}

/// Makes an account for a provider sign-in no account uses yet.
async fn sign_up(state: &AppState, found: &Finished) -> AppResult<UserRow> {
    let open = settings::current(state).await?.allow_registration;
    let mut tx = state.db.begin().await?;
    // The same lock as password sign-ups, so only one first user is admin.
    sqlx::query("SELECT pg_advisory_xact_lock(7275)")
        .execute(&mut *tx)
        .await?;
    let has_users: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(&mut *tx)
        .await?;
    if has_users && !open {
        return Err(AppError::Denied(
            "no account uses this sign-in yet, and sign-ups are closed. Sign in with your \
             password and link it in Settings, or ask the server's admin",
        ));
    }
    let role = if has_users { Role::User } else { Role::Admin };
    let base = found
        .label
        .as_deref()
        .and_then(username_from)
        .unwrap_or_else(|| "user".into());
    let mut username = None;
    for n in 1..=100u32 {
        let candidate = if n == 1 {
            base.clone()
        } else {
            let suffix = n.to_string();
            let keep = 32usize.saturating_sub(suffix.len());
            format!("{}{suffix}", base.chars().take(keep).collect::<String>())
        };
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users WHERE lower(username) = lower($1))",
        )
        .bind(&candidate)
        .fetch_one(&mut *tx)
        .await?;
        if !taken {
            username = Some(candidate);
            break;
        }
    }
    let username = username
        .ok_or_else(|| AppError::Internal("no free username for a new provider sign-in".into()))?;
    let row: UserRow = sqlx::query_as(
        "INSERT INTO users (username, password_hash, role) VALUES ($1, NULL, $2) \
         RETURNING id, username, role, created_at",
    )
    .bind(&username)
    .bind(role.as_str())
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO user_identities (user_id, issuer, subject, label, last_used_at) \
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(row.0)
    .bind(&found.issuer)
    .bind(&found.subject)
    .bind(&found.label)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

type IdentityRow = (i64, Option<String>, DateTime<Utc>, Option<DateTime<Utc>>);

fn to_identity((id, label, created_at, last_used_at): IdentityRow) -> Identity {
    Identity {
        id,
        label,
        created_at,
        last_used_at,
    }
}

/// `POST /auth/oidc/link`: links the provider sign-in to the caller's
/// account, which must be the one that began it.
pub async fn link(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<OidcFinish>,
) -> AppResult<Json<Identity>> {
    user.session_id()?;
    provider(&state)?;
    let found = take(&state, &req, Some(user.id)).await?;
    let row: Option<IdentityRow> = sqlx::query_as(
        "INSERT INTO user_identities (user_id, issuer, subject, label) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (issuer, subject) DO NOTHING \
         RETURNING id, label, created_at, last_used_at",
    )
    .bind(user.id)
    .bind(&found.issuer)
    .bind(&found.subject)
    .bind(&found.label)
    .fetch_optional(&state.db)
    .await?;
    row.map(|r| Json(to_identity(r)))
        .ok_or_else(|| AppError::Conflict {
            code: "identity_taken",
            message: "this sign-in is already linked to an account".into(),
        })
}

/// `GET /auth/identities`.
pub async fn identities(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<SignInMethods>> {
    user.session_id()?;
    let password: bool =
        sqlx::query_scalar("SELECT password_hash IS NOT NULL FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&state.db)
            .await?;
    let rows: Vec<IdentityRow> = sqlx::query_as(
        "SELECT id, label, created_at, last_used_at FROM user_identities \
         WHERE user_id = $1 ORDER BY id",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(SignInMethods {
        password,
        identities: rows.into_iter().map(to_identity).collect(),
    }))
}

/// `DELETE /auth/identities/{id}`: refused for the last way to sign in.
pub async fn unlink(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    user.session_id()?;
    let mut tx = state.db.begin().await?;
    let password: bool =
        sqlx::query_scalar("SELECT password_hash IS NOT NULL FROM users WHERE id = $1 FOR UPDATE")
            .bind(user.id)
            .fetch_one(&mut *tx)
            .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM user_identities WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    let done = sqlx::query("DELETE FROM user_identities WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if !password && count <= 1 {
        return Err(AppError::BadRequest(
            "set a password first, or you couldn't sign in any more".into(),
        ));
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::State;
    use axum::http::{HeaderMap, Method, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use base64::Engine;
    use serde_json::{Value, json};
    use url::Url;

    use crate::config::OidcConfig;
    use crate::oidc::{challenge, tests::id_token};
    use crate::testing::{self, TestApp};

    const RETURN_TO: &str = "http://localhost:8080/oidc";

    /// What the test provider hands out for each code: claims and the PKCE
    /// challenge the code was issued for.
    #[derive(Clone, Default)]
    struct Fake {
        issuer: String,
        codes: Arc<Mutex<HashMap<String, (Value, String)>>>,
    }

    async fn discovery(State(fake): State<Fake>) -> Response {
        axum::Json(json!({
            "issuer": fake.issuer,
            "authorization_endpoint": format!("{}authorize", fake.issuer),
            "token_endpoint": format!("{}token", fake.issuer),
        }))
        .into_response()
    }

    async fn token(State(fake): State<Fake>, headers: HeaderMap, body: String) -> Response {
        let basic = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode("booru:s3cret")
        );
        let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();
        let issued = form
            .get("code")
            .and_then(|c| fake.codes.lock().unwrap().remove(c));
        let ok = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            == Some(basic.as_str())
            && form.get("grant_type").map(String::as_str) == Some("authorization_code")
            && form.get("redirect_uri").map(String::as_str)
                == Some("http://localhost:8080/api/v1/auth/oidc/callback");
        match issued {
            Some((claims, expected))
                if ok
                    && form.get("code_verifier").map(|v| challenge(v)).as_ref()
                        == Some(&expected) =>
            {
                axum::Json(json!({
                    "access_token": "at",
                    "token_type": "Bearer",
                    "id_token": id_token(&claims),
                }))
                .into_response()
            }
            _ => (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({ "error": "invalid_grant" })),
            )
                .into_response(),
        }
    }

    async fn provider() -> Fake {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let fake = Fake {
            issuer: format!("http://127.0.0.1:{port}/"),
            ..Fake::default()
        };
        let router = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/token", post(token))
            .with_state(fake.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        fake
    }

    fn query(url: &str) -> HashMap<String, String> {
        Url::parse(url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }

    /// Begins a sign-in, approves it at the provider as `sub` (with
    /// `extra` claims), and follows the callback. Returns the handle and
    /// where the browser was sent.
    async fn approve(
        app: &TestApp,
        fake: &Fake,
        link_as: Option<&str>,
        sub: &str,
        extra: Value,
    ) -> (String, String) {
        let path = if link_as.is_some() {
            "/api/v1/auth/oidc/begin-link"
        } else {
            "/api/v1/auth/oidc/begin"
        };
        let r = app
            .call(
                Method::POST,
                path,
                link_as,
                Some(json!({ "return_to": RETURN_TO })),
            )
            .await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        let url = r.json()["url"].as_str().unwrap().to_owned();
        let handle = r.json()["handle"].as_str().unwrap().to_owned();
        assert!(url.starts_with(&format!("{}authorize?", fake.issuer)));
        let q = query(&url);
        assert_eq!(q["client_id"], "booru");
        assert_eq!(q["code_challenge_method"], "S256");
        let mut claims = json!({
            "iss": fake.issuer,
            "sub": sub,
            "aud": "booru",
            "exp": chrono::Utc::now().timestamp() + 300,
            "nonce": q["nonce"],
            "preferred_username": sub,
        });
        if let (Some(c), Some(e)) = (claims.as_object_mut(), extra.as_object()) {
            c.extend(e.clone());
        }
        let code = format!("code-{sub}-{}", q["state"]);
        fake.codes
            .lock()
            .unwrap()
            .insert(code.clone(), (claims, q["code_challenge"].clone()));
        let r = app
            .call(
                Method::GET,
                &format!(
                    "/api/v1/auth/oidc/callback?state={}&code={code}",
                    q["state"]
                ),
                None,
                None,
            )
            .await;
        assert_eq!(r.status, 303);
        let location = r.headers[header::LOCATION].to_str().unwrap().to_owned();
        (handle, location)
    }

    fn code_of(location: &str) -> String {
        assert!(location.starts_with(&format!("{RETURN_TO}?")), "{location}");
        query(location)
            .get("oidc_code")
            .cloned()
            .unwrap_or_else(|| panic!("no code: {location}"))
    }

    #[tokio::test]
    async fn sign_in_sign_up_and_link() {
        let fake = provider().await;
        let issuer = fake.issuer.clone();
        let Some(app) = testing::app_with(|c| {
            c.oidc = Some(OidcConfig {
                issuer: Url::parse(&issuer).unwrap(),
                client_id: "booru".into(),
                client_secret: "s3cret".into(),
                name: "Fake".into(),
                redirect_uri: Url::parse("http://localhost:8080/api/v1/auth/oidc/callback")
                    .unwrap(),
                public_url: Url::parse("http://localhost:8080/").unwrap(),
            });
        })
        .await
        else {
            return;
        };
        let finish = |code: String, handle: String| {
            let app = &app;
            async move {
                app.call(
                    Method::POST,
                    "/api/v1/auth/oidc/finish",
                    None,
                    Some(json!({ "code": code, "handle": handle })),
                )
                .await
            }
        };
        let info = app.call(Method::GET, "/api/v1/info", None, None).await;
        assert_eq!(info.json()["oidc"], "Fake");

        // Only the web app or a loopback address gets the code.
        for bad in ["https://evil.example/oidc", "http://example.com/", "nope"] {
            let r = app
                .call(
                    Method::POST,
                    "/api/v1/auth/oidc/begin",
                    None,
                    Some(json!({ "return_to": bad })),
                )
                .await;
            assert_eq!(r.status, 400, "{bad}");
        }

        // The first sign-in makes the admin account.
        let (handle, location) = approve(&app, &fake, None, "alice", json!({})).await;
        let code = code_of(&location);
        assert_eq!(finish(code.clone(), "wrong".into()).await.status, 400);
        let r = finish(code.clone(), handle.clone()).await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        assert_eq!(r.json()["user"]["username"], "alice");
        assert_eq!(r.json()["user"]["role"], "admin");
        let alice = r.json()["token"].as_str().unwrap().to_owned();
        let alice_id = r.json()["user"]["id"].as_i64().unwrap();
        // Used once only.
        assert_eq!(finish(code, handle).await.status, 400);

        // Again: the same account.
        let (handle, location) = approve(&app, &fake, None, "alice", json!({})).await;
        let r = finish(code_of(&location), handle).await;
        assert_eq!(r.json()["user"]["id"], alice_id);

        // A new account takes a free name; with sign-ups closed, none is made.
        app.call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(&alice),
            Some(json!({ "name": "StarBooru", "allow_registration": true, "trash_days": 0 })),
        )
        .await;
        let extra = json!({ "preferred_username": null, "email": "Alice@example.com" });
        let (handle, location) = approve(&app, &fake, None, "alice-2", extra).await;
        let r = finish(code_of(&location), handle).await;
        assert_eq!(r.json()["user"]["username"], "Alice2");
        assert_eq!(r.json()["user"]["role"], "user");
        let bob = app.register("bob").await;
        app.call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(&alice),
            Some(json!({ "name": "StarBooru", "allow_registration": false, "trash_days": 0 })),
        )
        .await;
        let (handle, location) = approve(&app, &fake, None, "carol", json!({})).await;
        let r = finish(code_of(&location), handle).await;
        assert_eq!(r.status, 403);

        // Provider sign-in accounts have no password until they set one.
        let r = app
            .call(
                Method::POST,
                "/api/v1/auth/login",
                None,
                Some(json!({ "username": "alice", "password": "" })),
            )
            .await;
        assert_eq!(r.status, 401);
        let r = app.get("/api/v1/auth/identities", &alice).await;
        assert_eq!(r.json()["password"], false);
        let ids = r.json()["identities"].as_array().unwrap().clone();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0]["label"], "alice");
        assert!(ids[0]["last_used_at"].is_string());
        let only = format!("/api/v1/auth/identities/{}", ids[0]["id"]);
        let r = app.call(Method::DELETE, &only, Some(&alice), None).await;
        assert_eq!(r.status, 400);

        // A password account links a provider sign-in; only it can finish
        // the link it began.
        let (handle, location) = approve(&app, &fake, Some(&bob), "bob-sso", json!({})).await;
        let code = code_of(&location);
        let link = |token: String, code: String, handle: String| {
            let app = &app;
            async move {
                app.call(
                    Method::POST,
                    "/api/v1/auth/oidc/link",
                    Some(&token),
                    Some(json!({ "code": code, "handle": handle })),
                )
                .await
            }
        };
        assert_eq!(finish(code.clone(), handle.clone()).await.status, 400);
        assert_eq!(
            link(alice.clone(), code.clone(), handle.clone())
                .await
                .status,
            400
        );
        let r = link(bob.clone(), code, handle).await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        assert_eq!(r.json()["label"], "bob-sso");
        let (handle, location) = approve(&app, &fake, None, "bob-sso", json!({})).await;
        let r = finish(code_of(&location), handle).await;
        assert_eq!(r.json()["user"]["username"], "bob");
        // A sign-in linked elsewhere can't be linked again.
        let (handle, location) = approve(&app, &fake, Some(&bob), "alice", json!({})).await;
        let r = link(bob.clone(), code_of(&location), handle).await;
        assert_eq!(r.status, 409);
        assert_eq!(r.json()["code"], "identity_taken");

        // Setting a password needs no current one, then unlinking works.
        let r = app
            .call(
                Method::PUT,
                "/api/v1/auth/password",
                Some(&alice),
                Some(json!({ "new": "a new password" })),
            )
            .await;
        assert_eq!(r.status, 204);
        let r = app.call(Method::DELETE, &only, Some(&bob), None).await;
        assert_eq!(r.status, 404);
        let r = app.call(Method::DELETE, &only, Some(&alice), None).await;
        assert_eq!(r.status, 204);
        let (handle, location) = approve(&app, &fake, None, "alice", json!({})).await;
        assert_eq!(finish(code_of(&location), handle).await.status, 403);

        // Failures come back as oidc_error.
        let (_, location) =
            approve(&app, &fake, None, "bob-sso", json!({ "nonce": "other" })).await;
        assert_eq!(location, format!("{RETURN_TO}?oidc_error=failed"));
        let r = app
            .call(
                Method::POST,
                "/api/v1/auth/oidc/begin",
                None,
                Some(json!({ "return_to": RETURN_TO })),
            )
            .await;
        let state = query(r.json()["url"].as_str().unwrap())["state"].clone();
        let callback = format!("/api/v1/auth/oidc/callback?state={state}&error=access_denied");
        let r = app.call(Method::GET, &callback, None, None).await;
        assert_eq!(
            r.headers[header::LOCATION],
            format!("{RETURN_TO}?oidc_error=denied").as_str()
        );
        // Unknown, used or expired sign-ins get a page, not a redirect.
        let r = app.call(Method::GET, &callback, None, None).await;
        assert_eq!(r.status, 400);
        assert!(String::from_utf8_lossy(&r.body).contains("expired"));
        let r = app
            .call(
                Method::POST,
                "/api/v1/auth/oidc/begin",
                None,
                Some(json!({ "return_to": RETURN_TO })),
            )
            .await;
        let state = query(r.json()["url"].as_str().unwrap())["state"].clone();
        sqlx::query("UPDATE oidc_logins SET created_at = now() - interval '11 minutes'")
            .execute(app.db())
            .await
            .unwrap();
        let r = app
            .call(
                Method::GET,
                &format!("/api/v1/auth/oidc/callback?state={state}&code=x"),
                None,
                None,
            )
            .await;
        assert_eq!(r.status, 400);

        // Disabled accounts can't sign in this way either.
        let bob_id = app.get("/api/v1/auth/me", &bob).await.json()["id"].clone();
        let r = app
            .call(
                Method::PATCH,
                &format!("/api/v1/admin/users/{bob_id}"),
                Some(&alice),
                Some(json!({ "disabled": true })),
            )
            .await;
        assert_eq!(r.status, 200);
        let (handle, location) = approve(&app, &fake, None, "bob-sso", json!({})).await;
        assert_eq!(finish(code_of(&location), handle).await.status, 403);

        app.finish().await;
    }

    #[tokio::test]
    async fn off_without_a_provider() {
        let Some(app) = testing::app().await else {
            return;
        };
        let r = app
            .call(
                Method::POST,
                "/api/v1/auth/oidc/begin",
                None,
                Some(json!({ "return_to": RETURN_TO })),
            )
            .await;
        assert_eq!(r.status, 404);
        let info = app.call(Method::GET, "/api/v1/info", None, None).await;
        assert!(info.json()["oidc"].is_null());
        app.finish().await;
    }
}
