mod favorites;
mod search;

use crate::spa;
use crate::state::AppState;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

pub fn build_router(state: AppState) -> Router {
    let base_path = state.config.base_path.clone();

    let sub = Router::new()
        .merge(favorites::routes())
        .merge(search::routes())
        .nest_service("/assets", ServeDir::new("client/build/assets"))
        .nest_service("/favicon.svg", ServeFile::new("client/build/favicon.svg"))
        .fallback(get(move || {
            let bp = base_path.clone();
            async move { spa_fallback(&bp) }
        }))
        .with_state(state.clone());

    if state.config.base_path.is_empty() {
        sub
    } else {
        Router::new().nest(&state.config.base_path, sub)
    }
}

fn spa_fallback(base_path: &str) -> impl IntoResponse {
    match spa::get_index_html(base_path) {
        Some(html) => Html(html).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": "Frontend not built. Run: cd client && bun install && bun run build"
            })),
        )
            .into_response(),
    }
}
