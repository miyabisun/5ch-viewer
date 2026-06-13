use crate::goch::dat::Res;
use serde::{Deserialize, Serialize};

/// Favorite thread (list API response).
/// Returned unordered (no ORDER BY) since sorting is the frontend's responsibility.
#[derive(Debug, Serialize)]
pub struct Favorite {
    pub server: String,
    pub board: String,
    pub board_name: String,
    pub thread_id: String,
    pub title: String,
    pub res_count: i64,
    pub read_res: i64,
    pub rating: i64,
    pub status: String,
}

/// Add-favorite request. Either a direct url or server/board/thread_id.
#[derive(Debug, Deserialize)]
pub struct AddRequest {
    pub url: Option<String>,
    pub server: Option<String>,
    pub board: Option<String>,
    pub thread_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProgressRequest {
    pub read_res: i64,
}

#[derive(Debug, Deserialize)]
pub struct RatingRequest {
    pub rating: i64,
}

/// Thread body (stored dat) response.
#[derive(Debug, Serialize)]
pub struct DatResponse {
    pub title: String,
    pub res_count: i64,
    pub read_res: i64,
    pub status: String,
    pub res: Vec<Res>,
}

/// Reload (cache-or-fetch full dat) result.
#[derive(Debug, Serialize)]
pub struct ReloadResponse {
    pub res_count: i64,
    pub read_res: i64,
    pub status: String,
    /// Whether the dat changed (false on NotModified).
    pub updated: bool,
}

/// One NGID entry returned by the list endpoint.
#[derive(Debug, Serialize)]
pub struct NgId {
    pub ng_id: String,
    pub created_at: i64,
}

/// Request body for POST /api/ng-ids.
#[derive(Debug, Deserialize)]
pub struct AddNgRequest {
    pub ng_id: String,
}

/// One thread's matching posts, returned by the id-search endpoint.
#[derive(Debug, Serialize)]
pub struct IdSearchThread {
    pub thread_id: String,
    pub title: String,
    pub res: Vec<Res>,
}
