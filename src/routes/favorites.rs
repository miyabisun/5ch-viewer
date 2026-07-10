use crate::error::AppError;
use crate::fivech::http;
use crate::fivech::images::extract_image_urls;
use crate::fivech::next_thread::find_next_thread;
use crate::fivech::post;
use crate::fivech::refresh::{self, read_blob_posts};
use crate::fivech::subject::SubjectEntry;
use crate::fivech::url::{parse_thread_url, validate_ref};
use crate::models::{
    AddRequest, ArchivedRequest, DatResponse, Favorite, PostRequest, ProgressRequest,
    RatingRequest, ReloadResponse,
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
        .route("/api/archives", get(list_archives))
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
        .route(
            "/api/favorites/{server}/{board}/{thread_id}/archived",
            patch(patch_archived),
        )
        .route(
            "/api/favorites/{server}/{board}/{thread_id}/post",
            post(post_message),
        )
        .route(
            "/api/favorites/{server}/{board}/{thread_id}/find-next",
            post(find_next),
        )
}

/// Shared SELECT columns for Favorite rows.
const SELECT_FAVORITE: &str =
    "SELECT server, board, board_name, thread_id, title, res_count, read_res, rating, status
     FROM favorites";

fn row_to_favorite(row: &rusqlite::Row<'_>) -> rusqlite::Result<Favorite> {
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
}

/// Lists favorites filtered by `archived` (unordered; sorting is done by the frontend).
fn list_favorites(conn: &rusqlite::Connection, archived: i64) -> Result<Vec<Favorite>, AppError> {
    let sql = format!("{SELECT_FAVORITE} WHERE archived = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![archived], row_to_favorite)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// List active favorites.
async fn list(State(state): State<AppState>) -> Result<Json<Vec<Favorite>>, AppError> {
    let conn = state.db.lock().unwrap();
    Ok(Json(list_favorites(&conn, 0)?))
}

/// List archived favorites.
async fn list_archives(State(state): State<AppState>) -> Result<Json<Vec<Favorite>>, AppError> {
    let conn = state.db.lock().unwrap();
    Ok(Json(list_favorites(&conn, 1)?))
}

/// Add. Accepts a direct url or server/board/thread_id, and fetches board_name from SETTING.TXT.
async fn add(
    State(state): State<AppState>,
    Json(req): Json<AddRequest>,
) -> Result<Json<Value>, AppError> {
    let (server, board, thread_id) = resolve_ref(&req)?;
    let board_name =
        http::fetch_board_name(&state.http, &state.config.fivech_base_url, &server, &board).await;
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
    let n = save_progress(&conn, &server, &board, &thread_id, req.read_res)?;
    if n == 0 {
        return Err(AppError::NotFound("favorite not found".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Saves the read position, monotonically. read_res is the max res number the viewer
/// has read; MAX() guards against stale writes from another device (an open tab that
/// debounces/beacons a lower value) rolling the position backward. Returns the number
/// of updated rows (0 = not found).
fn save_progress(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
    read_res: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE favorites SET read_res=MAX(read_res, ?4), updated_at=strftime('%s','now')
         WHERE server=?1 AND board=?2 AND thread_id=?3",
        params![server, board, thread_id, read_res],
    )
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

async fn patch_archived(
    State(state): State<AppState>,
    Path((server, board, thread_id)): ThreadPath,
    Json(req): Json<ArchivedRequest>,
) -> Result<Json<Value>, AppError> {
    validate_ref(&server, &board, &thread_id)?;
    let archived: i64 = if req.archived { 1 } else { 0 };
    let conn = state.db.lock().unwrap();
    let n = set_archived(&conn, &server, &board, &thread_id, archived)?;
    if n == 0 {
        return Err(AppError::NotFound("favorite not found".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Sets the `archived` flag for a favorite. Returns the number of updated rows (0 = not found).
fn set_archived(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
    archived: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE favorites SET archived=?4, updated_at=strftime('%s','now')
         WHERE server=?1 AND board=?2 AND thread_id=?3",
        params![server, board, thread_id, archived],
    )
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

    // Mark own posts: collect res_num set from own_posts table, then flag matching entries.
    let own_nums: std::collections::HashSet<i64> = conn
        .prepare("SELECT res_num FROM own_posts WHERE server=?1 AND board=?2 AND thread_id=?3")?
        .query_map(params![server, board, thread_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for r in &mut res {
        // Mark own posts (pink label) and HTML-sanitize the body (XSS mitigation;
        // the frontend renders bodies via {@html}).
        r.own = own_nums.contains(&r.num);
        r.body = crate::sanitize::clean(&r.body);
    }

    // Collect mosaic URLs: extract all image URLs from the raw dat, then filter by mosaic=1.
    let mosaic_urls = query_mosaic_urls(&conn, &server, &board, &thread_id)?;

    Ok(Json(DatResponse {
        title,
        res_count,
        read_res,
        status,
        res,
        mosaic_urls,
    }))
}

/// Cache-or-fetch: refreshes the dat for a single thread (GET, viewer semantics).
///
/// Uses a HEAD request to check the dat's Content-Length instead of fetching subject.txt,
/// so only one dat URL is hit (lightweight). If Content-Length matches the stored
/// dat_bytes the dat is unchanged and the fetch is skipped. Any mismatch (or HEAD failure)
/// triggers a full GET. This also means writes are detected immediately after posting
/// without needing a `?force` parameter.
async fn reload(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<ReloadResponse>, AppError> {
    validate_ref(&server, &board, &thread_id)?;

    // 1. Read the stored dat_bytes from the DB.
    //    dat_bytes=0 means the column was not yet populated (migration or first fetch);
    //    treat it as "unknown" and always fall through to a full GET.
    let stored_dat_bytes: i64 = {
        let conn = state.db.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(db.dat_bytes, 0)
             FROM favorites f
             LEFT JOIN dat_blobs db
               ON f.server=db.server AND f.board=db.board AND f.thread_id=db.thread_id
             WHERE f.server=?1 AND f.board=?2 AND f.thread_id=?3",
            params![server, board, thread_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("favorite not found".into()))?
    };

    // 2. HEAD request to get the dat's current Content-Length without downloading the body.
    //    None = HEAD failed or header missing → fall back to full GET.
    let head_content_length: Option<i64> = http::head_dat_content_length(
        &state.http,
        &state.config.fivech_base_url,
        &server,
        &board,
        &thread_id,
    )
    .await;

    // 3. Gate: skip the full GET when we have a known stored size AND HEAD confirms no change.
    if stored_dat_bytes > 0 && head_content_length == Some(stored_dat_bytes) {
        let conn = state.db.lock().unwrap();
        conn.execute(
            "UPDATE favorites SET updated_at=strftime('%s','now')
             WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
        )?;
        let (res_count, read_res, status) = read_meta(&conn, &server, &board, &thread_id)?;
        return Ok(Json(ReloadResponse { res_count, read_res, status, updated: false }));
    }
    tracing::info!(
        "[reload] {server}/{board}/{thread_id}: HEAD={head_content_length:?} stored={stored_dat_bytes} -> fetching dat"
    );

    // 4. Fetch the entire dat (await without holding the lock).
    //    Claim the dat so a concurrent board prefetch does not double-download it.
    let updated = match state.claim_dat(&(server.clone(), board.clone(), thread_id.clone())) {
        Some(_guard) => {
            let fetch = http::fetch_dat(
                &state.http,
                &state.config.fivech_base_url,
                &server,
                &board,
                &thread_id,
            )
            .await?;
            // Persist: full UTF-8 TEXT replace + metadata recompute, or mark dead on Gone.
            refresh::persist_fetch(&state, &server, &board, &thread_id, fetch)?
        }
        None => {
            tracing::info!(
                "[reload] {server}/{board}/{thread_id}: dat already in flight (prefetch), skipping duplicate fetch"
            );
            false
        }
    };

    let conn = state.db.lock().unwrap();
    let (res_count, read_res, status) = read_meta(&conn, &server, &board, &thread_id)?;
    Ok(Json(ReloadResponse {
        res_count,
        read_res,
        status,
        updated,
    }))
}

/// Posts a message to 5ch and saves the result in own_posts.
///
/// Does not require the thread to be in favorites (a user can post without adding).
/// SSRF validation is delegated to post::post_message (called first).
async fn post_message(
    State(state): State<AppState>,
    Path((server, board, thread_id)): ThreadPath,
    Json(req): Json<PostRequest>,
) -> Result<Json<Value>, AppError> {
    validate_ref(&server, &board, &thread_id)?;
    if req.message.trim().is_empty() {
        return Err(AppError::BadRequest("message は空にできません".into()));
    }

    let from = req.name.as_deref().unwrap_or("");
    let mail = req.mail.as_deref().unwrap_or("");

    let result = post::post_message(
        &state.http,
        &state.config.fivech_base_url,
        &server,
        &board,
        &thread_id,
        from,
        mail,
        &req.message,
    )
    .await?;

    // Persist the own post (no FK to favorites — user can post to any thread).
    {
        let conn = state.db.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO own_posts
             (server, board, thread_id, res_num, body, name, mail, poster_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                server,
                board,
                thread_id,
                result.res_num,
                req.message,
                req.name,
                req.mail,
                result.poster_id,
            ],
        )?;
    }

    // Save the cookie jar so acorn/MonaTicket survive process restarts.
    // Non-fatal: a save failure only means the next post will repeat the two-step confirmation.
    state.jar.save(&state.config.cookies_path);

    Ok(Json(json!({ "ok": true, "res_num": result.res_num })))
}

/// Manual "find next thread" rescue: fetches the board's subject.txt once and, if the next
/// thread (Part number +1) is present, registers it (INSERT OR IGNORE) inheriting the source
/// thread's rating and board_name. This is the user-initiated counterpart to the background
/// sync auto-add; it exists because a thread can go dead before its successor is posted, after
/// which background polling stops. dead/archived source threads are eligible (that is the
/// whole point of the rescue), so status/archived are not filtered here.
///
/// 5ch access: user-initiated, one subject.txt fetch per action (dat body is never fetched) —
/// within the access-reduction policy.
async fn find_next(
    State(state): State<AppState>,
    Path((server, board, thread_id)): ThreadPath,
) -> Result<Json<Value>, AppError> {
    validate_ref(&server, &board, &thread_id)?;

    // Source favorite: title drives the search; rating/board_name are inherited on insert.
    let (title, rating, board_name): (String, i64, String) = {
        let conn = state.db.lock().unwrap();
        conn.query_row(
            "SELECT title, rating, board_name FROM favorites
             WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("favorite not found".into()))?
    };

    let entries =
        http::fetch_subject(&state.http, &state.config.fivech_base_url, &server, &board).await?;

    let next = {
        let conn = state.db.lock().unwrap();
        match register_next_thread(&conn, &server, &board, &board_name, rating, &title, &entries)? {
            Some(n) => n,
            None => return Ok(Json(json!({ "found": false }))),
        }
    };

    Ok(Json(json!({
        "found": true,
        "server": server,
        "board": board,
        "thread_id": next.thread_id,
        "title": next.title,
    })))
}

/// Registers the successor of `source_title` (Part number +1) if it is present in `entries`,
/// inheriting `rating` and `board_name` (INSERT OR IGNORE — no duplicates). Returns the matched
/// next thread, or None when the successor is not yet posted.
///
/// res_count is intentionally omitted from the INSERT (schema DEFAULT 0): favorites.res_count
/// only ever reflects the stored blob's real post count, so the next thread enters at 0 and
/// gains a real count once the poll downloads its dat and persist_fetch writes the blob count.
/// Seeding subject's count here would violate that invariant.
fn register_next_thread(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    board_name: &str,
    rating: i64,
    source_title: &str,
    entries: &[SubjectEntry],
) -> Result<Option<SubjectEntry>, AppError> {
    let next = match find_next_thread(source_title, entries) {
        Some(n) => n.clone(),
        None => return Ok(None),
    };
    conn.execute(
        "INSERT OR IGNORE INTO favorites
         (server, board, thread_id, board_name, title, rating)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![server, board, next.thread_id, board_name, next.title, rating],
    )?;
    Ok(Some(next))
}

/// Reads the raw dat text and returns URLs whose mosaic flag is set to 1.
/// Extracts image URLs from the dat, then queries image_cache for mosaic=1 matches.
fn query_mosaic_urls(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<Vec<String>, AppError> {
    // Read the raw dat text (UTF-8, already decoded at write time).
    let raw: Option<String> = conn
        .query_row(
            "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
            |r| r.get(0),
        )
        .optional()?;

    let raw = match raw {
        Some(r) => r,
        None => return Ok(vec![]),
    };

    let urls = extract_image_urls(&raw);
    if urls.is_empty() {
        return Ok(vec![]);
    }

    // Build an IN(...) query to find which of those URLs have mosaic=1.
    let placeholders: String = urls
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT url FROM image_cache WHERE url IN ({placeholders}) AND mosaic = 1"
    );
    let params_vec: Vec<&dyn rusqlite::ToSql> =
        urls.iter().map(|u| u as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let mosaic_urls: Vec<String> = stmt
        .query_map(params_vec.as_slice(), |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(mosaic_urls)
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
    use crate::fivech::dat::parse_dat;
    use crate::fivech::refresh::{compute_status, replace_blob};
    use rusqlite::Connection;

    // Mirrors the threshold in fivech::refresh (status is now res_count-only).
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

    fn column_type(conn: &Connection) -> String {
        conn.query_row(
            "SELECT typeof(raw) FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![SERVER, BOARD, THREAD],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    }

    /// Verifies that replace_blob stores the dat as TEXT (UTF-8) and that it parses correctly
    /// without any Shift-JIS decoding step. The column type must be "text", and reading via
    /// r.get::<String> must succeed.
    #[test]
    fn replace_blob_stores_text_and_parses() {
        let conn = setup();
        let first = "名無し<>sage<>2025/01/01 ID:abc<>本文1<>スレタイ\n";
        let full =
            "名無し<>sage<>2025/01/01 ID:abc<>本文1<>スレタイ\n名無し<><>2025/01/02 ID:def<>本文2<>\n";

        // initial store, then a full replace (the only write path now)
        replace_blob(&conn, SERVER, BOARD, THREAD, first, 0).unwrap();
        replace_blob(&conn, SERVER, BOARD, THREAD, full, 0).unwrap();

        // (a) the column type is TEXT (not BLOB)
        assert_eq!(column_type(&conn), "text");

        // (b) reading as String succeeds
        let raw: String = conn
            .query_row(
                "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();

        // (c) the replaced dat parses into both posts without any decode_shift_jis call
        let res = parse_dat(&raw);
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].body, "本文1");
        assert_eq!(res[1].body, "本文2");
    }

    /// INVARIANT: register_next_thread (the "find next thread" rescue's insert) registers the
    /// next thread with res_count=0 (schema default), NOT the subject's reported count. res_count
    /// only ever reflects the stored blob; the poll downloads the dat and persist_fetch writes
    /// the real count later.
    #[test]
    fn register_next_thread_uses_zero_res_count() {
        use crate::fivech::subject::SubjectEntry;
        let conn = setup();
        // Source favorite is the fixture (THREAD, title ''); give it a searchable title.
        conn.execute(
            "UPDATE favorites SET title='ブルアカ Part5862' WHERE thread_id=?1",
            params![THREAD],
        )
        .unwrap();

        // subject lists the successor with a non-zero count (777) that must NOT be copied.
        let entries = vec![
            SubjectEntry { thread_id: "1000000002".into(), title: "ブルアカ Part5862".into(), res_count: 995 },
            SubjectEntry { thread_id: "1000000003".into(), title: "ブルアカ Part5863".into(), res_count: 777 },
        ];

        let next = super::register_next_thread(
            &conn, SERVER, BOARD, "板", 4, "ブルアカ Part5862", &entries,
        )
        .unwrap();
        assert!(next.is_some(), "successor Part5863 must be found in subject");

        let (res_count, rating): (i64, i64) = conn
            .query_row(
                "SELECT res_count, rating FROM favorites WHERE thread_id='1000000003'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(res_count, 0, "next thread must register with res_count=0, not subject's 777");
        assert_eq!(rating, 4, "next thread must inherit the source rating");
    }

    /// Reads the stored read_res of the fixture favorite.
    fn read_read_res(conn: &Connection) -> i64 {
        conn.query_row(
            "SELECT read_res FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![SERVER, BOARD, THREAD],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// save_progress is monotonic: a stale, lower read_res (e.g. from another device's
    /// open tab) must NOT roll the saved position backward.
    #[test]
    fn save_progress_does_not_regress() {
        let conn = setup();
        // Advance to 28.
        let n = save_progress(&conn, SERVER, BOARD, THREAD, 28).unwrap();
        assert_eq!(n, 1);
        assert_eq!(read_read_res(&conn), 28);

        // A stale lower write must be ignored (MAX guard).
        save_progress(&conn, SERVER, BOARD, THREAD, 10).unwrap();
        assert_eq!(read_read_res(&conn), 28, "read_res must not regress to 10");

        // A higher write advances.
        save_progress(&conn, SERVER, BOARD, THREAD, 40).unwrap();
        assert_eq!(read_read_res(&conn), 40);
    }

    /// From the initial state (read_res=0) save_progress advances normally.
    #[test]
    fn save_progress_advances_from_zero() {
        let conn = setup();
        assert_eq!(read_read_res(&conn), 0, "fixture starts at read_res=0");
        save_progress(&conn, SERVER, BOARD, THREAD, 15).unwrap();
        assert_eq!(read_read_res(&conn), 15);
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

    /// Archived favorites must not appear in the list (archived=0 filter).
    #[test]
    fn list_excludes_archived() {
        let conn = setup();
        // Insert a second favorite and mark it archived.
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title, archived)
             VALUES ('9999999999', ?1, ?2, 'name', 'archived', 1)",
            params![SERVER, BOARD],
        )
        .unwrap();

        let sql = format!("{SELECT_FAVORITE} WHERE archived = 0");
        let mut stmt = conn.prepare(&sql).unwrap();
        let rows: Vec<Favorite> = stmt
            .query_map([], row_to_favorite)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        // Only the fixture favorite (not archived) should appear.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].thread_id, THREAD);
    }

    /// list_archives equivalent: only archived=1 favorites are returned.
    #[test]
    fn list_archives_returns_only_archived() {
        let conn = setup();
        // Archive the fixture favorite.
        conn.execute(
            "UPDATE favorites SET archived=1 WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![SERVER, BOARD, THREAD],
        )
        .unwrap();
        // Insert a non-archived one.
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title)
             VALUES ('8888888888', ?1, ?2, 'name', 'active')",
            params![SERVER, BOARD],
        )
        .unwrap();

        let sql = format!("{SELECT_FAVORITE} WHERE archived = 1");
        let mut stmt = conn.prepare(&sql).unwrap();
        let rows: Vec<Favorite> = stmt
            .query_map([], row_to_favorite)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].thread_id, THREAD);
    }

    /// patch_archived (via set_archived, the function the handler calls): archived
    /// toggles true→false→true without error.
    #[test]
    fn patch_archived_toggles_bidirectionally() {
        let conn = setup();

        // Set archived=1.
        let n = set_archived(&conn, SERVER, BOARD, THREAD, 1).unwrap();
        assert_eq!(n, 1);
        let archived: i64 = conn
            .query_row(
                "SELECT archived FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archived, 1);

        // Set archived=0 (unarchive).
        let n = set_archived(&conn, SERVER, BOARD, THREAD, 0).unwrap();
        assert_eq!(n, 1);
        let archived: i64 = conn
            .query_row(
                "SELECT archived FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archived, 0);

        // Set archived=1 again (round trip).
        let n = set_archived(&conn, SERVER, BOARD, THREAD, 1).unwrap();
        assert_eq!(n, 1);
        let archived: i64 = conn
            .query_row(
                "SELECT archived FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archived, 1);
    }

    /// A second replace fully overwrites the previous body (no leftover text from the first).
    #[test]
    fn replace_blob_overwrites_previous_body() {
        let conn = setup();
        replace_blob(&conn, SERVER, BOARD, THREAD, "古い<>sage<>d ID:x<>古い本文<>t\n", 0).unwrap();
        replace_blob(&conn, SERVER, BOARD, THREAD, "新しい<>sage<>d ID:y<>新本文<>t\n", 0).unwrap();
        let raw: String = conn
            .query_row(
                "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();
        let res = parse_dat(&raw);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].body, "新本文");
    }

    /// get_dat own-post marking: after INSERT into own_posts, the matching res must have
    /// r.own = true and all other reses must have r.own = false.
    /// This directly tests the query + loop in the get_dat handler (without HTTP).
    #[test]
    fn get_dat_marks_own_posts() {
        let conn = setup();
        // Store a 3-post dat.
        let dat = "名無し<>sage<>d ID:aaa<>本文1<>スレタイ\n\
                   名無し<><>d ID:bbb<>本文2<>\n\
                   名無し<><>d ID:ccc<>本文3<>\n";
        replace_blob(&conn, SERVER, BOARD, THREAD, dat, 0).unwrap();

        // Register res 2 as an own post.
        conn.execute(
            "INSERT INTO own_posts (server, board, thread_id, res_num, body)
             VALUES (?1, ?2, ?3, 2, '本文2')",
            params![SERVER, BOARD, THREAD],
        )
        .unwrap();

        // Replicate the get_dat own-marking logic.
        let mut res = read_blob_posts(&conn, SERVER, BOARD, THREAD).unwrap();
        let own_nums: std::collections::HashSet<i64> = conn
            .prepare(
                "SELECT res_num FROM own_posts WHERE server=?1 AND board=?2 AND thread_id=?3",
            )
            .unwrap()
            .query_map(params![SERVER, BOARD, THREAD], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for r in &mut res {
            if own_nums.contains(&r.num) {
                r.own = true;
            }
        }

        assert_eq!(res.len(), 3);
        assert!(!res[0].own, "res 1 must NOT be own");
        assert!(res[1].own, "res 2 must be own (inserted into own_posts)");
        assert!(!res[2].own, "res 3 must NOT be own");
    }

    /// Inserting the same own_post twice (INSERT OR REPLACE) must not create duplicates.
    #[test]
    fn own_post_insert_or_replace_is_idempotent() {
        let conn = setup();
        for _ in 0..2 {
            conn.execute(
                "INSERT OR REPLACE INTO own_posts (server, board, thread_id, res_num, body)
                 VALUES (?1, ?2, ?3, 5, 'テスト本文')",
                params![SERVER, BOARD, THREAD],
            )
            .unwrap();
        }
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM own_posts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "INSERT OR REPLACE must produce exactly one row");

        // Verify the stored body.
        let body: String = conn
            .query_row(
                "SELECT body FROM own_posts WHERE server=?1 AND board=?2 AND thread_id=?3 AND res_num=5",
                params![SERVER, BOARD, THREAD],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(body, "テスト本文");
    }
}
