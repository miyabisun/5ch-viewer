//! Integration-test harness binary.
//!
//! Boots two HTTP servers in one process so the Playwright "総合テスト" (full-stack)
//! suite can exercise the *real* Rust backend against a controllable 5ch stand-in:
//!
//!   1. A mock 5ch server (subject.txt / dat / SETTING.TXT) whose responses can be
//!      reprogrammed at runtime via `POST /_control/thread`.
//!   2. The real app router (`routes::build_router`) on an in-memory SQLite DB, with
//!      `goch_base_url` pointed at the mock so every dat/subject fetch hits it instead
//!      of 5ch.io. SSRF validation, Monazilla UA, Shift_JIS and identity all stay live.
//!
//! Ports (override via env): APP_PORT=3001, MOCK_PORT=3002. The DB is `:memory:` and a
//! single shared Connection, so it survives for the whole process and is thrown away on
//! exit — production data is never touched.
//!
//! Control protocol (used by tests via the app server's `/_control/*` proxy or directly
//! against the mock):
//!   POST /_control/thread  { server, board, thread_id, title, res_count, dat_posts, gone }
//!     - res_count : number reported in subject.txt for this thread
//!     - dat_posts : number of posts the dat returns (full body)
//!     - gone      : when true, the dat endpoint returns 404 (thread dropped)
//!   POST /_control/seed-favorite { server, board, thread_id, title, res_count, blob_posts }
//!     - inserts a favorite + a dat_blobs row with `blob_posts` posts, and sets the
//!       favorite's metadata res_count to `res_count` (to reproduce drift: meta=117,
//!       blob=111).

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use goch_viewer::config::Config;
use goch_viewer::state::AppState;
use goch_viewer::{db, routes};
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// One thread's scenario inside the mock 5ch server.
#[derive(Clone)]
struct MockThread {
    board: String,
    thread_id: String,
    title: String,
    /// Count reported by subject.txt (the reload gate reads this).
    res_count: i64,
    /// Number of posts the dat returns when fetched.
    dat_posts: i64,
    /// When true the dat endpoint returns 404 (thread gone).
    gone: bool,
}

/// Mock state: programmed threads plus per-board subject.txt request counts. The counts let
/// tests assert that subject.txt is hit exactly once per board (not once per thread).
#[derive(Default)]
struct MockInner {
    threads: HashMap<(String, String), MockThread>,
    subject_hits: HashMap<String, u64>,
}

type MockState = Arc<Mutex<MockInner>>;

fn sjis(text: &str) -> Vec<u8> {
    let (cow, _, _) = encoding_rs::SHIFT_JIS.encode(text);
    cow.into_owned()
}

/// Builds a dat body (Shift_JIS) with `n` posts. The first post carries the thread title.
fn build_dat(title: &str, n: i64) -> Vec<u8> {
    let mut s = String::new();
    for i in 1..=n {
        let title_field = if i == 1 { title } else { "" };
        s.push_str(&format!(
            "名無し<>sage<>2025/01/01 00:00 ID:abc{i}<>本文{i}<>{title_field}\n"
        ));
    }
    sjis(&s)
}

#[derive(Deserialize)]
struct ThreadCtl {
    server: String,
    board: String,
    thread_id: String,
    #[serde(default)]
    title: String,
    res_count: i64,
    dat_posts: i64,
    #[serde(default)]
    gone: bool,
}

#[derive(Deserialize)]
struct SeedCtl {
    server: String,
    board: String,
    thread_id: String,
    #[serde(default)]
    title: String,
    res_count: i64,
    blob_posts: i64,
}

// ---- mock 5ch handlers -----------------------------------------------------

async fn mock_subject(
    State(mock): State<MockState>,
    AxPath(board): AxPath<String>,
) -> Response {
    let mut inner = mock.lock().unwrap();
    *inner.subject_hits.entry(board.clone()).or_insert(0) += 1;
    let mut lines = String::new();
    for t in inner.threads.values().filter(|t| t.board == board) {
        lines.push_str(&format!(
            "{}.dat<>{} ({})\n",
            t.thread_id, t.title, t.res_count
        ));
    }
    (StatusCode::OK, sjis(&lines)).into_response()
}

async fn mock_dat(
    State(mock): State<MockState>,
    AxPath((board, file)): AxPath<(String, String)>,
) -> Response {
    let thread_id = file.trim_end_matches(".dat").to_string();
    let inner = mock.lock().unwrap();
    match inner.threads.get(&(board.clone(), thread_id.clone())) {
        Some(t) if !t.gone => {
            (StatusCode::OK, build_dat(&t.title, t.dat_posts)).into_response()
        }
        _ => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn mock_setting(
    State(mock): State<MockState>,
    AxPath(board): AxPath<String>,
) -> Response {
    let inner = mock.lock().unwrap();
    let name = inner
        .threads
        .values()
        .find(|t| t.board == board)
        .map(|t| t.board.clone())
        .unwrap_or_else(|| board.clone());
    (StatusCode::OK, sjis(&format!("BBS_TITLE={name}\n"))).into_response()
}

async fn ctl_thread(State(mock): State<MockState>, Json(c): Json<ThreadCtl>) -> Json<bool> {
    mock.lock().unwrap().threads.insert(
        (c.board.clone(), c.thread_id.clone()),
        MockThread {
            board: c.board,
            thread_id: c.thread_id,
            title: if c.title.is_empty() {
                "テストスレ".into()
            } else {
                c.title
            },
            res_count: c.res_count,
            dat_posts: c.dat_posts,
            gone: c.gone,
        },
    );
    let _ = c.server;
    Json(true)
}

/// Returns how many times subject.txt has been requested for `board` (tests assert this is
/// exactly 1 per board per refresh, proving we do not hit subject once per thread).
async fn ctl_subject_hits(
    State(mock): State<MockState>,
    AxPath(board): AxPath<String>,
) -> Json<u64> {
    let inner = mock.lock().unwrap();
    Json(inner.subject_hits.get(&board).copied().unwrap_or(0))
}

/// Clears programmed threads and hit counters so each test starts from a clean mock.
async fn ctl_mock_reset(State(mock): State<MockState>) -> Json<bool> {
    let mut inner = mock.lock().unwrap();
    inner.threads.clear();
    inner.subject_hits.clear();
    Json(true)
}

// ---- app-side control (seeds the in-memory DB) -----------------------------

async fn ctl_seed(State(app): State<AppState>, Json(c): Json<SeedCtl>) -> Json<bool> {
    let title = if c.title.is_empty() {
        "テストスレ".to_string()
    } else {
        c.title
    };
    let blob = build_dat(&title, c.blob_posts);
    let conn = app.db.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO favorites
         (server, board, thread_id, board_name, title, res_count, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active')",
        params![c.server, c.board, c.thread_id, c.board, title, c.res_count],
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO dat_blobs (server, board, thread_id, raw)
         VALUES (?1, ?2, ?3, ?4)",
        params![c.server, c.board, c.thread_id, blob],
    )
    .unwrap();
    Json(true)
}

/// Wipes all seeded data so each test starts clean (the :memory: DB persists for the process).
async fn ctl_reset(State(app): State<AppState>) -> Json<bool> {
    let conn = app.db.lock().unwrap();
    conn.execute("DELETE FROM favorites", []).unwrap();
    Json(true) // dat_blobs cascade-deletes via the FK
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app_port: u16 = std::env::var("APP_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3001);
    let mock_port: u16 = std::env::var("MOCK_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3002);

    // 1. Mock 5ch server.
    let mock_state: MockState = Arc::new(Mutex::new(MockInner::default()));
    let mock_app = Router::new()
        .route("/{board}/subject.txt", get(mock_subject))
        .route("/{board}/dat/{file}", get(mock_dat))
        .route("/{board}/SETTING.TXT", get(mock_setting))
        .route("/_control/thread", post(ctl_thread))
        .route("/_control/subject-hits/{board}", get(ctl_subject_hits))
        .route("/_control/reset", post(ctl_mock_reset))
        .with_state(mock_state.clone());
    let mock_addr = format!("127.0.0.1:{mock_port}");
    let mock_listener = tokio::net::TcpListener::bind(&mock_addr).await.expect("bind mock");
    tokio::spawn(async move {
        axum::serve(mock_listener, mock_app).await.expect("mock serve");
    });

    // 2. Real app server (in-memory DB, pointed at the mock for 5ch access).
    let conn = Connection::open_in_memory().expect("open :memory:");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(db::SCHEMA).unwrap();

    let config = Config {
        port: app_port,
        base_path: String::new(),
        db_path: ":memory:".into(),
        goch_base_url: format!("http://127.0.0.1:{mock_port}"),
    };
    let app_state = AppState::new(conn, config);

    // The real router + test-only control endpoints (seed / reset the in-memory DB).
    // start_sync is intentionally NOT spawned: the integration tests drive reload
    // explicitly and the 60s background poll would only add nondeterminism.
    let control = Router::new()
        .route("/_control/seed-favorite", post(ctl_seed))
        .route("/_control/reset", post(ctl_reset))
        .with_state(app_state.clone());
    let app = control.merge(routes::build_router(app_state));

    let app_addr = format!("127.0.0.1:{app_port}");
    let app_listener = tokio::net::TcpListener::bind(&app_addr).await.expect("bind app");
    eprintln!("[itest-server] app=http://{app_addr} mock=http://127.0.0.1:{mock_port}");
    axum::serve(app_listener, app).await.expect("app serve");
}
