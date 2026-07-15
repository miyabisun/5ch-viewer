//! Image URL extraction, normalization, download, SSRF guard, and prefetch pipeline.
//!
//! ## Security threat model
//!
//! - SSRF: blocked by `is_safe_host` for direct IP/host literals AND for every
//!   redirect hop (see `build_image_http_client`). Out of scope: **DNS rebinding**
//!   attacks where `evil.com` resolves to a private IP at fetch time. This is
//!   accepted risk: the project runs behind Cloudflare Access (single
//!   authenticated user), and the practical attack surface is minimal. Future
//!   hardening: resolve hostnames first and pin the IP via `Client::builder().resolve()`.

use regex_lite::Regex;
use reqwest::Client;
use std::net::IpAddr;
use std::sync::LazyLock;
use tokio::sync::Semaphore;

// Matches image URLs in raw dat text (case-insensitive).
// Captures ttp(s):// and http(s):// URLs ending with common image extensions.
static IMAGE_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Use a character class that avoids the need to escape single-quote inside raw strings.
    Regex::new(r#"(?i)\b(h?ttps?)://[^\s<>"']+?\.(png|jpe?g|gif|webp)\b"#).unwrap()
});

/// Extracts deduplicated image URLs from raw dat text, in appearance order.
pub fn extract_image_urls(dat_text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for cap in IMAGE_URL_RE.find_iter(dat_text) {
        let url = cap.as_str().to_string();
        if seen.insert(url.clone()) {
            out.push(url);
        }
    }
    out
}

/// Normalizes an image URL into a cache path (scheme/query/fragment stripped, host lowercased).
/// Returns `None` when the URL is not a valid http/https/ttp/ttps image URL.
pub fn normalize_image_path(url: &str) -> Option<String> {
    // Strip leading ttp(s) or h?ttps?:// prefix.
    let after_scheme = ["https://", "http://", "ttps://", "ttp://"]
        .iter()
        .find_map(|p| url.strip_prefix(p))?;

    // Strip query string and fragment, then trailing slash.
    let path = after_scheme
        .split(['?', '#'])
        .next()
        .unwrap_or(after_scheme)
        .trim_end_matches('/');

    // Verify the path ends with a recognized image extension.
    let lower = path.to_ascii_lowercase();
    let has_ext = [".png", ".jpg", ".jpeg", ".gif", ".webp"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    if !has_ext {
        return None;
    }

    // Lowercase the host part only (everything before the first '/').
    let normalized = match path.find('/') {
        Some(slash_pos) => format!(
            "{}{}",
            path[..slash_pos].to_ascii_lowercase(),
            &path[slash_pos..]
        ),
        None => path.to_ascii_lowercase(),
    };

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// A downloaded image blob with its MIME type.
pub struct ImageBlob {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// Builds the image HTTP client (separate UA from the main 5ch client, no cookie jar).
///
/// Redirect policy: follows up to 5 hops, re-validates each redirect destination
/// with `is_safe_host` to prevent SSRF via open redirects. Non-http/https schemes
/// (file://, gopher://, ftp://, etc.) are rejected immediately.
pub fn build_image_http_client() -> Client {
    const IMAGE_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";
    Client::builder()
        .user_agent(IMAGE_UA)
        .timeout(std::time::Duration::from_secs(5))
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::ACCEPT_ENCODING,
                reqwest::header::HeaderValue::from_static("identity"),
            );
            h
        })
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // Limit to 5 hops to prevent redirect loops.
            if attempt.previous().len() >= 5 {
                return attempt.error("too many redirects");
            }
            // Only allow http and https; reject file://, gopher://, ftp://, etc.
            let url = attempt.url();
            if !matches!(url.scheme(), "http" | "https") {
                return attempt.stop();
            }
            // Re-validate the redirect destination host with the same SSRF guard
            // used for the initial URL, preventing open-redirect SSRF attacks.
            match url.host_str() {
                Some(h) if is_safe_host(h) => attempt.follow(),
                _ => attempt.stop(),
            }
        }))
        .build()
        .expect("Failed to build image HTTP client")
}

// FIVECH_ALLOW_LOOPBACK_FOR_TEST: TEST-ONLY env var.
// When set, loopback addresses (127.x.x.x / ::1) are allowed through the SSRF guard
// so integration tests can serve mock images from http://127.0.0.1:{MOCK_PORT}/mock/img/.
// NEVER set this variable in production. All other private/link-local/ULA IPs remain
// blocked regardless of this env var.

/// Returns `true` when the host is safe to connect to (rejects localhost/private/link-local).
pub(crate) fn is_safe_host(host: &str) -> bool {
    // Reject "localhost" by name.
    if host.eq_ignore_ascii_case("localhost") {
        return false;
    }
    // Try parsing as an IP address literal.
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_safe_ip(ip);
    }
    true
}

fn is_safe_ip(ip: IpAddr) -> bool {
    // TEST-ONLY: FIVECH_ALLOW_LOOPBACK_FOR_TEST permits loopback (127.x.x.x / ::1)
    // so that integration tests can download images from the local mock server.
    // The env var is honored ONLY in debug builds; in release builds it has no effect.
    // All other unsafe IPs (private, link-local, ULA) are rejected unconditionally.
    let allow_loopback =
        cfg!(debug_assertions) && std::env::var("FIVECH_ALLOW_LOOPBACK_FOR_TEST").is_ok();

    match ip {
        IpAddr::V4(v4) => {
            // Allow loopback in test mode only.
            if v4.is_loopback() && allow_loopback {
                return true;
            }
            // Reject: loopback (127.x.x.x), private (10.x, 172.16-31.x, 192.168.x),
            // link-local (169.254.x), unspecified (0.0.0.0), broadcast (255.255.255.255),
            // multicast (224.0.0.0/4).
            !v4.is_loopback()
                && !v4.is_private()
                && !v4.is_link_local()
                && !v4.is_unspecified()
                && !v4.is_broadcast()
                && !v4.is_multicast()
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped IPv6 (e.g. ::ffff:127.0.0.1): the Linux dual-stack kernel
            // routes these to the underlying IPv4 address, so re-validate as IPv4.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_safe_ip(IpAddr::V4(v4));
            }
            // Allow loopback (::1) in test mode only.
            if v6.is_loopback() && allow_loopback {
                return true;
            }
            // Reject: loopback (::1), unspecified (::), multicast (ff00::/8),
            // ULA (fc00::/7), link-local (fe80::/10).
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return false;
            }
            let segs = v6.segments();
            // ULA: fc00::/7 → first byte 0xfc or 0xfd.
            let first_byte = (segs[0] >> 8) as u8;
            if first_byte == 0xfc || first_byte == 0xfd {
                return false;
            }
            // Link-local: fe80::/10 → top 10 bits are 1111 1110 10.
            if segs[0] & 0xffc0 == 0xfe80 {
                return false;
            }
            true
        }
    }
}

// Maximum allowed image size: 5 MB.
const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// Accepted MIME types (SVG and others are rejected to prevent XSS vectors).
fn is_accepted_mime(mime: &str) -> bool {
    matches!(
        mime.split(';').next().unwrap_or("").trim(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

/// Resolves the effective URL for ttp/ttps schemes (tries https then http).
/// Returns the resolved URL string, or `None` if the scheme is unrecognized.
fn resolve_url(url: &str) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else if url.starts_with("ttps://") {
        Some(url.replacen("ttps://", "https://", 1))
    } else if url.starts_with("ttp://") {
        Some(url.replacen("ttp://", "http://", 1))
    } else {
        None
    }
}

/// Fetches a single image URL. Returns `None` on SSRF rejection, size excess, or wrong MIME.
/// Retries once on failure (no delay — fast-fail for background prefetch).
pub async fn fetch_image(client: &Client, url: &str) -> Option<ImageBlob> {
    let effective_url = resolve_url(url)?;

    // SSRF: parse the host and validate it.
    let parsed = url::Url::parse(&effective_url).ok()?;
    let host = parsed.host_str()?;
    if !is_safe_host(host) {
        tracing::debug!("[images] SSRF reject: {url}");
        return None;
    }

    fetch_image_inner(client, &effective_url).await
}

async fn fetch_image_inner(client: &Client, url: &str) -> Option<ImageBlob> {
    // Attempt 1, then retry once on failure.
    for attempt in 0..2u32 {
        match try_fetch_image(client, url).await {
            Some(blob) => return Some(blob),
            None if attempt == 0 => continue,
            None => return None,
        }
    }
    None
}

async fn try_fetch_image(client: &Client, url: &str) -> Option<ImageBlob> {
    // HEAD check: if Content-Length is present and exceeds 5 MB, skip.
    if let Ok(head_resp) = client.head(url).send().await {
        if let Some(len) = head_resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
        {
            if len > MAX_IMAGE_BYTES {
                tracing::debug!("[images] skip {url}: Content-Length={len} > 5MB");
                return None;
            }
        }
    }

    // GET with streaming to enforce the size limit.
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if !is_accepted_mime(&content_type) {
        tracing::debug!("[images] skip {url}: unsupported MIME {content_type}");
        return None;
    }

    // Accumulate the body with a size cap.
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        buf.extend_from_slice(&chunk);
        if buf.len() > MAX_IMAGE_BYTES {
            tracing::debug!("[images] skip {url}: body exceeded 5MB");
            return None;
        }
    }

    if buf.is_empty() {
        return None;
    }

    // Derive clean MIME string.
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or("image/jpeg")
        .trim()
        .to_string();

    Some(ImageBlob { bytes: buf, mime })
}

// Maximum concurrent image downloads during a prefetch pass.
const PREFETCH_CONCURRENCY: usize = 4;

/// Prefetches a batch of image URLs: skips already-cached entries, downloads the rest in parallel.
pub async fn prefetch_images(state: &crate::state::AppState, urls: Vec<String>) {
    if urls.is_empty() {
        return;
    }

    // Build an IN(...) placeholder list.
    let placeholders: String = urls
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("SELECT id, url, file_size FROM image_cache WHERE url IN ({placeholders})");

    // Find which URLs are already cached.
    let cached_rows: Vec<(i64, String, Option<i64>)> = {
        let conn = state.db.lock().unwrap();
        let mut stmt = conn.prepare(&sql).unwrap();
        let params: Vec<&dyn rusqlite::ToSql> =
            urls.iter().map(|u| u as &dyn rusqlite::ToSql).collect();
        stmt.query_map(params.as_slice(), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    };
    let already_cached: std::collections::HashSet<String> = cached_rows
        .into_iter()
        .filter_map(|(id, url, size)| {
            crate::image_cache::audit_file(
                std::path::Path::new(&state.config.image_cache_dir),
                id,
                size,
            )
            .ok()
            .filter(|ok| *ok)
            .map(|_| url)
        })
        .collect();

    let pending: Vec<String> = urls
        .into_iter()
        .filter(|u| !already_cached.contains(u))
        .collect();

    if pending.is_empty() {
        return;
    }

    let sem = std::sync::Arc::new(Semaphore::new(PREFETCH_CONCURRENCY));
    let mut handles = Vec::new();

    for url in pending {
        let path = match normalize_image_path(&url) {
            Some(p) => p,
            None => continue,
        };
        let id = {
            let conn = state.db.lock().unwrap();
            if conn
                .execute(
                    "INSERT OR IGNORE INTO image_cache (url, path, mosaic) VALUES (?1, ?2, 0)",
                    rusqlite::params![url, path],
                )
                .is_err()
            {
                continue;
            }
            match conn.query_row(
                "SELECT id FROM image_cache WHERE url=?1",
                rusqlite::params![url],
                |r| r.get(0),
            ) {
                Ok(id) => id,
                Err(_) => continue,
            }
        };
        let sem = sem.clone();
        let client = state.image_http.clone();
        let db = state.db.clone();
        let image_cache_dir = state.config.image_cache_dir.clone();
        let url_clone = url.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.ok()?;
            let blob = fetch_image(&client, &url_clone).await?;
            let size = blob.bytes.len() as i64;
            let mime = blob.mime;
            let bytes = blob.bytes;
            tokio::task::spawn_blocking(move || {
                crate::image_cache::write_verified(
                    std::path::Path::new(&image_cache_dir),
                    id,
                    &bytes,
                )
            })
            .await
            .ok()?
            .ok()?;
            let conn = db.lock().unwrap();
            conn.execute(
                "UPDATE image_cache SET path=?1, mime=?2, file_size=?3 WHERE id=?4",
                rusqlite::params![path, mime, size, id],
            )
            .ok()?;
            tracing::debug!("[images] cached {} ({} bytes)", url_clone, size);
            Some(())
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_image_urls ---

    #[test]
    fn extract_finds_jpg_png_gif_webp() {
        let dat = "本文 https://example.com/image.jpg stuff\nhttps://img.test/photo.PNG\n";
        let urls = extract_image_urls(dat);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("image.jpg"));
        assert!(urls[1].contains("photo.PNG"));
    }

    #[test]
    fn extract_handles_ttp_and_ttps() {
        let dat = "ttp://example.com/a.gif\nttps://example.com/b.webp\n";
        let urls = extract_image_urls(dat);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("ttp://"));
        assert!(urls[1].contains("ttps://"));
    }

    #[test]
    fn extract_strips_duplicates_and_preserves_order() {
        let dat = "https://a.com/x.png https://b.com/y.jpg https://a.com/x.png";
        let urls = extract_image_urls(dat);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("x.png"));
        assert!(urls[1].contains("y.jpg"));
    }

    #[test]
    fn extract_excludes_non_image_links() {
        let dat = "https://example.com/page.html https://img.com/pic.jpg";
        let urls = extract_image_urls(dat);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("pic.jpg"));
    }

    #[test]
    fn extract_ignores_query_fragment_for_dedup() {
        // The regex stops at the \b after the image extension, so both
        // `a.jpg?v=1` and `a.jpg?v=2` extract as the same base URL `a.jpg`.
        // Deduplication collapses them to 1 entry.
        let dat = "https://img.com/a.jpg?v=1 https://img.com/a.jpg?v=2";
        let urls = extract_image_urls(dat);
        assert_eq!(
            urls.len(),
            1,
            "same base URL with different queries deduplicates to 1"
        );
    }

    // --- normalize_image_path ---

    #[test]
    fn normalize_basic_jpg() {
        let path = normalize_image_path("https://i.imgur.com/Abc123.jpg");
        assert_eq!(path, Some("i.imgur.com/Abc123.jpg".into()));
    }

    #[test]
    fn normalize_strips_query_and_fragment() {
        let path = normalize_image_path("https://i.imgur.com/Abc123.jpg?w=1#y");
        assert_eq!(path, Some("i.imgur.com/Abc123.jpg".into()));
    }

    #[test]
    fn normalize_ttp_scheme() {
        let path = normalize_image_path("ttp://img.example.com/x.png");
        assert_eq!(path, Some("img.example.com/x.png".into()));
    }

    #[test]
    fn normalize_ttps_scheme() {
        let path = normalize_image_path("ttps://img.example.com/x.webp");
        assert_eq!(path, Some("img.example.com/x.webp".into()));
    }

    #[test]
    fn normalize_lowercases_host_but_not_path() {
        // Host must be lowercased; the path (case-sensitive on most servers) must not change.
        let path = normalize_image_path("https://IMG.EXAMPLE.COM/Path/To/Image.JPG");
        assert_eq!(path.as_deref(), Some("img.example.com/Path/To/Image.JPG"));
    }

    #[test]
    fn normalize_non_image_returns_none() {
        assert_eq!(normalize_image_path("https://example.com/page.html"), None);
    }

    #[test]
    fn normalize_rejects_non_http_schemes() {
        assert_eq!(normalize_image_path("ftp://example.com/x.png"), None);
    }

    // --- is_safe_host ---

    #[test]
    fn safe_host_rejects_localhost() {
        assert!(!is_safe_host("localhost"));
        assert!(!is_safe_host("LOCALHOST"));
    }

    #[test]
    fn safe_host_rejects_loopback_ipv4() {
        assert!(!is_safe_host("127.0.0.1"));
    }

    #[test]
    fn safe_host_rejects_private_ipv4() {
        assert!(!is_safe_host("10.0.0.1"));
        assert!(!is_safe_host("192.168.1.1"));
        assert!(!is_safe_host("172.16.0.1"));
    }

    #[test]
    fn safe_host_rejects_link_local_ipv4() {
        assert!(!is_safe_host("169.254.1.1"));
    }

    #[test]
    fn safe_host_rejects_loopback_ipv6() {
        assert!(!is_safe_host("::1"));
    }

    #[test]
    fn safe_host_rejects_ula_ipv6() {
        assert!(!is_safe_host("fc00::1"));
        assert!(!is_safe_host("fd12:3456:789a::1"));
    }

    #[test]
    fn safe_host_rejects_link_local_ipv6() {
        assert!(!is_safe_host("fe80::1"));
    }

    #[test]
    fn safe_host_accepts_public_hosts() {
        assert!(is_safe_host("example.com"));
        assert!(is_safe_host("i.imgur.com"));
        assert!(is_safe_host("8.8.8.8"));
    }

    #[test]
    fn safe_host_rejects_unspecified_ipv4() {
        assert!(!is_safe_host("0.0.0.0"));
    }

    #[test]
    fn safe_host_rejects_broadcast_ipv4() {
        assert!(!is_safe_host("255.255.255.255"));
    }

    #[test]
    fn safe_host_rejects_multicast_ipv4() {
        assert!(!is_safe_host("224.0.0.1"));
    }

    #[test]
    fn safe_host_rejects_unspecified_ipv6() {
        assert!(!is_safe_host("::"));
    }

    #[test]
    fn safe_host_rejects_multicast_ipv6() {
        assert!(!is_safe_host("ff02::1"));
    }

    #[test]
    fn safe_host_rejects_ipv4_mapped_loopback() {
        // ::ffff:127.0.0.1 maps to the IPv4 loopback; must be rejected in non-test mode.
        // In unit tests FIVECH_ALLOW_LOOPBACK_FOR_TEST is not set, so it is rejected.
        assert!(!is_safe_host("::ffff:127.0.0.1"));
    }

    #[test]
    fn safe_host_rejects_ipv4_mapped_private() {
        assert!(!is_safe_host("::ffff:10.0.0.1"));
    }

    #[test]
    fn safe_host_accepts_ipv4_mapped_public() {
        // ::ffff:8.8.8.8 is a public IP expressed as IPv4-mapped IPv6; must be accepted.
        assert!(is_safe_host("::ffff:8.8.8.8"));
    }
}
