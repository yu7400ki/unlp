use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::{self, Sentence};

const ID: RuleId = RuleId::new(Layer::Formulaic, 3);
const HINT: &str = "本文の太字は外す。強調は語の選択と文の位置で行う";

/// 本文の太字。
pub struct BoldInProse;

impl SentenceRule for BoldInProse {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "F03"
    }

    /// 太字で囲んだ箇所。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let ranges = sentence::bold(sentence.text());
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&BoldInProse, text)
    }

    #[test]
    fn a_bold_range_is_a_finding() {
        assert_eq!(excerpts("本文の**強調**だ。"), ["**強調**"]);
        assert_eq!(
            excerpts("**一つ**と**二つ**を挙げる。"),
            ["**一つ**", "**二つ**"]
        );
    }

    #[test]
    fn the_bold_of_a_conclusion_is_also_counted() {
        assert_eq!(
            excerpts("**意図したものです。**エラーが出る。"),
            ["**意図したものです。**"]
        );
    }

    #[test]
    fn a_sentence_without_bold_is_not_a_finding() {
        assert!(excerpts("強調を外した文だ。").is_empty());
        assert!(excerpts("**閉じない文だ。").is_empty());
    }

    #[test]
    fn a_marker_in_a_code_span_is_not_a_finding() {
        assert!(excerpts("`/**` と `/**` の扱いを決める。").is_empty());
        assert!(excerpts("計算は `2**8` と `2**16` で行う。").is_empty());
    }
}
