//! Where the thread opens and what stays still on the screen: the previous-read marker at the
//! bottom of what is new, older posts added below without moving it, and posting or refreshing
//! without moving what is on the screen.
use super::{click, fake, fake_js, favorite, shows, until_is, visit, Itest, Page, Test};
use serde_json::{json, Value};
use std::sync::Arc;

const THREAD_PATH: &str = "/egg/applism/1771127145";

/// The posts of the thread, newest first, as the page shows them: their numbers.
const ORDER: &str =
    "return [...document.querySelectorAll('.thread-body > .res')].map((n) => n.dataset.res);";

/// A dat of `count` posts (本文1…) with `read_res`, as the reverse-thread tests serve it.
fn reverse_dat(title: &str, count: u32, read_res: u32) -> Value {
    let res: Vec<Value> = (1..=count)
        .map(|n| {
            json!({"num": n, "name": "名無し", "mail": "", "date": format!("2025/01/01 00:00 ID:x{n}"),
                "body": format!("本文{n}"), "id": format!("x{n}"), "own": false})
        })
        .collect();
    json!({"title": title, "res_count": count, "read_res": read_res, "status": "active",
        "mosaic_urls": [], "res": res})
}

/// The answers of one favorite holding `dat`, whose saved position is `read_res`.
fn routes(fav: &Value, dat: &Value, read_res: u32) -> Value {
    json!([
        {"path": "/api/favorites", "json": [fav]},
        {"pattern": "/dat$", "json": dat},
        {"pattern": "/reload$", "json": {"res_count": dat["res_count"], "read_res": read_res, "status": "active"}},
        {"pattern": "/progress$", "method": "GET", "json": {"read_res": read_res}},
        {"pattern": "/progress$", "json": {"ok": true}},
        {"pattern": "^/api/(ng-ids|ng-words|ng-wacchoi)$", "method": "GET", "json": []},
    ])
}

/// The first posts are newest first, the marker says 前回ここまで, all `count` posts are there,
/// no spacer is made, and the thread body is scrolled to its top.
async fn opens_at_the_newest(page: &Page, first: &[&str], count: usize) {
    page.until(
        "the posts are laid out",
        &format!("return document.querySelectorAll('.thread-body > .res').length === {count};"),
        Value::Null,
    )
    .await;
    let order = page.js(ORDER, Value::Null).await;
    for (i, num) in first.iter().enumerate() {
        assert_eq!(order[i], json!(num), "{order}");
    }
    until_is(
        page,
        "the marker",
        "return t.$('[data-testid=\"read-boundary\"]')?.textContent.trim();",
        json!("前回ここまで"),
    )
    .await;
    let rest = page
        .js(
            "return [t.$('.entry-spacer'), t.$('.thread-body').scrollTop];",
            Value::Null,
        )
        .await;
    assert_eq!(rest, json!([null, 0]), "rest");
}

async fn phone_position_beyond_the_dat(page: Arc<Page>, app: Arc<Itest>) {
    page.look("light", 390, 700, 1.0, false).await;
    // Stale progress beyond the available dat: read_res=50 with only 30 posts.
    let fav = favorite("1771127145", "テストスレ", 30, 50);
    fake(&page, routes(&fav, &super::dat("テストスレ", 30, 50), 50)).await;
    visit(&page, &app, THREAD_PATH).await;
    opens_at_the_newest(&page, &["30"], 30).await;
}

async fn short_thread_shows_older_posts(page: Arc<Page>, app: Arc<Itest>) {
    let fav = favorite("1771127145", "テストスレ", 30, 28);
    fake(&page, routes(&fav, &super::dat("テストスレ", 30, 28), 28)).await;
    visit(&page, &app, THREAD_PATH).await;
    opens_at_the_newest(&page, &["30", "29", "28"], 30).await;
}

/// The idle callbacks the page asks for wait until the test runs them, one at a time.
const IDLE: &str = "{ const jobs = new Map(); let next = 1; \
    window.requestIdleCallback = (callback) => { const id = next++; jobs.set(id, callback); return id; }; \
    window.cancelIdleCallback = (id) => jobs.delete(id); \
    window.__runNextIdleJob = () => { const job = jobs.entries().next(); if (job.done) return false; \
      const [id, callback] = job.value; jobs.delete(id); \
      callback({ didTimeout: false, timeRemaining: () => 50 }); return true; }; }";

async fn older_posts_append_below(page: Arc<Page>, app: Arc<Itest>) {
    let fav = favorite("1771127145", "逆順テストスレ", 120, 100);
    let routes = routes(&fav, &reverse_dat("逆順テストスレ", 120, 100), 100);
    fake_js(&page, &format!("window.__routes = {routes}; {IDLE}")).await;
    visit(&page, &app, THREAD_PATH).await;

    let count = |n: usize| {
        format!("return document.querySelectorAll('.thread-body > .res').length === {n};")
    };
    page.until("21 posts", &count(21), Value::Null).await;
    let order = page.js(ORDER, Value::Null).await;
    assert_eq!(order[0], json!("120"), "order[0]");
    assert_eq!(order[20], json!("100"), "order[20]");
    let ends = page
        .js(
            "const end = t.$('[data-testid=\"thread-end\"]'); \
             const boundary = t.$('[data-testid=\"read-boundary\"]'); \
             return [end.textContent.trim(), end.nextElementSibling?.dataset.res, \
               boundary.textContent.trim(), boundary.nextElementSibling?.dataset.res, \
               t.text('本文99'), Number(t.$('.list-pane .unread')?.textContent)];",
            Value::Null,
        )
        .await;
    assert_eq!(ends[0], json!("おわり"), "{ends}");
    assert_eq!(ends[1], json!("120"), "{ends}");
    assert_eq!(ends[2], json!("前回ここまで"), "{ends}");
    assert_eq!(ends[3], json!("100"), "{ends}");
    assert_eq!(ends[4], Value::Null, "{ends}");
    assert!(ends[5].as_f64().unwrap() > 0.0, "{ends}");
    shows(&page, "t.$('.list-pane .unread')").await;

    // The marker sits at the bottom of the thread body.
    let marker = "const body = t.$('.thread-body').getBoundingClientRect(); \
        const marker = t.$('[data-testid=\"read-boundary\"]').getBoundingClientRect(); \
        return [marker.top, body.bottom - marker.bottom];";
    page.until(
        "the marker is at the bottom of the thread body",
        &format!("const [, gap] = (() => {{ {marker} }})(); return Math.abs(gap) <= 1;"),
        Value::Null,
    )
    .await;
    let before = page.js(marker, Value::Null).await;

    // An idle moment appends the next 50 older posts below, without moving the marker.
    assert_eq!(
        page.js("return window.__runNextIdleJob();", Value::Null)
            .await,
        json!(true),
        "page.js(\"return window.__runNextIdleJob();\", Value::Null) .await"
    );
    page.until("71 posts", &count(71), Value::Null).await;
    let texts = page
        .js(
            "return [t.text('本文99') !== null, t.text('本文50') !== null, t.text('本文49')];",
            Value::Null,
        )
        .await;
    assert_eq!(texts, json!([true, true, null]), "texts");
    let after = page.js(marker, Value::Null).await;
    let moved = after[0].as_f64().unwrap() - before[0].as_f64().unwrap();
    assert!(moved.abs() <= 1.0, "the marker moved by {moved}");

    assert_eq!(
        page.js("return window.__runNextIdleJob();", Value::Null)
            .await,
        json!(true),
        "page.js(\"return window.__runNextIdleJob();\", Value::Null) .await"
    );
    page.until("120 posts", &count(120), Value::Null).await;
    assert_eq!(
        page.js(ORDER, Value::Null).await[119],
        json!("1"),
        "page.js(ORDER, Value::Null).await[119]"
    );
}

async fn fully_read_thread_boundary(page: Arc<Page>, app: Arc<Itest>) {
    let fav = favorite("1771127145", "逆順テストスレ", 590, 590);
    let routes = routes(&fav, &reverse_dat("逆順テストスレ", 590, 590), 590);
    fake_js(&page, &format!("window.__routes = {routes}; {IDLE}")).await;
    visit(&page, &app, THREAD_PATH).await;

    page.until(
        "51 posts",
        "return document.querySelectorAll('.thread-body > .res').length === 51;",
        Value::Null,
    )
    .await;
    let seen = page
        .js(
            "const boundary = t.$('[data-testid=\"read-boundary\"]'); \
             const posts = document.querySelectorAll('.thread-body > .res'); \
             return [t.$('.entry-spacer'), boundary.nextElementSibling?.dataset.res, \
               posts[1].dataset.res, t.$('.thread-body').scrollTop];",
            Value::Null,
        )
        .await;
    assert_eq!(seen, json!([null, "590", "589", 0]), "seen");
}

// ---- posting or refreshing keeps the screen still ----

/// A stateful stand-in for the app: a thread of 60 posts read to 40; a post arrives as the
/// user's own on the next reload, and saving the position raises it.
const SERVER: &str = "{ const post = (num, own = false) => ({ num, name: '名無し', mail: '', \
      date: `2025/01/01 00:00 ID:x${num}`, \
      body: Array.from({ length: (num % 3) + 1 }, () => `本文${num}`).join('<br>'), id: `x${num}`, own }); \
    const state = JSON.parse(sessionStorage.getItem('state') ?? 'null') ?? { \
      res: Array.from({ length: 60 }, (_, i) => post(i + 1)), read_res: 40, arrivals: [], progress: [] }; \
    const save = () => sessionStorage.setItem('state', JSON.stringify(state)); \
    window.__state = state; window.__post = post; window.__save = save; \
    const meta = { server: 'egg', board: 'applism', board_name: 'アプリ', thread_id: '1771127145', \
      title: '未読スレ', rating: 0, status: 'active' }; \
    window.__routes = [ \
      { path: '/api/favorites', answer: () => [{ ...meta, res_count: state.res.length, read_res: state.read_res, \
          own_res: state.res.filter((r) => r.own).map((r) => r.num) }] }, \
      { pattern: '/dat$', answer: () => ({ title: meta.title, res_count: state.res.length, \
          read_res: state.read_res, status: 'active', mosaic_urls: [], res: state.res }) }, \
      { pattern: '/reload$', answer: () => { state.res.push(...state.arrivals); state.arrivals = []; save(); \
          return { res_count: state.res.length, read_res: state.read_res, status: 'active' }; } }, \
      { pattern: '/post$', answer: () => { state.arrivals.push(post(state.res.length + state.arrivals.length + 1, true)); \
          save(); return { ok: true }; } }, \
      { pattern: '/progress$', answer: ({ method, body }) => { if (method === 'POST') { \
          state.progress.push(body.read_res); state.read_res = Math.max(state.read_res, body.read_res); save(); } \
          return { ok: true }; } }, \
      { pattern: '^/api/(ng-ids|ng-words|ng-wacchoi)$', json: [] }, \
    ]; }";

/// The posts on the screen of the thread body, with their top below its top.
const ON_SCREEN: &str =
    "const body = t.$('.thread-body'); const view = body.getBoundingClientRect(); \
    return [...body.querySelectorAll(':scope > .res')] \
      .map((n) => ({ num: Number(n.dataset.res), rect: n.getBoundingClientRect() })) \
      .filter(({ rect }) => rect.bottom > view.top && rect.top < view.bottom) \
      .map(({ num, rect }) => [num, rect.top - view.top]);";

/// A frame and a moment later.
const SETTLE: &str =
    "return new Promise((done) => requestAnimationFrame(() => setTimeout(() => done(true), 50)));";

async fn keeps_the_screen_still(page: &Page, app: &Itest, scroll: &str, posting: bool) {
    page.look("light", 390, 700, 1.0, false).await;
    fake_js(page, SERVER).await;
    visit(page, app, "/").await;
    page.js("sessionStorage.removeItem('state');", Value::Null)
        .await;
    page.history("refresh").await;
    click(page, "t.$('.list-pane .thread')", "left").await;
    shows(page, "t.$('.thread-body > .res')").await;
    page.until(
        "60 posts",
        "return t.$('[data-testid=\"read-boundary\"]') && \
           document.querySelectorAll('.thread-body > .res').length === 60;",
        Value::Null,
    )
    .await;
    page.js(SETTLE, Value::Null).await;
    let at_entry = page.js(ON_SCREEN, Value::Null).await;

    page.js(scroll, Value::Null).await;
    page.js(SETTLE, Value::Null).await;
    let before = page.js(ON_SCREEN, Value::Null).await;
    let anchor = before[0].clone();
    let unread = "return document.querySelectorAll('.thread-body > .res.unread').length;";
    let unread_before = page.js(unread, Value::Null).await.as_u64().unwrap();

    post_or_refresh(page, posting).await;
    page.js(SETTLE, Value::Null).await;

    // The post at the top of the screen did not move.
    let top = page
        .js(
            "const n = t.$(`.thread-body > .res[data-res=\"${arguments[0]}\"]`); \
             return n.getBoundingClientRect().top - t.$('.thread-body').getBoundingClientRect().top;",
            json!([anchor[0]]),
        )
        .await;
    let moved = top.as_f64().unwrap() - anchor[1].as_f64().unwrap();
    assert!(moved.abs() <= 1.0, "the post at the top moved by {moved}");
    newest_is_61(page, posting, unread_before).await;
    leaving_saves_what_was_seen(page, &[at_entry, before]).await;
}

/// Writes a post (`posting`), or has one arrive and presses the footer's 更新, and waits
/// until the page shows it.
async fn post_or_refresh(page: &Page, posting: bool) {
    if posting {
        click(
            page,
            "[...document.querySelectorAll('button')].find((b) => t.shows(b) && (b.textContent.trim() === '書き込む' || b.getAttribute('aria-label') === '書き込む'))",
            "left",
        )
        .await;
        shows(page, "t.$('.post-textarea')").await;
        page.js(
            "const area = t.$('.post-textarea'); area.value = 'テスト'; \
             area.dispatchEvent(new Event('input', { bubbles: true }));",
            Value::Null,
        )
        .await;
        click(page, "t.$('.post-submit')", "left").await;
        page.until(
            "the own post",
            "return document.querySelectorAll('.thread-body > .res.own').length === 1;",
            Value::Null,
        )
        .await;
    } else {
        page.js(
            "window.__state.arrivals = [window.__post(61)]; window.__save();",
            Value::Null,
        )
        .await;
        click(
            page,
            "[...t.$('.detail-pane').querySelectorAll('button')].find((b) => b.textContent.trim() === '更新' || b.getAttribute('aria-label') === '更新')",
            "left",
        )
        .await;
        page.until(
            "post 61",
            "return t.$('.thread-body > .res[data-res=\"61\"]') !== null;",
            Value::Null,
        )
        .await;
    }
}

/// The newest post, 61, is first: the user's own after posting, unread after a refresh.
async fn newest_is_61(page: &Page, posting: bool, unread_before: u64) {
    let unread = "return document.querySelectorAll('.thread-body > .res.unread').length;";
    let first = page
        .js(
            "const first = t.$('.thread-body > .res'); return [first.dataset.res, \
               first.classList.contains('own'), first.classList.contains('unread')];",
            Value::Null,
        )
        .await;
    assert_eq!(first[0], json!("61"), "first[0]");
    let unread_after = page.js(unread, Value::Null).await.as_u64().unwrap();
    if posting {
        assert_eq!(first[1], json!(true), "{first}");
        assert_eq!(unread_after, unread_before, "unread_after");
    } else {
        assert_eq!(first[2], json!(true), "{first}");
        assert_eq!(unread_after, unread_before + 1, "unread_after");
    }
}

/// Leaving saves the newest post that was on the screen at any point (`earlier`, and now).
async fn leaving_saves_what_was_seen(page: &Page, earlier: &[Value]) {
    let after = page.js(ON_SCREEN, Value::Null).await;
    let shown = earlier
        .iter()
        .chain([&after])
        .flat_map(|list| list.as_array().unwrap().clone())
        .map(|row| row[0].as_u64().unwrap())
        .max()
        .unwrap();
    click(page, "t.$('[data-testid=\"tab-favorites\"]')", "left").await;
    page.until(
        "the position is saved",
        "return window.__state.progress.length > 0;",
        Value::Null,
    )
    .await;
    until_is(
        page,
        "the saved position",
        "return window.__state.read_res;",
        json!(shown),
    )
    .await;
    page.until(
        "the list shows 61 posts",
        "return t.$('.list-pane .thread')?.textContent.includes('アプリ 61');",
        Value::Null,
    )
    .await;
}

const INSIDE: &str = "t.$('.thread-body').scrollBy(0, -250);";
const BELOW: &str = "t.$('.thread-body').scrollBy(0, 2500);";
const TOP: &str = "t.$('.thread-body').scrollTop = 0;";

async fn posting_inside(page: Arc<Page>, app: Arc<Itest>) {
    keeps_the_screen_still(&page, &app, INSIDE, true).await;
}
async fn refresh_inside(page: Arc<Page>, app: Arc<Itest>) {
    keeps_the_screen_still(&page, &app, INSIDE, false).await;
}
async fn posting_below(page: Arc<Page>, app: Arc<Itest>) {
    keeps_the_screen_still(&page, &app, BELOW, true).await;
}
async fn refresh_below(page: Arc<Page>, app: Arc<Itest>) {
    keeps_the_screen_still(&page, &app, BELOW, false).await;
}
async fn posting_top(page: Arc<Page>, app: Arc<Itest>) {
    keeps_the_screen_still(&page, &app, TOP, true).await;
}
async fn refresh_top(page: Arc<Page>, app: Arc<Itest>) {
    keeps_the_screen_still(&page, &app, TOP, false).await;
}

pub const TESTS: &[Test] = &[
    test!(
        "phone: clamps the boundary to the newest post and fills the short view with older posts",
        phone_position_beyond_the_dat
    ),
    test!(
        "opening a short thread shows older posts naturally below the previous-read marker",
        short_thread_shows_older_posts
    ),
    test!(
        "newest posts render first and older posts append below without moving the read boundary",
        older_posts_append_below
    ),
    test!(
        "a 590-post thread with no unread posts places the boundary above res 590",
        fully_read_thread_boundary
    ),
    test!(
        "phone: posting keeps the screen still when inside the unread section",
        posting_inside
    ),
    test!(
        "phone: the refresh button keeps the screen still when inside the unread section",
        refresh_inside
    ),
    test!(
        "phone: posting keeps the screen still when among older posts below the marker",
        posting_below
    ),
    test!(
        "phone: the refresh button keeps the screen still when among older posts below the marker",
        refresh_below
    ),
    test!(
        "phone: posting keeps the screen still when at the top",
        posting_top
    ),
    test!(
        "phone: the refresh button keeps the screen still when at the top",
        refresh_top
    ),
];
