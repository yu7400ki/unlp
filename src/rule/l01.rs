use std::ops::Range;

use crate::rule::predicate::{is_noun, is_verb};
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, WordList, surface};
use crate::sentence::Sentence;
use crate::token::Token;

const ID: RuleId = RuleId::new(Layer::Lexical, 1);
const HINT: &str = "その分野で通っている語を使う: メニュー、メッセージ、ウィンドウ、キー、フラグ、フォーカスリング、ビルドする、高速パス、雛形、コールドスタート";
const PHRASES: &str = "phrases";
const VERBS: &str = "verbs";
const NOUNS: &str = "nouns";
const IDIOMS: &str = "idioms";

/// 慣習語の和語化。
pub struct NativizedTerm;

impl SentenceRule for NativizedTerm {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "L01"
    }

    /// 語リストにある句が現れた箇所と、原形が語リストにある動詞と名詞。語リストの慣用句と
    /// 重なる動詞と名詞は除く。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        let list = context.list(ID);
        let mut ranges = surface::matches(sentence.text(), list.words(PHRASES));
        ranges.extend(surface::outside(
            sentence.text(),
            lemmas(sentence.tokens(), list),
            list.words(IDIOMS),
        ));
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 原形が語リストにある動詞と名詞の範囲。活用形は原形で照合する。
fn lemmas(tokens: &[Token], list: &WordList) -> Vec<Range<usize>> {
    tokens
        .iter()
        .filter(|token| {
            list.words(VERBS).any(|lemma| is_verb(token, lemma))
                || list.words(NOUNS).any(|lemma| is_noun(token, lemma))
        })
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
        assert_eq!(excerpts("品書きに項目を追加する。"), ["品書き"]);
        assert_eq!(excerpts("ソースから建てた。"), ["ソースから建て"]);
        assert_eq!(excerpts("これがこちらのものかは照らせない。"), ["照らせ"]);
        assert_eq!(excerpts("事前検証を掛けてから返す。"), ["検証を掛"]);
        assert_eq!(excerpts("綴りの検査を掛ける。"), ["検査を掛"]);
        assert_eq!(excerpts("この規則を掛ける相手を選ぶ。"), ["規則を掛"]);
    }

    #[test]
    fn a_verb_of_the_list_is_a_finding_in_any_inflection() {
        assert_eq!(excerpts("古いログを掃く。"), ["掃く"]);
        assert_eq!(excerpts("古いログを掃いた。"), ["掃い"]);
        assert_eq!(excerpts("結果を濾して返す。"), ["濾し"]);
        assert_eq!(excerpts("候補を濾す。"), ["濾す"]);
    }

    #[test]
    fn a_noun_of_the_list_is_a_finding() {
        assert_eq!(excerpts("篩に掛けて素通しにする。"), ["篩", "素通し"]);
        assert_eq!(
            excerpts("持ち越しの段取りを決める。"),
            ["持ち越し", "段取り"]
        );
        assert_eq!(excerpts("設定の窓で旗を立てる。"), ["窓", "旗"]);
        assert_eq!(excerpts("同じ錠の下で輪を回す。"), ["錠", "輪"]);
        assert_eq!(
            excerpts("辞書を持つ器を入れ物の外に置く。"),
            ["器", "入れ物"]
        );
        assert_eq!(excerpts("窓の札に名前を出す。"), ["窓", "札"]);
        assert_eq!(
            excerpts("自動の覆いの有無と重ねの決めを束ねで書く。"),
            ["覆い", "重ね", "決め", "束ね"]
        );
    }

    #[test]
    fn a_verb_of_the_list_is_a_finding_as_it_is_written() {
        assert_eq!(excerpts("時間を測って結果を寄せる。"), ["測っ", "寄せる"]);
        assert_eq!(excerpts("失敗は黙って畳む。"), ["黙っ", "畳む"]);
    }

    #[test]
    fn a_verb_inside_an_idiom_of_the_list_is_not_a_finding() {
        assert_eq!(excerpts("設定が効いている。"), ["効い"]);
        assert!(excerpts("型が効いている。").is_empty());
        assert_eq!(excerpts("末尾の空白を落とす。"), ["落とす"]);
        assert!(excerpts("仕様をコードに落とす。").is_empty());
        let text = "型が効いている。";
        let context = harness::context(text).without_word(ID, IDIOMS, "型が効");
        assert_eq!(
            harness::excerpts_with(&NativizedTerm, &context, text),
            ["効い"]
        );
    }

    #[test]
    fn an_ordinary_sentence_is_not_a_finding() {
        assert!(excerpts("鍵を回す。").is_empty());
        assert!(excerpts("窓口に問い合わせる。").is_empty());
        assert!(excerpts("設定を決めてから層を重ねる。").is_empty());
        assert!(excerpts("電話を掛ける。").is_empty());
        assert!(excerpts("フィルタを掛ける。").is_empty());
        assert!(excerpts("倍率を掛ける。").is_empty());
        assert!(excerpts("鍵を掛ける。").is_empty());
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
            ["輪", "掃い", "品書き"]
        );
    }
}
