use std::ops::Range;

use crate::rule::{Finding, RuleId};
use crate::sentence::Sentence;

/// 抜粋に残す文字数。
const LIMIT: usize = 40;

/// 文の文字列に現れた句の範囲。始まりの順に並ぶ。
pub fn matches<'a>(text: &str, phrases: impl IntoIterator<Item = &'a str>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for phrase in phrases {
        if phrase.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(offset) = text[from..].find(phrase) {
            let start = from + offset;
            from = start + phrase.len();
            ranges.push(start..from);
        }
    }
    sorted(ranges)
}

/// 文の中の範囲の抜粋。`LIMIT` 文字を超える分は落とす。
fn excerpt(text: &str, range: Range<usize>) -> String {
    text[range].chars().take(LIMIT).collect()
}

/// 文の中の範囲を抜粋とする指摘。範囲の始まりの順に並ぶ。
pub fn findings_at(
    rule: RuleId,
    sentence: &Sentence,
    ranges: Vec<Range<usize>>,
    hint: &'static str,
) -> Vec<Finding> {
    sorted(ranges)
        .into_iter()
        .map(|range| {
            Finding::new(
                rule,
                sentence.segment().origin.clone(),
                excerpt(sentence.text(), range),
                hint,
            )
        })
        .collect()
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

fn sorted(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| (range.start, range.end));
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phrase_is_found_wherever_it_appears() {
        assert_eq!(matches("つまり、つまり、", ["つまり、"]), [0..12, 12..24]);
        assert_eq!(matches("要するに", ["つまり、"]), []);
    }

    #[test]
    fn the_ranges_follow_the_order_of_the_text() {
        assert_eq!(
            matches("総じて、非常に長い", ["非常に", "総じて"]),
            [0..9, 12..21]
        );
    }

    #[test]
    fn an_empty_phrase_matches_nothing() {
        assert_eq!(matches("文だ。", [""]), []);
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
