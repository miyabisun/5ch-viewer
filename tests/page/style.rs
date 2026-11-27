//! What the styles resolve to in a browser: the theme's colours and theme-color, the installed
//! app's manifest and icon, the rating colours and the unread badge, the styles a phone gets,
//! and the indentation of the anchor tree.
use super::{click, fake, favorite, fire, shows, until_is, visit, Itest, Page, Test};
use serde_json::{json, Value};
use std::sync::Arc;

/// The page's surface, light and dark.
const LIGHT: &str = "rgb(250, 250, 250)";
const DARK: &str = "rgb(25, 25, 25)";

/// The theme-color the browser applies now: the meta whose media matches, or has none.
const APPLIED: &str = "return [...document.querySelectorAll('meta[name=\"theme-color\"]')] \
    .find((meta) => !meta.media || matchMedia(meta.media).matches)?.content ?? null;";

/// `color` (any CSS colour) as the browser computes it.
const RGB: &str = "const probe = document.createElement('i'); probe.style.color = arguments[0]; \
    document.body.append(probe); const computed = getComputedStyle(probe).color; probe.remove(); \
    return computed;";

/// Waits until the page wears `surface`: its theme-color and its background.
async fn wears(page: &Page, surface: &str) {
    until_is(page, "the applied theme-color", APPLIED, json!(surface)).await;
    until_is(
        page,
        "the background",
        "return getComputedStyle(document.body).backgroundColor;",
        json!(surface),
    )
    .await;
}

async fn theme_is(page: &Page, theme: &str) {
    until_is(
        page,
        "data-theme",
        "return document.documentElement.dataset.theme;",
        json!(theme),
    )
    .await;
}

/// The installed app's theme-color (the standalone meta, first) is Sumi, whatever the theme.
async fn installed_app_keeps_sumi(page: &Page) {
    let first = page
        .js(
            "const meta = document.querySelector('meta[name=\"theme-color\"]'); \
             return [meta.media, meta.content];",
            Value::Null,
        )
        .await;
    assert_eq!(first[0], json!("(display-mode: standalone)"));
    assert_eq!(page.js(RGB, first[1].clone()).await, json!(DARK));
}

async fn toggle(page: &Page) {
    click(page, "t.$('[data-testid=\"theme-toggle\"]')", "left").await;
}

async fn empty_list(page: &Page) {
    fake(page, json!([{"path": "/api/favorites", "json": []}])).await;
}

async fn toggle_persists(page: Arc<Page>, app: Arc<Itest>) {
    empty_list(&page).await;
    visit(&page, &app, "/").await;
    theme_is(&page, "light").await;
    wears(&page, LIGHT).await;

    toggle(&page).await;
    theme_is(&page, "dark").await;
    wears(&page, DARK).await;
    let saved = page
        .js("return localStorage.getItem('5ch-theme');", Value::Null)
        .await;
    assert_eq!(saved, json!("dark"));

    page.history("refresh").await;
    theme_is(&page, "dark").await;
    wears(&page, DARK).await;

    toggle(&page).await;
    theme_is(&page, "light").await;
    wears(&page, LIGHT).await;
    let saved = page
        .js("return localStorage.getItem('5ch-theme');", Value::Null)
        .await;
    assert_eq!(saved, json!("light"));
}

async fn follows_the_os(page: Arc<Page>, app: Arc<Itest>) {
    page.look("dark", 1280, 720, 1.0, false).await;
    empty_list(&page).await;
    visit(&page, &app, "/").await;
    theme_is(&page, "dark").await;
    wears(&page, DARK).await;

    toggle(&page).await;
    wears(&page, LIGHT).await;
    page.history("refresh").await;
    theme_is(&page, "light").await;
    wears(&page, LIGHT).await;
}

async fn image_viewer_darkens(page: Arc<Page>, app: Arc<Itest>) {
    let fav = json!({"server": "egg", "board": "b", "thread_id": "1", "title": "画像",
        "res_count": 1, "read_res": 0});
    app.post(
        &format!("{}/_control/seed-image", app.app),
        json!({"url": "https://example.com/a.jpg", "path": "example.com/a.jpg", "mime": "image/png"}),
    )
    .await;
    fake(
        &page,
        json!([
            {"path": "/api/favorites", "json": [fav]},
            {"pattern": "/dat$", "json": {"server": "egg", "board": "b", "thread_id": "1",
                "title": "画像", "res_count": 1, "read_res": 0, "mosaic_urls": [],
                "res": [{"num": 1, "name": "名無し", "mail": "", "date": "", "id": null,
                    "body": "https://example.com/a.jpg"}]}},
            {"pattern": "^/api/", "method": "GET", "json": []},
            {"pattern": "^/api/", "json": {"ok": true}},
        ]),
    )
    .await;
    visit(&page, &app, "/egg/b/1").await;
    wears(&page, LIGHT).await;

    click(&page, "t.$('.thumb-btn')", "left").await;
    shows(&page, "t.$('.viewer-bg')").await;
    until_is(
        &page,
        "the applied theme-color",
        APPLIED,
        json!("rgb(20, 20, 20)"),
    )
    .await;
    installed_app_keeps_sumi(&page).await;

    click(&page, "t.$('.viewer-close')", "left").await;
    until_is(
        &page,
        "the viewer",
        "return t.shows(t.$('.viewer-bg'));",
        json!(false),
    )
    .await;
    wears(&page, LIGHT).await;
}

/// The app's manifest, as the browser reads it: `{url, errors, data}`.
async fn manifest(page: &Page) -> Value {
    page.until(
        "the manifest is read",
        "return t.$('link[rel=\"manifest\"]') !== null;",
        Value::Null,
    )
    .await;
    page.cdp("Page.getAppManifest", json!({})).await
}

async fn manifest_backdrop(page: Arc<Page>, app: Arc<Itest>) {
    empty_list(&page).await;
    visit(&page, &app, "/").await;

    let crossorigin = page
        .js(
            "return t.$('link[rel=\"manifest\"]').getAttribute('crossorigin');",
            Value::Null,
        )
        .await;
    assert_eq!(crossorigin, json!("use-credentials"));
    let read = manifest(&page).await;
    assert_eq!(read["errors"], json!([]), "{read}");
    let parsed: Value = serde_json::from_str(read["data"].as_str().unwrap()).unwrap();
    assert_eq!(parsed["display"], json!("standalone"));
    assert_eq!(
        page.js(RGB, parsed["background_color"].clone()).await,
        json!(DARK)
    );
    assert_eq!(
        page.js(RGB, parsed["theme_color"].clone()).await,
        json!(DARK)
    );

    installed_app_keeps_sumi(&page).await;
    toggle(&page).await;
    wears(&page, DARK).await;
    installed_app_keeps_sumi(&page).await;
}

async fn maskable_icon(page: Arc<Page>, app: Arc<Itest>) {
    empty_list(&page).await;
    visit(&page, &app, "/").await;

    let read = manifest(&page).await;
    assert_eq!(read["errors"], json!([]), "{read}");
    let parsed: Value = serde_json::from_str(read["data"].as_str().unwrap()).unwrap();
    let purposes = |icon: &Value| -> Vec<String> {
        icon["purpose"]
            .as_str()
            .unwrap_or("any")
            .split(' ')
            .map(str::to_owned)
            .collect()
    };
    let icons = parsed["icons"].as_array().unwrap();
    assert!(icons
        .iter()
        .any(|icon| purposes(icon).contains(&"any".into())));
    let maskable = icons
        .iter()
        .find(|icon| purposes(icon).contains(&"maskable".into()))
        .expect("a maskable icon");

    // Drawn at 512px: an opaque background, nothing outside the safe zone (radius 0.4), and
    // artwork reaching between 0.3 and 0.4 of the size from the middle.
    let icon = page
        .js(
            "const url = new URL(arguments[0], arguments[1]).href; \
             return fetch(url).then(async (answer) => { \
               const type = answer.headers.get('content-type'); \
               const size = 512; const image = new Image(); image.src = url; await image.decode(); \
               const canvas = document.createElement('canvas'); canvas.width = canvas.height = size; \
               const context = canvas.getContext('2d'); context.drawImage(image, 0, 0, size, size); \
               const pixels = context.getImageData(0, 0, size, size).data; \
               const background = pixels.slice(0, 4); let outside = 0; let radius = 0; \
               for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) { \
                 const i = (y * size + x) * 4; \
                 const diff = Math.max(...[0, 1, 2, 3].map((c) => Math.abs(pixels[i + c] - background[c]))); \
                 const r = Math.hypot(x + 0.5 - size / 2, y + 0.5 - size / 2) / size; \
                 if (r > 0.4 && diff > 2) outside++; \
                 if (diff > 32) radius = Math.max(radius, r); } \
               return { type, opaque: background[3] === 255, outside, radius }; });",
            json!([maskable["src"], read["url"]]),
        )
        .await;
    assert!(
        icon["type"].as_str().unwrap().contains("image/svg+xml"),
        "{icon}"
    );
    assert_eq!(icon["opaque"], json!(true), "{icon}");
    assert_eq!(icon["outside"], json!(0), "{icon}");
    let radius = icon["radius"].as_f64().unwrap();
    assert!((0.3..=0.4).contains(&radius), "{icon}");
}

async fn rating_colour_bars(page: Arc<Page>, app: Arc<Itest>) {
    let favorites: Vec<Value> = (0..=5)
        .map(|r| {
            let mut fav = favorite(&format!("100000000{r}"), &format!("スレ{r}"), 10, 10);
            fav["rating"] = json!(r);
            fav
        })
        .collect();
    fake(
        &page,
        json!([{"path": "/api/favorites", "json": favorites}]),
    )
    .await;
    visit(&page, &app, "/").await;

    for r in 0..=5 {
        shows(&page, &format!("t.$('.thread.rate-{r}')")).await;
        let item = page
            .js(
                &format!(
                    "const items = document.querySelectorAll('.thread.rate-{r}'); \
                     return [items.length, items[0].dataset.rating, \
                       getComputedStyle(items[0]).borderLeftColor];"
                ),
                Value::Null,
            )
            .await;
        assert_eq!(item[0], json!(1), "rate-{r}: {item}");
        assert_eq!(item[1], json!(r.to_string()), "rate-{r}: {item}");
        let colour = item[2].as_str().unwrap();
        assert!(
            !colour.is_empty() && colour != "rgba(0, 0, 0, 0)",
            "rate-{r}: {item}"
        );
    }
}

async fn unread_badge_style(page: Arc<Page>, app: Arc<Itest>) {
    fake(
        &page,
        json!([{"path": "/api/favorites", "json": [
            favorite("2000000001", "unread", 10, 4),
            favorite("2000000002", "read", 10, 10),
        ]}]),
    )
    .await;
    visit(&page, &app, "/").await;
    shows(&page, "t.$('.unread')").await;

    let badge = page
        .js(
            "const badges = document.querySelectorAll('.unread'); const s = getComputedStyle(badges[0]); \
             return [badges.length, badges[0].textContent.trim(), s.backgroundColor, \
               parseFloat(s.borderTopLeftRadius)];",
            Value::Null,
        )
        .await;
    assert_eq!(badge[0], json!(1), "{badge}");
    assert_eq!(badge[1], json!("6"), "{badge}");
    assert_ne!(badge[2], json!("rgba(0, 0, 0, 0)"), "{badge}");
    assert!(badge[3].as_f64().unwrap() > 0.0, "{badge}");
}

/// The thread of the reply-menu tests, opened.
async fn reply_thread(page: &Page, app: &Itest) {
    let posts = json!([
        {"num": 1, "name": "名無し", "mail": "", "date": "2025/01/01(水) 00:00:00.00",
            "body": "一行目<br>二行目", "id": "TESTID1"},
        {"num": 2, "name": "名無し", "mail": "", "date": "2025/01/01(水) 00:01:00.00",
            "body": "短文<br>https://example.com/test.jpg", "id": null},
    ]);
    fake(
        page,
        json!([
            {"path": "/api/favorites", "json": [favorite("1771127145", "返信メニューテストスレ", 2, 0)]},
            {"pattern": "/dat$", "json": {"title": "返信メニューテストスレ", "res_count": 2,
                "read_res": 0, "status": "active", "res": posts}},
            {"pattern": "^/api/", "method": "GET", "json": []},
            {"pattern": "^/api/", "json": {"ok": true}},
        ]),
    )
    .await;
    visit(page, app, "/egg/applism/1771127145").await;
    shows(page, "t.$('.res[data-res=\"1\"] .body')").await;
}

const USER_SELECT: &str = "return getComputedStyle(t.$('.body')).userSelect;";

async fn desktop_keeps_selection(page: Arc<Page>, app: Arc<Itest>) {
    reply_thread(&page, &app).await;
    let select = page.js(USER_SELECT, Value::Null).await;
    assert_ne!(select, json!("none"));
}

/// A Pixel 5: 393 by 851 at 2.75, a phone's, that takes touches.
async fn on_a_pixel(page: &Page, theme: &str) {
    page.touches(true).await;
    page.look(theme, 393, 851, 2.75, true).await;
}

async fn mobile_turns_selection_off(page: Arc<Page>, app: Arc<Itest>) {
    on_a_pixel(&page, "light").await;
    reply_thread(&page, &app).await;
    let select = page.js(USER_SELECT, Value::Null).await;
    assert_eq!(select, json!("none"));
}

async fn built_css_has_no_callout(page: Arc<Page>, app: Arc<Itest>) {
    on_a_pixel(&page, "light").await;
    reply_thread(&page, &app).await;
    let css = page
        .js(
            "const hrefs = [...document.styleSheets].map((s) => s.href).filter(Boolean); \
             return Promise.all(hrefs.map((h) => fetch(h).then((r) => r.text()))) \
               .then((texts) => texts.join('\\n'));",
            Value::Null,
        )
        .await;
    let css = css.as_str().unwrap();
    let found = css.split('}').any(|rule| {
        rule.split_once('{').is_some_and(|(selector, body)| {
            selector.contains(".res")
                && body.replace(' ', "").contains("-webkit-touch-callout:none")
        })
    });
    assert!(found, "no .res rule with -webkit-touch-callout: none");
}

async fn anchor_tree_indents(page: Arc<Page>, app: Arc<Itest>) {
    let post = |num: u32, id: &str, body: &str| json!({"num": num, "name": "名無し", "mail": "", "date": format!("2025 ID:{id}"), "body": body});
    fake(
        &page,
        json!([
            {"path": "/api/favorites", "json": [favorite("1771127145", "テストスレ", 3, 0)]},
            {"pattern": "/dat$", "json": {"title": "テストスレ", "res_count": 3, "read_res": 0,
                "status": "active", "res": [
                    post(859, "a", "859のレス"),
                    post(863, "b", "&gt;&gt;859 863のレス"),
                    post(886, "c", "&gt;&gt;863 886のレス"),
                ]}},
            {"pattern": "^/api/", "method": "GET", "json": []},
        ]),
    )
    .await;
    visit(&page, &app, "/egg/applism/1771127145").await;
    // >>863 in res 886's body opens the tree with 863 as self.
    fire(&page, "t.$('.anchor[data-anchor=\"863\"]')", "click").await;
    shows(&page, "t.$('.modal .anchor-node')").await;

    let tree = page
        .js(
            "const rows = [...document.querySelectorAll('.modal .res')]; \
             const of = (num) => rows.filter((el) => el.querySelector('.num')?.textContent.trim() === num); \
             return ['859', '863', '886'].map((num) => [of(num).length, \
               of(num)[0]?.classList.contains('anchor-self'), of(num)[0]?.getBoundingClientRect().left]);",
            Value::Null,
        )
        .await;
    // Each res once; only self (863) highlighted.
    for (row, self_) in tree.as_array().unwrap().iter().zip([false, true, false]) {
        assert_eq!(row[0], json!(1), "{tree}");
        assert_eq!(row[1], json!(self_), "{tree}");
    }
    // ancestor(859).left < self(863).left < child(886).left
    let left = |i: usize| tree[i][2].as_f64().unwrap();
    assert!(left(0) < left(1) && left(1) < left(2), "{tree}");
}

pub const TESTS: &[Test] = &[
    test!(
        "theme toggle flips data-theme, persists, and survives reload",
        toggle_persists
    ),
    test!(
        "theme follows OS preference when no override stored",
        follows_the_os
    ),
    test!(
        "image viewer darkens theme-color while open and restores it on close",
        image_viewer_darkens
    ),
    test!(
        "installed app keeps theme-color on the manifest backdrop",
        manifest_backdrop
    ),
    test!(
        "installed app offers a full-bleed maskable icon",
        maskable_icon
    ),
    test!(
        "each item carries its rating color-bar class (0..5)",
        rating_colour_bars
    ),
    test!(
        "unread badge: shown (rounded, colored) when unread > 0, hidden at 0",
        unread_badge_style
    ),
    test!(
        "desktop: .body keeps text selection (user-select is not none)",
        desktop_keeps_selection
    ),
    test!(
        "mobile (touch): .body computed user-select is none",
        mobile_turns_selection_off
    ),
    test!(
        "mobile (touch): .res ships -webkit-touch-callout: none in the built CSS",
        built_css_has_no_callout
    ),
    test!(
        "anchor tree depth indentation: ancestor shallower than self, self shallower than child",
        anchor_tree_indents
    ),
];
