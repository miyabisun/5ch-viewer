//! dat parsing. 1 line = 1 post. Fields are separated by `<>`:
//!   name<>mail<>date ID etc.<>body<>thread title (first post only)
//! The post number is the line number (1-based).

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Res {
    pub num: i64,
    pub name: String,
    pub mail: String,
    /// Date + ID etc. (the dat's 3rd field as-is).
    pub date: String,
    pub body: String,
}

/// Parses dat content (already UTF-8 decoded) into an array of posts.
pub fn parse_dat(text: &str) -> Vec<Res> {
    let mut out = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split("<>").collect();
        if f.len() < 4 {
            continue;
        }
        out.push(Res {
            num: (i + 1) as i64,
            name: f[0].to_string(),
            mail: f[1].to_string(),
            date: f[2].to_string(),
            body: f[3].to_string(),
        });
    }
    out
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
}
