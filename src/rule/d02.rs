use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Density, 2);
const HINT: &str = "断片は読み手に文脈の復元を強いる。主題を補うか前後の文に繋げる";

/// 断片として数える日本語の文字数の上限。
const JA_CHARS: usize = 7;

/// 述語を持たない断片文。
pub struct PredicatelessFragment;

impl SentenceRule for PredicatelessFragment {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "D02"
    }

    /// 句点で終わり、`JA_CHARS` 字までの日本語で、述語になる Token を 1 つも持たない文。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        if !sentence.is_terminated() || sentence.ja_chars() > JA_CHARS || sentence.has_predicate() {
            return Vec::new();
        }
        let whole = 0..sentence.text().len();
        surface::findings_at(ID, sentence, vec![whole], HINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&PredicatelessFragment, text)
    }

    #[test]
    fn a_sentence_without_a_predicate_is_a_finding() {
        assert_eq!(excerpts("名詞の列挙。"), ["名詞の列挙。"]);
        assert_eq!(excerpts("これらは断片。"), ["これらは断片。"]);
        assert_eq!(excerpts("断片！"), ["断片！"]);
        assert_eq!(excerpts("空か。"), ["空か。"]);
        assert_eq!(excerpts("出どころ。"), ["出どころ。"]);
    }

    #[test]
    fn a_fragment_beyond_the_length_is_not_a_finding() {
        assert!(excerpts("9月10日に日本を出発。").is_empty());
        assert!(excerpts("1961年にノーベル賞を受賞。").is_empty());
    }

    #[test]
    fn a_predicate_keeps_the_sentence_out_of_the_findings() {
        assert!(excerpts("規則を数える。").is_empty());
        assert!(excerpts("断片は短い。").is_empty());
        assert!(excerpts("とても静かだ。").is_empty());
        assert!(excerpts("これは断片です。").is_empty());
    }

    #[test]
    fn an_auxiliary_outside_the_copula_is_not_a_predicate() {
        assert_eq!(excerpts("規則らしい断片。"), ["規則らしい断片。"]);
    }

    #[test]
    fn a_sentence_without_a_full_stop_is_not_a_finding() {
        assert!(excerpts("名詞の列挙").is_empty());
    }

    #[test]
    fn the_length_counts_only_the_japanese_chars() {
        assert_eq!(excerpts("JSON パーサー。"), ["JSON パーサー。"]);
    }
}
