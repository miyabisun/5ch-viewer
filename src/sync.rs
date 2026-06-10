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
             FROM favorites WHERE status != 'dead'",
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
