use crate::state::lock;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

static CACHED_HTML: Mutex<Option<(String, u64)>> = Mutex::new(None);

/// Reads client/build/index.html, injects `BASE_PATH`, and returns it.
/// In production, returns the cached copy regardless of mtime.
///
/// # Panics
///
/// Panics if a previous call panicked while holding the HTML cache lock.
#[must_use]
pub fn get_index_html(base_path: &str) -> Option<String> {
    let index_path = Path::new("client/build/index.html");

    let is_prod = std::env::var("NODE_ENV").is_ok_and(|v| v == "production");

    let mut cached = lock(&CACHED_HTML);

    let metadata = fs::metadata(index_path).ok()?;
    let mtime = u64::try_from(
        metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis(),
    )
    .ok()?;

    if let Some((ref html, ref cached_mtime)) = *cached {
        if is_prod || mtime == *cached_mtime {
            return Some(html.clone());
        }
    }

    let raw = fs::read_to_string(index_path).ok()?;
    let html = raw
        .replace(
            "<head>",
            &format!("<head>\n\t\t<base href=\"{base_path}/\">"),
        )
        .replace(
            "window.__BASE_PATH__ = \"\"",
            &format!(
                "window.__BASE_PATH__ = {}",
                serde_json::to_string(base_path).expect("serialising a &str to JSON cannot fail")
            ),
        );

    *cached = Some((html.clone(), mtime));
    Some(html)
}
