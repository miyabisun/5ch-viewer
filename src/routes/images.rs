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
        return Err(AppError::BadRequest("url contains control characters".into()));
    }
    Ok(())
}

/// `GET /api/images/{*path}` — serve a cached image file by its normalized URL path.
/// Returns 404 when the metadata or corresponding regular file is missing.
/// Cache-Control is set to immutable: images are content-addressed by URL (never change in place).
async fn serve_image(
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
            if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/webp") {
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
