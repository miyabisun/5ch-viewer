//! dat のパース。1 行 = 1 レス。フィールドは `<>` 区切り:
//!   名前<>メール<>日付 ID等<>本文<>スレッドタイトル(1レス目のみ)
//! レス番号は行番号（1 始まり）。

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Res {
    pub num: i64,
    pub name: String,
    pub mail: String,
    /// 日付 + ID 等（dat の 3 フィールド目をそのまま）。
    pub date: String,
    pub body: String,
}

/// dat 本文（UTF-8 デコード済み）をレス配列にパースする。
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

/// 1 レス目（5 フィールド目）からスレッドタイトルを取り出す。
pub fn title_from_dat(text: &str) -> Option<String> {
    let first = text.split('\n').next()?;
    let f: Vec<&str> = first.split("<>").collect();
    if f.len() >= 5 && !f[4].trim().is_empty() {
        Some(f[4].trim().to_string())
    } else {
        None
    }
}

/// Range 差分（Shift_JIS デコード済み）が「正常な追記」かを検証する。
/// あぼーん等で過去が書き換わった壊れた差分を弾く（不正なら全取得リペアへ）。
/// レスは必ず `名前<>メール<>日付ID<>本文<>` を含むため、ヘッダー欠損で検知できる。
pub fn validate_diff(text: &str) -> bool {
    if text.is_empty() {
        return true; // 追記ゼロ
    }
    // 完結したレスのみを受け入れる: 必ず改行終端。
    if !text.ends_with('\n') {
        return false;
    }
    for line in text.split('\n') {
        if line.is_empty() {
            continue; // 末尾 \n 由来の空要素
        }
        let f: Vec<&str> = line.split("<>").collect();
        if f.len() < 4 {
            return false; // ヘッダー欠損（例: 5 バイトだけ増えた壊れた差分）
        }
        if f[2].trim().is_empty() {
            return false; // 日付ID 欠損 = 壊れ / あぼーん置換
        }
    }
    true
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

    #[test]
    fn validate_diff_accepts_normal_append() {
        let text = "名無し<>sage<>2025/01/01 ID:abc<>本文1<>\n名無し<><>2025/01/02 ID:def<>本文2<>\n";
        assert!(validate_diff(text));
    }

    #[test]
    fn validate_diff_rejects_header_missing() {
        // 5 バイト問題相当: <> が足りない壊れた差分
        assert!(!validate_diff(" abc \n"));
        assert!(!validate_diff("a<>b<>c\n")); // フィールド3個
    }

    #[test]
    fn validate_diff_rejects_empty_date_field() {
        // 日付ID(3番目)が空 = あぼーん置換等
        assert!(!validate_diff("名無し<>sage<><>本文<>\n"));
    }

    #[test]
    fn validate_diff_rejects_non_newline_terminated() {
        assert!(!validate_diff("名無し<>sage<>2025 ID:x<>途中まで"));
    }

    #[test]
    fn validate_diff_accepts_empty() {
        assert!(validate_diff(""));
    }
}
