use crate::rule::predicate::{is_aux_verb, is_case_particle, is_particle};
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Structure, 5);
const HINT: &str = "否定と肯定の対句は要所に限る";

/// 表層でだけ取れる対句の句。
const PHRASES: [&str; 3] = ["だけでなく", "だけではなく", "のみならず"];

/// 後ろに肯定が続く「ない」の活用形。
const CONJUNCTIVE: &str = "連用形-一般";

/// 否定と肯定の対句。
pub struct NegativeContrast;

impl SentenceRule for NegativeContrast {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "S05"
    }

    /// 「で」に「は」と連用形の「ない」が続く箇所と、対句を導く句が現れた箇所。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let tokens = sentence.tokens();
        let mut ranges = surface::matches(sentence.text(), PHRASES);
        ranges.extend(
            tokens
                .windows(3)
                .enumerate()
                .filter(|(index, window)| {
                    let previous = index.checked_sub(1).map(|previous| &tokens[previous]);
                    is_de(&window[0], previous)
                        && is_particle(&window[1], "は")
                        && is_nai(&window[2])
                })
                .map(|(_, window)| window[0].byte_range.start..window[2].byte_range.end),
        );
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 断定の助動詞の連用形か、名詞句に続く格助詞の「で」。
fn is_de(token: &Token, previous: Option<&Token>) -> bool {
    (is_aux_verb(token, "だ") && token.surface == "で")
        || (is_case_particle(token, "で") && previous.is_some_and(is_nominal))
}

/// 名詞句の末尾になる品詞か。
/// 名詞句の末尾に立てる Token。閉じ括弧は括られた語の末尾として受ける。
fn is_nominal(token: &Token) -> bool {
    matches!(token.pos.pos1, Pos1::Noun | Pos1::Pronoun | Pos1::Suffix)
        || (token.pos.pos1 == Pos1::SupplementarySymbol && token.pos.pos2 == "括弧閉")
}

/// 後ろに肯定が続く形の「ない」。
fn is_nai(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Adjective
        && matches!(token.lemma.as_str(), "ない" | "無い")
        && token.cform.as_deref() == Some(CONJUNCTIVE)
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
        assert_eq!(excerpts("規則ではなく慣習に従う。"), ["ではなく"]);
        assert_eq!(excerpts("ここではなく向こうに置く。"), ["ではなく"]);
    }

    #[test]
    fn a_quoted_word_before_the_denial_is_a_finding() {
        assert_eq!(excerpts("「冒険」ではなく日常を描く。"), ["ではなく"]);
        assert!(excerpts("それは「冒険」ではない。").is_empty());
    }

    #[test]
    fn a_denial_that_ends_the_predicate_is_not_a_finding() {
        assert!(excerpts("設計は自明ではない。").is_empty());
        assert!(excerpts("それは規則ではなかった。").is_empty());
    }

    #[test]
    fn a_denial_without_a_noun_phrase_before_it_is_not_a_finding() {
        assert!(excerpts("「ではなく」の対句を数える。").is_empty());
    }

    #[test]
    fn a_phrase_of_the_pair_is_a_finding() {
        assert_eq!(excerpts("設定だけでなく引数も見る。"), ["だけでなく"]);
        assert_eq!(excerpts("設定のみならず引数も見る。"), ["のみならず"]);
        assert_eq!(excerpts("鉄だけではなく銅も使う。"), ["だけではなく"]);
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
