//! Image cache routes: serve cached image BLOBs and manage the per-URL mosaic flag.
//!
//! `GET /api/images/{*path}` — serve BLOB by normalized path (404 when not cached).
//! `POST /api/images/mosaic`  — set mosaic=1 for a URL.
//! `DELETE /api/images/mosaic`— set mosaic=0 for a URL.

use crate::error::AppError;
use crate::fivech::images::normalize_image_path;
use crate::models::MosaicRequest;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{header, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/images/mosaic", post(set_mosaic))
        .route("/api/images/mosaic", delete(unset_mosaic))
        // The wildcard route must come last so the static `mosaic` route takes precedence.
        .route("/api/images/{*path}", get(serve_image))
}

/// Validates that a URL is safe for mosaic storage (http/https, ≤2048 bytes, no control chars).
fn validate_mosaic_url(url: &str) -> Result<(), AppError> {
    if url.is_empty() || url.len() > 2048 {
        return Err(AppError::BadRequest("url is empty or too long".into()));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::BadRequest(
            "url must start with http:// or https://".into(),
        ));
    }
    if url.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest(
            "url contains control characters".into(),
        ));
    }
    Ok(())
}

/// `GET /api/images/{*path}` — serve a cached image file by its normalized URL path.
/// Returns 404 when the metadata or corresponding regular file is missing.
/// Cache-Control is set to immutable: images are content-addressed by URL (never change in place).
/// Errors are `no-store`: a missing image is usually still being prefetched in the background.
async fn serve_image(state: State<AppState>, path: Path<String>) -> Response {
    let mut resp = load_image(state, path).await.into_response();
    if !resp.status().is_success() {
        resp.headers_mut().insert(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
    }
    resp
}

async fn load_image(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Response, AppError> {
    let row: Option<(i64, String, i64)> = {
        let conn = state.db.lock().unwrap();
        conn.query_row(
            "SELECT id, mime, file_size FROM image_cache WHERE path=?1 AND file_size IS NOT NULL",
            params![path],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
    };

    match row {
        Some((id, mime, file_size)) => {
            if !matches!(
                mime.as_str(),
                "image/png" | "image/jpeg" | "image/gif" | "image/webp"
            ) {
                return Err(AppError::NotFound(format!("image MIME is invalid: {path}")));
            }
            let root = state.config.image_cache_dir.clone();
            let body = tokio::task::spawn_blocking(move || {
                crate::image_cache::read_verified(std::path::Path::new(&root), id, file_size)
            })
            .await
            .map_err(|e| AppError::Internal(format!("image read task failed: {e}")))?
            .map_err(|_| AppError::NotFound(format!("image file is unavailable: {path}")))?;
            let content_type: axum::http::HeaderValue = mime
                .parse()
                .unwrap_or_else(|_| "application/octet-stream".parse().unwrap());
            let cache_control: axum::http::HeaderValue =
                "public, max-age=31536000, immutable".parse().unwrap();
            Ok((
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type),
                    (header::CACHE_CONTROL, cache_control),
                    (
                        HeaderName::from_static("x-content-type-options"),
                        axum::http::HeaderValue::from_static("nosniff"),
                    ),
                ],
                axum::body::Bytes::from(body),
            )
                .into_response())
        }
        None => Err(AppError::NotFound(format!("image not cached: {path}"))),
    }
}

/// `POST /api/images/mosaic` — set mosaic=1 for the given URL.
/// Inserts a placeholder row when the URL is not yet cached (the file will be filled later).
async fn set_mosaic(
    State(state): State<AppState>,
    Json(req): Json<MosaicRequest>,
) -> Result<Json<Value>, AppError> {
    validate_mosaic_url(&req.url)?;
    let path = normalize_image_path(&req.url).unwrap_or_default();
    let conn = state.db.lock().unwrap();
    conn.execute(
        "INSERT INTO image_cache (url, path, mosaic) VALUES (?1, ?2, 1)
         ON CONFLICT(url) DO UPDATE SET mosaic = 1",
        params![req.url, path],
    )?;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /api/images/mosaic` — set mosaic=0 for the given URL.
async fn unset_mosaic(
    State(state): State<AppState>,
    Json(req): Json<MosaicRequest>,
) -> Result<Json<Value>, AppError> {
    validate_mosaic_url(&req.url)?;
    let conn = state.db.lock().unwrap();
    conn.execute(
        "UPDATE image_cache SET mosaic = 0 WHERE url = ?1",
        params![req.url],
    )?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use rusqlite::Connection;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    fn make_state(conn: Connection) -> AppState {
        let cookies = "/tmp/fivech_images_test_cookies.json";
        let jar = crate::fivech::cookie_jar::open(cookies);
        AppState {
            db: Arc::new(Mutex::new(conn)),
            http: crate::state::build_http_client(jar.clone()),
            image_http: crate::fivech::images::build_image_http_client(),
            jar,
            config: Config {
                port: 3000,
                base_path: String::new(),
                db_path: ":memory:".to_string(),
                image_cache_dir: "/tmp/fivech-images-test-missing".to_string(),
                cookies_path: cookies.to_string(),
                fivech_base_url: String::new(),
            },
            inflight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    async fn get(rows: &[(&str, &str)]) -> Vec<Response> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        for (path, mime) in rows {
            conn.execute(
                "INSERT INTO image_cache (url, path, mime, file_size) VALUES (?1, ?2, ?3, 10)",
                params![format!("https://{path}"), path, mime],
            )
            .unwrap();
        }
        let state = make_state(conn);
        let mut out = Vec::new();
        for path in [
            "i.example/none.png",
            "i.example/bad.png",
            "i.example/gone.png",
        ] {
            out.push(
                serve_image(State(state.clone()), Path(path.to_string()))
                    .await
                    .into_response(),
            );
        }
        out
    }

    /// A missing image is usually still being prefetched, so its 404 must not be cached
    /// by the browser or an edge (Cloudflare caches extension-typed 404s by default).
    #[tokio::test]
    async fn not_found_images_are_not_cacheable() {
        let responses = get(&[
            ("i.example/bad.png", "text/html"),
            ("i.example/gone.png", "image/png"),
        ])
        .await;
        for resp in responses {
            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
            assert_eq!(resp.headers()[header::CACHE_CONTROL], "no-store");
        }
    }
}
