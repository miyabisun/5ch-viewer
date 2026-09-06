//! Integration-test harness binary.
//!
//! Boots two HTTP servers in one process so the Playwright "総合テスト" (full-stack)
//! suite can exercise the *real* Rust backend against a controllable 5ch stand-in:
//!
//!   1. A mock 5ch server (subject.txt / dat / SETTING.TXT) whose responses can be
//!      reprogrammed at runtime via `POST /_control/thread`.
//!   2. The real app router (`routes::build_router`) on an in-memory SQLite DB, with
//!      `fivech_base_url` pointed at the mock so every dat/subject fetch hits it instead
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

use axum::extract::{Path as AxPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use viewer_of_5ch::config::Config;
use viewer_of_5ch::state::AppState;
use viewer_of_5ch::{db, routes};

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
    /// Optional image URLs to embed in the dat body (appended to post 1).
    image_urls: Vec<String>,
}

/// Mock state: programmed threads plus per-board subject.txt request counts. The counts let
/// tests assert that subject.txt is hit exactly once per board (not once per thread).
#[derive(Default)]
struct MockInner {
    threads: HashMap<(String, String), MockThread>,
    subject_hits: HashMap<String, u64>,
    /// How many times /test/bbs.cgi has been called in the current test scenario.
    /// Used to implement the two-step confirmation flow:
    ///   call 1 → confirmation page (x-chx-error: 0000 Confirmation)
    ///   call 2 → success (x-resnum: <next_res_num>)
    bbs_cgi_call_count: u64,
    /// The res number that bbs.cgi will report on the second (success) call.
    bbs_cgi_next_res: i64,
    /// Per-filename hit counter for the mock image endpoint (/mock/img/:file).
    image_hits: HashMap<String, u64>,
    /// Per-filename size override: when present, the mock serves a body of this byte length
    /// (all zeros) to simulate a large image for the 5MB size limit test.
    image_size_overrides: HashMap<String, usize>,
    /// Per-filename Content-Type override: when present, the mock returns this MIME type
    /// instead of the default (derived from file extension). Used to test MIME rejection.
    image_content_type_overrides: HashMap<String, String>,
}

type MockState = Arc<Mutex<MockInner>>;

fn sjis(text: &str) -> Vec<u8> {
    let (cow, _, _) = encoding_rs::SHIFT_JIS.encode(text);
    cow.into_owned()
}

/// Builds a dat body as UTF-8 text with `n` posts. The first post carries the thread title.
/// Optionally embeds `image_urls` into the body of post 1 (space-separated).
/// Used for DB seeding (dat_blobs.raw is UTF-8 TEXT).
fn build_dat_text(title: &str, n: i64) -> String {
    build_dat_text_with_images(title, n, &[])
}

fn build_dat_text_with_images(title: &str, n: i64, image_urls: &[String]) -> String {
    let mut s = String::new();
    for i in 1..=n {
        let title_field = if i == 1 { title } else { "" };
        let image_suffix = if i == 1 && !image_urls.is_empty() {
            format!(" {}", image_urls.join(" "))
        } else {
            String::new()
        };
        s.push_str(&format!(
            "名無し<>sage<>2025/01/01 00:00 ID:abc{i}<>本文{i}{image_suffix}<>{title_field}\n"
        ));
    }
    s
}

/// Builds a dat body as Shift_JIS bytes with `n` posts. The first post carries the thread title.
/// Used for mock HTTP responses that mimic the real 5ch server (which always returns Shift-JIS).
fn build_dat_sjis(title: &str, n: i64) -> Vec<u8> {
    sjis(&build_dat_text(title, n))
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
    /// Optional image URLs to embed in the dat body for image-cache integration tests.
    #[serde(default)]
    image_urls: Vec<String>,
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

async fn mock_subject(State(mock): State<MockState>, AxPath(board): AxPath<String>) -> Response {
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
            let dat_text = build_dat_text_with_images(&t.title, t.dat_posts, &t.image_urls);
            let dat_bytes = sjis(&dat_text);
            (StatusCode::OK, dat_bytes).into_response()
        }
        _ => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn mock_setting(State(mock): State<MockState>, AxPath(board): AxPath<String>) -> Response {
    let inner = mock.lock().unwrap();
    let name = inner
        .threads
        .values()
        .find(|t| t.board == board)
        .map(|t| t.board.clone())
        .unwrap_or_else(|| board.clone());
    (StatusCode::OK, sjis(&format!("BBS_TITLE={name}\n"))).into_response()
}

/// bbs.cgi mock: implements the two-step confirmation flow.
///
/// Call 1 (no acorn cookie / first call in scenario):
///   → returns a confirmation page HTML (SJIS) with x-chx-error: 0000 Confirmation
///     and a hidden `feature=confirmed:testfeaturehash000000001234567890ab` field.
///
/// Call 2 (feature field present in body / second call):
///   → returns a success response with x-resnum / x-posterid / x-postdate headers.
///
/// The call counter is reset by `/_control/reset` so each test starts clean.
async fn mock_bbs_cgi(
    State(mock): State<MockState>,
    Query(params): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    let mut inner = mock.lock().unwrap();
    let count = inner.bbs_cgi_call_count;
    inner.bbs_cgi_call_count += 1;

    // Decode the body to check whether the `feature` field is present.
    let body_str = String::from_utf8_lossy(&body);
    let has_feature =
        body_str.contains("feature=confirmed%3A") || body_str.contains("feature=confirmed:");
    let is_guid = params.get("guid").is_some_and(|v| v == "ON");

    // Second call (with feature + guid=ON) → success.
    if count > 0 && has_feature && is_guid {
        let res_num = inner.bbs_cgi_next_res;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-resnum",
            HeaderValue::from_str(&res_num.to_string()).unwrap(),
        );
        headers.insert("x-posterid", HeaderValue::from_static("TestPosterID1"));
        headers.insert(
            "x-postdate",
            HeaderValue::from_str(&format!(
                "{}.00",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            ))
            .unwrap(),
        );
        // Set a mock acorn cookie (session cookie without Max-Age, like real 5ch).
        headers.insert(
            "set-cookie",
            HeaderValue::from_static("acorn=mock_acorn_value; Path=/; Domain=.5ch.io"),
        );
        let success_html =
            sjis("<html><head><title>書きこみました。</title></head><body>OK</body></html>");
        return (StatusCode::OK, headers, success_html).into_response();
    }

    // First call → confirmation page.
    let confirmation_html = r#"<html>
<head><title>■ 書き込み確認 ■</title></head>
<body>
  <form action="bbs.cgi?guid=ON" method="post">
    <input type="hidden" name="bbs" value="applism" />
    <input type="hidden" name="key" value="1771127145" />
    <input type="hidden" name="feature" value="confirmed:testfeaturehash000000001234567890ab" />
    <input type="submit" name="submit" value="上記全てを承諾して書き込む" />
  </form>
</body></html>"#;
    let mut headers = HeaderMap::new();
    headers.insert("x-chx-error", HeaderValue::from_static("0000 Confirmation"));
    (StatusCode::OK, headers, sjis(confirmation_html)).into_response()
}

#[derive(Deserialize)]
struct BbsCgiCtl {
    next_res: i64,
}

/// Programs the bbs.cgi mock: sets the res number to return on success.
/// Call this before the post test to prime the mock.
async fn ctl_bbs_cgi(State(mock): State<MockState>, Json(c): Json<BbsCgiCtl>) -> Json<bool> {
    let mut inner = mock.lock().unwrap();
    inner.bbs_cgi_next_res = c.next_res;
    inner.bbs_cgi_call_count = 0;
    Json(true)
}

/// Returns the current bbs.cgi call count (for test assertions).
#[derive(Serialize)]
struct BbsCgiStatus {
    call_count: u64,
}

async fn ctl_bbs_cgi_status(State(mock): State<MockState>) -> Json<BbsCgiStatus> {
    let inner = mock.lock().unwrap();
    Json(BbsCgiStatus {
        call_count: inner.bbs_cgi_call_count,
    })
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
            image_urls: c.image_urls,
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
    inner.bbs_cgi_call_count = 0;
    inner.bbs_cgi_next_res = 0;
    inner.image_hits.clear();
    inner.image_size_overrides.clear();
    inner.image_content_type_overrides.clear();
    Json(true)
}

// ---- mock image handlers ---------------------------------------------------

/// A minimal 1×1 PNG for test image responses.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk length + type
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1×1
    0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, // 8-bit RGB, CRC
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, // IDAT chunk
    0x54, 0x08, 0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC,
    0x59, // IDAT data + CRC
    0xE7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, // IEND chunk
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// `GET /mock/img/:file` — serves a tiny PNG (or a large zero-filled body for size tests).
/// Increments a per-filename hit counter for GET requests only (HEAD requests are not counted
/// so tests can assert "image was fetched exactly once" independent of the HEAD pre-check).
async fn mock_image(
    method: Method,
    State(mock): State<MockState>,
    AxPath(file): AxPath<String>,
) -> Response {
    let (size_override, content_type) = {
        let mut inner = mock.lock().unwrap();
        // Only count GET requests; HEAD is the preflight size-check and should not affect hit counts.
        if method == Method::GET {
            *inner.image_hits.entry(file.clone()).or_insert(0) += 1;
        }
        let size = inner.image_size_overrides.get(&file).copied();
        // Use content-type override when set; otherwise derive from file extension.
        let ct = if let Some(ct_override) = inner.image_content_type_overrides.get(&file) {
            ct_override.clone()
        } else if file.ends_with(".jpg") || file.ends_with(".jpeg") {
            "image/jpeg".to_string()
        } else if file.ends_with(".gif") {
            "image/gif".to_string()
        } else if file.ends_with(".webp") {
            "image/webp".to_string()
        } else {
            "image/png".to_string()
        };
        (size, ct)
    };

    if let Some(size) = size_override {
        // Large body: serve zeros to trigger the 5MB guard in the image downloader.
        let body = vec![0u8; size];
        let mut resp_headers = HeaderMap::new();
        resp_headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
        resp_headers.insert(header::CONTENT_LENGTH, size.to_string().parse().unwrap());
        return (StatusCode::OK, resp_headers, body).into_response();
    }

    // Normal response: serve the tiny PNG bytes.
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
    (StatusCode::OK, resp_headers, TINY_PNG.to_vec()).into_response()
}

/// Returns how many times a specific image file has been requested.
async fn ctl_image_hits(State(mock): State<MockState>, AxPath(file): AxPath<String>) -> Json<u64> {
    let inner = mock.lock().unwrap();
    Json(inner.image_hits.get(&file).copied().unwrap_or(0))
}

#[derive(Deserialize)]
struct ImageSizeCtl {
    file: String,
    size: usize,
}

/// Programs the mock image server to return a body of `size` bytes for `file`.
/// Used to test the 5MB size limit guard.
async fn ctl_image_size(State(mock): State<MockState>, Json(c): Json<ImageSizeCtl>) -> Json<bool> {
    let mut inner = mock.lock().unwrap();
    inner.image_size_overrides.insert(c.file, c.size);
    Json(true)
}

#[derive(Deserialize)]
struct ImageContentTypeCtl {
    file: String,
    content_type: String,
}

/// Programs the mock image server to return the given Content-Type for `file`.
/// Used to test MIME rejection (e.g. text/html, image/svg+xml).
async fn ctl_image_content_type(
    State(mock): State<MockState>,
    Json(c): Json<ImageContentTypeCtl>,
) -> Json<bool> {
    let mut inner = mock.lock().unwrap();
    inner
        .image_content_type_overrides
        .insert(c.file, c.content_type);
    Json(true)
}

// ---- app-side control (seeds the in-memory DB) -----------------------------

async fn ctl_seed(State(app): State<AppState>, Json(c): Json<SeedCtl>) -> Json<bool> {
    let title = if c.title.is_empty() {
        "テストスレ".to_string()
    } else {
        c.title
    };
    // Build UTF-8 dat text for DB storage (dat_blobs.raw is TEXT, Shift-JIS decoded once on write).
    let dat_text = build_dat_text(&title, c.blob_posts);
    // Compute the Shift-JIS byte length so the HEAD gate has an accurate baseline.
    let dat_bytes = build_dat_sjis(&title, c.blob_posts).len() as i64;
    let conn = app.db.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO favorites
         (server, board, thread_id, board_name, title, res_count, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active')",
        params![c.server, c.board, c.thread_id, c.board, title, c.res_count],
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO dat_blobs (server, board, thread_id, raw, dat_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![c.server, c.board, c.thread_id, dat_text, dat_bytes],
    )
    .unwrap();
    Json(true)
}

#[derive(Deserialize)]
struct RefreshBoardCtl {
    server: String,
    board: String,
}

/// Test-only trigger for the board-level bulk refresh (`refresh::refresh_board`). Returns
/// immediately and runs the subject + bulk-dat download in a spawned task, mirroring the old
/// `POST /api/favorites/refresh` behavior so tests can poll the store for the result.
async fn ctl_refresh_board(
    State(app): State<AppState>,
    Json(c): Json<RefreshBoardCtl>,
) -> Json<bool> {
    tokio::spawn(async move {
        let n = viewer_of_5ch::fivech::refresh::refresh_board(&app, &c.server, &c.board).await;
        tracing::info!("[refresh] {}/{}: {n} dat(s) updated", c.server, c.board);
    });
    Json(true)
}

/// Wipes all seeded data so each test starts clean (the :memory: DB persists for the process).
async fn ctl_reset(State(app): State<AppState>) -> Json<bool> {
    let conn = app.db.lock().unwrap();
    // own_posts, ng_ids and ng_words have no FK to favorites, so they must be deleted
    // explicitly or rules would leak from one test into the next.
    conn.execute("DELETE FROM own_posts", []).unwrap();
    conn.execute("DELETE FROM ng_ids", []).unwrap();
    conn.execute("DELETE FROM ng_words", []).unwrap();
    conn.execute("DELETE FROM favorites", []).unwrap();
    conn.execute("DELETE FROM image_cache", []).unwrap();
    drop(conn);
    let image_root = std::path::Path::new(&app.config.image_cache_dir);
    let _ = std::fs::remove_dir_all(image_root);
    std::fs::create_dir(image_root).unwrap();
    Json(true) // dat_blobs cascade-deletes via the favorites FK
}

#[derive(Deserialize)]
struct SeedImageCtl {
    url: String,
    path: String,
    mime: String,
    #[serde(default)]
    mosaic: i64,
}

/// Seeds an image_cache row directly (bypasses the HTTP download pipeline).
/// Used by tests that need to verify the serve endpoint without hitting external URLs.
/// A minimal 1×1 PNG is written to the filesystem cache so the serve path is exercised.
async fn ctl_seed_image(State(app): State<AppState>, Json(c): Json<SeedImageCtl>) -> Json<bool> {
    // Minimal PNG: 1×1 pixel, RGB.
    let tiny_png: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59, 0xE7, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let id = {
        let conn = app.db.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO image_cache (url, path, mosaic) VALUES (?1, ?2, ?3)",
            params![c.url, c.path, c.mosaic],
        )
        .unwrap();
        conn.query_row(
            "SELECT id FROM image_cache WHERE url=?1",
            params![c.url],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    viewer_of_5ch::image_cache::write_verified(
        std::path::Path::new(&app.config.image_cache_dir),
        id,
        &tiny_png,
    )
    .unwrap();
    let conn = app.db.lock().unwrap();
    conn.execute(
        "UPDATE image_cache SET path=?1, mime=?2, file_size=?3, mosaic=?4 WHERE id=?5",
        params![c.path, c.mime, tiny_png.len() as i64, c.mosaic, id],
    )
    .unwrap();
    Json(true)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app_port: u16 = std::env::var("APP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3001);
    let mock_port: u16 = std::env::var("MOCK_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3002);

    // 1. Mock 5ch server.
    let mock_state: MockState = Arc::new(Mutex::new(MockInner::default()));
    let mock_app = Router::new()
        .route("/{board}/subject.txt", get(mock_subject))
        .route("/{board}/dat/{file}", get(mock_dat))
        .route("/{board}/SETTING.TXT", get(mock_setting))
        // bbs.cgi mock: two-step confirmation flow (call 1 = confirm page, call 2 = success).
        .route("/test/bbs.cgi", post(mock_bbs_cgi))
        // Mock image endpoint: serves tiny PNG/JPEG/GIF/WebP or large bodies for size tests.
        .route("/mock/img/{file}", get(mock_image))
        .route("/_control/thread", post(ctl_thread))
        .route("/_control/bbs-cgi", post(ctl_bbs_cgi))
        .route("/_control/bbs-cgi/status", get(ctl_bbs_cgi_status))
        .route("/_control/subject-hits/{board}", get(ctl_subject_hits))
        .route("/_control/image-hits/{file}", get(ctl_image_hits))
        .route("/_control/image-size", post(ctl_image_size))
        .route("/_control/image-content-type", post(ctl_image_content_type))
        .route("/_control/reset", post(ctl_mock_reset))
        .with_state(mock_state.clone());
    let mock_addr = format!("127.0.0.1:{mock_port}");
    let mock_listener = tokio::net::TcpListener::bind(&mock_addr)
        .await
        .expect("bind mock");
    tokio::spawn(async move {
        axum::serve(mock_listener, mock_app)
            .await
            .expect("mock serve");
    });

    // 2. Real app server (in-memory DB, pointed at the mock for 5ch access).
    let conn = Connection::open_in_memory().expect("open :memory:");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(db::SCHEMA).unwrap();

    let config = Config {
        port: app_port,
        base_path: String::new(),
        db_path: ":memory:".into(),
        image_cache_dir: format!("/tmp/fivech-itest-images-{}", std::process::id()),
        // Integration tests use an in-memory DB; cookies are not persisted.
        // Use a temp path that will not be written (the itest process is short-lived).
        cookies_path: "/tmp/fivech_itest_cookies.json".into(),
        fivech_base_url: format!("http://127.0.0.1:{mock_port}"),
    };
    let _ = std::fs::remove_dir_all(&config.image_cache_dir);
    std::fs::create_dir(&config.image_cache_dir).expect("create integration image cache");
    let app_state = AppState::new(conn, config);

    // The real router + test-only control endpoints (seed / reset the in-memory DB).
    // start_sync is intentionally NOT spawned: the integration tests drive reload
    // explicitly and the 60s background poll would only add nondeterminism.
    let control = Router::new()
        .route("/_control/seed-favorite", post(ctl_seed))
        .route("/_control/seed-image", post(ctl_seed_image))
        .route("/_control/reset", post(ctl_reset))
        .route("/_control/refresh-board", post(ctl_refresh_board))
        .with_state(app_state.clone());
    let app = control.merge(routes::build_router(app_state));

    let app_addr = format!("127.0.0.1:{app_port}");
    let app_listener = tokio::net::TcpListener::bind(&app_addr)
        .await
        .expect("bind app");
    eprintln!("[itest-server] app=http://{app_addr} mock=http://127.0.0.1:{mock_port}");
    axum::serve(app_listener, app).await.expect("app serve");
}
