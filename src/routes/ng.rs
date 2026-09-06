//! NGID management routes, NG word routes, NG wacchoi routes, and board-level search.
//!
//! NGID and NG word storage is scoped by `(server, board)` and never tied to a thread:
//! a rule applies to every thread of that board (already cached or fetched later) and to
//! no other board. NG filtering is applied client-side (ThreadView) — the server always
//! returns full res arrays and the client hides NG posts in the display layer.
//!
//! NG word `kind` is `"text"` (literal substring of the displayed body) or `"regex"`.
//! **Regex syntax is validated on the client, not here.** The engine that actually
//! evaluates a stored pattern is the browser's `RegExp` (filtering is client-side), so
//! that same engine is the single owner of "is this pattern valid". Re-checking with
//! `regex-lite` would disagree in both directions — it has no look-around (rejecting
//! patterns the browser runs fine) and accepts Rust-only syntax the browser refuses.
//! The server therefore validates only what it can own: the scope, the kind, and that
//! the pattern is non-empty.
//!
//! NG wacchoi storage is scoped by (suffix, board, week_key): `ng_wacchoi` table.
//! The week_key is a Thursday-anchored week identifier computed client-side and stored
//! as an opaque string — the server validates but does not interpret it.
//!
//! ID search (`GET /api/boards/{server}/{board}/id-search?id=xxx`) scans locally-cached
//! dat blobs for a given ID and returns matching posts grouped by thread. No 5ch access
//! is performed — this is a pure in-memory scan of the existing dat_blobs cache.
//!
//! Wacchoi search (`GET /api/boards/{server}/{board}/wacchoi-search?suffix=zzzz`) scans
//! locally-cached dat blobs for posts whose wacchoi suffix matches, grouped by thread.

use crate::error::AppError;
use crate::fivech::refresh::read_blob_posts;
use crate::fivech::url::validate_board;
use crate::models::{
    AddNgRequest, AddNgWacchoiRequest, AddNgWordRequest, IdSearchThread, NgId, NgWacchoi, NgWord,
};
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
static NG_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9+/._=\-]+$").unwrap());

// Wacchoi suffix: exactly 4 chars from [\w+] (alphanumeric, underscore, or '+').
// Matches the last 4 chars of the xxyy-zzzz token (after the hyphen).
// '+' appears in some wacchoi tokens so the character class includes it.
static SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\w+]{4}$").unwrap());

// Wacchoi token in a name string: [\w+]{4}-[\w+]{4} inside parentheses.
// Mirrors client/src/lib/wacchoi.js WACCHOI_RE (which uses JS lookbehind/lookahead;
// regex_lite does not support look-around, so boundary constraints are expressed
// differently here).
//
// Boundary strategy: require a non-[\w+] character (or nothing = start of parens
// content) immediately before the token, and a non-[\w+] character (or nothing =
// end of parens content) immediately after.  This prevents a 5-char token like
// "12345-67890" from yielding a false sub-match of "2345-6789".
//
//   (?:.*?[^\w+])?  -- optional: any prefix ending with a non-[\w+] separator
//   ([\w+]{4}-[\w+]{4})  -- the token (captured as group 1; group 2 = suffix)
//   (?:[^\w+].*?)?  -- optional: any suffix starting with a non-[\w+] separator
static WACCHOI_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\((?:.*?[^\w+])?([\w+]{4}-([\w+]{4}))(?:[^\w+].*?)?\)").unwrap());

// Maximum posts per thread in search results (DoS guard).
const SEARCH_MAX_PER_THREAD: usize = 50;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ng-ids", get(list_ng).post(add_ng).delete(remove_ng))
        .route(
            "/api/ng-words",
            get(list_ng_words).post(add_ng_word).delete(remove_ng_word),
        )
        .route("/api/ng-wacchoi", get(list_ng_wacchoi).post(add_ng_wacchoi))
        .route("/api/ng-wacchoi", delete(remove_ng_wacchoi))
        .route("/api/boards/{server}/{board}/id-search", get(id_search))
        .route(
            "/api/boards/{server}/{board}/wacchoi-search",
            get(wacchoi_search),
        )
}

/// List all registered NGID entries with their (server, board) scope
/// (unordered; sorting and per-board filtering are the frontend's job).
async fn list_ng(State(state): State<AppState>) -> Result<Json<Vec<NgId>>, AppError> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT server, board, ng_id, created_at FROM ng_ids")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(NgId {
                server: row.get(0)?,
                board: row.get(1)?,
                ng_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(rows))
}

/// Add a new NGID for one board (INSERT OR IGNORE — duplicate is a silent no-op).
async fn add_ng(
    State(state): State<AppState>,
    Json(req): Json<AddNgRequest>,
) -> Result<Json<Value>, AppError> {
    validate_board(&req.server, &req.board)?;
    validate_ng_id(&req.ng_id)?;
    let conn = state.db.lock().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO ng_ids (server, board, ng_id) VALUES (?1, ?2, ?3)",
        params![req.server, req.board, req.ng_id],
    )?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RemoveNgQuery {
    server: String,
    board: String,
    ng_id: String,
}

/// Remove an NGID from one board. Returns 404 when the entry does not exist.
/// Query params (not a path segment) because the key is a triple, matching
/// `DELETE /api/ng-wacchoi`.
async fn remove_ng(
    State(state): State<AppState>,
    Query(q): Query<RemoveNgQuery>,
) -> Result<Json<Value>, AppError> {
    validate_board(&q.server, &q.board)?;
    validate_ng_id(&q.ng_id)?;
    let conn = state.db.lock().unwrap();
    let n = conn.execute(
        "DELETE FROM ng_ids WHERE server=?1 AND board=?2 AND ng_id=?3",
        params![q.server, q.board, q.ng_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("ng_id not found".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

// --- NG word handlers ---

/// List all registered NG word entries with their (server, board) scope
/// (unordered; sorting and per-board filtering are the frontend's job).
async fn list_ng_words(State(state): State<AppState>) -> Result<Json<Vec<NgWord>>, AppError> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT server, board, kind, pattern, created_at FROM ng_words")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(NgWord {
                server: row.get(0)?,
                board: row.get(1)?,
                kind: row.get(2)?,
                pattern: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(rows))
}

/// Add a new NG word for one board (INSERT OR IGNORE — duplicate is a silent no-op).
async fn add_ng_word(
    State(state): State<AppState>,
    Json(req): Json<AddNgWordRequest>,
) -> Result<Json<Value>, AppError> {
    validate_board(&req.server, &req.board)?;
    validate_ng_word(&req.kind, &req.pattern)?;
    let conn = state.db.lock().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO ng_words (server, board, kind, pattern) VALUES (?1, ?2, ?3, ?4)",
        params![req.server, req.board, req.kind, req.pattern],
    )?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RemoveNgWordQuery {
    server: String,
    board: String,
    kind: String,
    pattern: String,
}

/// Remove an NG word from one board. Returns 404 when the entry does not exist.
async fn remove_ng_word(
    State(state): State<AppState>,
    Query(q): Query<RemoveNgWordQuery>,
) -> Result<Json<Value>, AppError> {
    validate_board(&q.server, &q.board)?;
    validate_ng_word(&q.kind, &q.pattern)?;
    let conn = state.db.lock().unwrap();
    let n = conn.execute(
        "DELETE FROM ng_words WHERE server=?1 AND board=?2 AND kind=?3 AND pattern=?4",
        params![q.server, q.board, q.kind, q.pattern],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("ng_word not found".into()));
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
    let results = board_post_search(&state, &server, &board, |r| {
        r.id.as_deref() == Some(q.id.as_str())
    })?;
    Ok(Json(results))
}

/// Scans all locally-cached dat blobs for the given `server`/`board` and returns posts
/// matching `keep`, grouped by thread (only threads with ≥1 match). Per-thread results are
/// capped at `SEARCH_MAX_PER_THREAD` (DoS guard) and bodies are HTML-sanitized for `{@html}`.
/// No 5ch access is performed — pure scan of the existing dat_blobs cache.
fn board_post_search(
    state: &AppState,
    server: &str,
    board: &str,
    keep: impl Fn(&crate::fivech::dat::Res) -> bool,
) -> Result<Vec<IdSearchThread>, AppError> {
    // Collect favorites for this board (all statuses — dead threads still have cached blobs).
    let thread_rows: Vec<(String, String)> = {
        let conn = state.db.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT thread_id, title FROM favorites WHERE server=?1 AND board=?2")?;
        let rows = stmt
            .query_map(params![server, board], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let mut results: Vec<IdSearchThread> = Vec::new();
    for (thread_id, title) in thread_rows {
        let posts = {
            let conn = state.db.lock().unwrap();
            read_blob_posts(&conn, server, board, &thread_id)?
        };

        let mut matched: Vec<_> = posts
            .into_iter()
            .filter(|r| keep(r))
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

    Ok(results)
}

// --- NG wacchoi handlers ---

/// List all registered NG wacchoi entries.
async fn list_ng_wacchoi(State(state): State<AppState>) -> Result<Json<Vec<NgWacchoi>>, AppError> {
    let conn = state.db.lock().unwrap();
    let mut stmt =
        conn.prepare("SELECT suffix, board, week_key, wacchoi, created_at FROM ng_wacchoi")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(NgWacchoi {
                suffix: row.get(0)?,
                board: row.get(1)?,
                week_key: row.get(2)?,
                wacchoi: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(rows))
}

/// Add a new NG wacchoi entry (INSERT OR IGNORE — duplicate is a silent no-op).
async fn add_ng_wacchoi(
    State(state): State<AppState>,
    Json(req): Json<AddNgWacchoiRequest>,
) -> Result<Json<Value>, AppError> {
    validate_suffix(&req.suffix)?;
    validate_week_key(&req.week_key)?;
    // board validation reuses the same SEGMENT_RE as validate_board (no thread_id needed).
    crate::fivech::url::validate_board("dummy", &req.board)
        .map_err(|_| AppError::BadRequest(format!("invalid board: {}", req.board)))?;
    let conn = state.db.lock().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO ng_wacchoi (suffix, board, week_key, wacchoi)
         VALUES (?1, ?2, ?3, ?4)",
        params![req.suffix, req.board, req.week_key, req.wacchoi],
    )?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RemoveNgWacchoiQuery {
    suffix: String,
    board: String,
    week_key: String,
}

/// Remove a NG wacchoi entry via query params. Returns 404 when the entry does not exist.
async fn remove_ng_wacchoi(
    State(state): State<AppState>,
    Query(q): Query<RemoveNgWacchoiQuery>,
) -> Result<Json<Value>, AppError> {
    validate_suffix(&q.suffix)?;
    validate_week_key(&q.week_key)?;
    crate::fivech::url::validate_board("dummy", &q.board)
        .map_err(|_| AppError::BadRequest(format!("invalid board: {}", q.board)))?;
    let conn = state.db.lock().unwrap();
    let n = conn.execute(
        "DELETE FROM ng_wacchoi WHERE suffix=?1 AND board=?2 AND week_key=?3",
        params![q.suffix, q.board, q.week_key],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("ng_wacchoi entry not found".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct WacchoiSearchQuery {
    suffix: String,
}

/// Board-level wacchoi suffix search: scans all locally-cached dat blobs for the given
/// `server`/`board` and returns posts whose wacchoi suffix (the 4 chars after the hyphen)
/// matches the query. No 5ch access is performed.
///
/// Response: `[{ thread_id, title, res: [Res, ...] }, ...]` (only threads with ≥1 match).
async fn wacchoi_search(
    State(state): State<AppState>,
    Path((server, board)): Path<(String, String)>,
    Query(q): Query<WacchoiSearchQuery>,
) -> Result<Json<Vec<IdSearchThread>>, AppError> {
    validate_board(&server, &board)?;
    validate_suffix(&q.suffix)?;
    let results = board_post_search(&state, &server, &board, |r| {
        extract_wacchoi_suffix(&r.name).as_deref() == Some(q.suffix.as_str())
    })?;
    Ok(Json(results))
}

/// Extracts the wacchoi suffix (the 4 chars after the hyphen in xxyy-zzzz) from a raw
/// dat name string. The name may still contain HTML tags like </b>…<b> (dat format).
/// The WACCHOI_NAME_RE regex targets the parenthesised token, so tags outside the
/// parentheses are harmless — no tag stripping needed before matching.
/// Returns None when no wacchoi token is present.
fn extract_wacchoi_suffix(name: &str) -> Option<String> {
    WACCHOI_NAME_RE
        .captures(name)
        .and_then(|c| c.get(2))
        .map(|m| m.as_str().to_string())
}

/// Validates an ng_id value: must be non-empty and match the allowlist pattern.
fn validate_ng_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || !NG_ID_RE.is_match(id) {
        return Err(AppError::BadRequest(format!("invalid ng_id: {id}")));
    }
    Ok(())
}

/// Validates an NG word rule: `kind` must be one of the two known kinds and `pattern`
/// must be non-empty. Regex syntax is deliberately not checked here — see the module
/// docs: the browser's `RegExp` evaluates stored patterns and is therefore the single
/// owner of regex validity.
fn validate_ng_word(kind: &str, pattern: &str) -> Result<(), AppError> {
    if kind != "text" && kind != "regex" {
        return Err(AppError::BadRequest(format!(
            "invalid ng_word kind: {kind}"
        )));
    }
    if pattern.is_empty() {
        return Err(AppError::BadRequest("ng_word pattern is empty".into()));
    }
    Ok(())
}

/// Validates a wacchoi suffix: must be exactly 4 word characters.
fn validate_suffix(suffix: &str) -> Result<(), AppError> {
    if !SUFFIX_RE.is_match(suffix) {
        return Err(AppError::BadRequest(format!(
            "invalid wacchoi suffix: {suffix}"
        )));
    }
    Ok(())
}

/// Validates a week_key: must be non-empty, not contain control characters, and be
/// at most 64 characters (the client generates a date-based string like "2026/06/11").
fn validate_week_key(week_key: &str) -> Result<(), AppError> {
    if week_key.is_empty() || week_key.len() > 64 {
        return Err(AppError::BadRequest(format!(
            "invalid week_key: {week_key}"
        )));
    }
    if week_key.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest(format!(
            "invalid week_key: {week_key}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::fivech::refresh::replace_blob;
    use rusqlite::Connection;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn
    }

    fn make_state(conn: Connection) -> AppState {
        let jar = crate::fivech::cookie_jar::open("/tmp/fivech_ng_test_cookies.json");
        let http = crate::state::build_http_client(jar.clone());
        let image_http = crate::fivech::images::build_image_http_client();
        AppState {
            db: Arc::new(Mutex::new(conn)),
            http,
            image_http,
            jar,
            config: Config {
                port: 3000,
                base_path: String::new(),
                db_path: ":memory:".to_string(),
                image_cache_dir: "/tmp/fivech-test-images".to_string(),
                cookies_path: "/tmp/fivech_ng_test_cookies.json".to_string(),
                fivech_base_url: String::new(),
            },
            inflight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn insert_favorite(conn: &Connection, server: &str, board: &str, thread_id: &str, title: &str) {
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title)
             VALUES (?1, ?2, ?3, 'board', ?4)",
            params![thread_id, server, board, title],
        )
        .unwrap();
    }

    // --- validate_suffix tests ---

    #[test]
    fn validate_suffix_accepts_4_word_chars() {
        assert!(validate_suffix("83IP").is_ok());
        assert!(validate_suffix("ZZZZ").is_ok());
        assert!(validate_suffix("a1b2").is_ok());
        assert!(validate_suffix("____").is_ok());
    }

    #[test]
    fn validate_suffix_accepts_plus_sign() {
        // '+' appears in some wacchoi tokens (e.g. "83+P"); must be accepted.
        assert!(validate_suffix("83+P").is_ok());
        assert!(validate_suffix("++++").is_ok());
        assert!(validate_suffix("a1+Z").is_ok());
    }

    #[test]
    fn validate_suffix_rejects_wrong_length() {
        assert!(validate_suffix("").is_err());
        assert!(validate_suffix("abc").is_err()); // 3 chars
        assert!(validate_suffix("abcde").is_err()); // 5 chars
    }

    #[test]
    fn validate_suffix_rejects_special_chars() {
        assert!(validate_suffix("ab-c").is_err()); // hyphen
        assert!(validate_suffix("ab.c").is_err()); // dot
        assert!(validate_suffix("ab c").is_err()); // space
    }

    // --- extract_wacchoi_suffix tests ---
    // (ng_wacchoi INSERT OR IGNORE idempotency is covered by
    // db::tests::ng_wacchoi_table_exists_and_accepts_rows, which is more exhaustive:
    // it also checks that a different suffix in the same board+week is a separate row.)

    #[test]
    fn extract_wacchoi_suffix_from_raw_dat_name() {
        // Raw dat name with HTML tags (as stored in dat_blobs before formatName stripping).
        let name = "iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>";
        assert_eq!(extract_wacchoi_suffix(name), Some("83IP".to_string()));
    }

    #[test]
    fn extract_wacchoi_suffix_matches_only_suffix_not_prefix() {
        // Different prefixes (xxyy) with same suffix (zzzz) must both match.
        let name_a = "名無し</b>(ﾜｯﾁｮｲ aaaa-83IP [::1])<b>";
        let name_b = "名無し</b>(ﾜｯﾁｮｲ bbbb-83IP [::2])<b>";
        assert_eq!(extract_wacchoi_suffix(name_a), Some("83IP".to_string()));
        assert_eq!(extract_wacchoi_suffix(name_b), Some("83IP".to_string()));
    }

    #[test]
    fn extract_wacchoi_suffix_returns_none_when_absent() {
        assert_eq!(extract_wacchoi_suffix("名無し"), None);
        assert_eq!(extract_wacchoi_suffix("名無し</b><b>"), None);
    }

    #[test]
    fn extract_wacchoi_suffix_rejects_5char_5char_token() {
        // A 5-char prefix or 5-char suffix must NOT yield a false 4-4 sub-match.
        // Previous bug: WACCHOI_NAME_RE without boundary guards matched "2345-6789"
        // from "(12345-67890)".
        assert_eq!(extract_wacchoi_suffix("(12345-67890)"), None);
        // 5-char prefix only
        assert_eq!(extract_wacchoi_suffix("(ﾜｯﾁｮｲ 12345-abcd [::1])"), None);
        // 5-char suffix only
        assert_eq!(extract_wacchoi_suffix("(ﾜｯﾁｮｲ abcd-12345 [::1])"), None);
    }

    #[test]
    fn extract_wacchoi_suffix_handles_plus_in_token() {
        // '+' may appear in wacchoi tokens (e.g. "7b+6-83+P"); both sides of the hyphen
        // must be extracted correctly.
        let name = "foo </b>(ﾜｯﾁｮｲ 7b+6-83+P [2400::])<b>";
        assert_eq!(extract_wacchoi_suffix(name), Some("83+P".to_string()));
    }

    // --- wacchoi-search filter logic ---

    #[test]
    fn wacchoi_search_matches_posts_by_suffix_regardless_of_prefix() {
        let conn = setup();
        insert_favorite(&conn, "egg", "test", "1000000001", "スレA");
        insert_favorite(&conn, "egg", "test", "1000000002", "スレB");

        // スレA: res1 has suffix 83IP with prefix 7bb6, res2 has different suffix ZZZZ.
        let dat_a = "iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<><>2025/01/01<>本文1_83IP<>スレA\n\
                     名無し</b>(ﾜｯﾁｮｲ aaaa-ZZZZ [::1])<><>2025/01/01<>本文2_ZZZZ<>\n";
        // スレB: res1 has same suffix 83IP but different prefix (IP violation check).
        let dat_b = "名無し</b>(ﾜｯﾁｮｲ cccc-83IP [::3])<><>2025/01/01<>本文3_83IP<>スレB\n";

        replace_blob(&conn, "egg", "test", "1000000001", dat_a, 0).unwrap();
        replace_blob(&conn, "egg", "test", "1000000002", dat_b, 0).unwrap();

        // Call the real search function (as wacchoi_search does) for suffix "83IP".
        let state = make_state(conn);
        let results = board_post_search(&state, "egg", "test", |r| {
            extract_wacchoi_suffix(&r.name).as_deref() == Some("83IP")
        })
        .unwrap();

        // Both threads have exactly one matching post each; only 1 result per thread.
        assert_eq!(results.len(), 2);

        let thread_a = results
            .iter()
            .find(|t| t.thread_id == "1000000001")
            .unwrap();
        // Only res1 matches (res2 has ZZZZ suffix).
        assert_eq!(thread_a.res.len(), 1);
        assert_eq!(thread_a.res[0].body, "本文1_83IP");

        let thread_b = results
            .iter()
            .find(|t| t.thread_id == "1000000002")
            .unwrap();
        // スレB's res1 also has suffix 83IP (different prefix is fine — suffix match wins).
        assert_eq!(thread_b.res.len(), 1);
        assert_eq!(thread_b.res[0].body, "本文3_83IP");
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
    fn validate_ng_word_accepts_both_kinds_with_a_non_empty_pattern() {
        assert!(validate_ng_word("text", "荒らし").is_ok());
        assert!(validate_ng_word("regex", "^荒ら.*し$").is_ok());
        // Regex syntax is the browser's to judge (see module docs): a pattern that
        // regex-lite could not compile is still stored as-is.
        assert!(validate_ng_word("regex", "(?<!foo)bar").is_ok());
    }

    #[test]
    fn validate_ng_word_rejects_empty_pattern() {
        assert!(validate_ng_word("text", "").is_err());
        assert!(validate_ng_word("regex", "").is_err());
    }

    #[test]
    fn validate_ng_word_rejects_unknown_kind() {
        assert!(validate_ng_word("glob", "荒らし").is_err());
        assert!(validate_ng_word("", "荒らし").is_err());
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

        replace_blob(&conn, "egg", "test", "1000000001", dat_a, 0).unwrap();
        replace_blob(&conn, "egg", "test", "1000000002", dat_b, 0).unwrap();

        // Call the real search function (as id_search does) for ID "target".
        let state = make_state(conn);
        let results =
            board_post_search(&state, "egg", "test", |r| r.id.as_deref() == Some("target"))
                .unwrap();

        // Only スレA has a matching post; スレB (no ID:target) is absent from the results.
        assert_eq!(results.len(), 1);
        let thread_a = &results[0];
        assert_eq!(thread_a.thread_id, "1000000001");
        assert_eq!(thread_a.res.len(), 1);
        assert_eq!(thread_a.res[0].body, "targetの本文");
    }
}
