use std::ops::Range;

use crate::rule::{Finding, RuleId};
use crate::sentence::{BOLD, Sentence};

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

/// 太字で囲んだ範囲。記法の `**` を含み、始まりの順に並ぶ。
pub fn bold(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find(BOLD) {
        let open = from + offset;
        let Some(offset) = text[open + BOLD.len()..].find(BOLD) else {
            break;
        };
        from = open + BOLD.len() + offset + BOLD.len();
        ranges.push(open..from);
    }
    ranges
}

/// 太字の内側。
pub fn inside_bold<'a>(text: &'a str, bold: &Range<usize>) -> &'a str {
    &text[bold.start + BOLD.len()..bold.end - BOLD.len()]
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
                sentence.text()[range].to_string(),
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
    fn a_bold_range_spans_its_markers() {
        let text = "**一つ**と**二つ**。";
        let ranges = bold(text);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&text[ranges[0].clone()], "**一つ**");
        assert_eq!(inside_bold(text, &ranges[1]), "二つ");
    }

    #[test]
    fn a_marker_without_its_pair_is_not_a_bold_range() {
        assert_eq!(bold("**閉じない。"), []);
        assert_eq!(bold("太字は無い。"), []);
    }
}
