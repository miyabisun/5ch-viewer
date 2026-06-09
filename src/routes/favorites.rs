use crate::error::AppError;
use crate::goch::dat::{parse_dat, title_from_dat};
use crate::goch::http::{self, DatFetch};
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

// End-of-thread thresholds (spec ch.7). dat size is in bytes.
const RES_WARN: i64 = 980;
const RES_DEAD: i64 = 1000;
const DAT_WARN: u64 = 900 * 1024;
const DAT_DEAD: u64 = 1024 * 1024;

type ThreadPath = Path<(String, String, String)>;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/favorites", get(list).post(add))
        .route("/api/favorites/{server}/{board}/{thread_id}", delete(remove))
        .route("/api/favorites/{server}/{board}/{thread_id}/dat", get(get_dat))
        .route("/api/favorites/{server}/{board}/{thread_id}/reload", post(reload))
        .route(
            "/api/favorites/{server}/{board}/{thread_id}/progress",
            patch(patch_progress),
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
    let board_name = http::fetch_board_name(&state.http, &server, &board).await;
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

async fn patch_progress(
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

    let raw: Option<Vec<u8>> = conn
        .query_row(
            "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
            |r| r.get(0),
        )
        .optional()?;

    let mut res = match raw {
        Some(bytes) => parse_dat(&http::decode_shift_jis(&bytes)),
        None => vec![],
    };
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

/// Fetches a Range diff and updates the dat (spec 6.3 / 6.4).
async fn reload(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<ReloadResponse>, AppError> {
    validate_ref(&server, &board, &thread_id)?;
    // 1. Fetch the previously stored dat_bytes.
    let (dat_bytes, old_res_count, last_tail): (u64, i64, Vec<u8>) = {
        let conn = state.db.lock().unwrap();
        let (db, rc) = conn
            .query_row(
                "SELECT dat_bytes, res_count FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![server, board, thread_id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("favorite not found".into()))?;
        // Last 6 bytes of the previous dat (for boundary matching). Empty if no dat stored.
        let tail = conn
            .query_row(
                "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![server, board, thread_id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|raw| raw[raw.len().saturating_sub(6)..].to_vec())
            .unwrap_or_default();
        (db as u64, rc, tail)
    };

    // 2. HTTP (await without holding the lock).
    let fetch =
        http::fetch_dat(&state.http, &server, &board, &thread_id, dat_bytes, &last_tail).await?;

    // 3. Update dat_blobs and obtain the new total byte count.
    let mut new_total: Option<u64> = {
        let conn = state.db.lock().unwrap();
        match &fetch {
            DatFetch::NotModified => None,
            DatFetch::Gone => {
                conn.execute(
                    "UPDATE favorites SET status='dead', updated_at=strftime('%s','now')
                     WHERE server=?1 AND board=?2 AND thread_id=?3",
                    params![server, board, thread_id],
                )?;
                None
            }
            DatFetch::Append { bytes, total } => {
                conn.execute(
                    "INSERT INTO dat_blobs (server, board, thread_id, raw) VALUES (?1,?2,?3,?4)
                     ON CONFLICT(server, board, thread_id) DO UPDATE SET raw = raw || excluded.raw",
                    params![server, board, thread_id, bytes],
                )?;
                Some(*total)
            }
            DatFetch::Replace { bytes, total } => {
                replace_blob(&conn, &server, &board, &thread_id, bytes)?;
                Some(*total)
            }
        }
    };

    // 3.5 Regression detection: if res_count after Append drops below the old value, the tail is
    //     contiguous but an intermediate post was physically deleted (a deletion that slipped past
    //     boundary matching). Repair via a full fetch.
    if matches!(fetch, DatFetch::Append { .. }) {
        let new_rc = {
            let conn = state.db.lock().unwrap();
            parse_dat(&read_blob_text(&conn, &server, &board, &thread_id)?).len() as i64
        };
        if new_rc < old_res_count {
            tracing::info!("[dat] res_count regressed {old_res_count} -> {new_rc}, full refetch");
            if let DatFetch::Replace { bytes, total } =
                http::fetch_dat(&state.http, &server, &board, &thread_id, 0, &[]).await?
            {
                let conn = state.db.lock().unwrap();
                replace_blob(&conn, &server, &board, &thread_id, &bytes)?;
                new_total = Some(total);
            }
        }
    }

    // 4. Recompute and update metadata (res_count / title / dat_bytes / status).
    let conn = state.db.lock().unwrap();
    if let Some(total) = new_total {
        let text = read_blob_text(&conn, &server, &board, &thread_id)?;
        let res_count = parse_dat(&text).len() as i64;
        let title = title_from_dat(&text).unwrap_or_default();
        let status = compute_status(res_count, total);
        conn.execute(
            "UPDATE favorites SET res_count=?4, dat_bytes=?5, status=?6,
             title = CASE WHEN title='' THEN ?7 ELSE title END,
             updated_at=strftime('%s','now')
             WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id, res_count, total as i64, status, title],
        )?;
    }

    let (res_count, read_res, status) = conn.query_row(
        "SELECT res_count, read_res, status FROM favorites
         WHERE server=?1 AND board=?2 AND thread_id=?3",
        params![server, board, thread_id],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)),
    )?;

    Ok(Json(ReloadResponse {
        res_count,
        read_res,
        status,
        updated: new_total.is_some(),
    }))
}

/// Reads the raw from dat_blobs, Shift_JIS-decodes it, and returns it.
fn read_blob_text(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<String, AppError> {
    let raw: Vec<u8> = conn.query_row(
        "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
        params![server, board, thread_id],
        |r| r.get(0),
    )?;
    Ok(http::decode_shift_jis(&raw))
}

/// Replaces the raw in dat_blobs entirely (inserts if absent).
fn replace_blob(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO dat_blobs (server, board, thread_id, raw) VALUES (?1,?2,?3,?4)
         ON CONFLICT(server, board, thread_id) DO UPDATE SET raw=excluded.raw",
        params![server, board, thread_id, bytes],
    )?;
    Ok(())
}

fn compute_status(res_count: i64, dat_bytes: u64) -> &'static str {
    if res_count >= RES_DEAD || dat_bytes >= DAT_DEAD {
        "dead"
    } else if res_count >= RES_WARN || dat_bytes >= DAT_WARN {
        "warned"
    } else {
        "active"
    }
}
