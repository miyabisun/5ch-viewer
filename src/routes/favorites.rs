use crate::error::AppError;
use crate::goch::http;
use crate::goch::refresh::{self, read_blob_posts};
use crate::goch::url::{parse_thread_url, validate_ref};
use crate::models::{
    AddRequest, DatResponse, Favorite, ProgressRequest, RatingRequest, ReloadResponse,
};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

type ThreadPath = Path<(String, String, String)>;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/favorites", get(list).post(add))
        .route("/api/favorites/refresh", post(refresh_all))
        .route("/api/favorites/{server}/{board}/{thread_id}", delete(remove))
        .route("/api/favorites/{server}/{board}/{thread_id}/dat", get(get_dat))
        .route("/api/favorites/{server}/{board}/{thread_id}/reload", get(reload))
        .route(
            "/api/favorites/{server}/{board}/{thread_id}/progress",
            // Read position: GET fetches, POST saves (POST so sendBeacon can be used on unload).
            get(get_progress).post(post_progress),
        )
        .route(
            "/api/favorites/{server}/{board}/{thread_id}/rating",
            patch(patch_rating),
        )
}

/// List (unordered; sorting is done by the frontend).
async fn list(State(state): State<AppState>) -> Result<Json<Vec<Favorite>>, AppError> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT server, board, board_name, thread_id, title, res_count, read_res, rating, status
         FROM favorites",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Favorite {
                server: row.get(0)?,
                board: row.get(1)?,
                board_name: row.get(2)?,
                thread_id: row.get(3)?,
                title: row.get(4)?,
                res_count: row.get(5)?,
                read_res: row.get(6)?,
                rating: row.get(7)?,
                status: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(rows))
}

/// Add. Accepts a direct url or server/board/thread_id, and fetches board_name from SETTING.TXT.
async fn add(
    State(state): State<AppState>,
    Json(req): Json<AddRequest>,
) -> Result<Json<Value>, AppError> {
    let (server, board, thread_id) = resolve_ref(&req)?;
    let board_name =
        http::fetch_board_name(&state.http, &state.config.goch_base_url, &server, &board).await;
    let title = req.title.unwrap_or_default();
    {
        let conn = state.db.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO favorites (server, board, thread_id, board_name, title)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![server, board, thread_id, board_name, title],
        )?;
    }
    Ok(Json(json!({
        "ok": true, "server": server, "board": board, "thread_id": thread_id,
    })))
}

/// Refreshes all favorites with one subject.txt read per board, downloading every grown
/// dat in the background. Returns immediately so the list display is never blocked: the
/// heavy work (subject + bulk dat) runs in a spawned task. Failures are logged inside
/// `refresh_board` (never silently swallowed).
async fn refresh_all(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    // Distinct (server, board) pairs that have at least one non-dead favorite.
    let boards: Vec<(String, String)> = {
        let conn = state.db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT DISTINCT server, board FROM favorites WHERE status != 'dead'")?;
        let boards = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        boards
    };

    let count = boards.len();
    tokio::spawn(async move {
        for (server, board) in boards {
            let n = refresh::refresh_board(&state, &server, &board).await;
            tracing::info!("[refresh] {server}/{board}: {n} dat(s) updated");
        }
    });

    // The board count is informational; the work continues in the background.
    Ok(Json(json!({ "ok": true, "boards": count })))
}

fn resolve_ref(req: &AddRequest) -> Result<(String, String, String), AppError> {
    let (server, board, thread_id) = if let Some(url) = &req.url {
        let t = parse_thread_url(url)
            .ok_or_else(|| AppError::BadRequest(format!("invalid thread url: {url}")))?;
        (t.server, t.board, t.thread_id)
    } else {
        match (&req.server, &req.board, &req.thread_id) {
            (Some(s), Some(b), Some(t)) => (s.clone(), b.clone(), t.clone()),
            _ => {
                return Err(AppError::BadRequest(
                    "url または server/board/thread_id が必要".into(),
                ))
            }
        }
    };
    // SSRF mitigation: strictly validate user input (URL/direct/search result).
    validate_ref(&server, &board, &thread_id)?;
    Ok((server, board, thread_id))
}

async fn remove(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<Value>, AppError> {
    validate_ref(&server, &board, &thread_id)?;
    let conn = state.db.lock().unwrap();
    let n = conn.execute(
        "DELETE FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
        params![server, board, thread_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("favorite not found".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Read position: GET the saved read_res for a thread (viewer semantics).
async fn get_progress(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<Value>, AppError> {
    validate_ref(&server, &board, &thread_id)?;
    let conn = state.db.lock().unwrap();
    let read_res: i64 = conn
        .query_row(
            "SELECT read_res FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("favorite not found".into()))?;
    Ok(Json(json!({ "read_res": read_res })))
}

/// Read position: POST saves read_res. POST (not PATCH) so the client can use
/// navigator.sendBeacon on unload, which only issues POST.
async fn post_progress(
    State(state): State<AppState>,
    Path((server, board, thread_id)): ThreadPath,
    Json(req): Json<ProgressRequest>,
) -> Result<Json<Value>, AppError> {
    validate_ref(&server, &board, &thread_id)?;
    let conn = state.db.lock().unwrap();
    let n = conn.execute(
        "UPDATE favorites SET read_res=?4, updated_at=strftime('%s','now')
         WHERE server=?1 AND board=?2 AND thread_id=?3",
        params![server, board, thread_id, req.read_res],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("favorite not found".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

async fn patch_rating(
    State(state): State<AppState>,
    Path((server, board, thread_id)): ThreadPath,
    Json(req): Json<RatingRequest>,
) -> Result<Json<Value>, AppError> {
    validate_ref(&server, &board, &thread_id)?;
    if !(0..=5).contains(&req.rating) {
        return Err(AppError::BadRequest("rating は 0〜5".into()));
    }
    let conn = state.db.lock().unwrap();
    let n = conn.execute(
        "UPDATE favorites SET rating=?4, updated_at=strftime('%s','now')
         WHERE server=?1 AND board=?2 AND thread_id=?3",
        params![server, board, thread_id, req.rating],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("favorite not found".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Returns the stored dat as an array of posts.
async fn get_dat(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<DatResponse>, AppError> {
    validate_ref(&server, &board, &thread_id)?;
    let conn = state.db.lock().unwrap();
    let (title, res_count, read_res, status) = conn
        .query_row(
            "SELECT title, res_count, read_res, status FROM favorites
             WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("favorite not found".into()))?;

    let mut res = read_blob_posts(&conn, &server, &board, &thread_id)?;
    // HTML-sanitize post bodies (XSS mitigation; the frontend uses {@html}).
    for r in &mut res {
        r.body = crate::sanitize::clean(&r.body);
    }
    Ok(Json(DatResponse {
        title,
        res_count,
        read_res,
        status,
        res,
    }))
}

/// Cache-or-fetch: refreshes the dat for a thread (GET, viewer semantics).
///
/// Take-or-skip is gated by subject.txt's res_count to keep 5ch load low:
/// if the count has not grown, the dat is NOT fetched (only local metadata is updated).
/// When a fetch is needed, the entire dat is GET (no Range / no diff) and the BLOB is
/// fully replaced, so the stored bytes can never be corrupted into TEXT.
async fn reload(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<ReloadResponse>, AppError> {
    validate_ref(&server, &board, &thread_id)?;

    // 1. Determine how many posts we actually hold in the stored dat. The gate must
    //    compare subject.txt against the BLOB, not against `favorites.res_count`:
    //    those two can drift apart (e.g. res_count was bumped to the subject value
    //    while a prior fetch stored fewer posts). Trusting res_count then makes the
    //    gate believe "no growth" forever and the blob never catches up. Counting the
    //    parsed posts in the blob is self-healing.
    let stored_res_count: i64 = {
        let conn = state.db.lock().unwrap();
        // Existence check (404 a removed favorite, not a missing blob).
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![server, board, thread_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !exists {
            return Err(AppError::NotFound("favorite not found".into()));
        }
        read_blob_posts(&conn, &server, &board, &thread_id)?.len() as i64
    };

    // 2. Check subject.txt to see how many posts the board reports (load-reduction gate).
    //    On subject failure, fall back to fetching the dat (cannot prove "no change").
    let subject_count: Option<i64> = match http::fetch_subject(
        &state.http,
        &state.config.goch_base_url,
        &server,
        &board,
    )
    .await
    {
        Ok(entries) => entries
            .iter()
            .find(|e| e.thread_id == thread_id)
            .map(|e| e.res_count),
        Err(e) => {
            tracing::warn!("[reload] subject {server}/{board}: {e}");
            None
        }
    };

    // 3. Decide whether to fetch the dat (shared gate: only when subject reports growth).
    if !refresh::needs_fetch(subject_count, stored_res_count) {
        // No new posts: skip the 5ch dat fetch entirely. Only touch updated_at so the
        // stored status (res_count-derived warned/dead) is preserved unchanged.
        let conn = state.db.lock().unwrap();
        conn.execute(
            "UPDATE favorites SET updated_at=strftime('%s','now')
             WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
        )?;
        let (res_count, read_res, status) = read_meta(&conn, &server, &board, &thread_id)?;
        return Ok(Json(ReloadResponse {
            res_count,
            read_res,
            status,
            updated: false,
        }));
    }

    // 4. Fetch the entire dat (await without holding the lock).
    //    Log the decision so a "stuck thread" can be diagnosed from the server logs:
    //    subject vs stored counts and whether a fetch is triggered.
    tracing::info!(
        "[reload] {server}/{board}/{thread_id}: subject={subject_count:?} stored={stored_res_count} -> fetching dat"
    );

    // Claim the dat so a concurrent board prefetch does not also download it. If the prefetch
    // already holds it, wait for it to finish, then serve the (now refreshed) stored dat
    // instead of a duplicate fetch — the prefetch's full-replace is authoritative.
    let updated = match state.claim_dat(&(server.clone(), board.clone(), thread_id.clone())) {
        Some(_guard) => {
            let fetch = http::fetch_dat(
                &state.http,
                &state.config.goch_base_url,
                &server,
                &board,
                &thread_id,
            )
            .await?;
            // Persist: a full BLOB replace + metadata recompute, or mark dead on Gone.
            refresh::persist_fetch(&state, &server, &board, &thread_id, fetch)?
        }
        None => {
            tracing::info!(
                "[reload] {server}/{board}/{thread_id}: dat already in flight (prefetch), skipping duplicate fetch"
            );
            false
        }
    };

    // 5. Kick off a background prefetch of the rest of this board's favorites so opening
    //    one thread warms the others (the in-flight guard skips this thread). Best-effort.
    spawn_board_prefetch(&state, &server, &board);

    let conn = state.db.lock().unwrap();
    let (res_count, read_res, status) = read_meta(&conn, &server, &board, &thread_id)?;
    Ok(Json(ReloadResponse {
        res_count,
        read_res,
        status,
        updated,
    }))
}

/// Spawns a background board-level prefetch (one subject.txt + bulk dat for grown threads).
/// Non-blocking; failures are logged inside `refresh_board`.
fn spawn_board_prefetch(state: &AppState, server: &str, board: &str) {
    let state = state.clone();
    let server = server.to_string();
    let board = board.to_string();
    tokio::spawn(async move {
        let n = refresh::refresh_board(&state, &server, &board).await;
        if n > 0 {
            tracing::info!("[reload] prefetch {server}/{board}: {n} dat(s) updated");
        }
    });
}

/// Reads the favorite's current res_count / read_res / status.
fn read_meta(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<(i64, i64, String), AppError> {
    Ok(conn.query_row(
        "SELECT res_count, read_res, status FROM favorites
         WHERE server=?1 AND board=?2 AND thread_id=?3",
        params![server, board, thread_id],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goch::dat::parse_dat;
    use crate::goch::refresh::{compute_status, replace_blob};
    use rusqlite::Connection;

    // Mirrors the threshold in goch::refresh (status is now res_count-only).
    const RES_WARN: i64 = 980;

    const SERVER: &str = "egg";
    const BOARD: &str = "applism";
    const THREAD: &str = "1771127145";

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title)
             VALUES (?1, ?2, ?3, 'name', '')",
            params![THREAD, SERVER, BOARD],
        )
        .unwrap();
        conn
    }

    /// Encodes UTF-8 text to Shift_JIS bytes (as a real dat is stored).
    fn sjis(text: &str) -> Vec<u8> {
        let (cow, _, _) = encoding_rs::SHIFT_JIS.encode(text);
        cow.into_owned()
    }

    fn column_type(conn: &Connection) -> String {
        conn.query_row(
            "SELECT typeof(raw) FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![SERVER, BOARD, THREAD],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    }

    /// Regression: a full replace must keep `raw` as a BLOB so that later reads
    /// (`r.get::<Vec<u8>>`) succeed and the dat parses. Because we always store the
    /// whole body (never SQLite `||` concatenation), the column can never become TEXT.
    #[test]
    fn replace_blob_keeps_blob_type_and_parses() {
        let conn = setup();
        let first = sjis("名無し<>sage<>2025/01/01 ID:abc<>本文1<>スレタイ\n");
        let full = sjis(
            "名無し<>sage<>2025/01/01 ID:abc<>本文1<>スレタイ\n名無し<><>2025/01/02 ID:def<>本文2<>\n",
        );

        // initial store, then a full replace (the only write path now)
        replace_blob(&conn, SERVER, BOARD, THREAD, &first).unwrap();
        replace_blob(&conn, SERVER, BOARD, THREAD, &full).unwrap();

        // (a) the column type stays BLOB
        assert_eq!(column_type(&conn), "blob");

        // (b) reading as bytes succeeds (would error "Invalid column type Text" if TEXT)
        let raw: Vec<u8> = conn
            .query_row(
                "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();

        // (c) the replaced dat parses into both posts
        let res = parse_dat(&http::decode_shift_jis(&raw));
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].body, "本文1");
        assert_eq!(res[1].body, "本文2");
    }

    /// Reads the stored status of the fixture favorite.
    fn read_status(conn: &Connection) -> String {
        conn.query_row(
            "SELECT status FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![SERVER, BOARD, THREAD],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    }

    /// Regression: when subject reports no new posts, the reload skip path must NOT recompute
    /// status. A thread that turned 'warned' by res_count must keep 'warned' across no-growth
    /// reloads. The skip path only touches updated_at, so the stored status is preserved.
    #[test]
    fn skip_path_preserves_res_count_derived_status() {
        let conn = setup();
        // A near-full thread: res_count is in the warned range.
        let res_count: i64 = RES_WARN;
        let status = compute_status(res_count);
        assert_eq!(status, "warned", "fixture must be warned");
        conn.execute(
            "UPDATE favorites SET res_count=?4, status=?5
             WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![SERVER, BOARD, THREAD, res_count, status],
        )
        .unwrap();

        // Replicate the reload skip path (no subject growth): touch updated_at only.
        conn.execute(
            "UPDATE favorites SET updated_at=strftime('%s','now')
             WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![SERVER, BOARD, THREAD],
        )
        .unwrap();

        assert_eq!(read_status(&conn), "warned");
    }

    /// Counts the posts parsed from the stored blob (mirrors the reload gate's baseline).
    fn stored_count(conn: &Connection) -> i64 {
        read_blob_posts(conn, SERVER, BOARD, THREAD).unwrap().len() as i64
    }

    /// Regression (the "stuck at 111" bug): the reload gate must compare subject.txt
    /// against the actual stored blob, not against `favorites.res_count`. When res_count
    /// drifted ahead of the blob (res_count=117 but the blob holds only 111 posts), a
    /// gate keyed on res_count computes `117 > 117 == false` and never re-fetches, so the
    /// blob is stuck. Keying on the blob count yields `117 > 111 == true` (fetch needed).
    #[test]
    fn reload_gate_keys_on_blob_count_not_metadata() {
        let conn = setup();
        // Blob with 2 posts, but metadata res_count bumped to 3 (drifted ahead).
        let blob = sjis("名無し<>sage<>d ID:a<>本文1<>スレタイ\n名無し<><>d ID:b<>本文2<>\n");
        replace_blob(&conn, SERVER, BOARD, THREAD, &blob).unwrap();
        conn.execute(
            "UPDATE favorites SET res_count=3 WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![SERVER, BOARD, THREAD],
        )
        .unwrap();

        // The blob baseline is 2 (the truth), not the metadata's 3.
        assert_eq!(stored_count(&conn), 2);

        // Subject reports 3. Gate keyed on the blob -> needs_fetch (3 > 2).
        let subject_count = 3i64;
        assert!(
            subject_count > stored_count(&conn),
            "gate must fetch when the blob is behind subject, even if res_count says otherwise",
        );
        // Sanity: the broken gate (keyed on res_count=3) would NOT fetch.
        let metadata_res_count: i64 = conn
            .query_row(
                "SELECT res_count FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!(subject_count > metadata_res_count));
    }

    /// A second replace fully overwrites the previous body (no leftover bytes appended).
    #[test]
    fn replace_blob_overwrites_previous_body() {
        let conn = setup();
        replace_blob(&conn, SERVER, BOARD, THREAD, &sjis("古い<>sage<>d ID:x<>古い本文<>t\n")).unwrap();
        replace_blob(&conn, SERVER, BOARD, THREAD, &sjis("新しい<>sage<>d ID:y<>新本文<>t\n")).unwrap();
        let raw: Vec<u8> = conn
            .query_row(
                "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();
        let res = parse_dat(&http::decode_shift_jis(&raw));
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].body, "新本文");
    }
}
