//! dat parsing. 1 line = 1 post. Fields are separated by `<>`:
//!   name<>mail<>date ID etc.<>body<>thread title (first post only)
//! The post number is the line number (1-based).

use regex_lite::Regex;
use serde::Serialize;
use std::sync::LazyLock;

// Matches "ID:" followed by non-whitespace characters.
// 5ch IDs appear after the date/time and are delimited by whitespace (ASCII or full-width)
// or end of string. We capture everything after "ID:" until the next whitespace or EOL.
static ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ID:([^\s\u{3000}]+)").unwrap());

/// Extracts the raw ID string (the part after "ID:") from the dat date field.
/// Returns None when no ID token is present (e.g., the OP post on some boards).
pub fn extract_id(date: &str) -> Option<String> {
    ID_RE
        .captures(date)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Res {
    pub num: i64,
    pub name: String,
    pub mail: String,
    /// Date + ID etc. (the dat's 3rd field as-is).
    pub date: String,
    pub body: String,
    /// Extracted ID key (the part after "ID:"), or null when absent.
    pub id: Option<String>,
    /// True when this res was posted by the current user (matched via own_posts table).
    /// Set to false by parse_dat; overridden to true in get_dat handler.
    #[serde(skip_deserializing, default)]
    pub own: bool,
}

/// Splits one dat line into its `<>`-separated fields, or `None` if it is empty or
/// malformed (fewer than 4 fields). The single predicate shared by `parse_dat` (which
/// builds posts) and `count_dat_posts` (which only counts), so the two never disagree.
fn dat_fields(line: &str) -> Option<Vec<&str>> {
    if line.is_empty() {
        return None;
    }
    let f: Vec<&str> = line.split("<>").collect();
    (f.len() >= 4).then_some(f)
}

/// Parses dat content (already UTF-8 decoded) into an array of posts.
pub fn parse_dat(text: &str) -> Vec<Res> {
    text.split('\n')
        .enumerate()
        .filter_map(|(i, line)| {
            let f = dat_fields(line)?;
            let date = f[2].to_string();
            let id = extract_id(&date);
            Some(Res {
                num: (i + 1) as i64,
                name: f[0].to_string(),
                mail: f[1].to_string(),
                date,
                body: f[3].to_string(),
                id,
                own: false,
            })
        })
        .collect()
}

/// Counts the posts in dat content without allocating them — the cheap path for the
/// reload/prefetch gate, which only needs "how many posts do we hold". Counts exactly
/// the lines `parse_dat` would keep.
pub fn count_dat_posts(text: &str) -> i64 {
    text.split('\n').filter(|l| dat_fields(l).is_some()).count() as i64
}

/// Extracts the thread title from the first post (the 5th field).
pub fn title_from_dat(text: &str) -> Option<String> {
    let first = text.split('\n').next()?;
    let f: Vec<&str> = first.split("<>").collect();
    if f.len() >= 5 && !f[4].trim().is_empty() {
        Some(f[4].trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_dat() {
        let text = "名無し<>sage<>2025/01/01 ID:abc<>本文1<>スレタイ\n\
                    名無し2<><>2025/01/02 ID:def<>本文2<>\n";
        let res = parse_dat(text);
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].num, 1);
        assert_eq!(res[0].name, "名無し");
        assert_eq!(res[0].mail, "sage");
        assert_eq!(res[0].body, "本文1");
        assert_eq!(res[1].num, 2);
        assert_eq!(res[1].mail, "");
        assert_eq!(res[1].body, "本文2");
    }

    #[test]
    fn skips_malformed_and_trailing_empty() {
        let text = "a<>b<>c<>d\nbroken line\n\n";
        let res = parse_dat(text);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].num, 1);
    }

    #[test]
    fn extracts_title_from_first_line() {
        let text = "名無し<>sage<>2025 ID:x<>本文<>【テスト】スレッド Part1\n名無し<><>2025<>本文2<>\n";
        assert_eq!(
            title_from_dat(text),
            Some("【テスト】スレッド Part1".to_string())
        );
    }

    #[test]
    fn title_none_when_absent() {
        let text = "名無し<>sage<>2025 ID:x<>本文\n";
        assert_eq!(title_from_dat(text), None);
    }

    // --- extract_id tests ---

    #[test]
    fn extract_id_typical() {
        assert_eq!(
            extract_id("2025/01/01(水) 12:34:56.78 ID:klSUPSuq0"),
            Some("klSUPSuq0".to_string())
        );
    }

    #[test]
    fn extract_id_none_when_absent() {
        assert_eq!(extract_id("2025/01/01(水) 12:34:56.78"), None);
    }

    #[test]
    fn extract_id_full_width_space_separator() {
        // Some 5ch boards use a full-width space (U+3000) before or after the ID.
        assert_eq!(
            extract_id("2025/01/01(水) 12:34:56.78\u{3000}ID:abc123\u{3000}BE:123"),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn extract_id_at_end_of_string() {
        // ID at end with no trailing space.
        assert_eq!(
            extract_id("2025/01/01(水) 12:34:56.78 ID:AbC123"),
            Some("AbC123".to_string())
        );
    }

    #[test]
    fn extract_id_multiple_spaces() {
        // Extra spaces before ID are fine.
        assert_eq!(
            extract_id("2025/01/01  12:00:00  ID:xyz99"),
            Some("xyz99".to_string())
        );
    }

    #[test]
    fn extract_id_with_symbols() {
        // IDs can contain +, /, = (base64-like).
        assert_eq!(
            extract_id("2025/06/01(月) 00:00:00.00 ID:a+b/c=="),
            Some("a+b/c==".to_string())
        );
    }

    #[test]
    fn parse_dat_populates_id_field() {
        let text = "名無し<>sage<>2025/01/01 ID:hello<>本文<>スレ\n\
                    名無し<><>2025/01/02<>本文2<>\n";
        let res = parse_dat(text);
        assert_eq!(res[0].id, Some("hello".to_string()));
        assert_eq!(res[1].id, None);
    }
}
