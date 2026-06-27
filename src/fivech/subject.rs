//! subject.txt parsing.
//! Each line: `<thread_id>.dat<>title (resCount)`

use regex_lite::Regex;
use std::sync::LazyLock;

// Since the greedy `.+` consumes up to the last `(`, parentheses within the
// title are treated as part of the title (only the trailing `(number)` is resCount).
static SUBJECT_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)\.dat<>(.+)\((\d+)\)$").unwrap());

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectEntry {
    pub thread_id: String,
    pub title: String,
    pub res_count: i64,
}

pub fn parse_subject_txt(text: &str) -> Vec<SubjectEntry> {
    let mut entries = Vec::new();
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(caps) = SUBJECT_LINE_RE.captures(trimmed) {
            entries.push(SubjectEntry {
                thread_id: caps[1].to_string(),
                title: caps[2].trim().to_string(),
                res_count: caps[3].parse().unwrap_or(0),
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_subject_lines() {
        let text = "1771127145.dat<>【ブルアカ】ブルーアーカイブ -Blue Archive- Part5843 (491)\n\
                    1771200000.dat<>【ブルアカ】ブルーアーカイブ -Blue Archive- Part5844 (23)";
        let result = parse_subject_txt(text);
        assert_eq!(
            result,
            vec![
                SubjectEntry {
                    thread_id: "1771127145".into(),
                    title: "【ブルアカ】ブルーアーカイブ -Blue Archive- Part5843".into(),
                    res_count: 491,
                },
                SubjectEntry {
                    thread_id: "1771200000".into(),
                    title: "【ブルアカ】ブルーアーカイブ -Blue Archive- Part5844".into(),
                    res_count: 23,
                },
            ]
        );
    }

    #[test]
    fn skips_empty_lines() {
        let text = "1771127145.dat<>スレ Part1 (100)\n\n\n1771200000.dat<>スレ Part2 (200)\n";
        let result = parse_subject_txt(text);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].thread_id, "1771127145");
        assert_eq!(result[1].thread_id, "1771200000");
    }

    #[test]
    fn skips_malformed_lines() {
        let text = "1771127145.dat<>正常行 (100)\n\
                    this is not a valid line\n\
                    no-dat-here<>壊れた行 (50)\n\
                    1771200000.dat<>正常行2 (200)";
        let result = parse_subject_txt(text);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0],
            SubjectEntry {
                thread_id: "1771127145".into(),
                title: "正常行".into(),
                res_count: 100,
            }
        );
        assert_eq!(
            result[1],
            SubjectEntry {
                thread_id: "1771200000".into(),
                title: "正常行2".into(),
                res_count: 200,
            }
        );
    }

    #[test]
    fn returns_empty_for_empty_string() {
        assert_eq!(parse_subject_txt(""), vec![]);
    }

    #[test]
    fn trims_whitespace_from_title() {
        let text = "1771127145.dat<> スペース付きタイトル  (100)";
        let result = parse_subject_txt(text);
        assert_eq!(result[0].title, "スペース付きタイトル");
    }

    #[test]
    fn parses_res_count_as_integer() {
        // parse as integer even with leading zeros
        let text = "1771127145.dat<>テスト (0491)";
        let result = parse_subject_txt(text);
        assert_eq!(result[0].res_count, 491);
    }

    #[test]
    fn handles_title_containing_parentheses() {
        let text = "1771127145.dat<>スレ (仮) Part1 (300)";
        let result = parse_subject_txt(text);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "スレ (仮) Part1");
        assert_eq!(result[0].res_count, 300);
    }
}
