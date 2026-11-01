//! AI features: status, tag suggestions, translation of the text in an image,
//! the admin's settings and the built-in models.

use axum::Json;
use axum::extract::{Path, State};
use starbooru_core::{
    AiAdminStatus, AiGeneralUpdate, AiMode, AiStatus, AiTestResult, AiTool, AiToolUpdate,
    PanelTranslation, TagCategory, TagSuggestion, TagSuggestions, TranslateRequest, Translation,
    clean_general_update, clean_language, clean_tool_update, clean_translation, translate_prompt,
};

use super::admin::require_admin_session;
use crate::ai::{Route, ToolSetup};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::{ai, local_ai, settings};

/// Whether a tool can answer, or will once its model is there.
fn offered(state: &AppState, tool: &ToolSetup) -> bool {
    match tool.mode {
        AiMode::Off => false,
        AiMode::Api => tool.api.enabled(),
        AiMode::Local => state.local_ai.runtime().ready,
    }
}

/// `GET /ai`: which AI features clients should offer.
pub async fn status(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<AiStatus>> {
    let setup = settings::ai(&state).await?;
    let tool = setup.tool(AiTool::Tagging);
    let tagging = offered(&state, &tool);
    let translation = offered(&state, &setup.tool(AiTool::Translation));
    let comics = offered(&state, &setup.tool(AiTool::Comics));
    let model = tagging.then(|| match tool.mode {
        AiMode::Local => local_ai::active(&state, &tool)
            .map(|a| a.model.name.to_owned())
            .unwrap_or_default(),
        _ => tool.api.model.clone(),
    });
    Ok(Json(AiStatus {
        enabled: tagging || translation || comics,
        tagging,
        model,
        translation,
        comics,
    }))
}

/// `POST /posts/{id}/suggest-tags`: asks the tagging model for tags. The post
/// is not changed.
pub async fn suggest_tags(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<TagSuggestions>> {
    let setup = settings::ai(&state).await?;
    let route = ai::route(&state, &setup.tool(AiTool::Tagging)).await?;
    let parsed = ai::suggest(&state, &route, id).await?;
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
    let setup = settings::ai(&state).await?;
    let route = ai::route(&state, &setup.tool(AiTool::Translation)).await?;
    let text: Option<Option<String>> =
        sqlx::query_scalar("SELECT ocr_text FROM posts WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let text = text
        .ok_or(AppError::NotFound)?
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("this post has no text to translate".into()))?;
    let answer = match &route {
        Route::Api(provider) => ai::chat(provider, &translate_prompt(&text, &to), None).await?,
        Route::Local(active) => local_ai::translate_text(&state, active, &text, &to).await?,
    };
    let text = clean_translation(&answer);
    if text.is_empty() {
        return Err(AppError::Upstream(
            "the AI gave an empty translation".into(),
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
/// the comics tool, translates it, keeps and returns it.
pub async fn translate_panels(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<TranslateRequest>,
) -> AppResult<Json<PanelTranslation>> {
    let to = clean_language(&req.to).map_err(AppError::BadRequest)?;
    let setup = settings::ai(&state).await?;
    let route = ai::route(&state, &setup.tool(AiTool::Comics)).await?;
    Ok(Json(
        crate::panels::translate(&state, &route, id, &to).await?,
    ))
}

/// `GET /admin/ai`: every tool, this machine and what can run on it. Keys are
/// never returned.
pub async fn admin_status(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<AiAdminStatus>> {
    user.require_admin()?;
    Ok(Json(local_ai::admin_status(&state).await?))
}

/// `PUT /admin/ai`: the settings all tools share.
pub async fn put_general(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<AiGeneralUpdate>,
) -> AppResult<Json<AiAdminStatus>> {
    require_admin_session(&user)?;
    let update = clean_general_update(&req).map_err(AppError::BadRequest)?;
    let setup = settings::ai(&state).await?;
    if !update.auto_translate.is_empty() && setup.tool(AiTool::Comics).mode == AiMode::Off {
        return Err(AppError::BadRequest(
            "turn on comic translation first".into(),
        ));
    }
    settings::save_ai_general(&state, &update).await?;
    Ok(Json(local_ai::admin_status(&state).await?))
}

fn parse_tool(tool: &str) -> AppResult<AiTool> {
    AiTool::parse(tool).ok_or(AppError::NotFound)
}

/// `PUT /admin/ai/tools/{tool}`: takes effect at once. When it needs a model,
/// that starts downloading.
pub async fn put_tool(
    State(state): State<AppState>,
    user: AuthUser,
    Path(tool): Path<String>,
    Json(req): Json<AiToolUpdate>,
) -> AppResult<Json<AiAdminStatus>> {
    require_admin_session(&user)?;
    let tool = parse_tool(&tool)?;
    let update = clean_tool_update(tool, &req).map_err(AppError::BadRequest)?;
    settings::save_ai_tool(&state, tool, &update).await?;
    state.local_ai.wake();
    Ok(Json(local_ai::admin_status(&state).await?))
}

/// `POST /admin/ai/tools/{tool}/test`: whether the tool's API answers.
pub async fn test_tool(
    State(state): State<AppState>,
    user: AuthUser,
    Path(tool): Path<String>,
) -> AppResult<Json<AiTestResult>> {
    require_admin_session(&user)?;
    let setup = settings::ai(&state).await?.tool(parse_tool(&tool)?);
    if setup.mode != AiMode::Api {
        return Err(AppError::BadRequest("this tool doesn't use an API".into()));
    }
    let start = std::time::Instant::now();
    ai::chat(&setup.api, "Reply with: OK", None).await?;
    let ms = u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX);
    Ok(Json(AiTestResult { ms }))
}

/// `POST /admin/ai/models/{id}/install`: downloads a model in the background;
/// progress shows in `GET /admin/ai`.
pub async fn install_model(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<AiAdminStatus>> {
    require_admin_session(&user)?;
    state.local_ai.install(&id)?;
    Ok(Json(local_ai::admin_status(&state).await?))
}

/// `DELETE /admin/ai/models/{id}`: frees a model's disk space. A model in use
/// stays.
pub async fn uninstall_model(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<AiAdminStatus>> {
    require_admin_session(&user)?;
    local_ai::uninstall(&state, &id).await?;
    Ok(Json(local_ai::admin_status(&state).await?))
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

    /// A fake provider that answers everything.
    async fn text_provider_any(answer: &'static str) -> String {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || async move {
                json!({ "choices": [{ "message": { "content": answer } }] }).to_string()
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

    /// The panel's entry for a tool.
    fn tool_of<'a>(status: &'a Value, tool: &str) -> &'a Value {
        status["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["tool"] == tool)
            .unwrap()
    }

    fn model_of<'a>(tool: &'a Value, id: &str) -> &'a Value {
        tool["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap()
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
            json!({ "enabled": true, "model": "llava", "tagging": true, "translation": true,
                "comics": true })
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
        assert_eq!(r.status, StatusCode::FAILED_DEPENDENCY);
        let text = r.json().to_string();
        assert!(text.contains("AI provider answered 401"), "{text}");
        assert!(!text.contains("sk-wrong"));

        // The admin sees that a key is set, never the key.
        let r = app.get("/api/v1/admin/ai", &admin).await;
        assert_eq!(r.status, StatusCode::OK);
        let url = tool_of(&r.json(), "tagging")["settings"]["api_url"].clone();
        assert_eq!(
            tool_of(&r.json(), "tagging")["settings"]["api_key_set"],
            true
        );
        assert!(!r.json().to_string().contains("sk-wrong"));

        // Replacing the key fixes it; the new key isn't echoed either.
        let body = json!({ "mode": "api", "preset": "balanced", "api_url": url,
            "api_model": "llava", "api_key": "sk-test" });
        let tagging = "/api/v1/admin/ai/tools/tagging";
        let r = app
            .call(Method::PUT, tagging, Some(&admin), Some(body))
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert!(!r.json().to_string().contains("sk-test"));
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());

        // Each tool has its own API: the others still use the old key.
        let translation = tool_of(
            &app.get("/api/v1/admin/ai", &admin).await.json(),
            "translation",
        )
        .clone();
        assert_eq!(translation["settings"]["mode"], "api");

        // Clearing the key. The environment's isn't sent to another address.
        let body = json!({ "mode": "api", "preset": "balanced", "api_url": "http://x/v1",
            "api_model": "llava", "api_key": "" });
        let r = app
            .call(Method::PUT, tagging, Some(&admin), Some(body))
            .await;
        assert_eq!(
            tool_of(&r.json(), "tagging")["settings"]["api_key_set"],
            false
        );

        // Nonsense is refused.
        for body in [
            json!({ "mode": "api", "preset": "fast", "api_url": "ftp://x", "api_model": "m" }),
            json!({ "mode": "api", "preset": "fast", "api_url": "", "api_model": "" }),
            json!({ "mode": "local", "preset": "fast", "model": "m2m" }),
        ] {
            let r = app
                .call(Method::PUT, tagging, Some(&admin), Some(body))
                .await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json());
        }
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/ai/tools/nope",
                Some(&admin),
                Some(json!({ "mode": "off", "preset": "fast" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);

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
            json!({ "enabled": false, "model": null, "tagging": false, "translation": false,
                "comics": false })
        );
        let path = format!("/api/v1/posts/{id}/suggest-tags");
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = app.get("/api/v1/admin/ai", &admin).await.json();
        assert_eq!(r["autotag"], false);
        assert_eq!(r["auto_translate"], "");
        for tool in ["tagging", "translation", "comics"] {
            let tool = tool_of(&r, tool);
            assert_eq!(tool["settings"]["mode"], "off");
            assert_eq!(tool["state"], "off");
            assert_eq!(tool["active"], Value::Null);
        }
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
        // The provider refusing the request is a clean `fetch_failed`.
        assert_eq!(
            translate(id, "en").await.status,
            StatusCode::FAILED_DEPENDENCY
        );
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

    /// A stand-in for the AI worker. The first answer to `tag` kills it, the
    /// way running out of memory would, so tests cover its replacement.
    const FAKE_WORKER: &str = r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"op":"hello"'*) echo '{"reply":"hello","gpu":null}' ;;
    *'"op":"calibrate"'*) echo '{"reply":"calibrated","float_ms":4.87,"int8_ms":3.3,"gpu_ms":null}' ;;
    *'"op":"measure"'*) echo '{"reply":"measured","cpu_ms":700,"gpu_ms":null,"gpu_matches":false}' ;;
    *'"op":"unload"'*) echo '{"reply":"done"}' ;;
    *'"op":"tag"'*)
      if [ -e "$STARBOORU_AI_MODELS/damaged" ]; then
        echo '{"reply":"error","message":"a model file is damaged"}'
        continue
      fi
      if [ ! -e "$STARBOORU_AI_MODELS/crashed" ]; then
        touch "$STARBOORU_AI_MODELS/crashed"
        exit 9
      fi
      echo '{"reply":"tags","tags":[{"name":"Blue Sky","category":"general","score":0.9}],"rating":"general"}' ;;
    *'"op":"translate"'*) echo '{"reply":"translated","texts":["CIAO"]}' ;;
    *'"op":"detect"'*) echo '{"reply":"boxes","boxes":[{"x":0,"y":0,"w":1,"h":1}]}' ;;
    *'"op":"read"'*) echo '{"reply":"texts","texts":["こんにちは"]}' ;;
    *) echo '{"reply":"error","message":"unknown request"}' ;;
  esac
done
"#;

    fn fake_worker(dir: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("fake-worker");
        std::fs::write(&path, FAKE_WORKER).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[tokio::test]
    async fn runs_tools_on_the_built_in_models() {
        let worker_dir = tempfile::tempdir().unwrap();
        let worker = fake_worker(worker_dir.path());
        let Some(app) = testing::app_with(|c| c.ai.worker = Some(worker)).await else {
            return;
        };
        let ai = &app.state.local_ai;
        std::fs::create_dir_all(app.state.config.data_dir.join("ai/models")).unwrap();
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 9, "").await;
        let path = format!("/api/v1/posts/{id}/suggest-tags");
        let tagging = "/api/v1/admin/ai/tools/tagging";
        let local = |preset: &str, model: &str| json!({ "mode": "local", "preset": preset, "model": model });

        // Starting up times the machine, and keeps it.
        crate::local_ai::sweep(&app.state, false).await;
        let status = app.get("/api/v1/admin/ai", &admin).await.json();
        assert_eq!(status["runtime"]["ready"], true);
        assert!(status["hardware"]["threads"].as_u64().unwrap() >= 1);
        let stored: Option<String> =
            sqlx::query_scalar("SELECT ai_calibration::text FROM server_settings")
                .fetch_one(app.db())
                .await
                .unwrap();
        assert!(stored.unwrap().contains("float_ms"));
        // Nothing runs, or downloads, until it's turned on.
        assert_eq!(tool_of(&status, "tagging")["state"], "off");
        assert_eq!(
            model_of(tool_of(&status, "tagging"), "tresnet-320")["installed"],
            false
        );

        // Presets that can't be met on this machine are unavailable.
        let presets = &tool_of(&status, "comics")["presets"];
        assert_eq!(presets[0]["preset"], "fast");
        assert_eq!(presets[0]["available"], false);
        assert_eq!(presets[1]["available"], true);

        // Only one model is there: it answers while the preset's downloads.
        crate::local_ai::mark_all_installed(
            ai,
            starbooru_core::ai_catalog::model("tresnet-320").unwrap(),
        );
        let r = app
            .call(
                Method::PUT,
                tagging,
                Some(&admin),
                Some(local("balanced", "")),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let tool = tool_of(&r.json(), "tagging").clone();
        assert_eq!(tool["state"], "installing");
        // Both sizes of this model are the same file: the better one answers.
        assert_eq!(tool["active"], "tresnet-448");
        let status = app.get("/api/v1/ai", &admin).await.json();
        assert_eq!(status["tagging"], true);
        assert_eq!(status["translation"], false);

        // Its answer goes through the worker, which recovers from a crash.
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(
            r.json()["tags"],
            json!([{ "name": "blue_sky", "category": "general", "exists": false }])
        );
        assert_eq!(r.json()["rating"], "safe");

        // The admin's own pick wins over the preset.
        let r = app
            .call(
                Method::PUT,
                tagging,
                Some(&admin),
                Some(local("fast", "tresnet-320")),
            )
            .await;
        let tool = tool_of(&r.json(), "tagging").clone();
        assert_eq!(tool["state"], "ready");
        assert_eq!(tool["active"], "tresnet-320");

        // Installing shows up as progress. A model of another tool is refused
        // as a pick, and an unknown one can't be installed.
        let r = app
            .call(
                Method::POST,
                "/api/v1/admin/ai/models/nope/install",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app
            .call(
                Method::POST,
                "/api/v1/admin/ai/models/m2m/install",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let m2m = model_of(tool_of(&r.json(), "translation"), "m2m").clone();
        assert!(m2m["progress"]["total"].as_u64().unwrap() > 0);
        // Taken out of the queue again: it wasn't started.
        let r = app
            .call(
                Method::DELETE,
                "/api/v1/admin/ai/models/m2m",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let m2m = model_of(tool_of(&r.json(), "translation"), "m2m").clone();
        assert_eq!(m2m["progress"], Value::Null);

        // A model in use stays. Once it isn't, it goes, with its files.
        let tresnet = "/api/v1/admin/ai/models/tresnet-320";
        let r = app.call(Method::DELETE, tresnet, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json());
        let r = app
            .call(
                Method::PUT,
                tagging,
                Some(&admin),
                Some(json!({ "mode": "off", "preset": "fast" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let r = app.call(Method::DELETE, tresnet, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let tool = tool_of(&r.json(), "tagging").clone();
        assert_eq!(model_of(&tool, "tresnet-320")["installed"], false);
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);

        // Only admins.
        let user = app.register("user").await;
        for (method, url) in [
            (Method::POST, "/api/v1/admin/ai/models/m2m/install"),
            (Method::DELETE, "/api/v1/admin/ai/models/m2m"),
            (Method::POST, "/api/v1/admin/ai/tools/tagging/test"),
        ] {
            let r = app.call(method, url, Some(&user), None).await;
            assert_eq!(r.status, StatusCode::FORBIDDEN);
        }
        app.finish().await;
    }

    #[tokio::test]
    async fn a_damaged_model_repairs_itself() {
        let worker_dir = tempfile::tempdir().unwrap();
        let worker = fake_worker(worker_dir.path());
        let Some(app) = testing::app_with(|c| c.ai.worker = Some(worker)).await else {
            return;
        };
        let ai = &app.state.local_ai;
        let models = app.state.config.data_dir.join("ai/models");
        std::fs::create_dir_all(&models).unwrap();
        let model = starbooru_core::ai_catalog::model("tresnet-320").unwrap();
        crate::local_ai::mark_all_installed(ai, model);
        std::fs::write(models.join("damaged"), b"").unwrap();
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 9, "").await;
        crate::local_ai::sweep(&app.state, false).await;
        let setup = json!({ "mode": "local", "preset": "fast", "model": "tresnet-320" });
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/ai/tools/tagging",
                Some(&admin),
                Some(setup),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);

        let path = format!("/api/v1/posts/{id}/suggest-tags");
        let r = app.call(Method::POST, &path, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["code"], "ai_unavailable");
        assert!(!r.json()["message"].as_str().unwrap().contains('/'));
        // The hash failed, so it's gone and the tool is getting it back.
        assert!(!ai.is_installed(model));
        let status = app.get("/api/v1/admin/ai", &admin).await.json();
        assert_ne!(tool_of(&status, "tagging")["state"], "ready");
        app.finish().await;
    }

    #[tokio::test]
    async fn a_local_tool_without_its_model_says_so() {
        let Some(app) = testing::app_with(|c| {
            c.ai.worker = Some("/nonexistent/worker".into());
            c.ai.tagging = Some(starbooru_core::AiMode::Local);
        })
        .await
        else {
            return;
        };
        let admin = app.register("admin").await;
        let id = app.upload_png(&admin, 9, "").await;
        crate::local_ai::sweep(&app.state, false).await;
        // A build without the worker: an explanation, never an error page.
        let status = app.get("/api/v1/admin/ai", &admin).await.json();
        assert_eq!(status["runtime"]["ready"], false);
        assert_eq!(tool_of(&status, "tagging")["state"], "failed");
        assert!(tool_of(&status, "tagging")["error"].is_string());
        let on = app.get("/api/v1/ai", &admin).await.json();
        assert_eq!(on["tagging"], false);
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/posts/{id}/suggest-tags"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json());
        app.finish().await;
    }

    #[tokio::test]
    async fn the_api_test_answers_with_its_time() {
        let url = text_provider_any("OK").await;
        let Some(app) = testing::app_with(|c| {
            c.ai.url = url;
            c.ai.model = "m".into();
        })
        .await
        else {
            return;
        };
        let admin = app.register("admin").await;
        let test = "/api/v1/admin/ai/tools/tagging/test";
        let r = app.call(Method::POST, test, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert!(r.json()["ms"].is_u64());
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/ai/tools/tagging",
                Some(&admin),
                Some(json!({ "mode": "off", "preset": "fast" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let r = app.call(Method::POST, test, Some(&admin), None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
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
        let settings = app.get("/api/v1/admin/ai", &admin).await.json();
        assert_eq!(settings["auto_translate"], "");
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/ai",
                Some(&admin),
                Some(json!({ "autotag": false, "auto_translate": "pt-BR" })),
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
            ("Sorry, I can't read this.", StatusCode::FAILED_DEPENDENCY),
            ("{\"blocks\": []}", StatusCode::OK),
        ] {
            let body = json!({ "mode": "api", "preset": "balanced",
                "api_url": provider(answer).await, "api_model": "llava", "api_key": "sk-test" });
            let r = app
                .call(
                    Method::PUT,
                    "/api/v1/admin/ai/tools/comics",
                    Some(&admin),
                    Some(body),
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
