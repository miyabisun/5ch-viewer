//! The built page in a real browser, for what only a browser's engine shows: where things lie
//! and how big they are, scrolling, the colours the theme resolves to, the styles a phone gets,
//! and the browser's own history and reload. The rest of the page is tested in jsdom
//! (client/src/page/*.test.ts).
//!
//! One browser and one app at a time: the tests run one after another, in the order they are
//! written. The app is itest-server (the real router on an in-memory database, with a stand-in
//! for 5ch), serving the page from `client/build`: build it first (`cd client && bun run
//! build`). A test that needs data of a given shape answers the page's fetches itself
//! (`fake`, as the jsdom tests do); the others go to the app. After a test that failed, the
//! next one gets an app of its own. `cargo test --test page -- <words>` runs the tests whose
//! titles hold any of the words; `--skip <words>` leaves those out.
#![allow(clippy::unwrap_used)] // helpers outside #[test] fns
use serde_json::{json, Value};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};

use browser::Page;
use itest::Itest;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/itest.rs"]
mod itest;

type Run = Pin<Box<dyn Future<Output = ()> + Send>>;
/// A test: its title, and what it does on the page and the app.
type Test = (&'static str, fn(Arc<Page>, Arc<Itest>) -> Run);

macro_rules! test {
    ($title:literal, $test:ident) => {
        ($title, |page, app| Box::pin($test(page, app)))
    };
}

#[path = "page/layout.rs"]
mod layout;

/// The tests, in the order they run.
const PARTS: [&[Test]; 1] = [layout::TESTS];

/// What every document of the page runs first: the stand-in for the app's answers, and the
/// finders and measures of the tests.
const PAGE_JS: &str = include_str!("support/page.js");

/// The script that sets the answers of the stand-in, while one is set.
static FAKED: Mutex<Option<String>> = Mutex::new(None);

/// Has the documents the page opens from now on answer their fetches of /api/* from
/// `routes` (`[{method?, path | pattern, json, status?}]`, the first that matches), as the
/// jsdom tests' fake server does.
pub async fn fake(page: &Page, routes: Value) {
    unfake(page).await;
    let added = page
        .cdp(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"source": format!("window.__routes = {routes};")}),
        )
        .await;
    *FAKED.lock().unwrap() = added["identifier"].as_str().map(str::to_owned);
}

/// Has the documents the page opens from now on fetch from the app.
pub async fn unfake(page: &Page) {
    let faked = FAKED.lock().unwrap().take();
    if let Some(identifier) = faked {
        page.cdp(
            "Page.removeScriptToEvaluateOnNewDocument",
            json!({"identifier": identifier}),
        )
        .await;
    }
}

/// Opens `path` of the app.
pub async fn visit(page: &Page, app: &Itest, path: &str) {
    page.open(&format!("{}{path}", app.app)).await;
}

/// Waits until the element `element` (a script that finds it, with `t.…`) is on the screen.
pub async fn shows(page: &Page, element: &str) {
    page.until(
        &format!("{element} shows"),
        &format!("return t.shows({element});"),
        Value::Null,
    )
    .await;
}

/// Waits until the element `element` is in the page but not on the screen, or not there.
pub async fn hidden(page: &Page, element: &str) {
    page.until(
        &format!("{element} is hidden"),
        &format!("return !t.shows({element});"),
        Value::Null,
    )
    .await;
}

/// Waits until `script` returns `want`; fails saying what it returned last.
pub async fn until_is(page: &Page, what: &str, script: &str, want: Value) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let value = page.js(script, Value::Null).await;
        if value == want {
            return;
        }
        assert!(Instant::now() < deadline, "{what}: {value}, not {want}");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Clicks the middle of the element `element` with the mouse `button` ("left", "right"),
/// as a person does: the browser scrolls it into sight first.
pub async fn click(page: &Page, element: &str, button: &str) {
    let box_ = page
        .until(
            &format!("{element} can be clicked"),
            // in sight, still for a frame, and the element itself under its middle
            &format!(
                "const el = {element}; if (!t.shows(el)) return null; \
                 el.scrollIntoView({{block: 'nearest', inline: 'nearest'}}); \
                 const before = t.box(el).join(); \
                 return new Promise((done) => requestAnimationFrame(() => {{ \
                   const box = t.box(el); if (box.join() !== before) return done(null); \
                   const hit = document.elementFromPoint(box[0] + box[2] / 2, box[1] + box[3] / 2); \
                   done(hit && (hit === el || el.contains(hit)) ? box : null); }}));"
            ),
            Value::Null,
        )
        .await;
    let x = box_[0].as_f64().unwrap() + box_[2].as_f64().unwrap() / 2.0;
    let y = box_[1].as_f64().unwrap() + box_[3].as_f64().unwrap() / 2.0;
    page.mouse("moved", x, y, "none").await;
    page.mouse("pressed", x, y, button).await;
    page.mouse("released", x, y, button).await;
}

/// Fires `kind` ("click", "contextmenu") at the element `element`, once it is on the screen:
/// for a test that measures what opens, not the press that opens it.
pub async fn fire(page: &Page, element: &str, kind: &str) {
    page.until(
        &format!("{element} takes a {kind}"),
        &format!(
            "const el = {element}; if (!t.shows(el)) return false; \
             el.dispatchEvent(new MouseEvent('{kind}', {{bubbles: true, cancelable: true, \
               button: '{kind}' === 'contextmenu' ? 2 : 0}})); return true;"
        ),
        Value::Null,
    )
    .await;
}

/// The box of the element `element`: [left, top, width, height, right, bottom].
pub async fn box_of(page: &Page, element: &str) -> Vec<f64> {
    let found = page
        .until(
            &format!("{element} is there"),
            &format!("const el = {element}; return el ? t.box(el) : null;"),
            Value::Null,
        )
        .await;
    found
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect()
}

/// A favorite as `GET /api/favorites` lists it.
pub fn favorite(thread_id: &str, title: &str, res_count: u32, read_res: u32) -> Value {
    json!({
        "server": "egg", "board": "applism", "board_name": "アプリ", "thread_id": thread_id,
        "title": title, "res_count": res_count, "read_res": read_res, "rating": 0,
        "status": "active",
    })
}

/// A stored dat of `count` posts (本文1…), with `read_res`.
pub fn dat(title: &str, count: u32, read_res: u32) -> Value {
    let res: Vec<Value> = (1..=count)
        .map(|n| {
            json!({"num": n, "name": "名無し", "mail": "", "date": format!("2025 ID:x{n}"),
                "body": format!("本文{n}")})
        })
        .collect();
    json!({"title": title, "res_count": count, "read_res": read_res, "status": "active",
        "mosaic_urls": [], "res": res})
}

/// The answers of a list of `favorites` whose threads all hold `dat`.
pub fn thread_routes(favorites: &Value, dat: &Value) -> Value {
    json!([
        {"path": "/api/favorites", "json": favorites},
        {"pattern": "/dat$", "json": dat},
        {"pattern": "/reload$", "json": {"res_count": dat["res_count"], "read_res": 0, "status": "active"}},
        {"pattern": "/progress$", "json": {"ok": true}},
        {"pattern": "^/api/(ng-ids|ng-words|ng-wacchoi)$", "method": "GET", "json": []},
    ])
}

// ---- running them ----

/// Which tests a run takes, as `cargo test` passes its words on: titles that hold any of the
/// words (all, when there are none), and none that hold a word after `--skip`; with
/// `--exact`, a word is a whole title. Other options are left alone.
fn chosen(args: &[String]) -> impl Fn(&str) -> bool {
    let mut words = Vec::new();
    let mut skipped = Vec::new();
    let mut exact = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--skip" => skipped.extend(args.next().cloned()),
            "--exact" => exact = true,
            option if option.starts_with('-') => {}
            word => words.push(word.to_owned()),
        }
    }
    move |title: &str| {
        let holds = |word: &String| {
            if exact {
                title == word
            } else {
                title.contains(word.as_str())
            }
        };
        (words.is_empty() || words.iter().any(holds)) && !skipped.iter().any(holds)
    }
}

/// What a panic said.
fn said(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|&said| said.to_owned()))
        .unwrap_or_default()
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let chosen = chosen(&args);
    let tests: Vec<&Test> = PARTS
        .iter()
        .flat_map(|part| part.iter())
        .filter(|test| chosen(test.0))
        .collect();
    if args.iter().any(|arg| arg == "--list") {
        for (title, _) in &tests {
            println!("{title}: test");
        }
        return std::process::ExitCode::SUCCESS;
    }
    let count = tests.len();
    println!("\nrunning {count} tests");
    if count == 0 {
        return std::process::ExitCode::SUCCESS;
    }
    assert!(
        Itest::built(),
        "the page is not built: run `bun run build` in client/ first"
    );
    let began = Instant::now();
    let page = Arc::new(Page::start(&[PAGE_JS]).await);
    let mut serving: Option<Arc<Itest>> = None;
    let mut failed = Vec::new();
    for (title, test) in tests {
        let app = serving
            .get_or_insert_with(|| Arc::new(Itest::start()))
            .clone();
        // each test begins on an empty app and a page of no state, in a window of 1280 by 720
        // in the light, that takes no touches and fetches from the app
        app.reset().await;
        unfake(&page).await;
        visit(&page, &app, "/favicon.svg").await;
        page.js("localStorage.clear(); sessionStorage.clear();", Value::Null)
            .await;
        page.look("light", 1280, 720, 1.0, false).await;
        page.touches(false).await;
        let began = Instant::now();
        let ran = tokio::spawn(test(page.clone(), app)).await;
        let took = began.elapsed().as_secs_f64();
        match ran {
            Ok(()) => println!("test {title} ... ok ({took:.2}s)"),
            Err(error) => {
                println!("test {title} ... FAILED: {}", said(&*error.into_panic()));
                failed.push(*title);
                // after a test that failed, the next one gets an app of its own
                serving = None;
            }
        }
    }
    drop(serving);
    let page = Arc::try_unwrap(page).ok().unwrap();
    page.close().await;
    let seconds = began.elapsed().as_secs_f64();
    if failed.is_empty() {
        println!("\ntest result: ok. {count} passed; 0 failed; finished in {seconds:.2}s\n");
        std::process::ExitCode::SUCCESS
    } else {
        println!("\nfailures:");
        for title in &failed {
            println!("    {title}");
        }
        println!(
            "\ntest result: FAILED. {} passed; {} failed; finished in {seconds:.2}s\n",
            count - failed.len(),
            failed.len()
        );
        std::process::ExitCode::from(101)
    }
}
