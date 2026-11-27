//! From the page to SQLite and back in one go: the page talks to the real router on an
//! in-memory database, and the stand-in for 5ch serves the dats (no answers are faked).
use super::{click, fire, shows, until_is, visit, Itest, Page, Test};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::Arc;

const THREAD: &str = "/api/favorites/egg/applism/1771127145";

/// A favorite on `board` whose stored dat holds `blob_posts` (本文N, ID:abcN).
async fn seed(
    app: &Itest,
    board: &str,
    thread_id: &str,
    title: &str,
    res_count: u32,
    blob_posts: u32,
) {
    app.post(
        &format!("{}/_control/seed-favorite", app.app),
        json!({"server": "egg", "board": board, "thread_id": thread_id, "title": title,
            "res_count": res_count, "blob_posts": blob_posts}),
    )
    .await;
}

/// What the stand-in's subject.txt reports and its dat holds, for the thread of `THREAD`.
async fn program(app: &Itest, title: &str, count: u32) {
    app.post(
        &format!("{}/_control/thread", app.mock),
        json!({"server": "egg", "board": "applism", "thread_id": "1771127145", "title": title,
            "res_count": count, "dat_posts": count, "gone": false}),
    )
    .await;
}

/// The button named `name`, by its text or its label.
fn button(name: &str) -> String {
    format!(
        "[...document.querySelectorAll('button')].find((b) => t.shows(b) && \
         (b.textContent.trim() === '{name}' || b.getAttribute('aria-label') === '{name}'))"
    )
}

/// The struck-through header of a hidden post that says `text`.
fn ng_header(text: &str) -> String {
    format!(
        "[...document.querySelectorAll('del.ng')].find((el) => el.textContent.includes('{text}'))"
    )
}

async fn gone(page: &Page, element: &str) {
    until_is(
        page,
        &format!("{element} is gone"),
        &format!("return ({element}) ?? null;"),
        Value::Null,
    )
    .await;
}

async fn footer_refresh_heals_a_drifted_favorite(page: Arc<Page>, app: Arc<Itest>) {
    // Drifted: the metadata says 117, the stored dat holds 111; 5ch has 117.
    seed(&app, "applism", "1771127145", "テストスレ", 117, 111).await;
    program(&app, "テストスレ", 117).await;

    visit(&page, &app, "/").await;
    click(&page, "t.$('.info')", "left").await;
    // The footer 更新 runs the real HEAD -> dat -> DB replace -> getDat flow.
    fire(
        &page,
        "[...t.$('.detail-pane').querySelectorAll('button')].find((b) => b.textContent.trim() === '更新' || b.getAttribute('aria-label') === '更新')",
        "click",
    )
    .await;
    shows(&page, "t.text('本文117')").await;
    shows(&page, "t.text('本文112')").await;
}

async fn a_post_from_the_viewer_is_not_unread(page: Arc<Page>, app: Arc<Itest>) {
    let api = format!("{}{THREAD}", app.app);
    seed(&app, "applism", "1771127145", "未読テストスレ", 20, 20).await;
    app.post(&format!("{api}/progress"), json!({"read_res": 20}))
        .await;
    let (status, _) = app
        .send(
            Method::PATCH,
            &format!("{api}/rating"),
            &json!({"rating": 3}),
        )
        .await;
    assert_eq!(status, 200, "status");

    // The user writes res 21; 5ch's dat then holds it.
    app.post(
        &format!("{}/_control/bbs-cgi", app.mock),
        json!({"next_res": 21}),
    )
    .await;
    program(&app, "未読テストスレ", 21).await;
    app.post(&format!("{api}/post"), json!({"message": "テスト"}))
        .await;
    app.get(&format!("{api}/reload")).await;

    let listed = app.get(&format!("{}/api/favorites", app.app)).await;
    let row = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["thread_id"] == json!("1771127145"))
        .unwrap()
        .clone();
    assert_eq!(
        [&row["res_count"], &row["read_res"], &row["own_res"]],
        [&json!(21), &json!(20), &json!([21])],
        "{row}"
    );
    let news = format!("{}/api/news", app.app);
    assert_eq!(
        app.get(&news).await["items"],
        json!([]),
        "app.get(&news).await[\"items\"]"
    );
    visit(&page, &app, "/").await;
    shows(
        &page,
        "[...document.querySelectorAll('.list-pane .thread')].find((el) => el.textContent.includes('未読テストスレ'))",
    )
    .await;
    gone(&page, "t.$('.list-pane .thread .unread')").await;

    // Someone else replies: only that post is unread.
    program(&app, "未読テストスレ", 22).await;
    app.get(&format!("{api}/reload")).await;
    let items = app.get(&news).await["items"].clone();
    assert_eq!(items[0]["id"], json!("egg/applism/1771127145"), "{items}");
    assert_eq!(items.as_array().unwrap().len(), 1, "{items}");
    assert_eq!(items[0]["_news"]["unread"], json!(1), "{items}");
    page.history("refresh").await;
    until_is(
        &page,
        "the unread badge",
        "return t.$('.list-pane .thread .unread')?.textContent.trim() ?? null;",
        json!("1"),
    )
    .await;
}

/// The rules at `/api/ng-<kind>`, without their wall-clock `created_at`.
async fn rules(app: &Itest, kind: &str) -> Value {
    let mut listed = app.get(&format!("{}/api/ng-{kind}", app.app)).await;
    for rule in listed.as_array_mut().unwrap() {
        rule.as_object_mut().unwrap().remove("created_at");
    }
    listed
}

async fn an_ng_word_hides_posts_on_its_board_only(page: Arc<Page>, app: Arc<Itest>) {
    seed(&app, "applism", "1771127145", "板Aのスレ1", 3, 3).await;
    seed(&app, "applism", "1771127146", "板Aのスレ2", 3, 3).await;
    seed(&app, "other", "1771127147", "板Bのスレ", 3, 3).await;
    app.post(
        &format!("{}/api/ng-words", app.app),
        json!({"server": "egg", "board": "applism", "kind": "text", "pattern": "本文2"}),
    )
    .await;

    // Board A, thread 1: post 2 is hidden behind the NG Word disclosure.
    visit(&page, &app, "/egg/applism/1771127145").await;
    shows(&page, "t.text('本文1')").await;
    shows(&page, &ng_header("2 NG Word")).await;
    gone(&page, "t.text('本文2')").await;
    // Board A, thread 2: the same rule applies without registering it again.
    visit(&page, &app, "/egg/applism/1771127146").await;
    shows(&page, "t.text('本文1')").await;
    gone(&page, "t.text('本文2')").await;
    // Board B: identical body, no rule: shown.
    visit(&page, &app, "/egg/other/1771127147").await;
    shows(&page, "t.text('本文2')").await;
    gone(&page, "t.$('del.ng')").await;
}

async fn an_ng_id_hides_posts_on_its_board_only(page: Arc<Page>, app: Arc<Itest>) {
    seed(&app, "applism", "1771127145", "板Aのスレ1", 3, 3).await;
    seed(&app, "other", "1771127147", "板Bのスレ", 3, 3).await;
    // The seeded post N has ID:abcN, so abc2 is post 2 on both boards.
    app.post(
        &format!("{}/api/ng-ids", app.app),
        json!({"server": "egg", "board": "applism", "ng_id": "abc2"}),
    )
    .await;

    visit(&page, &app, "/egg/applism/1771127145").await;
    shows(&page, &ng_header("2 NG ID")).await;
    gone(&page, "t.text('本文2')").await;
    visit(&page, &app, "/egg/other/1771127147").await;
    shows(&page, "t.text('本文2')").await;
    gone(&page, "t.$('del.ng')").await;
}

async fn registering_and_removing_re_evaluate_the_view(page: Arc<Page>, app: Arc<Itest>) {
    seed(&app, "applism", "1771127145", "板Aのスレ1", 3, 3).await;
    visit(&page, &app, "/egg/applism/1771127145").await;
    shows(&page, "t.text('本文2')").await;

    // A word plus the poster ID from the res body's menu.
    fire(&page, "t.text('本文2')", "contextmenu").await;
    fire(&page, &button("NG Word に追加"), "click").await;
    shows(&page, "t.$('[data-testid=\"ng-word-form\"]')").await;
    fire(&page, &button("追加"), "click").await;

    // Hidden at once, and both rules reached SQLite with this board's scope.
    gone(&page, "t.text('本文2')").await;
    assert_eq!(
        rules(&app, "words").await,
        json!([{"server": "egg", "board": "applism", "kind": "text", "pattern": "本文2"}]),
        "rules(&app, \"words\").await"
    );
    assert_eq!(
        rules(&app, "ids").await,
        json!([{"server": "egg", "board": "applism", "ng_id": "abc2"}]),
        "rules(&app, \"ids\").await"
    );

    // NG ID wins the disclosure; removing it reveals the remaining NG Word reason.
    fire(&page, &ng_header("2 NG ID"), "contextmenu").await;
    fire(&page, &button("NG IDから削除"), "click").await;
    shows(&page, &ng_header("2 NG Word")).await;
    assert_eq!(
        rules(&app, "ids").await,
        json!([]),
        "rules(&app, \"ids\").await"
    );

    // Removing the word too brings the body back.
    fire(&page, &ng_header("2 NG Word"), "contextmenu").await;
    fire(&page, &button("NG Wordから削除"), "click").await;
    shows(&page, "t.text('本文2')").await;
    assert_eq!(
        rules(&app, "words").await,
        json!([]),
        "rules(&app, \"words\").await"
    );
}

pub const TESTS: &[Test] = &[
    test!(
        "footer refresh heals a drifted favorite (meta 117 / blob 111 -> 117 posts shown)",
        footer_refresh_heals_a_drifted_favorite
    ),
    test!(
        "a post from the viewer is not unread in the list or the news feed",
        a_post_from_the_viewer_is_not_unread
    ),
    test!(
        "an NG word registered on one board hides its posts in every thread of that board only",
        an_ng_word_hides_posts_on_its_board_only
    ),
    test!(
        "an NG ID registered on one board hides its posts on that board only",
        an_ng_id_hides_posts_on_its_board_only
    ),
    test!(
        "registering from the modal and removing from the NG menu both re-evaluate the view",
        registering_and_removing_re_evaluate_the_view
    ),
];
