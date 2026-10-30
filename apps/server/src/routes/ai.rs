//! AI features: status, tag suggestions, translation of the text in an image,
//! the admin's settings and the local runtime.

use axum::Json;
use axum::extract::{Path, State};
use starbooru_core::local_ai::clean_model_name;
use starbooru_core::{
    AiSettings, AiSettingsUpdate, AiStatus, LocalAiSettings, LocalAiStatus, LocalTestResult,
    PanelTranslation, PullRequest, TagCategory, TagSuggestion, TagSuggestions, TranslateRequest,
    Translation, clean_ai_settings, clean_language, clean_translation, translate_prompt,
};

use super::admin::require_admin_session;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::{ai, local_ai, settings};

/// `GET /ai`: which AI features clients should offer.
pub async fn status(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<AiStatus>> {
    let setup = settings::ai(&state).await?;
    let tagging = setup.tagging();
    let translation = setup.translation().is_some();
    Ok(Json(AiStatus {
        enabled: tagging.is_some() || translation,
        tagging: tagging.is_some(),
        model: tagging.map(|p| p.model),
        translation,
    }))
}

/// `POST /posts/{id}/suggest-tags`: asks the AI provider for tags. The post
/// is not changed.
pub async fn suggest_tags(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<TagSuggestions>> {
    let provider = settings::ai(&state)
        .await?
        .tagging()
        .ok_or_else(ai::turned_off)?;
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
        figures: parsed.figures,
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
    let provider = settings::ai(&state)
        .await?
        .translation()
        .ok_or_else(ai::turned_off)?;
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

/// `GET /posts/{id}/translations`: the translated text kept for the post's
/// image, one per language.
pub async fn panel_translations(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<PanelTranslation>>> {
    Ok(Json(crate::panels::list(&state, id).await?))
}

/// `POST /posts/{id}/translations`: reads the text in the post's image with
/// the translation model, translates it, keeps and returns it. The image is
/// sent to the model.
pub async fn translate_panels(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<TranslateRequest>,
) -> AppResult<Json<PanelTranslation>> {
    let to = clean_language(&req.to).map_err(AppError::BadRequest)?;
    let provider = settings::ai(&state)
        .await?
        .translation()
        .ok_or_else(ai::turned_off)?;
    Ok(Json(
        crate::panels::translate(&state, &provider, id, &to).await?,
    ))
}

fn to_settings(s: &ai::AiSetup) -> AiSettings {
    AiSettings {
        url: s.hosted.url.clone(),
        model: s.hosted.model.clone(),
        key_set: !s.hosted.key.is_empty(),
        autotag: s.autotag,
        local: LocalAiSettings {
            enabled: s.local_enabled,
            url: s.local_url.clone(),
            model: s.local_model.clone(),
        },
        tagging: s.tagging,
        translation: s.translation,
        auto_translate: s.auto_translate.clone(),
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
    let update = clean_ai_settings(&req).map_err(AppError::BadRequest)?;
    settings::save_ai(&state, &update).await?;
    Ok(Json(to_settings(&settings::ai(&state).await?)))
}

/// `GET /admin/ai/local`: this machine's hardware, how each preset would
/// run on it, and the runtime's models and download.
pub async fn local_status(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<LocalAiStatus>> {
    user.require_admin()?;
    let setup = settings::ai(&state).await?;
    Ok(Json(local_ai::status(&state, &setup).await))
}

/// `POST /admin/ai/local/pull`: downloads a model into the runtime in the
/// background; progress shows in `GET /admin/ai/local`.
pub async fn local_pull(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<PullRequest>,
) -> AppResult<Json<LocalAiStatus>> {
    require_admin_session(&user)?;
    let model = clean_model_name(&req.model).map_err(AppError::BadRequest)?;
    let setup = settings::ai(&state).await?;
    if setup.local_url.is_empty() {
        return Err(AppError::BadRequest(
            "enter the address of the local AI runtime first".into(),
        ));
    }
    local_ai::start_pull(&state, setup.local_url.clone(), model).await?;
    Ok(Json(local_ai::status(&state, &setup).await))
}

/// `POST /admin/ai/local/test`: times one tagging request on the local
/// model.
pub async fn local_test(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<LocalTestResult>> {
    require_admin_session(&user)?;
    let setup = settings::ai(&state).await?;
    Ok(Json(local_ai::test(&setup).await?))
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
        \"rating\": \"sensitive\", \"figures\": [{\"kind\": \"girl\", \"character\": \"miku\", \
        \"hair\": [\"twintails\"]}]}\n```";

    fn no_local() -> Value {
        json!({ "enabled": false, "url": "", "model": "" })
    }

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
        assert_eq!(
            status,
            json!({ "enabled": true, "model": "llava", "tagging": true, "translation": true })
        );

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
        assert_eq!(
            body["figures"],
            json!([{ "kind": "girl", "character": "miku", "tags": { "hair": ["twintails"] } }])
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
        // The figures too, through aliases, with their count tags.
        assert_eq!(
            post["figures"],
            json!([{ "kind": "girl", "character": "hatsune_miku",
                     "tags": { "hair": ["twintails"] } }])
        );
        for want in ["twintails", "1girl", "solo"] {
            assert!(names.contains(want), "{names}");
        }
        // A post that has figures keeps them.
        let mut again = parsed.clone();
        again.figures[0].kind = starbooru_core::FigureKind::Boy;
        crate::ai::add_tags(&app.state, id, &again).await.unwrap();
        let post = app.get(&format!("/api/v1/posts/{id}"), &admin).await.json();
        assert_eq!(post["figures"][0]["kind"], "girl");
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
        let body = json!({ "url": r.json()["url"], "model": "llava", "key": "sk-test",
            "autotag": false, "local": no_local(), "tagging": "hosted", "translation": "hosted" });
        let r = app
            .call(Method::PUT, "/api/v1/admin/ai", Some(&admin), Some(body))
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert!(!r.json().to_string().contains("sk-test"));
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());

        // Clearing the key.
        let body = json!({ "url": "http://x/v1", "model": "llava", "key": "", "autotag": true,
            "local": no_local(), "tagging": "hosted", "translation": "off" });
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
        assert_eq!(
            status,
            json!({ "enabled": false, "model": null, "tagging": false, "translation": false })
        );
        let path = format!("/api/v1/posts/{id}/suggest-tags");
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = app.get("/api/v1/admin/ai", &admin).await.json();
        assert_eq!(
            r,
            json!({ "url": "", "model": "", "key_set": false, "autotag": false,
                "local": no_local(), "tagging": "off", "translation": "off", "auto_translate": "" })
        );
        let path = format!("/api/v1/posts/{id}/translations");
        let r = app
            .call(
                Method::POST,
                &path,
                Some(&admin),
                Some(json!({ "to": "it" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
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

    /// A fake Ollama: chat (no key), version, tags, pull and ps.
    async fn ollama() -> String {
        use axum::routing::get;
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(|headers: HeaderMap, body: axum::Json<Value>| async move {
                    if headers.contains_key("authorization")
                        || body["model"] != "qwen3-vl:2b-instruct"
                    {
                        return (StatusCode::BAD_REQUEST, "{}".to_owned());
                    }
                    let answer = if body["messages"][0]["content"].is_array() {
                        ANSWER
                    } else {
                        "CIAO"
                    };
                    let reply = json!({ "choices": [{ "message": { "content": answer } }] });
                    (StatusCode::OK, reply.to_string())
                }),
            )
            .route(
                "/api/version",
                get(|| async { json!({ "version": "0.12.9" }).to_string() }),
            )
            .route(
                "/api/tags",
                get(|| async {
                    json!({ "models": [
                        { "name": "qwen3-vl:2b-instruct", "size": 1_900_000_000u64 },
                    ]})
                    .to_string()
                }),
            )
            .route(
                "/api/pull",
                post(|body: axum::Json<Value>| async move {
                    if body["model"] != "qwen3-vl:4b-instruct" {
                        return "{\"error\":\"pull model manifest: file does not exist\"}\n"
                            .to_owned();
                    }
                    "{\"status\":\"pulling manifest\"}\n\
                     {\"status\":\"pulling 1a2b\",\"total\":100,\"completed\":50}\n\
                     {\"status\":\"success\"}\n"
                        .to_owned()
                }),
            )
            .route(
                "/api/ps",
                get(|| async {
                    json!({ "models": [
                        { "name": "qwen3-vl:2b-instruct", "size": 100, "size_vram": 100 },
                    ]})
                    .to_string()
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    async fn wait_for_pull(app: &testing::TestApp, admin: &str) -> Value {
        for _ in 0..100 {
            let status = app.get("/api/v1/admin/ai/local", admin).await.json();
            if status["pull"]["done"] == true {
                return status["pull"].clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("the download never finished");
    }

    #[tokio::test]
    async fn runs_tools_on_the_local_runtime() {
        let url = ollama().await;
        let Some(app) = testing::app_with(|c| {
            c.ai.local_enabled = true;
            c.ai.local_url = url;
            c.ai.local_model = "qwen3-vl:2b-instruct".into();
        })
        .await
        else {
            return;
        };
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 9, "").await;

        // With no hosted provider, both tools default to local.
        let status = app.get("/api/v1/ai", &admin).await.json();
        assert_eq!(
            status,
            json!({ "enabled": true, "model": "qwen3-vl:2b-instruct", "tagging": true,
                "translation": true })
        );
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/posts/{id}/suggest-tags"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["rating"], "sensitive");

        // The machine, the presets and the runtime.
        let r = app.get("/api/v1/admin/ai/local", &admin).await;
        assert_eq!(r.status, StatusCode::OK);
        let local = r.json();
        assert_eq!(local["runtime"]["version"], "0.12.9");
        assert_eq!(local["runtime_error"], Value::Null);
        assert_eq!(local["tiers"][0]["model"], "qwen3-vl:2b-instruct");
        assert_eq!(local["tiers"][0]["installed"], true);
        assert_eq!(local["tiers"][1]["installed"], false);
        assert!(local["hardware"]["threads"].as_u64().unwrap() >= 1);
        let user = app.register("user").await;
        let r = app.get("/api/v1/admin/ai/local", &user).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);

        // Downloads report progress, and their errors.
        let pull = |model: &str| json!({ "model": model });
        let r = app
            .call(
                Method::POST,
                "/api/v1/admin/ai/local/pull",
                Some(&admin),
                Some(pull("a b")),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = app
            .call(
                Method::POST,
                "/api/v1/admin/ai/local/pull",
                Some(&admin),
                Some(pull("qwen3-vl:4b-instruct")),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let done = wait_for_pull(&app, &admin).await;
        assert_eq!(done["status"], "success");
        assert_eq!(
            (done["completed"].as_u64(), done["total"].as_u64()),
            (Some(50), Some(100))
        );
        assert_eq!(done["error"], Value::Null);
        let r = app
            .call(
                Method::POST,
                "/api/v1/admin/ai/local/pull",
                Some(&admin),
                Some(pull("missing:1b")),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let failed = wait_for_pull(&app, &admin).await;
        assert!(
            failed["error"].as_str().unwrap().contains("does not exist"),
            "{failed}"
        );

        // The speed test.
        let r = app
            .call(
                Method::POST,
                "/api/v1/admin/ai/local/test",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["gpu_percent"], 100);
        assert_eq!(r.json()["fit"], "good");

        // Each tool can be turned off on its own.
        let settings = |tagging: &str, local: bool| {
            json!({ "url": "", "model": "", "autotag": false, "tagging": tagging,
                "translation": "local", "local": { "enabled": local, "url": "http://127.0.0.1:9",
                "model": "" } })
        };
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/ai",
                Some(&admin),
                Some(settings("hosted", true)),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/ai",
                Some(&admin),
                Some(settings("local", false)),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/ai",
                Some(&admin),
                Some(settings("off", true)),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["tagging"], "off");
        assert_eq!(r.json()["local"]["url"], "http://127.0.0.1:9");
        let status = app.get("/api/v1/ai", &admin).await.json();
        assert_eq!(
            (status["tagging"].as_bool(), status["translation"].as_bool()),
            (Some(false), Some(true))
        );
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/posts/{id}/suggest-tags"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        // The runtime now points nowhere: its error is reported, not fatal.
        let local = app.get("/api/v1/admin/ai/local", &admin).await.json();
        assert_eq!(local["runtime"], Value::Null);
        assert!(
            local["runtime_error"]
                .as_str()
                .unwrap()
                .contains("local AI runtime")
        );
        app.finish().await;
    }

    const PANELS: &str = "Sure:\n```json\n{\"blocks\": [\
        {\"box\": [600, 40, 950, 300], \"text\": \"なにっ!?\", \"translation\": \"Cosa?!\"}, \
        {\"box\": [50, 700, 400, 900], \"text\": \"ドン\", \"translation\": \"BUM\"}, \
        {\"box\": [1, 2], \"translation\": \"dropped\"}]}\n```";

    #[tokio::test]
    async fn translates_panels_and_keeps_them() {
        let url = provider(PANELS).await;
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
        let id = app.upload_png(&admin, 3, "").await;
        let path = format!("/api/v1/posts/{id}/translations");
        assert_eq!(app.get(&path, &admin).await.json(), json!([]));

        let r = app
            .call(
                Method::POST,
                &path,
                Some(&admin),
                Some(json!({ "to": "it" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let made = r.json();
        assert_eq!(made["lang"], "it");
        assert_eq!(made["model"], "llava");
        assert_eq!(
            made["blocks"],
            json!([
                { "x": 600, "y": 40, "w": 350, "h": 260, "text": "なにっ!?", "translation": "Cosa?!" },
                { "x": 50, "y": 700, "w": 350, "h": 200, "text": "ドン", "translation": "BUM" },
            ])
        );
        // Kept, and asking again replaces it instead of adding one.
        let r = app
            .call(
                Method::POST,
                &path,
                Some(&admin),
                Some(json!({ "to": "it" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let kept = app.get(&path, &admin).await.json();
        assert_eq!(kept.as_array().map(Vec::len), Some(1));
        assert_eq!(kept[0]["blocks"], made["blocks"]);

        let r = app
            .call(
                Method::POST,
                &path,
                Some(&admin),
                Some(json!({ "to": "no way" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = app
            .call(
                Method::POST,
                "/api/v1/posts/999999/translations",
                Some(&admin),
                Some(json!({ "to": "it" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app
            .call(
                Method::GET,
                "/api/v1/posts/999999/translations",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);

        // Automatic translation: off until a language is set.
        let other = app.upload_png(&admin, 4, "").await;
        crate::panels::auto_translate(&app.state, other)
            .await
            .unwrap();
        let other_path = format!("/api/v1/posts/{other}/translations");
        assert_eq!(app.get(&other_path, &admin).await.json(), json!([]));
        let mut settings = app.get("/api/v1/admin/ai", &admin).await.json();
        assert_eq!(settings["auto_translate"], "");
        settings["auto_translate"] = json!("pt-BR");
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/ai",
                Some(&admin),
                Some(settings),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        crate::panels::auto_translate(&app.state, other)
            .await
            .unwrap();
        let kept = app.get(&other_path, &admin).await.json();
        assert_eq!(kept[0]["lang"], "pt-BR");
        assert_eq!(kept[0]["blocks"].as_array().map(Vec::len), Some(2));

        // An answer with no blocks is an error; one with no text is not
        // kept over a good translation.
        for (answer, status) in [
            ("Sorry, I can't read this.", StatusCode::BAD_GATEWAY),
            ("{\"blocks\": []}", StatusCode::OK),
        ] {
            let mut settings = app.get("/api/v1/admin/ai", &admin).await.json();
            settings["url"] = json!(provider(answer).await);
            let r = app
                .call(
                    Method::PUT,
                    "/api/v1/admin/ai",
                    Some(&admin),
                    Some(settings),
                )
                .await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
            let r = app
                .call(
                    Method::POST,
                    &path,
                    Some(&admin),
                    Some(json!({ "to": "it" })),
                )
                .await;
            assert_eq!(r.status, status, "{:?}", r.json());
            let kept = app.get(&path, &admin).await.json();
            assert_eq!(kept[0]["blocks"], made["blocks"]);
        }

        // Deleting the post takes its translations with it.
        sqlx::query("DELETE FROM posts WHERE id = $1")
            .bind(other)
            .execute(app.db())
            .await
            .unwrap();
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM post_translations")
            .fetch_one(app.db())
            .await
            .unwrap();
        assert_eq!(left, 1);
        app.finish().await;
    }
}
