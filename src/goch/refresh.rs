//! Board-level prefetch: refresh every favorite of a board with a single subject.txt read.
//!
//! subject.txt reports the res_count of *all* threads on a board at once, so one read tells
//! us which favorites grew. We then bulk-download only the dats that actually changed and
//! replace their blobs. This keeps the 5ch connection cost (the dominant term) minimal:
//! subject.txt is hit once per board (never once per thread), and dats are fetched only for
//! grown threads.
//!
//! The same path powers both the explicit list refresh (all boards) and the open-time
//! prefetch (the opened thread's board). An in-flight guard (`AppState::claim_dat`) prevents
//! the foreground viewer reload and this background prefetch from downloading the same dat
//! twice.

use crate::error::AppError;
use crate::goch::dat::{parse_dat, title_from_dat};
use crate::goch::http::{self, DatFetch};
use crate::state::AppState;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

// End-of-thread thresholds (spec ch.7). Status is derived from res_count only;
// dat byte size is no longer used for status (removed: dat_bytes, DAT_WARN, DAT_DEAD).
const RES_WARN: i64 = 980;
const RES_DEAD: i64 = 1000;

/// The reload/prefetch gate: fetch the dat only when subject.txt proves the thread grew past
/// the count we already hold in the blob. When subject is unavailable or the thread is absent
/// from it (`None`), we cannot prove "no change", so we fetch (a 404 dat is handled as Gone).
/// Shared by the individual reload and the board prefetch so the two gates never disagree.
pub fn needs_fetch(subject_count: Option<i64>, stored_res_count: i64) -> bool {
    match subject_count {
        Some(sc) => sc > stored_res_count,
        None => true,
    }
}

/// One favorite considered for refresh: its id and the res_count actually held in the blob.
struct BoardThread {
    thread_id: String,
    /// Posts parsed from the stored dat blob (the self-healing baseline; see the reload gate).
    stored_res_count: i64,
}

/// Refreshes every non-dead favorite on `board` using one subject.txt read.
///
/// Returns the number of dats actually fetched (for diagnostics/tests). Failures are logged,
/// never silently swallowed: a subject failure aborts the board (we cannot prove what grew),
/// and a per-thread dat failure is logged and skipped (other threads still refresh).
pub async fn refresh_board(state: &AppState, server: &str, board: &str) -> usize {
    let threads = match collect_board_threads(state, server, board) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("[refresh] db {server}/{board}: {e}");
            return 0;
        }
    };
    if threads.is_empty() {
        return 0;
    }

    // One subject.txt read covers the whole board.
    let entries = match http::fetch_subject(&state.http, &state.config.goch_base_url, server, board)
        .await
    {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("[refresh] subject {server}/{board}: {e}");
            return 0;
        }
    };
    let subject: HashMap<&str, i64> =
        entries.iter().map(|e| (e.thread_id.as_str(), e.res_count)).collect();

    let mut fetched = 0;
    for t in &threads {
        // Gate on the blob count (self-healing): fetch only grown threads, skip the rest.
        if !needs_fetch(subject.get(t.thread_id.as_str()).copied(), t.stored_res_count) {
            continue;
        }
        if refresh_thread(state, server, board, &t.thread_id).await {
            fetched += 1;
        }
    }
    fetched
}

/// Fetches and persists one thread's dat (full GET + blob replace, or mark dead on 404).
///
/// Returns true when a dat fetch was actually performed. Skips (returns false) when another
/// task already holds the in-flight claim, so the foreground reload and this prefetch never
/// double-download. Errors are logged, not swallowed.
pub async fn refresh_thread(state: &AppState, server: &str, board: &str, thread_id: &str) -> bool {
    let key = (server.to_string(), board.to_string(), thread_id.to_string());
    let _guard = match state.claim_dat(&key) {
        Some(g) => g,
        None => {
            tracing::debug!("[refresh] {server}/{board}/{thread_id}: already in flight, skip");
            return false;
        }
    };

    let fetch = match http::fetch_dat(
        &state.http,
        &state.config.goch_base_url,
        server,
        board,
        thread_id,
    )
    .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("[refresh] dat {server}/{board}/{thread_id}: {e}");
            return false;
        }
    };

    if let Err(e) = persist_fetch(state, server, board, thread_id, fetch) {
        tracing::error!("[refresh] persist {server}/{board}/{thread_id}: {e}");
        return false;
    }
    true
}

/// Persists a dat fetch result: full blob replace + metadata recompute, or mark dead on Gone.
pub fn persist_fetch(
    state: &AppState,
    server: &str,
    board: &str,
    thread_id: &str,
    fetch: DatFetch,
) -> Result<bool, AppError> {
    match fetch {
        DatFetch::Gone => {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "UPDATE favorites SET status='dead', updated_at=strftime('%s','now')
                 WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![server, board, thread_id],
            )?;
            Ok(false)
        }
        DatFetch::Replace { bytes } => {
            let text = http::decode_shift_jis(&bytes);
            let res_count = parse_dat(&text).len() as i64;
            let title = title_from_dat(&text).unwrap_or_default();
            let status = compute_status(res_count);
            tracing::info!(
                "[refresh] {server}/{board}/{thread_id}: fetched {res_count} posts, replacing blob"
            );
            let conn = state.db.lock().unwrap();
            replace_blob(&conn, server, board, thread_id, &bytes)?;
            conn.execute(
                "UPDATE favorites SET res_count=?4, status=?5,
                 title = CASE WHEN title='' THEN ?6 ELSE title END,
                 updated_at=strftime('%s','now')
                 WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![server, board, thread_id, res_count, status, title],
            )?;
            Ok(true)
        }
    }
}

/// Reads every non-dead favorite of a board with its stored blob res_count.
fn collect_board_threads(
    state: &AppState,
    server: &str,
    board: &str,
) -> Result<Vec<BoardThread>, AppError> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT thread_id FROM favorites
         WHERE server=?1 AND board=?2 AND status != 'dead'",
    )?;
    let ids: Vec<String> = stmt
        .query_map(params![server, board], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut out = Vec::with_capacity(ids.len());
    for thread_id in ids {
        let stored = read_blob_posts(&conn, server, board, &thread_id)?.len() as i64;
        out.push(BoardThread {
            thread_id,
            stored_res_count: stored,
        });
    }
    Ok(out)
}

/// Replaces the raw in dat_blobs entirely (inserts if absent). Always a full body, so the
/// column can never be corrupted into TEXT by concatenation.
pub fn replace_blob(
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

/// Reads the stored dat blob and parses it into posts (empty when no blob exists). The single
/// source of truth for "what posts do we actually hold", so the reload gate and the prefetch
/// gate never disagree on the stored count.
pub fn read_blob_posts(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<Vec<crate::goch::dat::Res>, AppError> {
    let raw: Option<Vec<u8>> = conn
        .query_row(
            "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match raw {
        Some(bytes) => parse_dat(&http::decode_shift_jis(&bytes)),
        None => vec![],
    })
}

/// Derives thread status from res_count alone (dat byte size is not used).
/// dead  = 1000 or more posts (thread is full).
/// warned = 980..999 (approaching the limit).
/// active = below 980.
pub fn compute_status(res_count: i64) -> &'static str {
    if res_count >= RES_DEAD {
        "dead"
    } else if res_count >= RES_WARN {
        "warned"
    } else {
        "active"
    }
}
