use crate::error::AppError;
use crate::goch::dat::{parse_dat, title_from_dat};
use crate::goch::http::{self, DatFetch};
use crate::goch::url::parse_thread_url;
use crate::models::{
    AddRequest, DatResponse, Favorite, ProgressRequest, RatingRequest, ReloadResponse,
};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

// 終了閾値（spec 第7章）。dat サイズはバイト単位。
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

/// 一覧（順不同。並べ替えはフロント）。
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

/// 追加。url 直接 か server/board/thread_id を受け、SETTING.TXT で board_name を取得。
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
    if let Some(url) = &req.url {
        let t = parse_thread_url(url)
            .ok_or_else(|| AppError::BadRequest(format!("invalid thread url: {url}")))?;
        return Ok((t.server, t.board, t.thread_id));
    }
    match (&req.server, &req.board, &req.thread_id) {
        (Some(s), Some(b), Some(t)) => Ok((s.clone(), b.clone(), t.clone())),
        _ => Err(AppError::BadRequest(
            "url または server/board/thread_id が必要".into(),
        )),
    }
}

async fn remove(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<Value>, AppError> {
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

/// 保存済み dat をレス配列にして返す。
async fn get_dat(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<DatResponse>, AppError> {
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
    // レス本文を HTML サニタイズ（XSS 対策。フロントは {@html} 前提）。
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

/// Range 差分取得して dat を更新（spec 6.3 / 6.4）。
async fn reload(State(state): State<AppState>, Path((server, board, thread_id)): ThreadPath) -> Result<Json<ReloadResponse>, AppError> {
    // 1. 前回保存した dat_bytes を取得。
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
        // 前回 dat の末尾6バイト（境界照合用）。dat 未保存なら空。
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

    // 2. HTTP（lock を握らずに await）。
    let fetch =
        http::fetch_dat(&state.http, &server, &board, &thread_id, dat_bytes, &last_tail).await?;

    // 3. dat_blobs を更新し、新しい総バイト数を得る。
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

    // 3.5 退行検出: Append 後の res_count が旧値を下回ったら、末尾連続だが中間レスが
    //     物理削除された（境界照合をすり抜けるあぼーん）。全取得でリペアする。
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

    // 4. メタ（res_count / title / dat_bytes / status）を再計算して更新。
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

/// dat_blobs の raw を Shift_JIS デコードして返す。
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

/// dat_blobs の raw を丸ごと置換（無ければ挿入）する。
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
