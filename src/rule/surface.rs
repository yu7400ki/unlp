use std::cmp::Reverse;
use std::ops::Range;

use crate::rule::{Finding, RuleId};
use crate::sentence::Sentence;

/// 抜粋に残す文字数。
const LIMIT: usize = 40;

/// 文の文字列に現れた句の範囲。始まりの順に並び、重なる一致は前の範囲だけを残す。
pub fn matches<'a>(text: &str, phrases: impl IntoIterator<Item = &'a str>) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    for phrase in phrases {
        if phrase.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(offset) = text[from..].find(phrase) {
            let start = from + offset;
            from = start + phrase.len();
            found.push(start..from);
        }
    }
    disjoint(found)
}

/// 文の中の範囲の抜粋。連続する空白を 1 つに畳み、`LIMIT` 文字を超える分は落とす。
fn excerpt(text: &str, range: Range<usize>) -> String {
    let mut excerpt = String::new();
    let mut count = 0;
    for c in text[range].chars() {
        if c == ' ' && excerpt.ends_with(' ') {
            continue;
        }
        excerpt.push(c);
        count += 1;
        if count == LIMIT {
            break;
        }
    }
    excerpt
}

/// 文の中の範囲を抜粋とする指摘。範囲の始まりの順に並び、重なる範囲は前の範囲だけを残す。
pub fn findings_at(
    rule: RuleId,
    sentence: &Sentence,
    ranges: Vec<Range<usize>>,
    hint: &'static str,
) -> Vec<Finding> {
    findings_around(rule, sentence, ranges, 0, hint)
}

/// 文の中の範囲を、前後 `around` 文字を添えた抜粋とする指摘。重なりの判定は添える前の範囲で
/// 行う。
pub fn findings_around(
    rule: RuleId,
    sentence: &Sentence,
    ranges: Vec<Range<usize>>,
    around: usize,
    hint: &'static str,
) -> Vec<Finding> {
    let text = sentence.text();
    disjoint(ranges)
        .into_iter()
        .map(|range| {
            Finding::new(
                rule,
                sentence.segment().origin.clone(),
                excerpt(text, with_surroundings(text, range, around)),
                hint,
            )
        })
        .collect()
}

/// 前後 `around` 文字を含めて広げた範囲。
fn with_surroundings(text: &str, range: Range<usize>, around: usize) -> Range<usize> {
    let start = text[..range.start]
        .char_indices()
        .rev()
        .take(around)
        .last()
        .map_or(range.start, |(index, _)| index);
    let end = text[range.end..]
        .char_indices()
        .nth(around)
        .map_or(text.len(), |(index, _)| range.end + index);
    start..end
}

/// 文の文字列に現れた句を抜粋とする指摘。
pub fn findings<'a>(
    rule: RuleId,
    sentence: &Sentence,
    phrases: impl IntoIterator<Item = &'a str>,
    hint: &'static str,
) -> Vec<Finding> {
    findings_at(rule, sentence, matches(sentence.text(), phrases), hint)
}

/// 始まりの順に並べ、重なる範囲は前の範囲だけを残す。同じ位置に始まる範囲は長い方を採る。
fn disjoint(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| (range.start, Reverse(range.end)));
    let mut kept: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if kept.last().is_none_or(|last| last.end <= range.start) {
            kept.push(range);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phrase_is_found_wherever_it_appears() {
        assert_eq!(matches("つまり、つまり、", ["つまり、"]), [0..12, 12..24]);
        assert!(matches("要するに", ["つまり、"]).is_empty());
    }

    #[test]
    fn the_ranges_follow_the_order_of_the_text() {
        assert_eq!(
            matches("総じて、非常に長い", ["非常に", "総じて"]),
            [0..9, 12..21]
        );
    }

    #[test]
    fn an_overlapping_phrase_is_not_a_second_range() {
        assert_eq!(
            matches(
                "直感的な意味論を導入する。窓を開ける。",
                ["直感的な意味論", "意味論を", "窓"]
            ),
            [0..21, 39..42]
        );
        assert_eq!(
            matches(
                "地とホバーの手応えを分ける。鍵を回す。",
                ["地とホバー", "ホバーの手応え", "鍵"]
            ),
            [0..15, 42..45]
        );
        assert_eq!(matches("窓と鍵", ["窓", "鍵"]), [0..3, 6..9]);
        assert_eq!(matches("窓と窓", ["窓"]), [0..3, 6..9]);
    }

    #[test]
    fn an_empty_phrase_matches_nothing() {
        assert!(matches("文だ。", [""]).is_empty());
    }

    #[test]
    fn an_excerpt_folds_a_run_of_blanks() {
        let text = "共通の引数:        、      、";
        assert_eq!(excerpt(text, 0..text.len()), "共通の引数: 、 、");
        assert_eq!(
            excerpt("規則を 数える。", 0.."規則を 数える。".len()),
            "規則を 数える。"
        );
    }

    #[test]
    fn an_excerpt_keeps_the_range_up_to_the_limit() {
        assert_eq!(excerpt("規則を数える。", 0..9), "規則を");
        let text = "あ".repeat(LIMIT + 1);
        let long = excerpt(&text, 0..text.len());
        assert_eq!(long.chars().count(), LIMIT);
        assert_eq!(long, "あ".repeat(LIMIT));
    }
}
