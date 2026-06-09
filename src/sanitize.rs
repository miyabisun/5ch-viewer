//! レス本文の HTML サニタイズ。5ch 本文は `<br>` や `<a>`、HTML エンティティを
//! 含むため、許可リスト方式（ammonia）で安全化する。許可は `a`(href のみ) と `br`。

use ammonia::Builder;
use std::collections::HashSet;
use std::sync::LazyLock;

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let tags: HashSet<&str> = ["a", "br"].into_iter().collect();
    let mut builder = Builder::default();
    // 許可タグを a, br のみに絞る。a の href 属性と URL スキーム制限は
    // ammonia のデフォルト（http/https/mailto 等のみ、javascript: 等は除去）に従う。
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
        // p は許可外なのでタグは消え、テキストは残る
        assert!(out.contains("x"));
    }

    #[test]
    fn strips_javascript_href() {
        let out = clean(r#"<a href="javascript:alert(1)">x</a>"#);
        assert!(!out.contains("javascript"));
    }

    #[test]
    fn keeps_entities_as_text() {
        // アンカー >>123 は &gt;&gt;123 として残る（フロントでリンク化する）
        let out = clean("&gt;&gt;123 本文");
        assert!(out.contains("&gt;&gt;123") || out.contains(">>123"));
    }

    #[test]
    fn handles_plain_text() {
        assert_eq!(clean("ただのテキスト"), "ただのテキスト");
    }
}
