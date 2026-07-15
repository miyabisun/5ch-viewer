//! HTML sanitization of post bodies.
//! Pipeline: (1) strip thread-internal anchors (read.cgi hrefs → text-only),
//! (2) normalize ttp/ttps schemes in href to http/https,
//! (3) ammonia allowlist sanitize (keep `a`/`br`; drop javascript: etc.).

use ammonia::Builder;
use regex_lite::Regex;
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;

// Matches any <a ...> tag (with attributes) followed by its content and </a>.
// Used to locate anchor elements whose href determines keep-vs-strip behaviour.
static ANCHOR_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<a\b([^>]*?)>(.*?)</a>").unwrap());

// Extracts the value of the href attribute from the attribute string of an <a> tag.
static HREF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\bhref\s*=\s*"([^"]*)""#).unwrap());

// Matches ttp:// or ttps:// at the start of a href value (5ch broken-scheme URLs).
static TTP_HREF_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Replaces ttp(s):// inside href="..." attributes only.
    Regex::new(r#"(?i)(href\s*=\s*")ttps?://"#).unwrap()
});

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let tags: HashSet<&str> = ["a", "br"].into_iter().collect();
    let mut builder = Builder::default();
    // Restrict allowed tags to a and br. The a tag's href attribute and URL scheme
    // restrictions follow ammonia's defaults (only http/https/mailto etc., removing javascript: etc.).
    builder.tags(tags);
    builder
});

/// Returns true when the href points to a thread-internal anchor (read.cgi path).
/// Absolute external URLs (http/https/ttp/ttps) are excluded from stripping.
fn is_thread_anchor_href(href: &str) -> bool {
    let h = href.trim();
    // Absolute external URLs are not thread anchors.
    if h.starts_with("http://")
        || h.starts_with("https://")
        || h.starts_with("ttp://")
        || h.starts_with("ttps://")
    {
        return false;
    }
    // 5ch thread-internal links contain read.cgi in the path.
    h.contains("read.cgi")
}

/// Step 1: Strip <a> tags whose href is a thread-internal anchor.
/// Replaces the whole <a ...>text</a> with just the inner text.
fn strip_thread_anchors(html: &str) -> Cow<'_, str> {
    ANCHOR_TAG_RE.replace_all(html, |caps: &regex_lite::Captures| {
        let attrs = caps.get(1).map_or("", |m| m.as_str());
        let inner = caps.get(2).map_or("", |m| m.as_str());
        if let Some(href_cap) = HREF_RE.captures(attrs) {
            let href = href_cap.get(1).map_or("", |m| m.as_str());
            if is_thread_anchor_href(href) {
                return inner.to_string();
            }
        }
        // Not a thread anchor: leave the whole match intact.
        caps.get(0).map_or("", |m| m.as_str()).to_string()
    })
}

/// Step 2: Normalize ttp:// → http:// and ttps:// → https:// inside href attributes only.
/// This lets ammonia preserve the <a> tag (its default allowlist only permits http/https).
fn normalize_ttp_hrefs(html: &str) -> Cow<'_, str> {
    TTP_HREF_RE.replace_all(html, |caps: &regex_lite::Captures| {
        let prefix = caps.get(1).map_or("", |m| m.as_str()); // e.g. href="
        let full = caps.get(0).map_or("", |m| m.as_str());
        // Determine whether it was ttps or ttp by checking the original match.
        if full.to_ascii_lowercase().contains("ttps://") {
            format!("{prefix}https://")
        } else {
            format!("{prefix}http://")
        }
    })
}

pub fn clean(html: &str) -> String {
    let s1 = strip_thread_anchors(html);
    let s2 = normalize_ttp_hrefs(s1.as_ref());
    SANITIZER.clean(s2.as_ref()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_br_and_link() {
        let out = clean(r#"行1<br>行2 <a href="https://example.com">link</a>"#);
        assert!(out.contains("<br>"));
        assert!(out.contains("<a"));
        assert!(out.contains("href"));
        assert!(out.contains("link"));
    }

    #[test]
    fn strips_script_and_handlers() {
        let out = clean(r#"<p onclick="evil()">x</p><script>alert(1)</script>"#);
        assert!(!out.contains("script"));
        assert!(!out.contains("alert"));
        assert!(!out.contains("onclick"));
        // p is not allowed so the tag is removed, but the text remains
        assert!(out.contains("x"));
    }

    #[test]
    fn strips_javascript_href() {
        let out = clean(r#"<a href="javascript:alert(1)">x</a>"#);
        assert!(!out.contains("javascript"));
    }

    #[test]
    fn keeps_entities_as_text() {
        // the anchor >>123 remains as &gt;&gt;123 (the frontend turns it into a link)
        let out = clean("&gt;&gt;123 本文");
        assert!(out.contains("&gt;&gt;123") || out.contains(">>123"));
    }

    #[test]
    fn handles_plain_text() {
        assert_eq!(clean("ただのテキスト"), "ただのテキスト");
    }

    // --- thread-anchor stripping ---

    #[test]
    fn strips_thread_anchor_relative_path() {
        // Typical 5ch dat: relative read.cgi href wrapping >>N text.
        let out = clean(r#"<a href="../test/read.cgi/board/1771127145/1">&gt;&gt;1</a>"#);
        assert!(!out.contains("<a"), "anchor tag must be removed: {out}");
        assert!(
            out.contains("&gt;&gt;1") || out.contains(">>1"),
            "inner text must survive: {out}"
        );
    }

    #[test]
    fn strips_thread_anchor_absolute_path() {
        // Absolute path variant (starts with /test/read.cgi/...).
        let out = clean(r#"<a href="/test/read.cgi/board/1771127145/2">&gt;&gt;2</a>"#);
        assert!(!out.contains("<a"), "anchor tag must be removed: {out}");
        assert!(
            out.contains("&gt;&gt;2") || out.contains(">>2"),
            "inner text must survive: {out}"
        );
    }

    #[test]
    fn strips_multiple_thread_anchors() {
        // Two thread-anchor links in one body; both must be stripped.
        let out = clean(
            r#"<a href="../test/read.cgi/b/123/1">&gt;&gt;1</a> と <a href="../test/read.cgi/b/123/2">&gt;&gt;2</a>"#,
        );
        assert!(!out.contains("<a"), "no anchor tags must remain: {out}");
        assert!(
            out.contains("&gt;&gt;1") || out.contains(">>1"),
            ">>1 text must survive: {out}"
        );
        assert!(
            out.contains("&gt;&gt;2") || out.contains(">>2"),
            ">>2 text must survive: {out}"
        );
    }

    #[test]
    fn preserves_external_https_link() {
        // http/https external URLs must keep their <a> tag.
        let out = clean(r#"<a href="https://example.com">example</a>"#);
        assert!(out.contains("<a"), "external link must be kept: {out}");
        assert!(out.contains("href"), "href must be kept: {out}");
    }

    // --- ttp/ttps scheme normalization ---

    #[test]
    fn normalizes_ttp_to_http() {
        let out = clean(r#"<a href="ttp://example.com">x</a>"#);
        assert!(out.contains("<a"), "ttp link must be kept as <a>: {out}");
        // "ttp://" must not appear as a standalone scheme prefix (without leading 'h').
        assert!(
            !out.contains(r#"href="ttp://"#),
            "ttp:// scheme must be normalized away in href: {out}"
        );
        assert!(
            out.contains("http://example.com"),
            "href must be normalized to http://: {out}"
        );
    }

    #[test]
    fn normalizes_ttps_to_https() {
        let out = clean(r#"<a href="ttps://example.com">x</a>"#);
        assert!(out.contains("<a"), "ttps link must be kept as <a>: {out}");
        // "ttps://" must not appear as a standalone scheme prefix (without leading 'h').
        assert!(
            !out.contains(r#"href="ttps://"#),
            "ttps:// scheme must be normalized away in href: {out}"
        );
        assert!(
            out.contains("https://example.com"),
            "href must be normalized to https://: {out}"
        );
    }

    #[test]
    fn does_not_normalize_ttp_in_body_text() {
        // ttp:// appearing as plain text (not inside href="...") must not be touched.
        let out = clean("本文に ttp://example.com というテキストがある");
        assert!(
            out.contains("ttp://example.com"),
            "plain-text ttp must be untouched: {out}"
        );
    }
}
