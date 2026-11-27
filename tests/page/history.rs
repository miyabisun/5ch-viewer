//! The image viewer and the browser's own history: Back closes only the image, Forward and a
//! reload bring it back, and no close skips the thread. Each test runs on a desktop and on a
//! phone (a Pixel 5 that takes touches).
use super::{click, fake_js, fire, shows, until_is, visit, Itest, Page, Test};
use serde_json::{json, Value};
use std::sync::Arc;

const PATH: &str = "/egg/applism/123";
const THUMB: &str = "t.$('.thread-body .res[data-res=\"35\"] .thumb-btn')";
const VIEWER: &str = "t.$('.viewer-bg')";

/// The stand-in for the app: two favorites, a thread of 60 posts whose post 35 holds two
/// pictures (until `sessionStorage.noImages` is set) and whose post 36 answers it, and the
/// number of times the thread's dat was read (`window.__reads()`), kept across reloads.
const SERVER: &str = "{ const fav = { server: 'egg', board: 'applism', board_name: 'アプリ', \
      thread_id: '123', title: '画像と戻る操作', res_count: 60, read_res: 35, rating: 0, status: 'active' }; \
    window.__reads = () => Number(sessionStorage.getItem('reads') ?? 0); \
    window.__routes = [ \
      { path: '/api/favorites', json: [fav, { ...fav, thread_id: '456', title: '別のスレッド' }] }, \
      { pattern: '/dat$', answer: () => { sessionStorage.setItem('reads', window.__reads() + 1); \
          const images = !sessionStorage.getItem('noImages'); \
          return { ...fav, mosaic_urls: [], res: Array.from({ length: 60 }, (_, i) => ({ \
            num: i + 1, name: '名無し', mail: '', date: '2026/09/24', id: i === 34 ? 'PICTURES' : null, \
            body: `本文${i + 1}<br>読み進めていた位置のテストです。` + \
              (i === 34 && images ? '<br>https://example.com/one.jpg<br>https://example.com/two.jpg' : '') + \
              (i === 35 ? '<br>&gt;&gt;35' : '') })) }; } }, \
      { pattern: '^/api/', method: 'GET', json: [] }, \
      { pattern: '^/api/', json: { ok: true } }, \
    ]; }";

/// A desktop of 1280 by 800 in the light, or a Pixel 5 in the dark.
async fn as_on(page: &Page, phone: bool) {
    if phone {
        page.touches(true).await;
        page.look("dark", 393, 851, 2.75, true).await;
    } else {
        page.look("light", 1280, 800, 1.0, false).await;
    }
}

/// Sets up the app's pictures and the stand-in, for a test on a desktop or a phone.
async fn serve(page: &Page, app: &Itest, phone: bool) {
    for name in ["one", "two"] {
        app.post(
            &format!("{}/_control/seed-image", app.app),
            json!({"url": format!("https://example.com/{name}.jpg"),
                "path": format!("example.com/{name}.jpg"), "mime": "image/png"}),
        )
        .await;
    }
    fake_js(page, SERVER).await;
    as_on(page, phone).await;
}

async fn path_is(page: &Page, path: &str) {
    until_is(page, "the path", "return location.pathname;", json!(path)).await;
}

async fn viewer_gone(page: &Page) {
    until_is(page, "the viewer", "return t.$('.viewer-bg');", Value::Null).await;
}

async fn footer_is(page: &Page, text: &str) {
    until_is(
        page,
        "the viewer's footer",
        "return t.$('.viewer-footer')?.textContent.trim() ?? null;",
        json!(text),
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

/// Opens the thread from the list and brings post 35's pictures into sight.
async fn enter(page: &Page, app: &Itest) {
    visit(page, app, "/").await;
    click(
        page,
        "[...document.querySelectorAll('.info')].find((el) => el.textContent.includes('画像と戻る操作'))",
        "left",
    )
    .await;
    path_is(page, PATH).await;
    page.until(
        "post 35's picture is in sight",
        &format!(
            "const el = {THUMB}; if (!el) return false; el.scrollIntoViewIfNeeded(); return true;"
        ),
        Value::Null,
    )
    .await;
}

async fn back(page: &Page) {
    page.history("back").await;
}

/// A finger drawn through `points` (x, y) and lifted at the last.
async fn swipe(page: &Page, points: &[(f64, f64)]) {
    let last = points.len() - 1;
    for (i, &(x, y)) in points.iter().enumerate() {
        page.touch(if i == 0 { "touchStart" } else { "touchMove" }, x, y)
            .await;
        if i == last {
            page.touch("touchEnd", x, y).await;
        }
    }
}

async fn back_closes_only_the_image(page: &Page, app: &Itest, phone: bool) {
    serve(page, app, phone).await;
    enter(page, app).await;
    // where the thread body rests, once it no longer moves
    let scroll = page
        .until(
            "the thread body rests",
            "window.__body = t.$('.thread-body'); const before = window.__body.scrollTop; \
             return new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => \
               done(window.__body.scrollTop === before ? [before] : null))));",
            Value::Null,
        )
        .await[0]
        .clone();
    let reads = page.js("return window.__reads();", Value::Null).await;

    fire(page, THUMB, "click").await;
    shows(page, VIEWER).await;
    fire(page, &button("次の画像"), "click").await;
    footer_is(page, "2 / 2").await;
    page.key("Control+ArrowLeft").await;
    footer_is(page, "2 / 2").await;
    if phone {
        fire(page, &button("前の画像"), "click").await;
        swipe(page, &[(280.0, 350.0), (250.0, 350.0), (120.0, 350.0)]).await;
        footer_is(page, "2 / 2").await;
    }

    // Back closes the image only: the same thread body, where it was, read no more.
    back(page).await;
    path_is(page, PATH).await;
    viewer_gone(page).await;
    let kept = page
        .js(
            "return [window.__body.isConnected, window.__body.scrollTop, window.__reads()];",
            Value::Null,
        )
        .await;
    assert_eq!(kept[0], json!(true), "the thread body was made anew");
    let moved = kept[1].as_f64().unwrap() - scroll.as_f64().unwrap();
    assert!(moved.abs() < 0.5, "the thread scrolled by {moved}");
    assert_eq!(kept[2], reads, "the dat was read again");

    // Forward brings the last image back, without reading the dat.
    page.history("forward").await;
    footer_is(page, "2 / 2").await;
    assert_eq!(
        page.js("return window.__reads();", Value::Null).await,
        reads
    );
    back(page).await;
    viewer_gone(page).await;
    back(page).await;
    path_is(page, "/").await;
    until_is(
        page,
        "the thread body",
        "return t.$('.thread-body');",
        Value::Null,
    )
    .await;
}

async fn close_controls_take_one_entry(page: &Page, app: &Itest, phone: bool) {
    serve(page, app, phone).await;
    enter(page, app).await;
    for action in ["button", "background", "escape", "swipe", "double"] {
        fire(page, THUMB, "click").await;
        shows(page, VIEWER).await;
        match action {
            "button" => fire(page, "t.$('.viewer-close')", "click").await,
            "background" => {
                page.mouse("moved", 4.0, 4.0, "none").await;
                page.mouse("pressed", 4.0, 4.0, "left").await;
                page.mouse("released", 4.0, 4.0, "left").await;
            }
            "escape" => page.key("Escape").await,
            "swipe" => swipe(page, &[(200.0, 220.0), (200.0, 250.0), (200.0, 380.0)]).await,
            _ => {
                page.js(
                    "const close = t.$('.viewer-close'); close.click(); close.click();",
                    Value::Null,
                )
                .await;
            }
        }
        viewer_gone(page).await;
        path_is(page, PATH).await;
    }
    back(page).await;
    path_is(page, "/").await;
}

async fn menu_and_popups_share_history(page: &Page, app: &Itest, phone: bool) {
    serve(page, app, phone).await;
    enter(page, app).await;
    fire(page, THUMB, "click").await;
    shows(page, VIEWER).await;
    let menu = "t.$('[data-testid=\"image-menu\"]')";
    if phone {
        // a finger held on the picture
        let picture = super::box_of(page, "t.$('.viewer-img')").await;
        let (x, y) = (picture[0] + picture[2] / 2.0, picture[1] + picture[3] / 2.0);
        page.touch("touchStart", x, y).await;
        shows(page, menu).await;
        page.touch("touchEnd", x, y).await;
    } else {
        click(page, "t.$('.viewer-img')", "right").await;
    }
    viewer_gone(page).await;
    shows(page, menu).await;
    fire(page, &button("モザイクをかける"), "click").await;
    fire(page, THUMB, "click").await;
    page.until(
        "the picture is under a mosaic",
        "return t.$('.viewer-img')?.classList.contains('mosaic');",
        Value::Null,
    )
    .await;
    back(page).await;
    viewer_gone(page).await;

    for trigger in [
        "t.$('.thread-body .anchor[data-anchor=\"35\"]')",
        "t.$('.thread-body .id-badge[data-id=\"PICTURES\"]')",
    ] {
        fire(page, trigger, "click").await;
        fire(page, "t.$('.modal .thumb-btn')", "click").await;
        shows(page, VIEWER).await;
        back(page).await;
        viewer_gone(page).await;
        shows(page, "t.$('.modal')").await;
        fire(page, "t.$('.modal .thumb-btn')", "click").await;
        shows(page, VIEWER).await;
        page.key("Escape").await;
        viewer_gone(page).await;
        shows(page, "t.$('.modal')").await;
        fire(page, "t.$('.modal-close')", "click").await;
    }
    back(page).await;
    path_is(page, "/").await;
}

async fn direct_urls_and_reloads(page: &Page, app: &Itest, phone: bool) {
    serve(page, app, phone).await;
    visit(page, app, "/register").await;
    visit(page, app, PATH).await;
    fire(page, THUMB, "click").await;
    fire(page, &button("次の画像"), "click").await;
    footer_is(page, "2 / 2").await;
    // A reload restores the last image.
    page.history("refresh").await;
    footer_is(page, "2 / 2").await;
    back(page).await;
    viewer_gone(page).await;
    path_is(page, PATH).await;

    // Leaving for another page replaces the image's entry.
    fire(page, THUMB, "click").await;
    shows(page, VIEWER).await;
    page.js(
        "t.$('[data-testid=\"tab-register\"]').click();",
        Value::Null,
    )
    .await;
    path_is(page, "/register").await;
    back(page).await;
    path_is(page, PATH).await;
    viewer_gone(page).await;

    // So does opening another thread.
    fire(page, THUMB, "click").await;
    shows(page, VIEWER).await;
    page.js(
        "[...document.querySelectorAll('.info')].find((el) => el.textContent.includes('別のスレッド')).click();",
        Value::Null,
    )
    .await;
    path_is(page, "/egg/applism/456").await;
    viewer_gone(page).await;
    back(page).await;
    path_is(page, PATH).await;
    viewer_gone(page).await;

    // A picture that is gone after a reload leaves the thread and drops the image's state.
    fire(page, THUMB, "click").await;
    shows(page, VIEWER).await;
    page.js("sessionStorage.setItem('noImages', '1');", Value::Null)
        .await;
    page.history("refresh").await;
    viewer_gone(page).await;
    page.until(
        "60 posts",
        "return document.querySelectorAll('.thread-body .res').length === 60;",
        Value::Null,
    )
    .await;
    until_is(
        page,
        "the image's history state",
        "return history.state?.imageViewer ?? null;",
        Value::Null,
    )
    .await;
    back(page).await;
    path_is(page, "/register").await;
}

async fn desktop_back(page: Arc<Page>, app: Arc<Itest>) {
    back_closes_only_the_image(&page, &app, false).await;
}
async fn mobile_back(page: Arc<Page>, app: Arc<Itest>) {
    back_closes_only_the_image(&page, &app, true).await;
}
async fn desktop_close(page: Arc<Page>, app: Arc<Itest>) {
    close_controls_take_one_entry(&page, &app, false).await;
}
async fn mobile_close(page: Arc<Page>, app: Arc<Itest>) {
    close_controls_take_one_entry(&page, &app, true).await;
}
async fn desktop_menu(page: Arc<Page>, app: Arc<Itest>) {
    menu_and_popups_share_history(&page, &app, false).await;
}
async fn mobile_menu(page: Arc<Page>, app: Arc<Itest>) {
    menu_and_popups_share_history(&page, &app, true).await;
}
async fn desktop_direct(page: Arc<Page>, app: Arc<Itest>) {
    direct_urls_and_reloads(&page, &app, false).await;
}
async fn mobile_direct(page: Arc<Page>, app: Arc<Itest>) {
    direct_urls_and_reloads(&page, &app, true).await;
}

pub const TESTS: &[Test] = &[
    test!(
        "desktop: Back closes only the image and Forward restores it without refetch or scroll reset",
        desktop_back
    ),
    test!(
        "mobile: Back closes only the image and Forward restores it without refetch or scroll reset",
        mobile_back
    ),
    test!(
        "desktop: close controls consume one entry, repeated close and immediate reopening do not skip the thread",
        desktop_close
    ),
    test!(
        "mobile: close controls consume one entry, repeated close and immediate reopening do not skip the thread",
        mobile_close
    ),
    test!(
        "desktop: image menu and popup images share the close history",
        desktop_menu
    ),
    test!(
        "mobile: image menu and popup images share the close history",
        mobile_menu
    ),
    test!(
        "desktop: direct URLs, reload and navigating away leave coherent entries",
        desktop_direct
    ),
    test!(
        "mobile: direct URLs, reload and navigating away leave coherent entries",
        mobile_direct
    ),
];
