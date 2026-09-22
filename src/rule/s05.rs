use crate::rule::predicate::{is_aux_verb, is_topic_particle};
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Structure, 5);
const HINT: &str = "否定と肯定の対句は要所に限る";

/// 表層でだけ取れる対句の句。
const PHRASES: [&str; 2] = ["だけでなく", "のみならず"];

/// 否定と肯定の対句。
pub struct NegativeContrast;

impl SentenceRule for NegativeContrast {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "S05"
    }

    /// 断定の「で」に係助詞と「ない」が続く箇所と、対句を導く句が現れた箇所。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let mut ranges = surface::matches(sentence.text(), PHRASES);
        ranges.extend(
            sentence
                .tokens()
                .windows(3)
                .filter(|window| {
                    is_copula(&window[0])
                        && is_topic_particle(&window[1], "は")
                        && is_nai(&window[2])
                })
                .map(|window| window[0].byte_range.start..window[2].byte_range.end),
        );
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

fn is_copula(token: &Token) -> bool {
    is_aux_verb(token, "だ") && token.surface == "で"
}

fn is_nai(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Adjective && matches!(token.lemma.as_str(), "ない" | "無い")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&NegativeContrast, text)
    }

    #[test]
    fn a_denial_before_an_assertion_is_a_finding() {
        assert_eq!(
            excerpts("静かに失敗するのではなく、明示的に落とす。"),
            ["ではなく"]
        );
        assert_eq!(excerpts("重要ではなく簡潔だ。"), ["ではなく"]);
        assert_eq!(excerpts("直すのでは無く消す。"), ["では無く"]);
    }

    #[test]
    fn a_phrase_of_the_pair_is_a_finding() {
        assert_eq!(excerpts("設定だけでなく引数も見る。"), ["だけでなく"]);
        assert_eq!(excerpts("設定のみならず引数も見る。"), ["のみならず"]);
    }

    #[test]
    fn a_denial_without_the_copula_is_not_a_finding() {
        assert!(excerpts("規則が無い。").is_empty());
        assert!(excerpts("そこには無い。").is_empty());
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("設定だけでなく、直すのではなく消す。"),
            ["だけでなく", "ではなく"]
        );
    }
}
