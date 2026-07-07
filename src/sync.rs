//! Background monitoring. Polls only subject.txt per board to update
//! res_count, determine end-of-thread, and auto-add the next thread (does not fetch the dat body).

use crate::error::AppError;
use crate::fivech::http;
use crate::fivech::next_thread::find_next_thread;
use crate::fivech::refresh::compute_status;
use crate::fivech::subject::SubjectEntry;
use crate::state::AppState;
use std::collections::HashMap;
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(60);

/// Watch query shared by run_once() and its tests.
///
/// Non-archived favorites only. active/warned are always watched. dead threads keep being
/// watched — but for next-thread search ONLY (see try_add_next_thread) — for a bounded 7-day
/// window (604800s) after their last real update. This closes the "next thread posted after
/// the current thread went dead" race without polling dead-only boards forever (5ch access
/// policy). Once the next thread is registered it enters as an active row and keeps the board
/// live; the dead row falls off after 7 days; archiving (archived=1) also removes it.
const WATCH_QUERY: &str =
    "SELECT server, board, thread_id, title, rating, status
     FROM favorites
     WHERE archived = 0
       AND (status != 'dead' OR updated_at >= strftime('%s','now') - 604800)";

pub fn start_sync(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(INTERVAL);
        loop {
            ticker.tick().await;
            if let Err(e) = run_once(&state).await {
                tracing::error!("[sync] {e}");
            }
        }
    });
}

struct Watch {
    server: String,
    board: String,
    thread_id: String,
    title: String,
    rating: i64,
    status: String,
}

async fn run_once(state: &AppState) -> Result<(), AppError> {
    let watches: Vec<Watch> = {
        let conn = state.db.lock().unwrap();
        let mut stmt = conn.prepare(WATCH_QUERY)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Watch {
                    server: r.get(0)?,
                    board: r.get(1)?,
                    thread_id: r.get(2)?,
                    title: r.get(3)?,
                    rating: r.get(4)?,
                    status: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    if watches.is_empty() {
        return Ok(());
    }

    // Group by board (fetch the same subject.txt once and share it).
    let mut by_board: HashMap<(String, String), Vec<Watch>> = HashMap::new();
    for w in watches {
        by_board
            .entry((w.server.clone(), w.board.clone()))
            .or_default()
            .push(w);
    }

    for ((server, board), threads) in by_board {
        match http::fetch_subject(&state.http, &state.config.fivech_base_url, &server, &board).await {
            Ok(entries) => {
                for w in &threads {
                    // dead rows are read-only: search for the next thread but never write back
                    // to the dead row (touching updated_at would extend the 7-day window forever).
                    if w.status == "dead" {
                        try_add_next_thread(state, &server, &board, w, &entries);
                    } else {
                        process_thread(state, &server, &board, w, &entries);
                    }
                }
            }
            Err(e) => tracing::warn!("[sync] subject {server}/{board}: {e}"),
        }
    }
    Ok(())
}

fn process_thread(state: &AppState, server: &str, board: &str, w: &Watch, entries: &[SubjectEntry]) {
    let found = entries.iter().find(|e| e.thread_id == w.thread_id);

    // Reflect the state from subject. If absent from subject, treat as dropped (dead).
    // in_danger = true when status is warned or dead (res>=980) OR the thread has vanished
    // from subject.txt. At this point we start looking for the next thread in subject.
    // (Matches sentinel's behaviour: warned branch at res>=980 triggers findNextThread.)
    let in_danger = match found {
        Some(e) => {
            let status = compute_status(e.res_count);
            let conn = state.db.lock().unwrap();
            if let Err(err) = conn.execute(
                "UPDATE favorites SET res_count=?4,
                 title = CASE WHEN title='' THEN ?5 ELSE title END,
                 status=?6, updated_at=strftime('%s','now')
                 WHERE server=?1 AND board=?2 AND thread_id=?3",
                rusqlite::params![server, board, w.thread_id, e.res_count, e.title, status],
            ) {
                tracing::error!("[sync] update {server}/{board}/{}: {err}", w.thread_id);
            }
            // Enter danger zone at warned (res>=980), not only at dead.
            status == "warned" || status == "dead"
        }
        None => {
            let conn = state.db.lock().unwrap();
            if let Err(err) = conn.execute(
                "UPDATE favorites SET status='dead', updated_at=strftime('%s','now')
                 WHERE server=?1 AND board=?2 AND thread_id=?3",
                rusqlite::params![server, board, w.thread_id],
            ) {
                tracing::error!("[sync] mark dead {server}/{board}/{}: {err}", w.thread_id);
            }
            true
        }
    };

    if !in_danger {
        return;
    }

    // In the danger zone: look for the next thread in subject.
    try_add_next_thread(state, server, board, w, entries);
}

/// Finds and (if new) registers the next thread for `w` from the already-fetched subject
/// entries. Shared by process_thread (active/warned) and the dead read-only path in run_once.
/// Does a pure string match — no additional 5ch HTTP requests. None = next thread not yet
/// posted; skip. Only writes the newly-inserted next-thread row; never mutates the `w` row
/// (so the dead path stays read-only and never touches updated_at, keeping the 7-day window
/// bounded per the 5ch access policy).
///
/// The search title prefers the DB-stored title, falling back to what subject.txt reports
/// for this thread (if still listed).
fn try_add_next_thread(state: &AppState, server: &str, board: &str, w: &Watch, entries: &[SubjectEntry]) {
    let title = if !w.title.is_empty() {
        w.title.clone()
    } else {
        entries
            .iter()
            .find(|e| e.thread_id == w.thread_id)
            .map(|e| e.title.clone())
            .unwrap_or_default()
    };
    if title.is_empty() {
        return;
    }

    if let Some(next) = find_next_thread(&title, entries) {
        let conn = state.db.lock().unwrap();
        let board_name: String = conn
            .query_row(
                "SELECT board_name FROM favorites WHERE server=?1 AND board=?2 AND thread_id=?3",
                rusqlite::params![server, board, w.thread_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| board.to_string());
        // INSERT OR IGNORE prevents duplicates structurally; the affected-row count tells us
        // whether this was a new registration (rating inherited from the current thread,
        // viewer-specific) or an already-registered next thread (skipped, no log noise).
        match conn.execute(
            "INSERT OR IGNORE INTO favorites
             (server, board, thread_id, board_name, title, res_count, rating)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                server, board, next.thread_id, board_name, next.title, next.res_count, w.rating
            ],
        ) {
            Err(err) => {
                tracing::error!("[sync] auto-add next {server}/{board}/{}: {err}", next.thread_id);
            }
            Ok(0) => tracing::debug!("[sync] next thread already registered: {}", next.title),
            Ok(_) => tracing::info!("[sync] next thread added: {} -> {}", title, next.title),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::db::SCHEMA;
    use crate::fivech::subject::SubjectEntry;
    use crate::state::AppState;
    use rusqlite::{Connection, params};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

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

    fn subject_entry(thread_id: &str, title: &str, res_count: i64) -> SubjectEntry {
        SubjectEntry {
            thread_id: thread_id.to_string(),
            title: title.to_string(),
            res_count,
        }
    }

    fn insert_favorite(conn: &Connection, thread_id: &str, title: &str, status: &str, archived: i64) {
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title, status, archived)
             VALUES (?1, 'egg', 'applism', '板', ?2, ?3, ?4)",
            params![thread_id, title, status, archived],
        )
        .unwrap();
    }

    /// WATCH_QUERY semantics: non-archived active/warned are always watched; a recently-dead
    /// (updated within 7 days) non-archived thread is ALSO watched (for next-thread search);
    /// a stale dead thread (updated >7 days ago) and any archived thread are excluded.
    #[test]
    fn watch_query_includes_recent_dead_excludes_stale_and_archived() {
        let conn = setup();
        insert_favorite(&conn, "1001", "スレA", "active", 0);
        insert_favorite(&conn, "1002", "スレB", "warned", 0);
        // Freshly inserted dead row: updated_at defaults to now -> within the 7-day window.
        insert_favorite(&conn, "1003", "スレC", "dead", 0);
        // Archived (even though active) -> excluded.
        insert_favorite(&conn, "1004", "アーカイブスレ", "active", 1);
        // Dead + archived -> excluded (archived filter wins).
        insert_favorite(&conn, "1005", "アーカイブ済み死亡スレ", "dead", 1);
        // Stale dead: last updated 8 days ago -> outside the 7-day window -> excluded.
        insert_favorite(&conn, "1006", "古い死亡スレ", "dead", 0);
        conn.execute(
            "UPDATE favorites SET updated_at = strftime('%s','now') - 8*86400 WHERE thread_id='1006'",
            [],
        )
        .unwrap();

        let mut stmt = conn.prepare(super::WATCH_QUERY).unwrap();
        let mut ids: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(2))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        ids.sort();

        // active, warned, and the recently-dead 1003 pass; archived and stale-dead are excluded.
        assert_eq!(ids, vec!["1001", "1002", "1003"]);
    }

    /// Archived threads that become dead must not trigger next-thread insertion.
    /// This verifies the root cause: because archived = 0 filter excludes them from
    /// WATCH_QUERY, process_thread is never called for archived threads and the
    /// INSERT OR IGNORE in find_next_thread cannot fire.
    #[test]
    fn archived_dead_thread_does_not_auto_add_next() {
        let conn = setup();
        // Insert an archived thread that is already dead.
        insert_favorite(&conn, "1001", "古いスレ Part1", "dead", 1);
        let count_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_before, 1);

        // WATCH_QUERY (the exact query run_once() uses) returns 0 rows,
        // so process_thread is never called for this thread.
        let mut stmt = conn.prepare(super::WATCH_QUERY).unwrap();
        let watches: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(2))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(watches.is_empty(), "archived dead thread must not appear in sync watch list");

        // Verify: no next-thread INSERT happened because watches is empty.
        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_after, 1, "no next thread should be auto-added for archived threads");
    }

    /// process_thread updates res_count (and status) for an active, non-archived thread
    /// still present in subject.txt below the warned threshold.
    #[test]
    fn process_thread_updates_res_count_for_active_non_archived() {
        let conn = setup();
        insert_fav_with_rating(&conn, "1001", "アクティブ", "active", 0);

        let state = make_state(conn);
        let w = super::Watch {
            server: "egg".to_string(),
            board: "applism".to_string(),
            thread_id: "1001".to_string(),
            title: "アクティブ".to_string(),
            rating: 0,
            status: "active".to_string(),
        };
        // res_count=42 is below the warned threshold (980), so no next-thread search runs.
        let entries = vec![subject_entry("1001", "アクティブ", 42)];

        super::process_thread(&state, "egg", "applism", &w, &entries);

        let conn = state.db.lock().unwrap();
        let res_count: i64 = conn
            .query_row(
                "SELECT res_count FROM favorites WHERE thread_id='1001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(res_count, 42);
    }

    // --- process_thread warned-trigger tests ---

    fn insert_fav_with_rating(
        conn: &Connection,
        thread_id: &str,
        title: &str,
        status: &str,
        rating: i64,
    ) {
        conn.execute(
            "INSERT INTO favorites
             (thread_id, server, board, board_name, title, status, archived, rating)
             VALUES (?1, 'egg', 'applism', '板', ?2, ?3, 0, ?4)",
            params![thread_id, title, status, rating],
        )
        .unwrap();
    }

    /// Regression: a thread at warned (res=980..1001) must trigger next-thread auto-add.
    /// Previously only dead threads triggered this; warned threads were silently skipped.
    #[test]
    fn process_thread_warned_triggers_next_thread_registration() {
        let conn = setup();
        // Current thread at res=980 (warned zone).
        insert_fav_with_rating(&conn, "1000000001", "ブルアカ Part5862", "active", 3);

        let state = make_state(conn);
        let w = super::Watch {
            server: "egg".to_string(),
            board: "applism".to_string(),
            thread_id: "1000000001".to_string(),
            title: "ブルアカ Part5862".to_string(),
            rating: 3,
            status: "active".to_string(),
        };
        let entries = vec![
            subject_entry("1000000001", "ブルアカ Part5862", 980),
            subject_entry("1000000002", "ブルアカ Part5863", 5),
        ];

        super::process_thread(&state, "egg", "applism", &w, &entries);

        let conn = state.db.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "next thread must be auto-added when current thread is warned");

        let next_title: String = conn
            .query_row(
                "SELECT title FROM favorites WHERE thread_id='1000000002'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(next_title, "ブルアカ Part5863");
    }

    /// Rating must be inherited from the current thread when the next thread is auto-added.
    #[test]
    fn process_thread_next_thread_inherits_rating() {
        let conn = setup();
        insert_fav_with_rating(&conn, "1000000001", "ブルアカ Part5862", "active", 5);

        let state = make_state(conn);
        let w = super::Watch {
            server: "egg".to_string(),
            board: "applism".to_string(),
            thread_id: "1000000001".to_string(),
            title: "ブルアカ Part5862".to_string(),
            rating: 5,
            status: "active".to_string(),
        };
        let entries = vec![
            subject_entry("1000000001", "ブルアカ Part5862", 995),
            subject_entry("1000000002", "ブルアカ Part5863", 1),
        ];

        super::process_thread(&state, "egg", "applism", &w, &entries);

        let conn = state.db.lock().unwrap();
        let rating: i64 = conn
            .query_row(
                "SELECT rating FROM favorites WHERE thread_id='1000000002'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rating, 5, "next thread must inherit rating from the current thread");
    }

    /// If the next thread is already in favorites, INSERT OR IGNORE must not create a duplicate.
    #[test]
    fn process_thread_does_not_duplicate_already_registered_next() {
        let conn = setup();
        insert_fav_with_rating(&conn, "1000000001", "ブルアカ Part5862", "active", 3);
        // Next thread already registered.
        insert_fav_with_rating(&conn, "1000000002", "ブルアカ Part5863", "active", 0);

        let count_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_before, 2);

        let state = make_state(conn);
        let w = super::Watch {
            server: "egg".to_string(),
            board: "applism".to_string(),
            thread_id: "1000000001".to_string(),
            title: "ブルアカ Part5862".to_string(),
            rating: 3,
            status: "active".to_string(),
        };
        let entries = vec![
            subject_entry("1000000001", "ブルアカ Part5862", 990),
            subject_entry("1000000002", "ブルアカ Part5863", 10),
        ];

        super::process_thread(&state, "egg", "applism", &w, &entries);

        let conn = state.db.lock().unwrap();
        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_after, 2, "already-registered next thread must not be duplicated");
    }

    /// When next thread is not yet posted in subject, nothing is registered.
    #[test]
    fn process_thread_does_not_register_when_next_absent_from_subject() {
        let conn = setup();
        insert_fav_with_rating(&conn, "1000000001", "ブルアカ Part5862", "active", 2);

        let state = make_state(conn);
        let w = super::Watch {
            server: "egg".to_string(),
            board: "applism".to_string(),
            thread_id: "1000000001".to_string(),
            title: "ブルアカ Part5862".to_string(),
            rating: 2,
            status: "active".to_string(),
        };
        // Subject only contains the current thread; no Part5863 yet.
        let entries = vec![subject_entry("1000000001", "ブルアカ Part5862", 985)];

        super::process_thread(&state, "egg", "applism", &w, &entries);

        let conn = state.db.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "next thread must not be registered when absent from subject");
    }

    /// Threads below res=980 (active) must not trigger next-thread search.
    #[test]
    fn process_thread_active_does_not_trigger_next_thread_search() {
        let conn = setup();
        insert_fav_with_rating(&conn, "1000000001", "ブルアカ Part5862", "active", 0);

        let state = make_state(conn);
        let w = super::Watch {
            server: "egg".to_string(),
            board: "applism".to_string(),
            thread_id: "1000000001".to_string(),
            title: "ブルアカ Part5862".to_string(),
            rating: 0,
            status: "active".to_string(),
        };
        let entries = vec![
            subject_entry("1000000001", "ブルアカ Part5862", 500), // active: below 980
            subject_entry("1000000002", "ブルアカ Part5863", 1),
        ];

        super::process_thread(&state, "egg", "applism", &w, &entries);

        let conn = state.db.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "next thread must NOT be registered when current thread is active");
    }

    // --- try_add_next_thread on a dead row (read-only) tests ---

    /// try_add_next_thread must register the next thread (rating inherited) WITHOUT mutating the
    /// dead source row's res_count / status / updated_at (read-only invariant; touching
    /// updated_at would extend the 7-day watch window indefinitely).
    #[test]
    fn dead_row_search_adds_next_and_leaves_dead_row_untouched() {
        let conn = setup();
        insert_fav_with_rating(&conn, "1000000001", "ブルアカ Part5862", "dead", 4);
        // Pin a known res_count and a stable-but-recent updated_at so we can assert immutability.
        conn.execute(
            "UPDATE favorites SET res_count=1002, updated_at=1700000000 WHERE thread_id='1000000001'",
            [],
        )
        .unwrap();

        let state = make_state(conn);
        let w = super::Watch {
            server: "egg".to_string(),
            board: "applism".to_string(),
            thread_id: "1000000001".to_string(),
            title: "ブルアカ Part5862".to_string(),
            rating: 4,
            status: "dead".to_string(),
        };
        let entries = vec![
            subject_entry("1000000001", "ブルアカ Part5862", 1002),
            subject_entry("1000000002", "ブルアカ Part5863", 7),
        ];

        super::try_add_next_thread(&state, "egg", "applism", &w, &entries);

        let conn = state.db.lock().unwrap();
        // Next thread registered with inherited rating.
        let (title, rating): (String, i64) = conn
            .query_row(
                "SELECT title, rating FROM favorites WHERE thread_id='1000000002'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(title, "ブルアカ Part5863");
        assert_eq!(rating, 4, "next thread must inherit rating from the dead source thread");

        // Dead source row is untouched: res_count / status / updated_at unchanged.
        let (res_count, status, updated_at): (i64, String, i64) = conn
            .query_row(
                "SELECT res_count, status, updated_at FROM favorites WHERE thread_id='1000000001'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(res_count, 1002, "dead row res_count must not change");
        assert_eq!(status, "dead", "dead row status must not change");
        assert_eq!(updated_at, 1700000000, "dead row updated_at must not change");
    }

    /// The dead-row search must not duplicate an already-registered next thread (INSERT OR IGNORE).
    #[test]
    fn dead_row_search_does_not_duplicate_already_registered_next() {
        let conn = setup();
        insert_fav_with_rating(&conn, "1000000001", "ブルアカ Part5862", "dead", 3);
        insert_fav_with_rating(&conn, "1000000002", "ブルアカ Part5863", "active", 0);

        let state = make_state(conn);
        let w = super::Watch {
            server: "egg".to_string(),
            board: "applism".to_string(),
            thread_id: "1000000001".to_string(),
            title: "ブルアカ Part5862".to_string(),
            rating: 3,
            status: "dead".to_string(),
        };
        let entries = vec![
            subject_entry("1000000001", "ブルアカ Part5862", 1002),
            subject_entry("1000000002", "ブルアカ Part5863", 10),
        ];

        super::try_add_next_thread(&state, "egg", "applism", &w, &entries);

        let conn = state.db.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "already-registered next thread must not be duplicated");
    }
}
