//! Thread URL parsing.
//! Supported formats (both 5ch.net / 5ch.io. Migrated to io in 2026-03. Since the
//! net format remains in archive URLs, both are accepted as input):
//!   https://[server].5ch.io/test/read.cgi/[board]/[thread_id]/
//!   https://[server].5ch.io/[board]/[thread_id]/   (legacy/short form)

use crate::error::AppError;
use regex_lite::Regex;
use std::sync::LazyLock;

static THREAD_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://([^.]+)\.5ch\.(?:net|io)/(?:test/read\.cgi/)?([^/]+)/(\d+)").unwrap()
});

// SSRF mitigation: server/board allow only alphanumerics, hyphen, and underscore; thread_id only digits.
// This prevents host/path injection (slash, dot, @, etc.) during URL assembly.
static SEGMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());
static THREAD_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9]+$").unwrap());

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

/// Strictly validates server/board/thread_id (defense-in-depth against SSRF).
/// Call at the entry of every path that receives user input (URL/direct/search result).
pub fn validate_ref(server: &str, board: &str, thread_id: &str) -> Result<(), AppError> {
    let check = |re: &Regex, name: &str, value: &str| {
        re.is_match(value)
            .then_some(())
            .ok_or_else(|| AppError::BadRequest(format!("invalid {name}: {value}")))
    };
    check(&SEGMENT_RE, "server", server)?;
    check(&SEGMENT_RE, "board", board)?;
    check(&THREAD_ID_RE, "thread_id", thread_id)?;
    Ok(())
}

/// Validates only server and board segments (no thread_id). Used for board-level
/// endpoints (e.g. id-search) where thread_id is not part of the path.
pub fn validate_board(server: &str, board: &str) -> Result<(), AppError> {
    let check = |re: &Regex, name: &str, value: &str| {
        re.is_match(value)
            .then_some(())
            .ok_or_else(|| AppError::BadRequest(format!("invalid {name}: {value}")))
    };
    check(&SEGMENT_RE, "server", server)?;
    check(&SEGMENT_RE, "board", board)?;
    Ok(())
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
        // 2ch.net is out of scope (only 5ch.net / 5ch.io)
        assert_eq!(
            parse_thread_url("https://eagle.2ch.net/test/read.cgi/news/1700000000/"),
            None
        );
    }

    #[test]
    fn parses_5ch_io_url() {
        // post-migration 5ch.io format
        assert_eq!(
            parse_thread_url("https://kizuna.5ch.io/test/read.cgi/iPhone/1773047861/"),
            Some(tref("kizuna", "iPhone", "1773047861"))
        );
    }

    #[test]
    fn validate_ref_accepts_normal_values() {
        assert!(validate_ref("kizuna", "iPhone", "1773047861").is_ok());
        assert!(validate_ref("rio2016", "news4vip", "1780976160").is_ok());
        assert!(validate_ref("a-b_c", "x_y-z", "0").is_ok());
    }

    #[test]
    fn validate_ref_rejects_slash() {
        assert!(validate_ref("evil/host", "board", "123").is_err());
        assert!(validate_ref("server", "../etc", "123").is_err());
    }

    #[test]
    fn validate_ref_rejects_dot() {
        // prevent host injection (different domain)
        assert!(validate_ref("attacker.com", "board", "123").is_err());
        assert!(validate_ref("server", "bo.ard", "123").is_err());
    }

    #[test]
    fn validate_ref_rejects_special_chars() {
        assert!(validate_ref("server@evil", "board", "123").is_err());
        assert!(validate_ref("server", "bo ard", "123").is_err());
        assert!(validate_ref("server:8080", "board", "123").is_err());
    }

    #[test]
    fn validate_ref_rejects_non_numeric_thread_id() {
        assert!(validate_ref("server", "board", "12a3").is_err());
        assert!(validate_ref("server", "board", "../1").is_err());
        assert!(validate_ref("server", "board", "").is_err());
    }

    #[test]
    fn validate_ref_rejects_empty_segments() {
        assert!(validate_ref("", "board", "123").is_err());
        assert!(validate_ref("server", "", "123").is_err());
    }

    #[test]
    fn validate_board_accepts_normal_values() {
        assert!(validate_board("kizuna", "iPhone").is_ok());
        assert!(validate_board("eagle", "livejupiter").is_ok());
    }

    #[test]
    fn validate_board_rejects_bad_server() {
        assert!(validate_board("evil/host", "board").is_err());
        assert!(validate_board("", "board").is_err());
    }

    #[test]
    fn validate_board_rejects_bad_board() {
        assert!(validate_board("server", "bo.ard").is_err());
        assert!(validate_board("server", "").is_err());
    }
}
