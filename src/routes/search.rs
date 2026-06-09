use crate::error::AppError;
use crate::goch::search::{self, SearchResult};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/search", get(search_handler))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
}

/// スレタイ検索（find.5ch.net をラップ。spec 8.2A / 10）。
async fn search_handler(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, AppError> {
    let q = query.q.trim();
    if q.is_empty() {
        return Err(AppError::BadRequest("q が必要".into()));
    }
    let results = search::search(&state.http, q).await?;
    Ok(Json(results))
}
