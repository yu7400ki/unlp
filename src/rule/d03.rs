use crate::rule::predicate::{is_aux_verb, is_case_particle, is_particle, is_verb};
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;
use crate::token::Token;

const ID: RuleId = RuleId::new(Layer::Density, 3);
const HINT: &str = "根拠が本文にあるなら言い切る。不確実なら何が不確実かを書く";

/// 推量を表す活用形。
const PRESUMPTIVE: &str = "意志推量形";

/// 文の Token 列の位置から、そこに始まる緩和の並びの末尾の位置を返す照合。
type Hedge = fn(&[Token], usize) -> Option<usize>;

const HEDGES: [Hedge; 6] = [maybe, presumed, thought, possible, could_say, supposed];

/// 無根拠の緩和と譲歩。
pub struct UnfoundedHedge;

impl SentenceRule for UnfoundedHedge {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "D03"
    }

    /// 敬体の文書で、緩和の並びが現れた箇所。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        if !context.is_polite() {
            return Vec::new();
        }
        let tokens = sentence.tokens();
        let ranges = (0..tokens.len())
            .filter_map(|index| {
                let end = HEDGES.iter().find_map(|hedge| hedge(tokens, index))?;
                Some(tokens[index].byte_range.start..tokens[end].byte_range.end)
            })
            .collect();
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 「かもしれない」の末尾の位置。
fn maybe(tokens: &[Token], index: usize) -> Option<usize> {
    let end = index + 2;
    let window = tokens.get(index..=end)?;
    (is_particle(&window[0], "か")
        && is_particle(&window[1], "も")
        && is_verb(&window[2], "しれる"))
    .then_some(end)
}

/// 「だろう」の位置。
fn presumed(tokens: &[Token], index: usize) -> Option<usize> {
    let token = tokens.get(index)?;
    (is_aux_verb(token, "だ") && is_presumptive(token)).then_some(index)
}

/// 「と思われる」「と考えられる」の末尾の位置。
fn thought(tokens: &[Token], index: usize) -> Option<usize> {
    let end = index + 2;
    let window = tokens.get(index..=end)?;
    (is_case_particle(&window[0], "と")
        && (is_verb(&window[1], "思う") || is_verb(&window[1], "考える"))
        && (is_aux_verb(&window[2], "れる") || is_aux_verb(&window[2], "られる")))
    .then_some(end)
}

/// 「可能性がある」の末尾の位置。
fn possible(tokens: &[Token], index: usize) -> Option<usize> {
    let end = index + 3;
    let window = tokens.get(index..=end)?;
    (window[0].surface == "可能"
        && window[1].surface == "性"
        && is_case_particle(&window[2], "が")
        && (is_verb(&window[3], "ある") || is_verb(&window[3], "有る")))
    .then_some(end)
}

/// 「と言えるでしょう」の末尾の位置。
fn could_say(tokens: &[Token], index: usize) -> Option<usize> {
    let end = index + 2;
    let window = tokens.get(index..=end)?;
    (is_case_particle(&window[0], "と")
        && is_verb(&window[1], "言える")
        && is_aux_verb(&window[2], "です")
        && is_presumptive(&window[2]))
    .then_some(end)
}

/// 「はずです」の末尾の位置。
fn supposed(tokens: &[Token], index: usize) -> Option<usize> {
    let end = index + 1;
    let window = tokens.get(index..=end)?;
    (window[0].lemma == "はず" && is_aux_verb(&window[1], "です")).then_some(end)
}

fn is_presumptive(token: &Token) -> bool {
    token.cform.as_deref() == Some(PRESUMPTIVE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::polite_excerpts(&UnfoundedHedge, text)
    }

    #[test]
    fn a_hedge_is_a_finding_in_any_inflection() {
        assert_eq!(excerpts("壊れるかもしれません。"), ["かもしれ"]);
        assert_eq!(excerpts("壊れるかもしれない。"), ["かもしれ"]);
        assert_eq!(excerpts("壊れるだろう。"), ["だろう"]);
        assert_eq!(excerpts("壊れると思われます。"), ["と思われ"]);
        assert_eq!(excerpts("壊れると考えられました。"), ["と考えられ"]);
        assert_eq!(excerpts("壊れる可能性があります。"), ["可能性があり"]);
        assert_eq!(excerpts("壊れる可能性がある。"), ["可能性がある"]);
        assert_eq!(excerpts("壊れると言えるでしょう。"), ["と言えるでしょう"]);
        assert_eq!(excerpts("直るはずです。"), ["はずです"]);
    }

    #[test]
    fn an_assertion_is_not_a_finding() {
        assert!(excerpts("壊れます。").is_empty());
        assert!(excerpts("壊れると分かりました。").is_empty());
        assert!(excerpts("壊れると言えます。").is_empty());
        assert!(excerpts("その可能性を認めます。").is_empty());
        assert!(excerpts("可能性は低いです。").is_empty());
        assert!(excerpts("直る手はずを整えます。").is_empty());
    }

    #[test]
    fn a_plain_document_is_outside_the_rule() {
        assert!(harness::excerpts(&UnfoundedHedge, "壊れるかもしれない。").is_empty());
        assert!(
            harness::excerpts(
                &UnfoundedHedge,
                "規則を数えます。壊れるかもしれない。壊れるかもしれない。"
            )
            .is_empty()
        );
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("壊れる可能性があり、直らないかもしれません。"),
            ["可能性があり", "かもしれ"]
        );
    }
}
