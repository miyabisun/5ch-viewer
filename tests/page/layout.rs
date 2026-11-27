//! Where the parts of the page lie: the two columns on a PC and the single view on a phone,
//! the pinned title, the footer's buttons, and modals that fit a phone's width.
use super::{
    box_of, click, dat, fake, favorite, fire, hidden, shows, thread_routes, until_is, visit, Itest,
    Page, Test,
};
use serde_json::{json, Value};
use std::sync::Arc;

const THREAD_PATH: &str = "/egg/applism/1771127145";

/// The list of one favorite whose thread holds `count` posts.
async fn one_thread(page: &Page, count: u32) {
    let favorites = json!([favorite("1771127145", "テストスレ", count, 0)]);
    fake(
        page,
        thread_routes(&favorites, &dat("テストスレ", count, 0)),
    )
    .await;
}

async fn top_page_loads(page: Arc<Page>, app: Arc<Itest>) {
    fake(&page, json!([{"path": "/api/favorites", "json": []}])).await;
    visit(&page, &app, "/").await;
    page.until(
        "the title names the app",
        "return document.title.includes('viewer-of-5ch');",
        Value::Null,
    )
    .await;
}

// Responsive layout: classic 2ch-style 2 columns on PC (list left + detail right, both
// visible at once), single view on phones. Breakpoint is 768px.
async fn pc_selecting_keeps_the_list(page: Arc<Page>, app: Arc<Itest>) {
    page.look("light", 1024, 800, 1.0, false).await;
    one_thread(&page, 1).await;
    visit(&page, &app, "/").await;
    // Detail pane starts with the placeholder; the list is visible.
    shows(&page, "t.$('.thread .info')").await;
    shows(&page, "t.$('.placeholder')").await;

    click(&page, "t.$('.thread .info')", "left").await;
    // Both panes are visible at once, and the URL is synced.
    shows(&page, "t.text('本文1')").await;
    shows(&page, "t.$('.thread .info')").await;
    until_is(
        &page,
        "the path",
        "return location.pathname;",
        json!(THREAD_PATH),
    )
    .await;
    until_is(
        &page,
        "the placeholder",
        "return t.$('.placeholder');",
        Value::Null,
    )
    .await;
}

async fn pc_direct_thread_url(page: Arc<Page>, app: Arc<Itest>) {
    page.look("light", 1024, 800, 1.0, false).await;
    one_thread(&page, 1).await;
    visit(&page, &app, THREAD_PATH).await;
    shows(&page, "t.$('.thread .info')").await;
    shows(&page, "t.text('本文1')").await;
}

async fn pc_scrolling_the_detail_keeps_the_list(page: Arc<Page>, app: Arc<Itest>) {
    page.look("light", 1024, 800, 1.0, false).await;
    one_thread(&page, 60).await;
    visit(&page, &app, THREAD_PATH).await;
    shows(&page, "t.text('本文1')").await;
    let before = page
        .js("return t.$('.list-pane').scrollTop;", Value::Null)
        .await;

    // Scroll the thread body (the scroll container) to the bottom.
    page.until(
        "the thread body scrolls",
        "const body = t.$('.thread-body'); body.scrollTo(0, body.scrollHeight); \
         return body.scrollTop > 0;",
        Value::Null,
    )
    .await;

    // The list pane did not scroll, and the favorite is still in sight.
    let after = page
        .js("return t.$('.list-pane').scrollTop;", Value::Null)
        .await;
    assert_eq!(after, before, "the list pane scrolled");
    shows(&page, "t.$('.thread .info')").await;
    // The window does not scroll: main is fixed to the viewport height.
    let window = page
        .js(
            "return [scrollY, document.documentElement.scrollHeight <= innerHeight];",
            Value::Null,
        )
        .await;
    assert_eq!(window, json!([0, true]), "the window scrolls");
}

async fn phone_opening_hides_the_list(page: Arc<Page>, app: Arc<Itest>) {
    page.look("light", 390, 800, 1.0, false).await;
    one_thread(&page, 1).await;
    visit(&page, &app, "/").await;
    shows(&page, "t.$('.thread .info')").await;
    // No desktop placeholder on phones (present in DOM but hidden via CSS).
    hidden(&page, "t.$('.placeholder')").await;

    click(&page, "t.$('.thread .info')", "left").await;
    shows(&page, "t.text('本文1')").await;
    // The list pane is collapsed in single view.
    hidden(&page, "t.$('.list-pane')").await;
    hidden(&page, "t.$('.thread .info')").await;
    until_is(
        &page,
        "the path",
        "return location.pathname;",
        json!(THREAD_PATH),
    )
    .await;
}

/// Scrolls the thread body to its bottom, and waits until it has moved.
async fn scroll_to_the_bottom(page: &Page) {
    page.until(
        "the thread body scrolls",
        "const body = t.$('.thread-body'); body.scrollTo(0, body.scrollHeight); \
         return body.scrollTop > 0;",
        Value::Null,
    )
    .await;
}

async fn the_title_stays_pinned(page: Arc<Page>, app: Arc<Itest>) {
    one_thread(&page, 60).await;
    visit(&page, &app, THREAD_PATH).await;
    let title = "document.querySelector('[data-testid=\"thread-title\"]')";
    shows(&page, title).await;
    until_is(
        &page,
        "the title",
        &format!("return {title}.textContent.trim();"),
        json!("テストスレ"),
    )
    .await;

    scroll_to_the_bottom(&page).await;
    // Still in the viewport, near the top.
    let top = box_of(&page, title).await[1];
    assert!((0.0..120.0).contains(&top), "the title is at {top}");
}

async fn phone_title_flush_below_the_nav(page: Arc<Page>, app: Arc<Itest>) {
    page.look("light", 390, 700, 1.0, false).await;
    one_thread(&page, 60).await;
    visit(&page, &app, THREAD_PATH).await;
    let title = "document.querySelector('[data-testid=\"thread-title\"]')";
    shows(&page, title).await;

    scroll_to_the_bottom(&page).await;
    let nav_bottom = box_of(&page, "t.$('nav')").await[5];
    let title_top = box_of(&page, title).await[1];
    assert!(
        (title_top - nav_bottom).abs() <= 1.0,
        "the title is at {title_top}, the nav ends at {nav_bottom}"
    );
}

async fn phone_write_left_refresh_right(page: Arc<Page>, app: Arc<Itest>) {
    page.look("light", 390, 700, 1.0, false).await;
    one_thread(&page, 40).await;
    visit(&page, &app, THREAD_PATH).await;
    shows(&page, "t.text('本文1')").await;

    let write = box_of(&page, "[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === '書き込む' || b.getAttribute('aria-label') === '書き込む')").await;
    let refresh = box_of(&page, "t.$('.detail-pane') && [...t.$('.detail-pane').querySelectorAll('button')].find((b) => b.textContent.trim() === '更新' || b.getAttribute('aria-label') === '更新')").await;
    assert!(
        write[0] < refresh[0],
        "書き込む at {}, 更新 at {}",
        write[0],
        refresh[0]
    );
}

// ---- modals at a phone's width ----

const LONG_ID: &str = "verylongidAAAAAAAAAAAAAAAAAAAA";
const LONG_URL: &str = "https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WACCHOI_NAME: &str = "iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>";

/// The answers of a thread whose posts are as wide as a post gets: a long ID, a long URL, a
/// wacchoi name, and a deep >>N chain.
async fn wide_thread(page: &Page) {
    let post = |num: u32, name: &str, id: &str, body: String| {
        json!({"num": num, "name": name, "mail": "", "id": id, "body": body,
            "date": format!("2025/01/01(水) 00:0{}:00.00 ID:{id}", num - 1)})
    };
    let res = json!([
        post(1, WACCHOI_NAME, LONG_ID, format!("本文1 {LONG_URL}")),
        post(2, WACCHOI_NAME, LONG_ID, "&gt;&gt;1 本文2".into()),
        post(3, "名無し", "c", "&gt;&gt;2 本文3".into()),
        post(4, "名無し", "d", "&gt;&gt;3 本文4".into()),
        post(5, "名無し", "e", "&gt;&gt;4 本文5".into()),
        post(6, "名無し", "f", format!("&gt;&gt;5 {LONG_URL}")),
    ]);
    let found = json!([{
        "thread_id": "1771127145",
        "title": "とても長いスレタイトルxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "res": [{"num": 1, "name": "名無し", "mail": "", "date": format!("2025/01/01 ID:{LONG_ID}"),
            "body": format!("検索ヒット本文 {LONG_URL}"), "id": LONG_ID}],
    }]);
    let mut fav = favorite("1771127145", "モーダルオーバーフローテストスレ", 6, 0);
    fav["res_count"] = json!(6);
    fake(
        page,
        json!([
            {"path": "/api/favorites", "json": [fav]},
            {"pattern": "/dat$", "json": {"title": "モーダルオーバーフローテストスレ", "res_count": 6,
                "read_res": 0, "status": "active", "res": res}},
            {"pattern": "/reload$", "json": {"res_count": 6, "read_res": 0, "status": "active"}},
            {"pattern": "^/api/(ng-ids|ng-words|ng-wacchoi)$", "method": "GET", "json": []},
            {"pattern": "^/api/(ng-ids|ng-words|ng-wacchoi)$", "json": {"ok": true}},
            {"pattern": "/(id|wacchoi)-search$", "json": found},
        ]),
    )
    .await;
}

/// Opens the wide thread in a window of 390 by 700.
async fn on_a_phone(page: &Page, app: &Itest) {
    page.look("light", 390, 700, 1.0, false).await;
    wide_thread(page).await;
    visit(page, app, THREAD_PATH).await;
    shows(page, "t.$('.res[data-res=\"1\"] .body')").await;
}

/// The modal open now fits the window: neither the document, the modal nor its content
/// scrolls sideways, and no field or button reaches past the right edge.
async fn fits(page: &Page) {
    shows(page, "t.$('.modal')").await;
    let measured = page
        .js(
            "const doc = document.documentElement; \
             const m = t.$('.modal'); const c = t.$('.modal-content'); \
             const fields = [...m.querySelectorAll('input, textarea, button')] \
               .map((el) => el.getBoundingClientRect().right); \
             return { width: innerWidth, doc: [doc.scrollWidth, doc.clientWidth], \
               modal: [m.scrollWidth, m.clientWidth], content: [c.scrollWidth, c.clientWidth], \
               fields };",
            Value::Null,
        )
        .await;
    for part in ["doc", "modal", "content"] {
        let [scroll, client] =
            [&measured[part][0], &measured[part][1]].map(|n| n.as_f64().unwrap());
        assert!(scroll <= client + 1.0, "the {part} overflows: {measured}");
    }
    let width = measured["width"].as_f64().unwrap();
    for right in measured["fields"].as_array().unwrap() {
        assert!(
            right.as_f64().unwrap() <= width + 1.0,
            "a field overflows: {measured}"
        );
    }
}

/// The button named `name` (by its text or its label), as a script.
fn button(name: &str) -> String {
    format!(
        "[...document.querySelectorAll('button')].find((b) => t.shows(b) && \
         (b.textContent.trim() === '{name}' || b.getAttribute('aria-label') === '{name}'))"
    )
}

async fn write_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, &button("書き込む"), "click").await;
    shows(&page, "t.$('.post-form')").await;
    fits(&page).await;
}

async fn id_list_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, "t.$('.resid')", "click").await;
    shows(&page, "t.$('[data-testid=\"id-list\"]')").await;
    fits(&page).await;
}

async fn id_menu_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, "t.$('.resid')", "contextmenu").await;
    shows(&page, "t.$('[data-testid=\"id-menu\"]')").await;
    fits(&page).await;
}

async fn id_search_result_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, "t.$('.resid')", "contextmenu").await;
    fire(&page, &button("取得済みスレから検索"), "click").await;
    shows(&page, "t.$('[data-testid=\"id-search-result\"]')").await;
    fits(&page).await;
}

async fn wacchoi_list_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, "t.$('.wacchoi-badge')", "click").await;
    shows(&page, "t.$('[data-testid=\"wacchoi-list\"]')").await;
    fits(&page).await;
}

async fn wacchoi_menu_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, "t.$('.wacchoi-badge')", "contextmenu").await;
    shows(&page, "t.$('[data-testid=\"wacchoi-menu\"]')").await;
    fits(&page).await;
}

async fn wacchoi_search_result_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, "t.$('.wacchoi-badge')", "contextmenu").await;
    fire(&page, &button("取得済みスレから検索"), "click").await;
    shows(&page, "t.$('[data-testid=\"wacchoi-search-result\"]')").await;
    fits(&page).await;
}

async fn ng_word_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, "t.$('.res[data-res=\"1\"] .body')", "contextmenu").await;
    fire(&page, &button("NG Word に追加"), "click").await;
    shows(&page, "t.$('[data-testid=\"ng-word-form\"]')").await;
    fits(&page).await;
}

async fn anchor_tree_modal(page: Arc<Page>, app: Arc<Itest>) {
    on_a_phone(&page, &app).await;
    fire(&page, "t.$('.anchor[data-anchor=\"1\"]')", "click").await;
    shows(&page, "t.$('.anchor-node')").await;
    fits(&page).await;
}

async fn desktop_post_form_width(page: Arc<Page>, app: Arc<Itest>) {
    page.look("light", 1024, 800, 1.0, false).await;
    wide_thread(&page).await;
    visit(&page, &app, THREAD_PATH).await;
    shows(&page, "t.$('.res[data-res=\"1\"] .body')").await;

    fire(&page, &button("書き込む"), "click").await;
    shows(&page, "t.$('.post-form')").await;
    let width = box_of(&page, "t.$('.post-form')").await[2];
    assert!(width > 400.0, "the post form is {width}px wide");
}

pub const TESTS: &[Test] = &[
    test!("top page loads (favorites empty)", top_page_loads),
    test!(
        "PC: selecting a thread keeps the list visible and shows the detail beside it",
        pc_selecting_keeps_the_list
    ),
    test!(
        "PC: direct thread URL opens list + detail together",
        pc_direct_thread_url
    ),
    test!(
        "PC: scrolling in the detail pane does not move the list pane",
        pc_scrolling_the_detail_keeps_the_list
    ),
    test!(
        "phone: opening a thread hides the list (full-screen detail)",
        phone_opening_hides_the_list
    ),
    test!(
        "the thread title stays pinned at the top while scrolling",
        the_title_stays_pinned
    ),
    test!(
        "on phone: the thread title pins flush below the NavBar (no gap)",
        phone_title_flush_below_the_nav
    ),
    test!(
        "footer: write button is at the left end and refresh at the right end",
        phone_write_left_refresh_right
    ),
    test!(
        "phone: no horizontal overflow: write (post) modal",
        write_modal
    ),
    test!(
        "phone: no horizontal overflow: id-list modal (long ID)",
        id_list_modal
    ),
    test!(
        "phone: no horizontal overflow: id-menu modal",
        id_menu_modal
    ),
    test!(
        "phone: no horizontal overflow: id-search-result modal (long URL / long title)",
        id_search_result_modal
    ),
    test!(
        "phone: no horizontal overflow: wacchoi-list modal",
        wacchoi_list_modal
    ),
    test!(
        "phone: no horizontal overflow: wacchoi-menu modal",
        wacchoi_menu_modal
    ),
    test!(
        "phone: no horizontal overflow: wacchoi-search-result modal",
        wacchoi_search_result_modal
    ),
    test!(
        "phone: no horizontal overflow: ng-word modal (segmented control + textarea + two actions)",
        ng_word_modal
    ),
    test!(
        "phone: no horizontal overflow: anchor-tree modal (deep >>N chain + long URL)",
        anchor_tree_modal
    ),
    test!(
        "desktop viewport (1024px): post-form keeps a comfortable width",
        desktop_post_form_width
    ),
];
