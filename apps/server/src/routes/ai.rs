//! AI features: status, tag suggestions, translation of the text in an image
//! and the admin's provider settings.

use axum::Json;
use axum::extract::{Path, State};
use starbooru_core::{
    AiSettings, AiSettingsUpdate, AiStatus, TagCategory, TagSuggestion, TagSuggestions,
    TranslateRequest, Translation, clean_ai_settings, clean_language, clean_translation,
    translate_prompt,
};

use super::admin::require_admin_session;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::{ai, settings};

/// `GET /ai`: whether clients should offer AI features.
pub async fn status(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<AiStatus>> {
    let provider = settings::ai(&state).await?;
    let enabled = provider.enabled();
    Ok(Json(AiStatus {
        enabled,
        model: enabled.then_some(provider.model),
    }))
}

/// `POST /posts/{id}/suggest-tags`: asks the AI provider for tags. The post
/// is not changed.
pub async fn suggest_tags(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<TagSuggestions>> {
    let provider = settings::ai(&state).await?;
    if !provider.enabled() {
        return Err(AppError::BadRequest(
            "AI features are turned off on this server".into(),
        ));
    }
    let parsed = ai::suggest(&state, &provider, id).await?;
    let names: Vec<String> = parsed.tags.iter().map(|(n, _)| n.clone()).collect();
    // Each name, its canonical name (through aliases) and that tag's
    // category when it exists.
    let resolved: Vec<(String, String, Option<i16>)> = sqlx::query_as(
        "SELECT n.name, COALESCE(a.name, n.name), COALESCE(a.category, t.category) \
         FROM unnest($1::text[]) AS n(name) \
         LEFT JOIN (SELECT al.alias, tt.name, tt.category FROM tag_aliases al \
                    JOIN tags tt ON tt.id = al.tag_id) a ON a.alias = n.name \
         LEFT JOIN tags t ON t.name = n.name",
    )
    .bind(&names)
    .fetch_all(&state.db)
    .await?;
    let mut tags: Vec<TagSuggestion> = Vec::with_capacity(names.len());
    for (name, category) in &parsed.tags {
        let Some((_, resolved, found)) = resolved.iter().find(|(n, _, _)| n == name) else {
            continue;
        };
        let resolved = resolved.clone();
        if tags.iter().any(|t| t.name == resolved) {
            continue;
        }
        tags.push(TagSuggestion {
            category: found
                .and_then(TagCategory::from_db)
                .or(*category)
                .unwrap_or_default(),
            exists: found.is_some(),
            name: resolved,
        });
    }
    Ok(Json(TagSuggestions {
        tags,
        rating: parsed.rating,
    }))
}

/// `POST /posts/{id}/translate`: translates the text OCR read in the post's
/// image. Only the text is sent, not the image. Nothing is stored.
pub async fn translate(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<TranslateRequest>,
) -> AppResult<Json<Translation>> {
    let to = clean_language(&req.to).map_err(AppError::BadRequest)?;
    let provider = settings::ai(&state).await?;
    if !provider.enabled() {
        return Err(AppError::BadRequest(
            "AI features are turned off on this server".into(),
        ));
    }
    let text: Option<Option<String>> =
        sqlx::query_scalar("SELECT ocr_text FROM posts WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let text = text
        .ok_or(AppError::NotFound)?
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("this post has no text to translate".into()))?;
    let answer = ai::chat(&provider, &translate_prompt(&text, &to), None).await?;
    let text = clean_translation(&answer);
    if text.is_empty() {
        return Err(AppError::Upstream(
            "the AI provider gave an empty translation".into(),
        ));
    }
    Ok(Json(Translation { text }))
}

fn to_settings(p: &ai::Provider) -> AiSettings {
    AiSettings {
        url: p.url.clone(),
        model: p.model.clone(),
        key_set: !p.key.is_empty(),
        autotag: p.autotag,
    }
}

/// `GET /admin/ai`. The key is never returned.
pub async fn get_settings(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<AiSettings>> {
    user.require_admin()?;
    Ok(Json(to_settings(&settings::ai(&state).await?)))
}

/// `PUT /admin/ai`: takes effect at once.
pub async fn put_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<AiSettingsUpdate>,
) -> AppResult<Json<AiSettings>> {
    require_admin_session(&user)?;
    let (url, model) = clean_ai_settings(&req).map_err(AppError::BadRequest)?;
    settings::save_ai(&state, &url, &model, req.key.as_deref(), req.autotag).await?;
    Ok(Json(to_settings(&settings::ai(&state).await?)))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::{HeaderMap, Method, StatusCode};
    use axum::routing::post;
    use serde_json::{Value, json};

    use crate::testing;

    /// A fake OpenAI-compatible provider. Answers `answer` when the request
    /// carries the right key, model and image, else 401 / 400.
    async fn provider(answer: &'static str) -> String {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(
                move |headers: HeaderMap, body: axum::Json<Value>| async move {
                    if headers.get("authorization").and_then(|v| v.to_str().ok())
                        != Some("Bearer sk-test")
                    {
                        return (StatusCode::UNAUTHORIZED, "{}".to_owned());
                    }
                    let url = body["messages"][0]["content"][1]["image_url"]["url"]
                        .as_str()
                        .unwrap_or_default();
                    if !url.starts_with("data:image/jpeg;base64,") || body["model"] != "llava" {
                        return (StatusCode::BAD_REQUEST, "{}".to_owned());
                    }
                    let reply = json!({ "choices": [{ "message": { "content": answer } }] });
                    (StatusCode::OK, reply.to_string())
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}/v1")
    }

    /// A fake provider for text-only requests: answers `answer` when the
    /// message is a plain string with the fenced text in it.
    async fn text_provider(answer: &'static str) -> String {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |body: axum::Json<Value>| async move {
                let content = body["messages"][0]["content"].as_str().unwrap_or_default();
                if !content.contains("<text>\nTOW AWAY ZONE\n</text>")
                    || !content.contains("\"it\"")
                {
                    return (StatusCode::BAD_REQUEST, "{}".to_owned());
                }
                let reply = json!({ "choices": [{ "message": { "content": answer } }] });
                (StatusCode::OK, reply.to_string())
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}/v1")
    }

    const ANSWER: &str = "```json\n{\"tags\": [{\"name\": \"Blue Sky\", \"category\": \"general\"}, \
        {\"name\": \"miku\", \"category\": \"character\"}, {\"name\": \"some_artist\", \
        \"category\": \"artist\"}, {\"name\": \"brand_new\"}, \"-bad\"], \
        \"rating\": \"sensitive\"}\n```";

    fn tag_names(post: &Value) -> String {
        post["tags"].to_string()
    }

    #[tokio::test]
    async fn suggests_tags_without_changing_the_post() {
        let url = provider(ANSWER).await;
        let Some(app) = testing::app_with(|c| {
            c.ai.url = url;
            c.ai.key = "sk-test".into();
            c.ai.model = "llava".into();
        })
        .await
        else {
            return;
        };
        let admin = app.register("admin").await;
        app.upload_png(&admin, 1, "blue_sky hatsune_miku").await;
        let r = app
            .call(
                Method::POST,
                "/api/v1/tags/hatsune_miku/aliases",
                Some(&admin),
                Some(json!({ "alias": "miku" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let id = app.upload_png(&admin, 2, "").await;

        let status = app.get("/api/v1/ai", &admin).await.json();
        assert_eq!(status, json!({ "enabled": true, "model": "llava" }));

        let path = format!("/api/v1/posts/{id}/suggest-tags");
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let body = r.json();
        assert_eq!(body["rating"], "sensitive");
        assert_eq!(
            body["tags"],
            json!([
                { "name": "blue_sky", "category": "general", "exists": true },
                { "name": "hatsune_miku", "category": "general", "exists": true },
                { "name": "some_artist", "category": "artist", "exists": false },
                { "name": "brand_new", "category": "general", "exists": false },
            ])
        );

        // Nothing changed on the post.
        let post = app.get(&format!("/api/v1/posts/{id}"), &admin).await.json();
        assert_eq!(post["tags"], json!([]));

        // Adding the suggestions goes through the normal tag path.
        let parsed = starbooru_core::parse_suggestions(ANSWER);
        crate::ai::add_tags(&app.state, id, &parsed).await.unwrap();
        let post = app.get(&format!("/api/v1/posts/{id}"), &admin).await.json();
        let names = tag_names(&post);
        for want in ["blue_sky", "hatsune_miku", "some_artist", "brand_new"] {
            assert!(names.contains(want), "{names}");
        }
        assert!(!names.contains("\"miku\""), "{names}");
        let count: i32 = sqlx::query_scalar("SELECT post_count FROM tags WHERE name = 'blue_sky'")
            .fetch_one(app.db())
            .await
            .unwrap();
        assert_eq!(count, 2);
        let cat: i16 = sqlx::query_scalar("SELECT category FROM tags WHERE name = 'some_artist'")
            .fetch_one(app.db())
            .await
            .unwrap();
        assert_eq!(cat, 1);
        app.finish().await;
    }

    #[tokio::test]
    async fn provider_errors_are_clean_and_keys_stay_secret() {
        let url = provider(ANSWER).await;
        let Some(app) = testing::app_with(|c| {
            c.ai.url = url;
            c.ai.key = "sk-wrong".into();
            c.ai.model = "llava".into();
        })
        .await
        else {
            return;
        };
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 3, "").await;
        let path = format!("/api/v1/posts/{id}/suggest-tags");
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::BAD_GATEWAY);
        let text = r.json().to_string();
        assert!(text.contains("AI provider answered 401"), "{text}");
        assert!(!text.contains("sk-wrong"));

        // The admin sees that a key is set, never the key.
        let r = app.get("/api/v1/admin/ai", &admin).await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(r.json()["key_set"], true);
        assert!(!r.json().to_string().contains("sk-wrong"));

        // Replacing the key fixes it; the new key isn't echoed either.
        let body =
            json!({ "url": r.json()["url"], "model": "llava", "key": "sk-test", "autotag": false });
        let r = app
            .call(Method::PUT, "/api/v1/admin/ai", Some(&admin), Some(body))
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert!(!r.json().to_string().contains("sk-test"));
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());

        // Clearing the key.
        let body = json!({ "url": "http://x/v1", "model": "llava", "key": "", "autotag": true });
        let r = app
            .call(Method::PUT, "/api/v1/admin/ai", Some(&admin), Some(body))
            .await;
        assert_eq!(r.json()["key_set"], false);
        assert_eq!(r.json()["autotag"], true);

        // Only admins.
        let user = app.register("user").await;
        let r = app.get("/api/v1/admin/ai", &user).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        app.finish().await;
    }

    #[tokio::test]
    async fn off_by_default() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 4, "").await;
        let status = app.get("/api/v1/ai", &admin).await.json();
        assert_eq!(status, json!({ "enabled": false, "model": null }));
        let path = format!("/api/v1/posts/{id}/suggest-tags");
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = app.get("/api/v1/admin/ai", &admin).await.json();
        assert_eq!(
            r,
            json!({ "url": "", "model": "", "key_set": false, "autotag": false })
        );
        app.finish().await;
    }

    #[tokio::test]
    async fn autotags_uploads_when_on() {
        let url = provider(ANSWER).await;
        let Some(app) = testing::app_with(|c| {
            c.ai.url = url;
            c.ai.key = "sk-test".into();
            c.ai.model = "llava".into();
            c.ai.autotag = true;
        })
        .await
        else {
            return;
        };
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 5, "mine").await;
        let mut tagged = false;
        for _ in 0..100 {
            let post = app.get(&format!("/api/v1/posts/{id}"), &admin).await.json();
            if tag_names(&post).contains("brand_new") {
                assert!(tag_names(&post).contains("mine"));
                tagged = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(tagged);
        app.finish().await;
    }

    #[tokio::test]
    async fn translates_the_text_in_an_image() {
        let url = text_provider("```\nZONA RIMOZIONE FORZATA\n```").await;
        let Some(app) = testing::app_with(|c| {
            c.ai.url = url;
            c.ai.model = "llama".into();
        })
        .await
        else {
            return;
        };
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 6, "").await;
        let other = app.upload_png(&admin, 7, "").await;
        sqlx::query("UPDATE posts SET ocr_text = 'TOW AWAY ZONE', ocr_at = now() WHERE id = $1")
            .bind(id)
            .execute(app.db())
            .await
            .unwrap();
        let translate = |post: i64, to: &str| {
            let body = json!({ "to": to });
            let app = &app;
            let admin = &admin;
            async move {
                app.call(
                    Method::POST,
                    &format!("/api/v1/posts/{post}/translate"),
                    Some(admin),
                    Some(body),
                )
                .await
            }
        };

        let r = translate(id, "it").await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json(), json!({ "text": "ZONA RIMOZIONE FORZATA" }));

        // A bad language, a post without text, a missing post.
        assert_eq!(
            translate(id, "italian!").await.status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(translate(other, "it").await.status, StatusCode::BAD_REQUEST);
        assert_eq!(translate(999_999, "it").await.status, StatusCode::NOT_FOUND);
        // The provider refusing the request is a clean 502.
        assert_eq!(translate(id, "en").await.status, StatusCode::BAD_GATEWAY);
        app.finish().await;
    }

    #[tokio::test]
    async fn translation_needs_a_provider() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 8, "").await;
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/posts/{id}/translate"),
                Some(&admin),
                Some(json!({ "to": "en" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        app.finish().await;
    }
}
