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
use crate::fivech::dat::{count_dat_posts, parse_dat, title_from_dat};
use crate::fivech::http::{self, DatFetch};
use crate::fivech::subject::SubjectEntry;
use crate::state::AppState;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

// End-of-thread thresholds (spec ch.7). Status is derived from res_count only;
// dat byte size is no longer used for status (removed: dat_bytes, DAT_WARN, DAT_DEAD).
// RES_DEAD=1002 matches sentinel's resDeadThreshold (1000/1001 are warned, not dead).
const RES_WARN: i64 = 980;
const RES_DEAD: i64 = 1002;

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
/// Returns the number of dats actually fetched (for diagnostics/tests). Thin wrapper over
/// [`refresh_board_with_subject`], which owns the shared subject-read + bulk-dat-DL logic
/// (also used by background sync so the two paths never disagree).
pub async fn refresh_board(state: &AppState, server: &str, board: &str) -> usize {
    refresh_board_with_subject(state, server, board)
        .await
        .map_or(0, |(_entries, fetched)| fetched)
}

/// Board refresh core: one subject.txt read, then a dat DL for every thread that grew past
/// the count held in its blob. Returns the fetched subject entries (so the background sync
/// state machine can drive next-thread search from the same read) and the number of dats
/// actually downloaded. Returns `None` when the board could not be refreshed (db error or a
/// subject fetch failure — we cannot prove what grew).
///
/// The subject is always fetched, even when no non-dead thread needs refreshing, because the
/// background sync watch loop still needs the entries to search for the successors of
/// recently-dead threads.
///
/// Failures are logged, never silently swallowed: a subject failure aborts the board, and a
/// per-thread dat failure is logged and skipped (other threads still refresh).
pub async fn refresh_board_with_subject(
    state: &AppState,
    server: &str,
    board: &str,
) -> Option<(Vec<SubjectEntry>, usize)> {
    let threads = match collect_board_threads(state, server, board) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("[refresh] db {server}/{board}: {e}");
            return None;
        }
    };

    // One subject.txt read covers the whole board.
    let entries = match http::fetch_subject(&state.http, &state.config.fivech_base_url, server, board)
        .await
    {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("[refresh] subject {server}/{board}: {e}");
            return None;
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
    Some((entries, fetched))
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
        &state.config.fivech_base_url,
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
            let dat_bytes = bytes.len() as i64;
            let text = http::decode_shift_jis(&bytes);
            let res_count = parse_dat(&text).len() as i64;
            let title = title_from_dat(&text).unwrap_or_default();
            let status = compute_status(res_count);
            tracing::info!(
                "[refresh] {server}/{board}/{thread_id}: fetched {res_count} posts, replacing blob"
            );
            let conn = state.db.lock().unwrap();
            replace_blob(&conn, server, board, thread_id, &text, dat_bytes)?;
            conn.execute(
                "UPDATE favorites SET res_count=?4, status=?5,
                 title = CASE WHEN title='' THEN ?6 ELSE title END,
                 updated_at=strftime('%s','now')
                 WHERE server=?1 AND board=?2 AND thread_id=?3",
                params![server, board, thread_id, res_count, status, title],
            )?;
            drop(conn);

            // Kick off image prefetch in the background (non-blocking for the caller).
            let image_urls = crate::fivech::images::extract_image_urls(&text);
            if !image_urls.is_empty() {
                let state2 = state.clone();
                tokio::spawn(async move {
                    crate::fivech::images::prefetch_images(&state2, image_urls).await;
                });
            }

            Ok(true)
        }
    }
}

/// Reads every non-dead, non-archived favorite of a board with its stored blob res_count.
/// Archived threads are excluded: archiving means "stop tracking", so no 5ch dat fetches
/// should be triggered for them (aligned with the project's 5ch-access-reduction policy).
fn collect_board_threads(
    state: &AppState,
    server: &str,
    board: &str,
) -> Result<Vec<BoardThread>, AppError> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT thread_id FROM favorites
         WHERE server=?1 AND board=?2 AND status != 'dead' AND archived = 0",
    )?;
    let ids: Vec<String> = stmt
        .query_map(params![server, board], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut out = Vec::with_capacity(ids.len());
    for thread_id in ids {
        let stored = count_blob_posts(&conn, server, board, &thread_id)?;
        out.push(BoardThread {
            thread_id,
            stored_res_count: stored,
        });
    }
    Ok(out)
}

/// Replaces the raw in dat_blobs entirely (inserts if absent). Always stores the full
/// UTF-8-decoded body, so a subsequent read never needs Shift-JIS decoding.
/// `dat_bytes` is the original Shift-JIS byte length of the dat, used by the HEAD gate
/// to detect changes without downloading the full body.
pub fn replace_blob(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
    text: &str,
    dat_bytes: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO dat_blobs (server, board, thread_id, raw, dat_bytes) VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(server, board, thread_id) DO UPDATE SET raw=excluded.raw, dat_bytes=excluded.dat_bytes",
        params![server, board, thread_id, text, dat_bytes],
    )?;
    Ok(())
}

/// Reads the stored dat text (UTF-8, decoded once at write time — no Shift-JIS decode here).
/// `None` when no blob exists yet.
fn read_blob_raw(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT raw FROM dat_blobs WHERE server=?1 AND board=?2 AND thread_id=?3",
            params![server, board, thread_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Reads the stored dat text and parses it into posts (empty when no blob exists). The single
/// source of truth for "what posts do we actually hold", so the reload gate and the prefetch
/// gate never disagree on the stored count.
pub fn read_blob_posts(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<Vec<crate::fivech::dat::Res>, AppError> {
    Ok(read_blob_raw(conn, server, board, thread_id)?
        .map(|text| parse_dat(&text))
        .unwrap_or_default())
}

/// Counts the posts in the stored dat without allocating them (0 when no blob exists). The
/// cheap path for the reload/prefetch gate's baseline, which only needs the count — counts
/// exactly the posts `read_blob_posts` would return.
pub fn count_blob_posts(
    conn: &rusqlite::Connection,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<i64, AppError> {
    Ok(read_blob_raw(conn, server, board, thread_id)?
        .map(|text| count_dat_posts(&text))
        .unwrap_or(0))
}

/// Derives thread status from res_count alone (dat byte size is not used).
/// dead   = 1002 or more posts (thread is full; matches sentinel resDeadThreshold=1002).
/// warned = 980..=1001 (danger zone: approaching or nominally over the 1000-res mark).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::SCHEMA;
    use crate::state::AppState;
    use rusqlite::Connection;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    fn make_state(conn: Connection) -> AppState {
        let jar = crate::fivech::cookie_jar::open("/tmp/fivech_test_cookies.json");
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
                cookies_path: "/tmp/fivech_test_cookies.json".to_string(),
                fivech_base_url: String::new(),
            },
            inflight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    fn insert_fav(conn: &Connection, thread_id: &str, status: &str, archived: i64) {
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title, status, archived)
             VALUES (?1, 'egg', 'applism', '板', 'タイトル', ?2, ?3)",
            rusqlite::params![thread_id, status, archived],
        )
        .unwrap();
    }

    /// When a board has both active and archived threads, collect_board_threads must
    /// return only the active (non-archived) ones.  Archived threads must never trigger
    /// a 5ch dat fetch (5ch-access-reduction policy).
    #[test]
    fn collect_board_threads_excludes_archived() {
        let conn = setup();
        // active + non-archived: should appear
        insert_fav(&conn, "1001", "active", 0);
        // warned + non-archived: should appear
        insert_fav(&conn, "1002", "warned", 0);
        // active + archived: must NOT appear
        insert_fav(&conn, "1003", "active", 1);
        // dead + non-archived: excluded by status != 'dead'
        insert_fav(&conn, "1004", "dead", 0);
        // dead + archived: excluded by both conditions
        insert_fav(&conn, "1005", "dead", 1);

        let state = make_state(conn);
        let threads = collect_board_threads(&state, "egg", "applism").unwrap();
        let mut ids: Vec<&str> = threads.iter().map(|t| t.thread_id.as_str()).collect();
        ids.sort();

        // Only the two non-dead, non-archived threads pass.
        assert_eq!(
            ids,
            vec!["1001", "1002"],
            "only non-dead AND non-archived threads should be collected for prefetch"
        );
    }

    /// When an archived thread is the only thread on a board, collect_board_threads
    /// returns an empty list so no subject.txt fetch is triggered at all.
    #[test]
    fn collect_board_threads_all_archived_returns_empty() {
        let conn = setup();
        insert_fav(&conn, "2001", "active", 1);
        insert_fav(&conn, "2002", "warned", 1);

        let state = make_state(conn);
        let threads = collect_board_threads(&state, "egg", "applism").unwrap();
        assert!(
            threads.is_empty(),
            "all-archived board must yield empty list so no 5ch request is made"
        );
    }

    /// Invariant (the stuck-at-111 class of bug, killed by design): persist_fetch is the
    /// single writer of favorites.res_count, and it writes the real post count parsed from the
    /// freshly downloaded blob — never a stale value. A pre-existing res_count (here 111) must
    /// be overwritten with the blob's actual count.
    #[test]
    fn persist_fetch_sets_res_count_to_blob_post_count() {
        let conn = setup();
        insert_fav(&conn, "1001", "active", 0);
        // Seed a stale/drifted res_count to prove persist_fetch overwrites it with the blob count.
        conn.execute("UPDATE favorites SET res_count=111 WHERE thread_id='1001'", [])
            .unwrap();
        let state = make_state(conn);

        // 3-post dat (ASCII bytes: decode_shift_jis is a no-op on ASCII; no image URLs so no
        // background spawn is triggered). parse_dat counts exactly 3 posts.
        let bytes = b"name<>mail<>date ID:a<>body1<>title\n\
                      name<>mail<>date ID:b<>body2<>\n\
                      name<>mail<>date ID:c<>body3<>\n"
            .to_vec();
        let updated =
            persist_fetch(&state, "egg", "applism", "1001", DatFetch::Replace { bytes }).unwrap();
        assert!(updated, "a non-empty Replace must report an update");

        let conn = state.db.lock().unwrap();
        let res_count: i64 = conn
            .query_row("SELECT res_count FROM favorites WHERE thread_id='1001'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(res_count, 3, "res_count must equal the blob's real post count, not the stale 111");
    }

    // --- compute_status threshold tests (sentinel parity) ---

    /// Below RES_WARN: active.
    #[test]
    fn compute_status_active_below_warn() {
        assert_eq!(compute_status(0), "active");
        assert_eq!(compute_status(979), "active");
    }

    /// At and above RES_WARN but below RES_DEAD: warned.
    #[test]
    fn compute_status_warned_range() {
        assert_eq!(compute_status(980), "warned");
        assert_eq!(compute_status(1000), "warned"); // was wrongly "dead" before the fix
        assert_eq!(compute_status(1001), "warned"); // sentinel resDeadThreshold-1
    }

    /// At and above RES_DEAD (1002): dead (matches sentinel resDeadThreshold=1002).
    #[test]
    fn compute_status_dead_at_1002() {
        assert_eq!(compute_status(1002), "dead");
        assert_eq!(compute_status(1024), "dead");
    }
}
