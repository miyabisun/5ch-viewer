//! NGID management routes and board-level ID search.
//!
//! NGID storage is global (not per-board or per-thread): a single `ng_ids` table holds
//! all filtered IDs. NG filtering is applied client-side (ThreadView) — the server always
//! returns full res arrays and the client hides NG posts in the display layer.
//!
//! ID search (`GET /api/boards/{server}/{board}/id-search?id=xxx`) scans locally-cached
//! dat blobs for a given ID and returns matching posts grouped by thread. No 5ch access
//! is performed — this is a pure in-memory scan of the existing dat_blobs cache.

use crate::error::AppError;
use crate::goch::refresh::read_blob_posts;
use crate::goch::url::validate_board;
use crate::models::{AddNgRequest, IdSearchThread, NgId};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use regex_lite::Regex;
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::LazyLock;

// Allowlist for ng_id values: alphanumeric + common base64/ID symbols.
// Prevents PRIMARY KEY pollution and log injection without being overly restrictive.
static NG_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9+/._=\-]+$").unwrap());

// Maximum posts per thread in id-search results (DoS guard).
const SEARCH_MAX_PER_THREAD: usize = 50;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ng-ids", get(list_ng).post(add_ng))
        .route("/api/ng-ids/{ng_id}", delete(remove_ng))
        .route("/api/boards/{server}/{board}/id-search", get(id_search))
}

/// List all registered NGID entries (unordered; sorting is the frontend's job).
async fn list_ng(State(state): State<AppState>) -> Result<Json<Vec<NgId>>, AppError> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT ng_id, created_at FROM ng_ids")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(NgId {
                ng_id: row.get(0)?,
                created_at: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(rows))
}

/// Add a new NGID (INSERT OR IGNORE — duplicate is a silent no-op).
async fn add_ng(
    State(state): State<AppState>,
    Json(req): Json<AddNgRequest>,
) -> Result<Json<Value>, AppError> {
    validate_ng_id(&req.ng_id)?;
    let conn = state.db.lock().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO ng_ids (ng_id) VALUES (?1)",
        params![req.ng_id],
    )?;
    Ok(Json(json!({ "ok": true })))
}

/// Remove an NGID. Returns 404 when the entry does not exist.
async fn remove_ng(
    State(state): State<AppState>,
    Path(ng_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validate_ng_id(&ng_id)?;
    let conn = state.db.lock().unwrap();
    let n = conn.execute("DELETE FROM ng_ids WHERE ng_id=?1", params![ng_id])?;
    if n == 0 {
        return Err(AppError::NotFound("ng_id not found".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct IdSearchQuery {
    id: String,
}

/// Board-level ID search: scans all locally-cached dat blobs for the given `server`/`board`
/// and returns posts whose extracted ID matches the query. No 5ch access is performed.
///
/// Response: `[{ thread_id, title, res: [Res, ...] }, ...]` (only threads with ≥1 match).
/// Each matched post's body is HTML-sanitized (as in `get_dat`) because the frontend uses
/// `{@html}` to display it.
async fn id_search(
    State(state): State<AppState>,
    Path((server, board)): Path<(String, String)>,
    Query(q): Query<IdSearchQuery>,
) -> Result<Json<Vec<IdSearchThread>>, AppError> {
    validate_board(&server, &board)?;
    validate_ng_id(&q.id)?;

    // Collect favorites for this board (all statuses — dead threads still have cached blobs).
    let thread_rows: Vec<(String, String)> = {
        let conn = state.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT thread_id, title FROM favorites WHERE server=?1 AND board=?2",
        )?;
        let rows = stmt
            .query_map(params![server, board], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let mut results: Vec<IdSearchThread> = Vec::new();
    for (thread_id, title) in thread_rows {
        let posts = {
            let conn = state.db.lock().unwrap();
            read_blob_posts(&conn, &server, &board, &thread_id)?
        };

        // Filter to posts whose extracted ID matches the query.
        let mut matched: Vec<_> = posts
            .into_iter()
            .filter(|r| r.id.as_deref() == Some(q.id.as_str()))
            .take(SEARCH_MAX_PER_THREAD)
            .collect();

        if matched.is_empty() {
            continue;
        }

        // Sanitize bodies (the frontend uses {@html}).
        for r in &mut matched {
            r.body = crate::sanitize::clean(&r.body);
        }

        results.push(IdSearchThread {
            thread_id,
            title,
            res: matched,
        });
    }

    Ok(Json(results))
}

/// Validates an ng_id value: must be non-empty and match the allowlist pattern.
fn validate_ng_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || !NG_ID_RE.is_match(id) {
        return Err(AppError::BadRequest(format!("invalid ng_id: {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goch::refresh::replace_blob;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn
    }

    fn insert_favorite(conn: &Connection, server: &str, board: &str, thread_id: &str, title: &str) {
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title)
             VALUES (?1, ?2, ?3, 'board', ?4)",
            params![thread_id, server, board, title],
        )
        .unwrap();
    }

    #[test]
    fn validate_ng_id_accepts_typical_ids() {
        assert!(validate_ng_id("klSUPSuq0").is_ok());
        assert!(validate_ng_id("a+b/c==").is_ok());
        assert!(validate_ng_id("ABC_123-xyz").is_ok());
    }

    #[test]
    fn validate_ng_id_rejects_empty() {
        assert!(validate_ng_id("").is_err());
    }

    #[test]
    fn validate_ng_id_rejects_special_chars() {
        // Angle brackets and spaces are rejected to prevent log injection.
        assert!(validate_ng_id("<script>").is_err());
        assert!(validate_ng_id("id with space").is_err());
    }

    #[test]
    fn insert_or_ignore_is_idempotent() {
        let conn = setup();
        conn.execute("INSERT OR IGNORE INTO ng_ids (ng_id) VALUES ('abc')", [])
            .unwrap();
        conn.execute("INSERT OR IGNORE INTO ng_ids (ng_id) VALUES ('abc')", [])
            .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_ids", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn delete_returns_zero_rows_for_missing_id() {
        let conn = setup();
        let n = conn
            .execute("DELETE FROM ng_ids WHERE ng_id='nonexistent'", [])
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn id_search_matches_posts_by_id() {
        let conn = setup();
        insert_favorite(&conn, "egg", "test", "1000000001", "スレA");
        insert_favorite(&conn, "egg", "test", "1000000002", "スレB");

        // スレA: post 1 with ID:target, post 2 with different ID.
        let dat_a = "名無し<>sage<>2025/01/01 ID:target<>targetの本文<>スレA\n\
                     名無し<><>2025/01/02 ID:other<>他の本文<>\n";
        // スレB: no posts with ID:target.
        let dat_b = "名無し<><>2025/01/01 ID:other<>別スレの本文<>スレB\n";

        replace_blob(&conn, "egg", "test", "1000000001", dat_a).unwrap();
        replace_blob(&conn, "egg", "test", "1000000002", dat_b).unwrap();

        // Simulate the search: manually replicate the filter logic.
        let posts_a = crate::goch::dat::parse_dat(dat_a);
        let matched_a: Vec<_> = posts_a
            .into_iter()
            .filter(|r| r.id.as_deref() == Some("target"))
            .collect();
        assert_eq!(matched_a.len(), 1);
        assert_eq!(matched_a[0].body, "targetの本文");

        let posts_b = crate::goch::dat::parse_dat(dat_b);
        let matched_b: Vec<_> = posts_b
            .into_iter()
            .filter(|r| r.id.as_deref() == Some("target"))
            .collect();
        assert!(matched_b.is_empty());
    }
}

