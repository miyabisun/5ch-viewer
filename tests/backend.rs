//! The real app router and SQLite behind its HTTP API, against the stand-in for 5ch
//! (itest-server, started by each test on ports the OS chooses): the reload's HEAD gate, the
//! board refresh, the rescue of the next thread, the image cache, and board-scoped NG rules.
//! What the page does with these answers is tested in jsdom (client/src/page/*.test.ts) and,
//! from the page to SQLite in one go, in tests/page.rs.
#![allow(clippy::unwrap_used)] // helpers outside #[test] fns
use reqwest::Method;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use itest::Itest;

#[path = "support/itest.rs"]
mod itest;

/// One thread of the stand-in and of the app.
struct Favorite {
    server: &'static str,
    board: &'static str,
    thread_id: &'static str,
    title: &'static str,
}

impl Favorite {
    fn key(&self) -> Value {
        json!({"server": self.server, "board": self.board, "thread_id": self.thread_id,
            "title": self.title})
    }
    fn api(&self, app: &Itest, rest: &str) -> String {
        format!(
            "{}/api/favorites/{}/{}/{}{rest}",
            app.app, self.server, self.board, self.thread_id
        )
    }
}

/// A favorite whose metadata says `res_count` and whose stored dat holds `blob_posts`.
async fn seed(app: &Itest, thread: &Favorite, res_count: u32, blob_posts: u32) {
    let mut body = thread.key();
    body["res_count"] = json!(res_count);
    body["blob_posts"] = json!(blob_posts);
    app.post(&format!("{}/_control/seed-favorite", app.app), body)
        .await;
}

/// What the stand-in's subject.txt reports (`res_count`) and its dat holds (`dat_posts`).
async fn program(app: &Itest, thread: &Favorite, res_count: u32, dat_posts: u32, extra: Value) {
    let mut body = thread.key();
    body["res_count"] = json!(res_count);
    body["dat_posts"] = json!(dat_posts);
    for (key, value) in extra.as_object().into_iter().flatten() {
        body[key] = value.clone();
    }
    app.post(&format!("{}/_control/thread", app.mock), body)
        .await;
}

async fn reload(app: &Itest, thread: &Favorite) -> Value {
    app.get(&thread.api(app, "/reload")).await
}

async fn subject_hits(app: &Itest, board: &str) -> Value {
    app.get(&format!("{}/_control/subject-hits/{board}", app.mock))
        .await
}

/// Asks `check` every 100ms until it is true; false after `timeout`.
async fn poll<F, Fut>(timeout: Duration, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if check().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

// ---- the reload's HEAD gate ----

const APPLISM: Favorite = Favorite {
    server: "egg",
    board: "applism",
    thread_id: "1771127145",
    title: "テストスレ",
};

#[tokio::test]
async fn reload_skips_the_dat_fetch_when_head_content_length_matches_stored_dat_bytes() {
    let app = Itest::start();
    seed(&app, &APPLISM, 5, 5).await;
    // The same 5 posts: HEAD's Content-Length matches the stored dat_bytes.
    program(&app, &APPLISM, 5, 5, json!({})).await;

    assert_eq!(reload(&app, &APPLISM).await["updated"], json!(false));
    // subject.txt was never fetched (reload uses HEAD, not subject.txt).
    assert_eq!(subject_hits(&app, "applism").await, json!(0));
}

#[tokio::test]
async fn reload_fetches_the_dat_when_head_content_length_grows() {
    let app = Itest::start();
    seed(&app, &APPLISM, 5, 5).await;
    program(&app, &APPLISM, 10, 10, json!({})).await;

    assert_eq!(reload(&app, &APPLISM).await["updated"], json!(true));
    assert_eq!(subject_hits(&app, "applism").await, json!(0));
}

// ---- status at the RES_WARN (980) and RES_DEAD (1002) boundaries ----

const DATBYTES: Favorite = Favorite {
    server: "egg",
    board: "datbytes",
    thread_id: "1771200000",
    title: "バイトサイズ検証スレ",
};

#[tokio::test]
async fn status_is_warned_at_res_count_980_and_stays_warned_after_re_fetch() {
    let app = Itest::start();
    seed(&app, &DATBYTES, 980, 980).await;
    program(&app, &DATBYTES, 981, 981, json!({})).await;

    let reloaded = reload(&app, &DATBYTES).await;
    assert_eq!(reloaded["updated"], json!(true), "{reloaded}");
    assert_eq!(reloaded["status"], json!("warned"), "{reloaded}");
}

#[tokio::test]
async fn status_is_warned_at_res_count_1000_byte_size_does_not_matter() {
    let app = Itest::start();
    seed(&app, &DATBYTES, 999, 999).await;
    program(&app, &DATBYTES, 1000, 1000, json!({})).await;

    let reloaded = reload(&app, &DATBYTES).await;
    assert_eq!(reloaded["updated"], json!(true), "{reloaded}");
    assert_eq!(reloaded["status"], json!("warned"), "{reloaded}");
}

// ---- the board refresh (refresh_board) ----

const fn on_applism(thread_id: &'static str) -> Favorite {
    Favorite {
        server: "egg",
        board: "applism",
        thread_id,
        title: "",
    }
}

/// The posts the stored dat of `thread` holds; -1 while it cannot be read.
async fn stored_posts(app: &Itest, thread: &Favorite) -> i64 {
    let (status, body) = app
        .send(Method::GET, &thread.api(app, "/dat"), &Value::Null)
        .await;
    if status == 200 {
        i64::try_from(body["res"].as_array().unwrap().len()).unwrap()
    } else {
        -1
    }
}

async fn refresh_board(app: &Itest) {
    app.post(
        &format!("{}/_control/refresh-board", app.app),
        json!({"server": "egg", "board": "applism"}),
    )
    .await;
}

#[tokio::test]
async fn refresh_bulk_downloads_every_grown_thread_on_a_board_with_one_subject_read() {
    let app = Itest::start();
    let threads = ["1771127001", "1771127002", "1771127003"].map(on_applism);
    for thread in &threads {
        seed(&app, thread, 100, 100).await;
        program(&app, thread, 130, 130, json!({})).await;
    }

    refresh_board(&app).await;

    // Every thread's stored dat catches up to 130.
    for thread in &threads {
        let caught_up = poll(Duration::from_secs(10), || async {
            stored_posts(&app, thread).await == 130
        })
        .await;
        assert!(caught_up, "{} did not reach 130 posts", thread.thread_id);
    }
    // subject.txt was read once for the whole board, not once per thread.
    assert_eq!(subject_hits(&app, "applism").await, json!(1));
}

#[tokio::test]
async fn refresh_skips_threads_whose_subject_count_did_not_grow() {
    let app = Itest::start();
    let thread = on_applism("1771127001");
    seed(&app, &thread, 100, 100).await;
    // subject matches the blob: no growth. The stand-in would serve 999 if asked.
    program(&app, &thread, 100, 999, json!({})).await;

    refresh_board(&app).await;

    let read = poll(Duration::from_secs(10), || async {
        subject_hits(&app, "applism").await == json!(1)
    })
    .await;
    assert!(read, "subject.txt was not read");
    assert_eq!(stored_posts(&app, &thread).await, 100);
}

// ---- find-next ----

const SRC: Favorite = Favorite {
    server: "egg",
    board: "applism",
    thread_id: "1771127100",
    title: "ブルアカ Part5862",
};
const NEXT: Favorite = Favorite {
    server: "egg",
    board: "applism",
    thread_id: "1771127101",
    title: "ブルアカ Part5863",
};

async fn favorites(app: &Itest) -> Vec<Value> {
    app.get(&format!("{}/api/favorites", app.app))
        .await
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn find_next_registers_the_successor_thread_and_returns_found_true() {
    let app = Itest::start();
    seed(&app, &SRC, 1002, 1).await;
    // Only the successor is listed in subject (the source already dropped off).
    program(&app, &NEXT, 5, 1, json!({})).await;

    let found = app.post(&SRC.api(&app, "/find-next"), Value::Null).await;
    assert_eq!(found["found"], json!(true), "{found}");
    assert_eq!(found["thread_id"], json!(NEXT.thread_id), "{found}");
    assert_eq!(found["title"], json!(NEXT.title), "{found}");

    // The successor is now a real favorite row.
    let listed = favorites(&app).await;
    let next = listed
        .iter()
        .find(|f| f["thread_id"] == json!(NEXT.thread_id))
        .expect("the successor is listed");
    assert_eq!(next["title"], json!(NEXT.title));
}

#[tokio::test]
async fn find_next_returns_found_false_when_no_successor_is_in_subject() {
    let app = Itest::start();
    seed(&app, &SRC, 1002, 1).await;

    let found = app.post(&SRC.api(&app, "/find-next"), Value::Null).await;
    assert_eq!(found["found"], json!(false), "{found}");
    assert!(favorites(&app)
        .await
        .iter()
        .all(|f| f["thread_id"] != json!(NEXT.thread_id)));
}

// ---- the image cache ----

const IMGTEST: Favorite = Favorite {
    server: "egg",
    board: "imgtest",
    thread_id: "9900000001",
    title: "画像キャッシュテストスレ",
};

/// The stand-in's picture `file`: its URL, and the path the cache serves it at.
fn mock_image(app: &Itest, file: &str) -> (String, String) {
    let host = app.mock.trim_start_matches("http://");
    (
        format!("{}/mock/img/{file}", app.mock),
        format!("{host}/mock/img/{file}"),
    )
}

async fn seed_image(app: &Itest, url: &str, path: &str) {
    app.post(
        &format!("{}/_control/seed-image", app.app),
        json!({"url": url, "path": path, "mime": "image/png", "mosaic": 0}),
    )
    .await;
}

async fn image_status(app: &Itest, path: &str) -> u16 {
    app.send(
        Method::GET,
        &format!("{}/api/images/{path}", app.app),
        &Value::Null,
    )
    .await
    .0
}

async fn image_hits(app: &Itest, file: &str) -> Value {
    app.get(&format!("{}/_control/image-hits/{file}", app.mock))
        .await
}

#[tokio::test]
async fn a_seeded_image_is_served_via_get_api_images() {
    let app = Itest::start();
    seed_image(
        &app,
        "https://example.com/test-image.png",
        "example.com/test-image.png",
    )
    .await;

    let (status, headers, _) = app
        .answer(
            Method::GET,
            &format!("{}/api/images/example.com/test-image.png", app.app),
            &Value::Null,
        )
        .await;
    assert_eq!(status, 200);
    let header = |name: &str| headers[name].to_str().unwrap().to_owned();
    assert!(header("content-type").contains("image/png"));
    assert_eq!(header("x-content-type-options"), "nosniff");
    assert!(header("cache-control").contains("max-age=31536000"));
}

#[tokio::test]
async fn a_dat_image_url_triggers_prefetch_and_is_cached_after_reload() {
    let app = Itest::start();
    let (url, path) = mock_image(&app, "test1.png");
    seed(&app, &IMGTEST, 5, 5).await;
    program(&app, &IMGTEST, 10, 10, json!({"image_urls": [url]})).await;

    assert_eq!(reload(&app, &IMGTEST).await["updated"], json!(true));
    let cached = poll(Duration::from_secs(3), || async {
        image_status(&app, &path).await == 200
    })
    .await;
    assert!(cached, "the picture was not cached");
    assert_eq!(image_hits(&app, "test1.png").await, json!(1));
}

#[tokio::test]
async fn a_cached_image_is_not_re_downloaded_on_a_second_reload() {
    let app = Itest::start();
    let (url, path) = mock_image(&app, "test2.png");
    seed(&app, &IMGTEST, 5, 5).await;
    program(&app, &IMGTEST, 10, 10, json!({"image_urls": [url]})).await;
    reload(&app, &IMGTEST).await;
    poll(Duration::from_secs(3), || async {
        image_status(&app, &path).await == 200
    })
    .await;
    assert_eq!(image_hits(&app, "test2.png").await, json!(1));

    // The thread grows; the same picture is in it.
    program(&app, &IMGTEST, 15, 15, json!({"image_urls": [url]})).await;
    reload(&app, &IMGTEST).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(image_hits(&app, "test2.png").await, json!(1));
}

#[tokio::test]
async fn an_image_exceeding_5_mb_is_rejected_and_not_cached() {
    let app = Itest::start();
    let (url, path) = mock_image(&app, "huge.png");
    app.post(
        &format!("{}/_control/image-size", app.mock),
        json!({"file": "huge.png", "size": 6 * 1024 * 1024}),
    )
    .await;
    seed(&app, &IMGTEST, 5, 5).await;
    program(&app, &IMGTEST, 10, 10, json!({"image_urls": [url]})).await;
    reload(&app, &IMGTEST).await;

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(image_status(&app, &path).await, 404);
}

#[tokio::test]
async fn an_image_with_a_text_html_mime_type_is_rejected_and_not_cached() {
    let app = Itest::start();
    let (url, path) = mock_image(&app, "html-page.png");
    app.post(
        &format!("{}/_control/image-content-type", app.mock),
        json!({"file": "html-page.png", "content_type": "text/html; charset=utf-8"}),
    )
    .await;
    seed(&app, &IMGTEST, 5, 5).await;
    program(&app, &IMGTEST, 10, 10, json!({"image_urls": [url]})).await;
    reload(&app, &IMGTEST).await;

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(image_status(&app, &path).await, 404);
}

#[tokio::test]
async fn ssrf_a_private_ip_image_is_not_downloaded() {
    let app = Itest::start();
    seed(&app, &IMGTEST, 5, 5).await;
    program(
        &app,
        &IMGTEST,
        10,
        10,
        json!({"image_urls": ["http://10.0.0.1/x.png"]}),
    )
    .await;

    assert_eq!(reload(&app, &IMGTEST).await["updated"], json!(true));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(image_status(&app, "10.0.0.1/x.png").await, 404);
}

#[tokio::test]
async fn mosaic_post_sets_it_delete_clears_it_reflected_in_mosaic_urls() {
    let app = Itest::start();
    let url = "https://example.com/mosaic-test.png";
    seed(&app, &IMGTEST, 5, 5).await;
    seed_image(&app, url, "example.com/mosaic-test.png").await;
    program(&app, &IMGTEST, 10, 5, json!({"image_urls": [url]})).await;
    reload(&app, &IMGTEST).await;

    let mosaics = || async {
        app.get(&IMGTEST.api(&app, "/dat")).await["mosaic_urls"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    assert!(!mosaics().await.contains(&json!(url)));

    let mosaic = format!("{}/api/images/mosaic", app.app);
    let set = app.post(&mosaic, json!({"url": url})).await;
    assert_eq!(set["ok"], json!(true), "{set}");
    assert!(mosaics().await.contains(&json!(url)));

    let (status, _) = app
        .send(Method::DELETE, &mosaic, &json!({"url": url}))
        .await;
    assert_eq!(status, 200);
    assert!(!mosaics().await.contains(&json!(url)));
}

#[tokio::test]
async fn the_image_is_served_regardless_of_its_mosaic_flag() {
    let app = Itest::start();
    let url = "https://example.com/blob-mosaic.png";
    seed_image(&app, url, "example.com/blob-mosaic.png").await;
    app.post(
        &format!("{}/api/images/mosaic", app.app),
        json!({"url": url}),
    )
    .await;

    let (status, headers, _) = app
        .answer(
            Method::GET,
            &format!("{}/api/images/example.com/blob-mosaic.png", app.app),
            &Value::Null,
        )
        .await;
    assert_eq!(status, 200);
    assert!(headers["content-type"]
        .to_str()
        .unwrap()
        .contains("image/png"));
}

#[tokio::test]
async fn an_uncached_image_path_returns_404() {
    let app = Itest::start();
    assert_eq!(image_status(&app, "nonexistent.com/no-such.png").await, 404);
}

#[tokio::test]
async fn mosaic_post_rejects_an_invalid_url() {
    let app = Itest::start();
    let (status, _) = app
        .send(
            Method::POST,
            &format!("{}/api/images/mosaic", app.app),
            &json!({"url": "ftp://bad.com/x.png"}),
        )
        .await;
    assert_eq!(status, 400);
}

// ---- board-scoped NG rules through the real router and SQLite ----

/// The rules at `url`, without their wall-clock `created_at`.
async fn rules(app: &Itest, url: &str) -> Vec<Value> {
    app.get(url)
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| {
            let mut rule = rule.clone();
            rule.as_object_mut().unwrap().remove("created_at");
            rule
        })
        .collect()
}

fn ng(app: &Itest, kind: &str, query: &str) -> String {
    format!("{}/api/ng-{kind}{query}", app.app)
}

const WORD_QUERY: &str = "?server=egg&board=applism&kind=text&pattern=%E6%9C%AC%E6%96%872";

#[tokio::test]
async fn an_ng_word_round_trips_through_the_real_api_and_sqlite() {
    let app = Itest::start();
    let rule = json!({"server": "egg", "board": "applism", "kind": "text", "pattern": "本文2"});
    app.post(&ng(&app, "words", ""), rule.clone()).await;
    assert_eq!(rules(&app, &ng(&app, "words", "")).await, vec![rule]);

    let (status, _) = app
        .send(Method::DELETE, &ng(&app, "words", WORD_QUERY), &Value::Null)
        .await;
    assert_eq!(status, 200);
    assert!(rules(&app, &ng(&app, "words", "")).await.is_empty());

    // Deleting a rule that is not there is a 404, not a silent success.
    let (status, _) = app
        .send(Method::DELETE, &ng(&app, "words", WORD_QUERY), &Value::Null)
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn an_ng_wacchoi_whose_suffix_contains_a_slash_round_trips() {
    let app = Itest::start();
    // The suffix is drawn from the base64 alphabet; '/' used to be refused with a 400.
    let entry = json!({"suffix": "/dMI", "board": "applism", "week_key": "2024/12/29",
        "wacchoi": "86fb-/dMI"});
    app.post(&ng(&app, "wacchoi", ""), entry.clone()).await;
    assert_eq!(
        rules(&app, &ng(&app, "wacchoi", "")).await,
        vec![entry.clone()]
    );

    let query = "?suffix=%2FdMI&board=applism&week_key=2024%2F12%2F29";
    let (status, _) = app
        .send(Method::DELETE, &ng(&app, "wacchoi", query), &Value::Null)
        .await;
    assert_eq!(status, 200);
    assert!(rules(&app, &ng(&app, "wacchoi", "")).await.is_empty());

    // A suffix outside the token alphabet is still rejected and stores nothing.
    let mut bad = entry;
    bad["suffix"] = json!("ab-c");
    let (status, _) = app.send(Method::POST, &ng(&app, "wacchoi", ""), &bad).await;
    assert_eq!(status, 400);
    assert!(rules(&app, &ng(&app, "wacchoi", "")).await.is_empty());
}

#[tokio::test]
async fn the_same_ng_word_on_two_boards_is_two_independent_rules() {
    let app = Itest::start();
    let on_a = json!({"server": "egg", "board": "applism", "kind": "text", "pattern": "本文2"});
    let mut on_b = on_a.clone();
    on_b["board"] = json!("other");
    app.post(&ng(&app, "words", ""), on_a.clone()).await;
    app.post(&ng(&app, "words", ""), on_b.clone()).await;
    let listed = rules(&app, &ng(&app, "words", "")).await;
    assert!(
        listed.contains(&on_a) && listed.contains(&on_b),
        "{listed:?}"
    );

    // Removing board A's rule leaves board B's in place.
    let (status, _) = app
        .send(Method::DELETE, &ng(&app, "words", WORD_QUERY), &Value::Null)
        .await;
    assert_eq!(status, 200);
    assert_eq!(rules(&app, &ng(&app, "words", "")).await, vec![on_b]);
}

#[tokio::test]
async fn re_adding_the_same_scope_kind_pattern_keeps_a_single_row() {
    let app = Itest::start();
    let rule = json!({"server": "egg", "board": "applism", "kind": "text", "pattern": "本文2"});
    app.post(&ng(&app, "words", ""), rule.clone()).await;
    app.post(&ng(&app, "words", ""), rule.clone()).await;
    assert_eq!(rules(&app, &ng(&app, "words", "")).await.len(), 1);

    // A different kind with the same pattern is a separate rule.
    let mut regex = rule;
    regex["kind"] = json!("regex");
    app.post(&ng(&app, "words", ""), regex).await;
    assert_eq!(rules(&app, &ng(&app, "words", "")).await.len(), 2);
}

#[tokio::test]
async fn an_empty_pattern_and_an_unknown_kind_are_rejected_and_store_nothing() {
    let app = Itest::start();
    for rule in [
        json!({"server": "egg", "board": "applism", "kind": "text", "pattern": ""}),
        json!({"server": "egg", "board": "applism", "kind": "glob", "pattern": "本文2"}),
    ] {
        let (status, _) = app.send(Method::POST, &ng(&app, "words", ""), &rule).await;
        assert_eq!(status, 400, "{rule}");
    }
    assert!(rules(&app, &ng(&app, "words", "")).await.is_empty());
}

#[tokio::test]
async fn an_ng_id_cannot_be_created_without_a_board_scope() {
    let app = Itest::start();
    // The pre-migration global shape (ng_id alone) is no longer a valid request body.
    let (status, _) = app
        .send(
            Method::POST,
            &ng(&app, "ids", ""),
            &json!({"ng_id": "abc1"}),
        )
        .await;
    assert!(!(200..300).contains(&status), "{status}");
    assert!(rules(&app, &ng(&app, "ids", "")).await.is_empty());

    let scoped = json!({"server": "egg", "board": "applism", "ng_id": "abc1"});
    app.post(&ng(&app, "ids", ""), scoped.clone()).await;
    assert_eq!(rules(&app, &ng(&app, "ids", "")).await, vec![scoped]);
}
