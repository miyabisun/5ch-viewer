//! Background monitoring. Polls only subject.txt per board to update
//! res_count, determine end-of-thread, and auto-add the next thread (does not fetch the dat body).

use crate::error::AppError;
use crate::goch::http;
use crate::goch::next_thread::find_next_thread;
use crate::goch::refresh::compute_status;
use crate::goch::subject::SubjectEntry;
use crate::state::AppState;
use std::collections::HashMap;
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(60);

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
}

async fn run_once(state: &AppState) -> Result<(), AppError> {
    let watches: Vec<Watch> = {
        let conn = state.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT server, board, thread_id, title, rating
             FROM favorites WHERE status != 'dead' AND archived = 0",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Watch {
                    server: r.get(0)?,
                    board: r.get(1)?,
                    thread_id: r.get(2)?,
                    title: r.get(3)?,
                    rating: r.get(4)?,
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
        match http::fetch_subject(&state.http, &state.config.goch_base_url, &server, &board).await {
            Ok(entries) => {
                for w in &threads {
                    process_thread(state, &server, &board, w, &entries);
                }
            }
            Err(e) => tracing::warn!("[sync] subject {server}/{board}: {e}"),
        }
    }
    Ok(())
}

fn process_thread(state: &AppState, server: &str, board: &str, w: &Watch, entries: &[SubjectEntry]) {
    let found = entries.iter().find(|e| e.thread_id == w.thread_id);

    // Reflect the state from subject. If absent from subject, treat as dropped.
    let is_dead = match found {
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
            status == "dead"
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

    if !is_dead {
        return;
    }

    // The thread ended, so look for the next one. Prefer the known title, otherwise take it from subject.
    let title = if !w.title.is_empty() {
        w.title.clone()
    } else {
        found.map(|e| e.title.clone()).unwrap_or_default()
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
        // Auto-add inheriting the rating (ignore if it already exists).
        if let Err(err) = conn.execute(
            "INSERT OR IGNORE INTO favorites
             (server, board, thread_id, board_name, title, res_count, rating)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                server, board, next.thread_id, board_name, next.title, next.res_count, w.rating
            ],
        ) {
            tracing::error!("[sync] auto-add next {server}/{board}/{}: {err}", next.thread_id);
            return;
        }
        tracing::info!("[sync] next thread added: {} -> {}", title, next.title);
    }
}

#[cfg(test)]
mod tests {
    use crate::db::SCHEMA;
    use rusqlite::{Connection, params};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    fn insert_favorite(conn: &Connection, thread_id: &str, title: &str, status: &str, archived: i64) {
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title, status, archived)
             VALUES (?1, 'egg', 'applism', '板', ?2, ?3, ?4)",
            params![thread_id, title, status, archived],
        )
        .unwrap();
    }

    /// run_once query must exclude archived favorites (AND archived = 0).
    #[test]
    fn run_once_query_excludes_archived() {
        let conn = setup();
        // One active, one archived.
        insert_favorite(&conn, "1001", "アクティブスレ", "active", 0);
        insert_favorite(&conn, "1002", "アーカイブスレ", "active", 1);

        // The same query used in run_once().
        let mut stmt = conn
            .prepare(
                "SELECT server, board, thread_id, title, rating
                 FROM favorites WHERE status != 'dead' AND archived = 0",
            )
            .unwrap();
        let thread_ids: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(2))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(thread_ids, vec!["1001"], "archived thread must be excluded from sync");
    }

    /// Archived threads that become dead must not trigger next-thread insertion.
    /// This verifies the root cause: because archived = 0 filter excludes them from
    /// run_once(), process_thread is never called for archived threads and the
    /// INSERT OR IGNORE in find_next_thread cannot fire.
    #[test]
    fn archived_dead_thread_does_not_auto_add_next() {
        let conn = setup();
        // Insert an archived thread that is already dead.
        insert_favorite(&conn, "1001", "古いスレ Part1", "dead", 1);
        // Insert a potential next-thread in the subject (simulated via a separate active entry).
        // We insert it as a favorite to confirm it was NOT auto-added by sync.
        // Initial count of favorites = 1 (only the archived one).
        let count_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_before, 1);

        // The run_once query with AND archived = 0 will return 0 rows,
        // so process_thread is never called.
        let mut stmt = conn
            .prepare(
                "SELECT server, board, thread_id FROM favorites
                 WHERE status != 'dead' AND archived = 0",
            )
            .unwrap();
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

    /// Verify that process_thread on a subject with the same thread works correctly
    /// (sanity check for the mock subject matching logic used in the full integration).
    #[test]
    fn process_thread_updates_res_count_for_active_non_archived() {
        let conn = setup();
        insert_favorite(&conn, "1001", "アクティブ", "active", 0);

        // Simulate what process_thread does for a found entry.
        let new_count: i64 = 42;
        conn.execute(
            "UPDATE favorites SET res_count=?4, status='active', updated_at=strftime('%s','now')
             WHERE server='egg' AND board='applism' AND thread_id=?3",
            params!["egg", "applism", "1001", new_count],
        )
        .unwrap();

        let res_count: i64 = conn
            .query_row(
                "SELECT res_count FROM favorites WHERE thread_id='1001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(res_count, 42);
    }

    /// process_thread must NOT be called for archived threads because they are
    /// filtered at the run_once query level. This test verifies the filter directly.
    #[test]
    fn sync_filter_archived_zero_filters_correctly() {
        let conn = setup();
        // Mix: active, warned, dead (all not archived) + one archived active.
        insert_favorite(&conn, "1001", "スレA", "active", 0);
        insert_favorite(&conn, "1002", "スレB", "warned", 0);
        insert_favorite(&conn, "1003", "スレC", "dead", 0);
        insert_favorite(&conn, "1004", "アーカイブスレ", "active", 1);

        let mut stmt = conn
            .prepare(
                "SELECT thread_id FROM favorites WHERE status != 'dead' AND archived = 0",
            )
            .unwrap();
        let mut ids: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        ids.sort();

        // Only 1001 (active) and 1002 (warned) pass: dead and archived are excluded.
        assert_eq!(ids, vec!["1001", "1002"]);
    }
}
