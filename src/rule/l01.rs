use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, lemma, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Lexical, 1);
const HINT: &str = "その分野で通っている語を使う: メニュー、メッセージ、ウィンドウ、キー、フラグ、ビルドする、キャッシュ、コールドスタート";
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
        ranges.extend(lemma::verbs(sentence.tokens(), list, VERBS));
        surface::findings_at(ID, sentence, ranges, HINT)
    }
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
        assert_eq!(excerpts("値が据わっていない。"), ["据わっ"]);
        assert_eq!(excerpts("結果を濾して返す。"), ["濾し"]);
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
    fn overlapping_phrases_are_one_finding() {
        assert_eq!(excerpts("地とホバーの手応えを分ける。"), ["地とホバー"]);
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("電文を掃いてから品書きに戻す。"),
            ["電文", "掃い", "品書き"]
        );
    }
}
