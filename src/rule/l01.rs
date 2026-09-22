use std::ops::Range;

use crate::rule::predicate::is_verb;
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, WordList, surface};
use crate::sentence::Sentence;
use crate::token::Token;

const ID: RuleId = RuleId::new(Layer::Lexical, 1);
const HINT: &str = "その分野で通っている語を使う: メニュー、メッセージ、ウィンドウ、キー、フラグ、フォーカスリング、ビルドする、高速パス、雛形、コールドスタート";
const PHRASES: &str = "phrases";
const VERBS: &str = "verbs";

/// 慣習語の和語化。
pub struct NativizedTerm;

impl SentenceRule for NativizedTerm {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "L01"
    }

    /// 語リストにある句が現れた箇所と、原形が語リストにある動詞。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        let list = context.list(ID);
        let mut ranges = surface::matches(sentence.text(), list.words(PHRASES));
        ranges.extend(verbs(sentence.tokens(), list));
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 原形が語リストにある動詞の範囲。活用形は原形で照合する。
fn verbs(tokens: &[Token], list: &WordList) -> Vec<Range<usize>> {
    tokens
        .iter()
        .filter(|token| list.words(VERBS).any(|lemma| is_verb(token, lemma)))
        .map(|token| token.byte_range.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&NativizedTerm, text)
    }

    #[test]
    fn a_phrase_of_the_list_is_a_finding() {
        assert_eq!(excerpts("品書きに項目を足す。"), ["品書き"]);
        assert_eq!(excerpts("設定の窓で旗を立てる。"), ["設定の窓", "旗を立て"]);
        assert_eq!(
            excerpts("昇格していない窓で回し直してください。"),
            ["昇格していない窓"]
        );
        assert_eq!(excerpts("ソースから建てた。"), ["ソースから建て"]);
        assert_eq!(excerpts("これがこちらのものかは照らせない。"), ["照らせ"]);
    }

    #[test]
    fn a_verb_of_the_list_is_a_finding_in_any_inflection() {
        assert_eq!(excerpts("古いログを掃く。"), ["掃く"]);
        assert_eq!(excerpts("古いログを掃いた。"), ["掃い"]);
        assert_eq!(excerpts("結果を濾して返す。"), ["濾し"]);
        assert_eq!(excerpts("候補を濾す。"), ["濾す"]);
    }

    #[test]
    fn an_ordinary_sentence_is_not_a_finding() {
        assert!(excerpts("鍵を回す。").is_empty());
        assert!(excerpts("窓を開ける。").is_empty());
        assert!(excerpts("電話を掛ける。").is_empty());
        assert!(excerpts("マンションが次々と建てられる。").is_empty());
        assert!(excerpts("仕様に照らして確認する。").is_empty());
    }

    #[test]
    fn a_metaphor_of_the_register_layer_is_not_a_finding() {
        assert_eq!(excerpts("地とホバーの手応えを分ける。"), ["地とホバー"]);
        assert!(excerpts("初回だけ代を払う。").is_empty());
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("焦点の輪を掃いてから品書きに戻す。"),
            ["焦点の輪", "掃い", "品書き"]
        );
    }
}
