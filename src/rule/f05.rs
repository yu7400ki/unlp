use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Formulaic, 5);
const HINT: &str = "中身のない強調は削る。程度は数で書く";
const WORDS: &str = "words";

/// 空虚な強調。
pub struct EmptyEmphasis;

impl SentenceRule for EmptyEmphasis {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "F05"
    }

    /// 語リストにある強調の語が現れた箇所。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        surface::findings(ID, sentence, context.list(ID).words(WORDS), HINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&EmptyEmphasis, text)
    }

    #[test]
    fn a_word_of_the_list_is_a_finding() {
        assert_eq!(excerpts("非常に短い。"), ["非常に"]);
        assert_eq!(excerpts("順序が不可欠だ。"), ["不可欠"]);
        assert_eq!(excerpts("根本的な誤りだ。"), ["根本的な"]);
    }

    #[test]
    fn a_sentence_without_the_words_is_not_a_finding() {
        assert!(excerpts("3 割短い。").is_empty());
        assert!(excerpts("順序が要る。").is_empty());
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(excerpts("極めて包括的な規則だ。"), ["極めて", "包括的"]);
    }
}
