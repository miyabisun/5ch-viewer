use crate::error::AppError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde_json::json;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/news", get(get_news))
}

struct NewsRow {
    server: String,
    board: String,
    board_name: String,
    thread_id: String,
    title: String,
    res_count: i64,
    read_res: i64,
    rating: i64,
    status: String,
    updated_at: i64,
}

/// GET /api/news — starred threads with unread posts, as JSON Feed 1.1.
///
/// Polled by the news-server aggregator (via a Cloudflare Access service
/// token) to place unread threads on the unified timeline. One item per
/// thread; `updated_at` moves forward when new posts arrive, which floats the
/// thread back up on the aggregator side. Like every other route, auth is
/// delegated to Cloudflare Access at the edge.
async fn get_news(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let rows = {
        let conn = state.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT server, board, board_name, thread_id, title,
                    res_count, read_res, rating, status, updated_at
             FROM favorites
             WHERE rating > 0 AND archived = 0 AND res_count > read_res
             ORDER BY updated_at DESC
             LIMIT 100",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(NewsRow {
                    server: row.get(0)?,
                    board: row.get(1)?,
                    board_name: row.get(2)?,
                    thread_id: row.get(3)?,
                    title: row.get(4)?,
                    res_count: row.get(5)?,
                    read_res: row.get(6)?,
                    rating: row.get(7)?,
                    status: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let base = resolve_base_url(&headers, &state.config);
    let feed = build_json_feed(&rows, &base);

    Ok((
        [(header::CONTENT_TYPE, "application/feed+json; charset=utf-8")],
        axum::Json(feed),
    ))
}

/// Derive the base URL from request headers (reverse proxy or direct access).
fn resolve_base_url(headers: &HeaderMap, config: &crate::config::Config) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");

    let default_host = format!("localhost:{}", config.port);
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .unwrap_or(&default_host);

    format!("{}://{}{}", proto, host, config.base_path)
}

fn build_json_feed(rows: &[NewsRow], base: &str) -> serde_json::Value {
    json!({
        "version": "https://jsonfeed.org/version/1.1",
        "title": "5ch Viewer - お気に入りスレの新着",
        "home_page_url": base,
        "items": rows.iter().map(|row| build_item(row, base)).collect::<Vec<_>>(),
    })
}

fn build_item(row: &NewsRow, base: &str) -> serde_json::Value {
    let unread = row.res_count - row.read_res;
    let mut obj = json!({
        // Same id format as the SPA route: /{server}/{board}/{thread_id}
        "id": format!("{}/{}/{}", row.server, row.board, row.thread_id),
        "url": format!("{}/{}/{}/{}", base, row.server, row.board, row.thread_id),
        "title": row.title,
        "content_text": format!("未読{}レス ({}/{})", unread, row.read_res, row.res_count),
        "_news": {
            "service": "5ch",
            "board": row.board_name,
            "res_count": row.res_count,
            "read_res": row.read_res,
            "unread": unread,
            "rating": row.rating,
            "status": row.status,
        },
    });
    if let Some(dt) = chrono::DateTime::from_timestamp(row.updated_at, 0) {
        obj["date_published"] = json!(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    }
    obj
}

#[cfg(test)]
mod tests {
    // News Feed Spec (JSON Feed 1.1)
    //
    // GET /api/news delivers favorite threads with unread posts for the
    // news-server aggregator:
    // - Only explicitly starred threads (rating > 0)
    // - Only threads with unread posts (res_count > read_res)
    // - Archived threads are excluded; dead threads stay while unread remains
    // - Sorted by updated_at DESC, limited to 100
    // - date_published is updated_at (unix seconds) as RFC3339 UTC
    // - item id/url use the SPA route format /{server}/{board}/{thread_id}
    //
    // Tests call the real `get_news` handler directly (repo style, no tower)
    // against an in-memory DB.

    use super::*;
    use crate::config::Config;
    use axum::body::to_bytes;
    use axum::extract::State;
    use rusqlite::{params, Connection};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn
    }

    fn make_state(conn: Connection) -> AppState {
        let jar = crate::fivech::cookie_jar::open("/tmp/fivech_news_test_cookies.json");
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
                image_cache_dir: "/tmp/fivech-test-images".to_string(),
                cookies_path: "/tmp/fivech_news_test_cookies.json".to_string(),
                fivech_base_url: String::new(),
            },
            inflight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_favorite(
        conn: &Connection,
        thread_id: &str,
        title: &str,
        res_count: i64,
        read_res: i64,
        archived: i64,
        status: &str,
        updated_at: i64,
    ) {
        conn.execute(
            "INSERT INTO favorites
             (thread_id, server, board, board_name, title, res_count, read_res, rating, archived, status, updated_at)
             VALUES (?1, 'egg', 'applism', 'スマホアプリ', ?2, ?3, ?4, 1, ?5, ?6, ?7)",
            params![thread_id, title, res_count, read_res, archived, status, updated_at],
        )
        .unwrap();
    }

    async fn get_news_feed(state: &AppState) -> serde_json::Value {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        headers.insert("x-forwarded-host", "5ch.example.com".parse().unwrap());
        let resp = get_news(State(state.clone()), headers)
            .await
            .unwrap()
            .into_response();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        assert!(
            content_type.starts_with("application/feed+json"),
            "content-type must be application/feed+json, got: {content_type}"
        );
        let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn item_ids(feed: &serde_json::Value) -> Vec<String> {
        feed["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[tokio::test]
    async fn news_includes_only_threads_with_unread_posts() {
        let conn = setup();
        insert_favorite(&conn, "1001", "未読あり", 100, 50, 0, "active", 1700000000);
        insert_favorite(&conn, "1002", "既読済み", 100, 100, 0, "active", 1700000001);

        let feed = get_news_feed(&make_state(conn)).await;
        assert_eq!(item_ids(&feed), vec!["egg/applism/1001"]);
    }

    #[tokio::test]
    async fn news_excludes_unstarred_threads() {
        let conn = setup();
        insert_favorite(&conn, "1001", "星あり", 100, 50, 0, "active", 1700000000);
        insert_favorite(&conn, "1002", "星なし", 100, 50, 0, "active", 1700000001);
        conn.execute(
            "UPDATE favorites SET rating = 0 WHERE thread_id = '1002'",
            [],
        )
        .unwrap();

        let feed = get_news_feed(&make_state(conn)).await;
        assert_eq!(item_ids(&feed), vec!["egg/applism/1001"]);
    }

    #[tokio::test]
    async fn news_excludes_archived_threads() {
        let conn = setup();
        insert_favorite(
            &conn,
            "1001",
            "アーカイブ済み",
            100,
            50,
            1,
            "active",
            1700000000,
        );
        insert_favorite(
            &conn,
            "1002",
            "アクティブ",
            100,
            50,
            0,
            "active",
            1700000001,
        );

        let feed = get_news_feed(&make_state(conn)).await;
        assert_eq!(item_ids(&feed), vec!["egg/applism/1002"]);
    }

    #[tokio::test]
    async fn news_keeps_dead_threads_with_unread_posts() {
        // A dead thread with unread posts is still worth surfacing (読み残し).
        let conn = setup();
        insert_favorite(
            &conn,
            "1001",
            "dat落ち未読あり",
            1002,
            900,
            0,
            "dead",
            1700000000,
        );

        let feed = get_news_feed(&make_state(conn)).await;
        assert_eq!(item_ids(&feed), vec!["egg/applism/1001"]);
        assert_eq!(feed["items"][0]["_news"]["status"], "dead");
    }

    #[tokio::test]
    async fn news_sorted_by_updated_at_desc() {
        let conn = setup();
        insert_favorite(&conn, "old", "古い", 10, 5, 0, "active", 1700000000);
        insert_favorite(&conn, "new", "新しい", 10, 5, 0, "active", 1700002000);
        insert_favorite(&conn, "mid", "中間", 10, 5, 0, "active", 1700001000);

        let feed = get_news_feed(&make_state(conn)).await;
        assert_eq!(
            item_ids(&feed),
            vec!["egg/applism/new", "egg/applism/mid", "egg/applism/old"]
        );
    }

    #[tokio::test]
    async fn news_items_follow_json_feed_1_1_with_news_extension() {
        let conn = setup();
        insert_favorite(
            &conn,
            "1771127145",
            "【ブルアカ】総合 Part1",
            850,
            800,
            0,
            "active",
            1700000000,
        );
        conn.execute(
            "UPDATE favorites SET rating = 2 WHERE thread_id = '1771127145'",
            [],
        )
        .unwrap();

        let feed = get_news_feed(&make_state(conn)).await;
        assert_eq!(feed["version"], "https://jsonfeed.org/version/1.1");
        assert_eq!(feed["home_page_url"], "https://5ch.example.com");

        let item = &feed["items"][0];
        assert_eq!(item["id"], "egg/applism/1771127145");
        assert_eq!(
            item["url"], "https://5ch.example.com/egg/applism/1771127145",
            "url must match the SPA route /{{server}}/{{board}}/{{thread_id}}"
        );
        assert_eq!(item["title"], "【ブルアカ】総合 Part1");
        assert_eq!(
            item["content_text"], "未読50レス (800/850)",
            "JSON Feed 1.1 requires content_text or content_html on every item"
        );
        assert_eq!(
            item["date_published"], "2023-11-14T22:13:20Z",
            "updated_at unix seconds must be emitted as RFC3339 UTC"
        );
        assert_eq!(item["_news"]["service"], "5ch");
        assert_eq!(item["_news"]["board"], "スマホアプリ");
        assert_eq!(item["_news"]["res_count"], 850);
        assert_eq!(item["_news"]["read_res"], 800);
        assert_eq!(item["_news"]["unread"], 50);
        assert_eq!(item["_news"]["rating"], 2);
        assert_eq!(item["_news"]["status"], "active");
    }

    #[tokio::test]
    async fn news_respects_base_path_in_urls() {
        let conn = setup();
        insert_favorite(&conn, "1001", "スレ", 10, 5, 0, "active", 1700000000);
        let mut state = make_state(conn);
        state.config.base_path = "/5ch".to_string();

        let feed = get_news_feed(&state).await;
        assert_eq!(feed["home_page_url"], "https://5ch.example.com/5ch");
        assert_eq!(
            feed["items"][0]["url"],
            "https://5ch.example.com/5ch/egg/applism/1001"
        );
    }
}
