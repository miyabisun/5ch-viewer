//! スレッド URL のパース。
//! 対応形式（5ch.net / 5ch.io 両対応。2026-03 に io へ移転。過去ログ URL に
//! net 形式が残るため入力は両方受理する）:
//!   https://[server].5ch.io/test/read.cgi/[board]/[thread_id]/
//!   https://[server].5ch.io/[board]/[thread_id]/   (旧/短縮形)

use regex_lite::Regex;
use std::sync::LazyLock;

static THREAD_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://([^.]+)\.5ch\.(?:net|io)/(?:test/read\.cgi/)?([^/]+)/(\d+)").unwrap()
});

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadRef {
    pub server: String,
    pub board: String,
    pub thread_id: String,
}

pub fn parse_thread_url(url: &str) -> Option<ThreadRef> {
    let caps = THREAD_URL_RE.captures(url)?;
    Some(ThreadRef {
        server: caps[1].to_string(),
        board: caps[2].to_string(),
        thread_id: caps[3].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tref(server: &str, board: &str, thread_id: &str) -> ThreadRef {
        ThreadRef {
            server: server.into(),
            board: board.into(),
            thread_id: thread_id.into(),
        }
    }

    #[test]
    fn parses_read_cgi_format() {
        assert_eq!(
            parse_thread_url("https://kizuna.5ch.net/test/read.cgi/iPhone/1771127145/"),
            Some(tref("kizuna", "iPhone", "1771127145"))
        );
    }

    #[test]
    fn parses_legacy_short_format() {
        assert_eq!(
            parse_thread_url("https://kizuna.5ch.net/iPhone/1771127145/"),
            Some(tref("kizuna", "iPhone", "1771127145"))
        );
    }

    #[test]
    fn parses_url_without_trailing_slash() {
        assert_eq!(
            parse_thread_url("https://eagle.5ch.net/test/read.cgi/livejupiter/1700000000"),
            Some(tref("eagle", "livejupiter", "1700000000"))
        );
    }

    #[test]
    fn parses_http_url() {
        assert_eq!(
            parse_thread_url("http://eagle.5ch.net/test/read.cgi/livejupiter/1700000000/"),
            Some(tref("eagle", "livejupiter", "1700000000"))
        );
    }

    #[test]
    fn returns_none_for_invalid_url() {
        assert_eq!(parse_thread_url("https://example.com/foo"), None);
    }

    #[test]
    fn returns_none_for_empty_string() {
        assert_eq!(parse_thread_url(""), None);
    }

    #[test]
    fn returns_none_for_non_5ch_url() {
        // 2ch.net は対象外（5ch.net / 5ch.io のみ）
        assert_eq!(
            parse_thread_url("https://eagle.2ch.net/test/read.cgi/news/1700000000/"),
            None
        );
    }

    #[test]
    fn parses_5ch_io_url() {
        // 移転後の 5ch.io 形式
        assert_eq!(
            parse_thread_url("https://kizuna.5ch.io/test/read.cgi/iPhone/1773047861/"),
            Some(tref("kizuna", "iPhone", "1773047861"))
        );
    }
}
