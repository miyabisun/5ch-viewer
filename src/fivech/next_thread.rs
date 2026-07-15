//! Next-thread detection. A port of sentinel's find-next-thread.js.

use crate::fivech::subject::SubjectEntry;
use regex_lite::Regex;
use std::sync::LazyLock;

static NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());

/// Removes all whitespace (half-width space, full-width U+3000, tabs, …) from a string.
/// Context matching is done on whitespace-stripped forms so a thread starter's spacing
/// wobble (e.g. `★503 【転載禁止】` vs `★504【転載禁止】`) does not break next-thread detection.
fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// A number within the title and its position (byte offset).
struct NumberPos {
    raw: String,
    num: u64,
    start: usize,
}

/// Finds the "next thread (Part number +1)" from the subject list, based on the current thread title.
///
/// Algorithm (following sentinel):
/// 1. Extract **all numbers with their positions** from the title.
/// 2. Try +1 **starting from the rightmost number** (the Part number tends to be near the end).
/// 3. Treat **up to 6 chars before and after** that number as "context"; a candidate title matches
///    if it "contains the prefix context" AND "contains the suffix context" AND
///    "contains the +1 numeric string" (to avoid false positives).
///    - Skip positions where both prefix and suffix contexts are empty (title is digits only).
/// 4. Also try the zero-padded form (e.g. `Part09` -> `Part10`).
/// 5. Returns the first matching entry if found, otherwise None.
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

    // Try +1 from the rightmost number (the Part number tends to be near the end).
    for pos in positions.iter().rev() {
        // start and raw.len() are byte counts, so this slice is safe on char boundaries.
        let prefix = &current_title[..pos.start];
        let suffix = &current_title[pos.start + pos.raw.len()..];

        // Slice up to 6 chars before and after, per char (to avoid UTF-8 byte-boundary panics).
        let prefix_chars: Vec<char> = prefix.chars().collect();
        let prefix_frag: String = prefix_chars[prefix_chars.len().saturating_sub(6)..]
            .iter()
            .collect();
        let suffix_frag: String = suffix.chars().take(6).collect();

        // Compare on whitespace-stripped forms (spacing wobble tolerance). The 6-char
        // slicing above stays char-based for UTF-8 safety; only the comparison drops spaces.
        let prefix_frag = strip_ws(&prefix_frag);
        let suffix_frag = strip_ws(&suffix_frag);

        // Skip if the title is digits only (no surrounding context).
        if prefix_frag.is_empty() && suffix_frag.is_empty() {
            continue;
        }

        // A candidate matches if its whitespace-stripped title satisfies the surrounding
        // context and contains the target digits. next_str / padded are pure digits.
        let find_matching = |needle: &str| {
            entries.iter().find(|e| {
                let cand = strip_ws(&e.title);
                (prefix_frag.is_empty() || cand.contains(&prefix_frag))
                    && (suffix_frag.is_empty() || cand.contains(&suffix_frag))
                    && cand.contains(needle)
            })
        };

        let next_str = (pos.num + 1).to_string();
        if let Some(found) = find_matching(&next_str) {
            return Some(found);
        }

        // Also try the zero-padded form (e.g. Part008 -> Part009).
        let padded = format!("{:0>width$}", next_str, width = pos.raw.len());
        if padded != next_str {
            if let Some(found) = find_matching(&padded) {
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
        // The numbers in "5ch総合 Part3" are 5 and 3. It should +1 the rightmost 3 and match Part4.
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
        let result = find_next_thread(
            "【ブルアカ】ブルーアーカイブ -Blue Archive- Part5843",
            &entries,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn does_not_false_match_when_title_starts_with_number() {
        let entries = vec![
            entry("1770000000", "無関係スレ 11", 100),
            entry("1770100000", "11スレ目", 50),
        ];
        // In "10スレ目" the number is at the start (empty prefix context).
        // It should match "11スレ目", which has the suffix context "スレ目".
        let result = find_next_thread("10スレ目", &entries);
        assert_eq!(result, Some(&entries[1]));
    }

    /// Production incident regression: the current thread has a space after 503
    /// ("★503 【転載禁止】") but the next thread does not ("★504【転載禁止】").
    /// Whitespace-insensitive context matching must still find ★504.
    #[test]
    fn finds_next_thread_across_space_wobble() {
        let entries = vec![
            entry("1770000000", "なんJBLAC(ブルアカ)部★503 【転載禁止】", 1002),
            entry("1770100000", "なんJBLAC(ブルアカ)部★504【転載禁止】", 34),
        ];
        let result = find_next_thread("なんJBLAC(ブルアカ)部★503 【転載禁止】", &entries);
        assert_eq!(result, Some(&entries[1]));
    }

    /// Full-width space (U+3000) wobble must also be tolerated.
    #[test]
    fn finds_next_thread_across_fullwidth_space_wobble() {
        let entries = vec![
            entry(
                "1770000000",
                "なんJBLAC(ブルアカ)部★503　【転載禁止】",
                1002,
            ),
            entry("1770100000", "なんJBLAC(ブルアカ)部★504【転載禁止】", 34),
        ];
        let result = find_next_thread("なんJBLAC(ブルアカ)部★503　【転載禁止】", &entries);
        assert_eq!(result, Some(&entries[1]));
    }

    /// Negative case: an unrelated thread whose number happens to be +1 must not match,
    /// because the surrounding context differs entirely.
    #[test]
    fn does_not_false_match_unrelated_plus_one() {
        let entries = vec![entry("1770100000", "全く別の板の雑談スレ★504", 5)];
        let result = find_next_thread("なんJBLAC(ブルアカ)部★503 【転載禁止】", &entries);
        assert_eq!(result, None);
    }

    #[test]
    fn handles_single_digit_increment() {
        let entries = vec![entry("1770100000", "テストスレ Part10", 5)];
        let result = find_next_thread("テストスレ Part9", &entries);
        assert_eq!(result, Some(&entries[0]));
    }
}
