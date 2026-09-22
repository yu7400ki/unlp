use crate::sentence::Sentence;

/// 連打として数える文の数の下限。
const MINIMUM: usize = 3;

/// 条件を満たす文が続く範囲。`MINIMUM` 文以上続くものだけを、始まりの順に返す。
pub fn runs<'a, 's>(
    sentences: &'a [Sentence<'s>],
    matches: impl Fn(&Sentence) -> bool,
) -> Vec<&'a [Sentence<'s>]> {
    sentences
        .chunk_by(|left, right| matches(left) == matches(right))
        .filter(|run| run.len() >= MINIMUM && matches(&run[0]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    /// 日本語の文字数が 3 字までの文が続く範囲の、先頭の文。
    fn short_runs(text: &str) -> Vec<Vec<String>> {
        harness::with_sentences(text, |sentences| {
            runs(sentences, |sentence| sentence.ja_chars() <= 3)
                .iter()
                .map(|run| run.iter().map(|s| s.text().to_string()).collect())
                .collect()
        })
    }

    #[test]
    fn a_run_shorter_than_the_minimum_is_not_a_run() {
        assert!(short_runs("短い。短い。").is_empty());
        assert!(short_runs("十分に長い文である。").is_empty());
    }

    #[test]
    fn a_run_spans_the_sentences_that_match() {
        assert_eq!(
            short_runs("短い。短い。短い。十分に長い文である。"),
            [["短い。", "短い。", "短い。"]]
        );
    }

    #[test]
    fn a_sentence_that_does_not_match_ends_the_run() {
        assert_eq!(
            short_runs("短い。短い。短い。十分に長い文である。短い。短い。短い。短い。"),
            [
                vec!["短い。", "短い。", "短い。"],
                vec!["短い。", "短い。", "短い。", "短い。"],
            ]
        );
    }
}
