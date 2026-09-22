use crate::measure;
use crate::rule::{Context, DocumentRule, Finding, Layer, RuleId};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Register, 3);
const HINT: &str = "読み手ごとに敬体か常体を決めて揃える";

/// 混在として数える割合の下限。
const MINIMUM: f64 = 0.2;

/// 敬体と常体の混在。
pub struct MixedRegister;

impl DocumentRule for MixedRegister {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "R03"
    }

    /// 敬体率と常体率がともに `MINIMUM` 以上である文書。抜粋は両方の割合。
    fn check(&self, sentences: &[Sentence], _context: &Context) -> Vec<Finding> {
        let polite = measure::polite_ratio(sentences).unwrap_or_default();
        let plain = measure::plain_ratio(sentences).unwrap_or_default();
        let Some(first) = sentences
            .first()
            .filter(|_| polite >= MINIMUM && plain >= MINIMUM)
        else {
            return Vec::new();
        };
        vec![Finding::new(
            ID,
            first.segment().origin.clone(),
            format!("敬体 {polite:.2} 常体 {plain:.2}"),
            HINT,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::document_excerpts(&MixedRegister, text)
    }

    /// 敬体の文を `polite` 文、常体の文を `plain` 文並べた文書。
    fn document(polite: usize, plain: usize) -> String {
        format!(
            "{}{}",
            "規則を数えます。".repeat(polite),
            "規則を数える。".repeat(plain)
        )
    }

    #[test]
    fn a_document_of_both_registers_is_a_finding() {
        assert_eq!(excerpts(&document(5, 5)), ["敬体 0.50 常体 0.50"]);
        assert_eq!(excerpts(&document(8, 2)), ["敬体 0.80 常体 0.20"]);
        assert_eq!(excerpts(&document(2, 8)), ["敬体 0.20 常体 0.80"]);
    }

    #[test]
    fn a_document_of_one_register_is_not_a_finding() {
        assert!(excerpts(&document(10, 0)).is_empty());
        assert!(excerpts(&document(0, 10)).is_empty());
    }

    #[test]
    fn a_register_below_the_minimum_is_not_a_mixture() {
        assert!(excerpts(&document(9, 1)).is_empty());
        assert!(excerpts(&document(1, 9)).is_empty());
    }

    #[test]
    fn a_document_without_a_full_stop_is_not_a_finding() {
        assert!(excerpts("見出しです\n見出しだ\n").is_empty());
    }

    #[test]
    fn a_procedure_of_requests_is_not_a_mixture() {
        let text = format!(
            "{}{}",
            "設定を確認してください。".repeat(7),
            "設定を保存します。".repeat(3)
        );
        assert!(excerpts(&text).is_empty(), "{text}");
    }

    #[test]
    fn a_polite_document_of_past_and_negative_forms_is_not_a_mixture() {
        assert!(
            excerpts(
                "規則を数えました。規則は残りません。規則を並べましょう。規則を数えます。規則は減りませんでした。"
            )
            .is_empty()
        );
    }
}
