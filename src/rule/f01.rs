use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Formulaic, 1);
const HINT: &str = "毎回付けない。相手が求めていない譲歩と申し出は削る";
const PHRASES: &str = "phrases";

/// 回答の締めの定型。
pub struct ClosingFormula;

impl SentenceRule for ClosingFormula {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "F01"
    }

    /// 語リストにある締めの句が現れた箇所。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        surface::findings(ID, sentence, context.list(ID).words(PHRASES), HINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&ClosingFormula, text)
    }

    #[test]
    fn a_phrase_of_the_list_is_a_finding() {
        assert_eq!(excerpts("ご指示ください。"), ["ご指示ください"]);
        assert_eq!(excerpts("ご指摘の通り直した。"), ["ご指摘の通り"]);
        assert_eq!(excerpts("お役に立てば幸いだ。"), ["お役に立て"]);
        assert_eq!(excerpts("いかがでしょうか。"), ["いかがでしょうか"]);
    }

    #[test]
    fn a_sentence_without_the_phrases_is_not_a_finding() {
        assert!(excerpts("指示の通り直した。").is_empty());
        assert!(excerpts("次は重みを決める。").is_empty());
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("修正が必要であればご相談ください。"),
            ["修正が必要であれば", "ご相談ください"]
        );
    }
}
