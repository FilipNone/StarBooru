//! Integration-test harness: a fresh database per test, the real router, and
//! requests sent with `tower::ServiceExt::oneshot`.
//!
//! Set `STARBOORU_TEST_DATABASE_URL` to a Postgres URL whose user may create
//! databases (the dev container works). Without it, database tests are skipped.

use std::io::Cursor;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Connection, Executor, PgConnection};
use tower::ServiceExt;

use crate::config::Config;
use crate::state::AppState;
use crate::storage::Storage;

/// The password `register` gives every account.
pub const PASSWORD: &str = "password123";

pub struct TestApp {
    pub state: AppState,
    pub router: Router,
    admin_url: String,
    db_name: String,
    _dir: tempfile::TempDir,
}

pub struct Resp {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl Resp {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

/// Returns `None` (and the test returns early) when no test database is configured.
pub async fn app() -> Option<TestApp> {
    app_with(|_| {}).await
}

pub async fn app_with(tweak: impl FnOnce(&mut Config)) -> Option<TestApp> {
    let Ok(admin_url) = std::env::var("STARBOORU_TEST_DATABASE_URL") else {
        eprintln!("skipped: STARBOORU_TEST_DATABASE_URL is not set");
        return None;
    };
    let mut nonce = [0u8; 6];
    rand::fill(&mut nonce);
    let db_name = format!("sbtest_{}", hex::encode(nonce));
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    admin
        .execute(sqlx::AssertSqlSafe(format!("CREATE DATABASE {db_name}")))
        .await
        .unwrap();
    admin.close().await.unwrap();

    let url = with_database(&admin_url, &db_name);
    let db: PgPool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();

    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::for_tests(dir.path());
    config.database_url = url.clone();
    tweak(&mut config);
    let storage = Storage::open(&config.data_dir)
        .await
        .unwrap()
        .with_tools(config.tools.clone());
    let state = AppState::new(config, db, storage);
    let router = crate::routes::router(state.clone());
    Some(TestApp {
        state,
        router,
        admin_url,
        db_name,
        _dir: dir,
    })
}

/// Replaces the database name (the URL path) of a Postgres URL.
fn with_database(url: &str, name: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (url, None),
    };
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let host_end = base
        .get(scheme_end..)
        .and_then(|rest| rest.find('/'))
        .map_or(base.len(), |i| scheme_end + i);
    let mut out = format!("{}/{name}", base.get(..host_end).unwrap_or(base));
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    out
}

impl TestApp {
    pub async fn send(&self, req: Request<Body>) -> Resp {
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        Resp {
            status,
            headers,
            body,
        }
    }

    pub async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Resp {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("x-forwarded-for", "203.0.113.7");
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        };
        self.send(req.unwrap()).await
    }

    pub async fn get(&self, path: &str, token: &str) -> Resp {
        self.call(Method::GET, path, Some(token), None).await
    }

    /// Registers an account and returns its token.
    pub async fn register(&self, username: &str) -> String {
        let r = self
            .call(
                Method::POST,
                "/api/v1/auth/register",
                None,
                Some(serde_json::json!({"username": username, "password": PASSWORD})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "register: {:?}", r.json());
        r.json()["token"].as_str().unwrap().to_owned()
    }

    /// Uploads `bytes` as a multipart form and returns the response.
    pub async fn upload(&self, token: &str, bytes: &[u8], fields: &[(&str, &str)]) -> Resp {
        let boundary = "starbooru-test-boundary";
        let mut body = Vec::new();
        for (name, value) in fields {
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
                )
                .as_bytes(),
            );
        }
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"f\"\r\n\
                 Content-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/posts")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap();
        self.send(req).await
    }

    /// Uploads a generated PNG and returns the new post's id.
    pub async fn upload_png(&self, token: &str, seed: u32, tags: &str) -> i64 {
        let r = self
            .upload(token, &png(8 + seed % 64, 8, seed), &[("tags", tags)])
            .await;
        assert_eq!(r.status, StatusCode::OK, "upload: {:?}", r.json());
        r.json()["id"].as_i64().unwrap()
    }

    pub fn db(&self) -> &PgPool {
        &self.state.db
    }

    /// Drops the test database. Call at the end of each test.
    pub fn db_name(&self) -> &str {
        &self.db_name
    }

    pub async fn finish(self) {
        let TestApp {
            state,
            router,
            admin_url,
            db_name,
            _dir,
        } = self;
        drop(router);
        state.db.close().await;
        drop(state);
        if let Ok(mut admin) = PgConnection::connect(&admin_url).await {
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "DROP DATABASE IF EXISTS {db_name} WITH (FORCE)"
                )))
                .await;
        }
    }
}

/// A small PNG whose pixels depend on `seed`, so each seed is a distinct file.
pub fn png(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        let v = seed
            .wrapping_mul(2_654_435_761)
            .wrapping_add(x * 31 + y * 17);
        image::Rgb([
            (v & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (seed & 0xff) as u8,
        ])
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[cfg(test)]
mod tests {
    use super::with_database;

    #[test]
    fn swaps_database_name() {
        assert_eq!(
            with_database("postgres://u:p@h:5432/starbooru", "x"),
            "postgres://u:p@h:5432/x"
        );
        assert_eq!(
            with_database("postgres://u:p@h:5432/db?sslmode=disable", "x"),
            "postgres://u:p@h:5432/x?sslmode=disable"
        );
        assert_eq!(with_database("postgres://h", "x"), "postgres://h/x");
    }
}
