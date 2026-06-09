//! 次スレ判定。sentinel の find-next-thread.js を移植する。
//!
//! ▼▼▼ ここはあなたが実装する箇所です（learning mode）▼▼▼
//! テストは下に用意済み。`cargo test next_thread` で Red→Green を回せます。

use crate::goch::subject::SubjectEntry;
use regex_lite::Regex;
use std::sync::LazyLock;

static NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());

/// タイトル中の数字とその位置（バイトオフセット）。
struct NumberPos {
    raw: String,
    num: u64,
    start: usize,
}

/// 現在のスレタイから「次スレ（Part 番号 +1）」を subject 一覧から探す。
///
/// アルゴリズム（sentinel 準拠）:
/// 1. タイトル中の数字を **位置付きで全て** 抽出する。
/// 2. **右端の数字から順に** +1 を試す（Part 番号は末尾寄りのため）。
/// 3. その数字の **前後 最大6文字** を「コンテキスト」とし、候補タイトルが
///    「前コンテキストを含む」かつ「後コンテキストを含む」かつ
///    「+1 した数値文字列を含む」場合にマッチとする（誤爆防止）。
///    - 前後コンテキストが両方とも空（タイトルが数字だけ）の位置はスキップ。
/// 4. ゼロパディング形式（例 `Part09` → `Part10`）も試す。
/// 5. 見つかれば最初の該当 entry、なければ None。
pub fn find_next_thread<'a>(
    current_title: &str,
    entries: &'a [SubjectEntry],
) -> Option<&'a SubjectEntry> {
    let positions: Vec<NumberPos> = NUMBER_RE
        .find_iter(current_title)
        .filter_map(|m| {
            m.as_str().parse::<u64>().ok().map(|num| NumberPos {
                raw: m.as_str().to_string(),
                num,
                start: m.start(),
            })
        })
        .collect();
    if positions.is_empty() {
        return None;
    }

    // 右端の数字から +1 を試す（Part 番号は末尾寄りのため）。
    for pos in positions.iter().rev() {
        // start と raw.len() はバイト数なので、このスライスは char 境界上で安全。
        let prefix = &current_title[..pos.start];
        let suffix = &current_title[pos.start + pos.raw.len()..];

        // 前後の最大6文字を char 単位で切り出す（UTF-8 のバイト境界 panic を避ける）。
        let prefix_chars: Vec<char> = prefix.chars().collect();
        let prefix_frag: String = prefix_chars[prefix_chars.len().saturating_sub(6)..]
            .iter()
            .collect();
        let suffix_frag: String = suffix.chars().take(6).collect();

        // タイトルが数字だけ（前後コンテキスト無し）ならスキップ。
        if prefix_frag.is_empty() && suffix_frag.is_empty() {
            continue;
        }

        let matches_context = |candidate: &str| {
            (prefix_frag.is_empty() || candidate.contains(&prefix_frag))
                && (suffix_frag.is_empty() || candidate.contains(&suffix_frag))
        };

        let next_str = (pos.num + 1).to_string();
        if let Some(found) = entries
            .iter()
            .find(|e| matches_context(&e.title) && e.title.contains(&next_str))
        {
            return Some(found);
        }

        // ゼロパディング形式（例 Part008 → Part009）も試す。
        let padded = format!("{:0>width$}", next_str, width = pos.raw.len());
        if padded != next_str {
            if let Some(found) = entries
                .iter()
                .find(|e| matches_context(&e.title) && e.title.contains(&padded))
            {
                return Some(found);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(thread_id: &str, title: &str, res_count: i64) -> SubjectEntry {
        SubjectEntry {
            thread_id: thread_id.into(),
            title: title.into(),
            res_count,
        }
    }

    #[test]
    fn finds_next_thread_for_real_buruaka_title() {
        let entries = vec![
            entry(
                "1771127145",
                "【ブルアカ】ブルーアーカイブ -Blue Archive- Part5843",
                491,
            ),
            entry(
                "1771200000",
                "【ブルアカ】ブルーアーカイブ -Blue Archive- Part5844",
                23,
            ),
        ];
        let result = find_next_thread(
            "【ブルアカ】ブルーアーカイブ -Blue Archive- Part5843",
            &entries,
        );
        assert_eq!(result, Some(&entries[1]));
    }

    #[test]
    fn handles_zero_padded_part_numbers() {
        let entries = vec![
            entry("1770000000", "テストスレ Part09", 900),
            entry("1770100000", "テストスレ Part10", 5),
        ];
        let result = find_next_thread("テストスレ Part09", &entries);
        assert_eq!(result, Some(&entries[1]));
    }

    #[test]
    fn prefers_rightmost_number_for_incrementing() {
        let entries = vec![
            entry("1770000000", "5ch総合 Part3", 100),
            entry("1770100000", "5ch総合 Part4", 10),
            entry("1770200000", "6ch総合 Part3", 10),
        ];
        // "5ch総合 Part3" の数字は 5 と 3。右端の 3 を +1 して Part4 にマッチすべき
        let result = find_next_thread("5ch総合 Part3", &entries);
        assert_eq!(result, Some(&entries[1]));
    }

    #[test]
    fn returns_none_when_title_has_no_numbers() {
        let entries = vec![entry("1770000000", "なにかのスレッド", 100)];
        let result = find_next_thread("数字なしタイトル", &entries);
        assert_eq!(result, None);
    }

    #[test]
    fn returns_none_when_no_matching_next_thread() {
        let entries = vec![entry("1770000000", "全然関係ないスレ Part1", 100)];
        let result = find_next_thread(
            "【ブルアカ】ブルーアーカイブ -Blue Archive- Part5843",
            &entries,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn returns_none_for_empty_entries() {
        let entries: Vec<SubjectEntry> = vec![];
        let result = find_next_thread("【ブルアカ】ブルーアーカイブ -Blue Archive- Part5843", &entries);
        assert_eq!(result, None);
    }

    #[test]
    fn does_not_false_match_when_title_starts_with_number() {
        let entries = vec![
            entry("1770000000", "無関係スレ 11", 100),
            entry("1770100000", "11スレ目", 50),
        ];
        // "10スレ目" は数字が先頭（前コンテキストが空）。
        // 後コンテキスト "スレ目" を持つ "11スレ目" にマッチすべき
        let result = find_next_thread("10スレ目", &entries);
        assert_eq!(result, Some(&entries[1]));
    }

    #[test]
    fn handles_single_digit_increment() {
        let entries = vec![entry("1770100000", "テストスレ Part10", 5)];
        let result = find_next_thread("テストスレ Part9", &entries);
        assert_eq!(result, Some(&entries[0]));
    }
}
