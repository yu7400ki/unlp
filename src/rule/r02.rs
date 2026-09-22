use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Register, 2);
const HINT: &str = "抽象語に温度を下げる: 変更しない、反応、コスト";
const PHRASES: &str = "phrases";

/// 身体的な比喩。
pub struct PhysicalMetaphor;

impl SentenceRule for PhysicalMetaphor {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "R02"
    }

    /// 語リストにある比喩が現れた箇所。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        surface::findings(ID, sentence, context.list(ID).words(PHRASES), HINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&PhysicalMetaphor, text)
    }

    #[test]
    fn a_phrase_of_the_list_is_a_finding() {
        assert_eq!(excerpts("元のファイルは無傷です。"), ["無傷"]);
        assert_eq!(excerpts("ホバーの手応えを整える。"), ["手応え"]);
        assert_eq!(excerpts("初回だけ代を払う。"), ["代を払"]);
        assert_eq!(excerpts("後で仕様に噛みつく。"), ["噛み"]);
        assert_eq!(excerpts("静かな失敗が続く。"), ["静かな失敗"]);
        assert_eq!(excerpts("優雅に終了する。"), ["優雅に"]);
        assert_eq!(excerpts("これは魔法ではない。"), ["魔法"]);
    }

    #[test]
    fn an_ordinary_sentence_is_not_a_finding() {
        assert!(excerpts("鍵を回す。").is_empty());
        assert!(excerpts("窓を開ける。").is_empty());
        assert!(excerpts("電話を掛ける。").is_empty());
        assert!(excerpts("静かに歩く。").is_empty());
        assert!(excerpts("失敗を返す。").is_empty());
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("無傷のまま優雅に返す魔法だ。"),
            ["無傷", "優雅に", "魔法"]
        );
    }
}
