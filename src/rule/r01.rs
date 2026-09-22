use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Register, 1);
const HINT: &str = "報告書や仕様では中立語に寄せる（「〜のような」「〜する」）";
const PHRASES: &str = "phrases";

/// 敬体の文書に混じる口語。
pub struct Colloquialism;

impl SentenceRule for Colloquialism {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "R01"
    }

    /// 敬体の文書で、語リストにある口語が現れた箇所。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        if !context.is_polite() {
            return Vec::new();
        }
        surface::findings(ID, sentence, context.list(ID).words(PHRASES), HINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts_with(
            &Colloquialism,
            &harness::CONTEXT.with_polite_ratio(1.0),
            text,
        )
    }

    #[test]
    fn a_phrase_of_the_list_is_a_finding() {
        assert_eq!(excerpts("ちょっと直します。"), ["ちょっと"]);
        assert_eq!(excerpts("そのやつを消します。"), ["やつ"]);
        assert_eq!(excerpts("すごく速くなります。"), ["すごく"]);
        assert_eq!(excerpts("めっちゃ速くなります。"), ["めっちゃ"]);
        assert_eq!(excerpts("「静か」的な語を選びます。"), ["」的な"]);
        assert_eq!(excerpts("「静か」的に振る舞います。"), ["」的に"]);
        assert_eq!(excerpts("規則みたいな形です。"), ["みたいな"]);
        assert_eq!(excerpts("消してしまいます。"), ["てしまいます"]);
        assert_eq!(excerpts("消してしまいました。"), ["てしまいました"]);
        assert_eq!(excerpts("消しちゃう。"), ["ちゃう"]);
        assert_eq!(excerpts("消しちゃった。"), ["ちゃった"]);
    }

    #[test]
    fn a_neutral_sentence_is_not_a_finding() {
        assert!(excerpts("規則を数えます。").is_empty());
        assert!(excerpts("静かな語を選びます。").is_empty());
        assert!(excerpts("消去します。").is_empty());
    }

    #[test]
    fn a_plain_document_is_outside_the_rule() {
        let context = harness::CONTEXT.with_polite_ratio(0.49);
        assert!(harness::excerpts_with(&Colloquialism, &context, "ちょっと直す。").is_empty());
        assert!(harness::excerpts(&Colloquialism, "ちょっと直す。").is_empty());
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("ちょっとしたやつを消してしまいました。"),
            ["ちょっと", "やつ", "てしまいました"]
        );
    }
}
