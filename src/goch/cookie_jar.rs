//! Persistent cookie jar for 5ch bbs.cgi posts.
//!
//! Wraps `cookie_store::CookieStore` in an `RwLock` and implements
//! `reqwest::cookie::CookieStore` so it can be passed to `ClientBuilder::cookie_provider`.
//!
//! After a successful post the caller should call `save()` to persist the jar to disk
//! (data/cookies.json). On the next process start `load()` restores the stored cookies
//! so that acorn/MonaTicket are re-sent and the confirmation two-step is skipped.

use reqwest::cookie::CookieStore;
use std::io::BufReader;
use std::sync::{Arc, RwLock};
use time::Duration as TimeDuration;
use url::Url;

/// Default Max-Age applied to session cookies (no Expires/Max-Age in Set-Cookie).
/// `cookie_store::serde::json::save` silently drops session cookies (no expiry),
/// so acorn/MonaTicket would not survive a process restart.  We work around this
/// by patching them with a 3-day Max-Age at store time, matching the real 5ch
/// cookie lifetime observed in practice.
const SESSION_COOKIE_MAX_AGE_DAYS: i64 = 3;

/// Thread-safe, serialisable cookie jar.
pub struct PersistentJar {
    inner: RwLock<cookie_store::CookieStore>,
}

impl std::fmt::Debug for PersistentJar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistentJar").finish()
    }
}

impl Default for PersistentJar {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentJar {
    /// Creates an empty jar.
    pub fn new() -> Self {
        PersistentJar { inner: RwLock::new(cookie_store::CookieStore::new()) }
    }

    /// Loads a previously saved jar from `path`. Returns an empty jar on any I/O error
    /// (first run, deleted file, corrupt JSON) — the consequence is a one-time confirmation
    /// two-step on the next post, after which the fresh cookies are saved.
    pub fn load(path: &str) -> Self {
        let store = std::fs::File::open(path)
            .ok()
            .and_then(|f| {
                cookie_store::serde::json::load(BufReader::new(f))
                    .map_err(|e| tracing::warn!("[cookie_jar] load {path}: {e}"))
                    .ok()
            })
            .unwrap_or_default();
        PersistentJar { inner: RwLock::new(store) }
    }

    /// Serialises the jar to `path` (JSON). Called after a successful post so the
    /// acorn/MonaTicket cookies survive process restarts.
    /// Non-fatal: logs a warning and returns `false` on failure.
    pub fn save(&self, path: &str) -> bool {
        // Ensure the parent directory exists (e.g. ./data/).
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("[cookie_jar] save {path}: mkdir: {e}");
                    return false;
                }
            }
        }
        let mut f = match std::fs::File::create(path) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("[cookie_jar] save {path}: create: {e}");
                return false;
            }
        };
        match cookie_store::serde::json::save(&self.inner.read().unwrap(), &mut f) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("[cookie_jar] save {path}: json: {e}");
                false
            }
        }
    }
}

/// Implements the reqwest `CookieStore` trait so `PersistentJar` can be used as a
/// `cookie_provider` on the shared HTTP client.
impl CookieStore for PersistentJar {
    fn set_cookies(
        &self,
        cookie_headers: &mut dyn Iterator<Item = &reqwest::header::HeaderValue>,
        url: &Url,
    ) {
        // Parse each Set-Cookie header value and patch session cookies (no Expires/Max-Age)
        // with a fixed Max-Age so they survive `save()`.  The `cookie_store` JSON serialiser
        // silently drops cookies without an expiry, which means acorn/MonaTicket — which 5ch
        // sends without an explicit lifetime — would be lost on every process restart.
        let cookies: Vec<_> = cookie_headers.filter_map(|val| {
            std::str::from_utf8(val.as_bytes())
                .ok()
                .and_then(|s| cookie::Cookie::parse(s).ok())
                .map(|c| c.into_owned())
        }).map(|mut c| {
            // If neither Expires nor Max-Age is set, the cookie is a session cookie.
            // Apply a default Max-Age so the JSON serialiser keeps it.
            if c.max_age().is_none() && c.expires().is_none() {
                c.set_max_age(TimeDuration::days(SESSION_COOKIE_MAX_AGE_DAYS));
            }
            c
        }).collect();
        self.inner.write().unwrap().store_response_cookies(cookies.into_iter(), url);
    }

    fn cookies(&self, url: &Url) -> Option<reqwest::header::HeaderValue> {
        let s = self
            .inner
            .read()
            .unwrap()
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");

        if s.is_empty() {
            return None;
        }

        reqwest::header::HeaderValue::from_str(&s).ok()
    }
}

/// Convenience alias for the shared, reference-counted jar.
pub type SharedJar = Arc<PersistentJar>;

/// Builds a new `SharedJar`, loading from `path` when it exists.
pub fn open(path: &str) -> SharedJar {
    Arc::new(PersistentJar::load(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_jar_returns_no_cookies() {
        let jar = PersistentJar::new();
        let url = Url::parse("https://egg.5ch.io/test/bbs.cgi").unwrap();
        assert!(jar.cookies(&url).is_none());
    }

    #[test]
    fn set_and_get_cookies() {
        let jar = PersistentJar::new();
        let url = Url::parse("https://egg.5ch.io/test/bbs.cgi").unwrap();

        // Simulate Set-Cookie headers.
        let h1 = reqwest::header::HeaderValue::from_static("acorn=abc123; Path=/; Domain=.5ch.io");
        let h2 = reqwest::header::HeaderValue::from_static("MonaTicket=xyz; Path=/; Domain=.5ch.io");
        let headers = [h1, h2];
        let mut iter = headers.iter();
        jar.set_cookies(&mut iter, &url);

        // The jar must now provide the cookies for the same domain.
        let got = jar.cookies(&url);
        assert!(got.is_some(), "jar must return cookies after set_cookies");
        let s = got.unwrap().to_str().unwrap().to_string();
        assert!(s.contains("acorn=abc123") || s.contains("MonaTicket=xyz"),
            "jar must contain at least one of the set cookies: {s}");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = std::env::temp_dir().join("goch_test_cookies.json");
        let path = tmp.to_str().unwrap();

        // Create a jar with one cookie and save it.
        let jar = PersistentJar::new();
        let url = Url::parse("https://egg.5ch.io/test/bbs.cgi").unwrap();
        let h = reqwest::header::HeaderValue::from_static(
            "acorn=persistent_value; Path=/; Domain=.5ch.io; Max-Age=86400",
        );
        let headers = [h];
        let mut iter = headers.iter();
        jar.set_cookies(&mut iter, &url);
        assert!(jar.save(path), "save must succeed");

        // Load from the saved file and verify the cookie is present.
        let jar2 = PersistentJar::load(path);
        let got = jar2.cookies(&url);
        // Cleanup before any assertion to avoid leaking the file.
        let _ = std::fs::remove_file(tmp);
        assert!(got.is_some(), "loaded jar must return the saved cookie");
    }

    #[test]
    fn load_missing_file_returns_empty_jar() {
        // A path that doesn't exist must not panic — returns an empty jar.
        let jar = PersistentJar::load("/tmp/goch_nonexistent_XXXXXX.json");
        let url = Url::parse("https://egg.5ch.io/test/bbs.cgi").unwrap();
        assert!(jar.cookies(&url).is_none());
    }

    /// Regression: `cookie_store::serde::json::save` silently drops session cookies
    /// (no Max-Age/Expires). The `set_cookies` implementation must patch such cookies
    /// with a fixed Max-Age so they survive save()/load() round-trips.
    ///
    /// This test simulates the real 5ch behaviour where `acorn` is sent as a session
    /// cookie without any Expires or Max-Age field.
    #[test]
    fn session_cookie_persists_after_save_and_load() {
        let tmp = std::env::temp_dir().join("goch_test_session_cookies.json");
        let path = tmp.to_str().unwrap();

        let jar = PersistentJar::new();
        let url = Url::parse("https://egg.5ch.io/test/bbs.cgi").unwrap();

        // A session cookie: no Max-Age, no Expires — like the real 5ch acorn cookie.
        let h = reqwest::header::HeaderValue::from_static(
            "acorn=sessionvalue123; Path=/; Domain=.5ch.io",
        );
        let headers = [h];
        let mut iter = headers.iter();
        jar.set_cookies(&mut iter, &url);

        // The cookie must be readable in the same process.
        let before = jar.cookies(&url);
        assert!(before.is_some(), "session cookie must be readable after set_cookies");

        // Save and reload: the session cookie must survive because we patched Max-Age.
        assert!(jar.save(path), "save must succeed");

        let jar2 = PersistentJar::load(path);
        let got = jar2.cookies(&url);
        let _ = std::fs::remove_file(&tmp);

        assert!(
            got.is_some(),
            "session cookie must survive save()/load() round-trip after Max-Age patch"
        );
        let s = got.unwrap().to_str().unwrap().to_string();
        assert!(
            s.contains("acorn=sessionvalue123"),
            "saved session cookie value must be restored: {s}"
        );
    }
}
