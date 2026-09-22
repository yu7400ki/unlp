use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Density, 2);
const HINT: &str = "断片は読み手に文脈の復元を強いる。主題を補うか前後の文に繋げる";

/// 抜粋に残す文字数。
const EXCERPT: usize = 40;

/// 述語を持たない断片文。
pub struct PredicatelessFragment;

impl SentenceRule for PredicatelessFragment {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "D02"
    }

    /// 句点で終わりながら、述語になる Token を 1 つも持たない文。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let text = sentence.text();
        if !text.ends_with(['。', '！', '？']) || sentence.tokens().iter().any(is_predicate) {
            return Vec::new();
        }
        vec![Finding::new(
            ID,
            sentence.segment().origin.clone(),
            text.chars().take(EXCERPT).collect(),
            HINT,
        )]
    }
}

fn is_predicate(token: &Token) -> bool {
    match token.pos.pos1 {
        Pos1::Verb | Pos1::Adjective | Pos1::AdjectivalNoun => true,
        Pos1::AuxVerb => matches!(token.lemma.as_str(), "だ" | "です"),
        _ => false,
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
    fn the_excerpt_stops_at_forty_chars() {
        let excerpts = excerpts(&format!("{}。", "名詞の羅列".repeat(10)));
        assert_eq!(excerpts.len(), 1);
        assert_eq!(excerpts[0].chars().count(), EXCERPT);
    }
}
