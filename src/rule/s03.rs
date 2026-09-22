use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Structure, 3);
const HINT: &str = "前置きを消して主張を書く";
const PHRASES: &str = "phrases";

/// 説明の足場語。
pub struct Scaffolding;

impl SentenceRule for Scaffolding {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "S03"
    }

    /// 語リストにある足場語が現れた箇所。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        surface::findings(ID, sentence, context.list(ID).words(PHRASES), HINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&Scaffolding, text)
    }

    #[test]
    fn a_phrase_of_the_list_is_a_finding() {
        assert_eq!(excerpts("つまり、私が設計した。"), ["つまり、"]);
        assert_eq!(excerpts("重要なのは順序だ。"), ["重要なのは"]);
        assert_eq!(excerpts("結論から言うと通った。"), ["結論から言うと"]);
        assert_eq!(excerpts("これは設計に他ならない。"), ["に他ならない"]);
    }

    #[test]
    fn a_sentence_without_the_phrases_is_not_a_finding() {
        assert!(excerpts("私が設計した。").is_empty());
        assert!(excerpts("要点を先に書く。").is_empty());
    }

    #[test]
    fn a_phrase_without_its_comma_is_not_a_finding() {
        assert!(excerpts("つまり何を直すか。").is_empty());
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("まとめると、つまり、総じて短い。"),
            ["まとめると", "つまり、", "総じて"]
        );
    }
}
