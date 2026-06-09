//! HTML sanitization of post bodies. Since 5ch bodies contain `<br>`, `<a>`, and
//! HTML entities, they are made safe via an allowlist approach (ammonia). Allowed: `a` (href only) and `br`.

use ammonia::Builder;
use std::collections::HashSet;
use std::sync::LazyLock;

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let tags: HashSet<&str> = ["a", "br"].into_iter().collect();
    let mut builder = Builder::default();
    // Restrict allowed tags to a and br. The a tag's href attribute and URL scheme
    // restrictions follow ammonia's defaults (only http/https/mailto etc., removing javascript: etc.).
    builder.tags(tags);
    builder
});

pub fn clean(html: &str) -> String {
    SANITIZER.clean(html).to_string()
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
}
