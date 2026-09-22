use crate::rule::{Context, DocumentRule, Finding, Layer, RuleId};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Goshu, 1);
const HINT: &str = "文末の述語は分野の慣習語にする（動作する、追加する、比較する）";

/// 指摘とする和語率のしきい値。
const THRESHOLD: f64 = 0.70;

/// 割合を判定する文末の述語の数の下限。
const MINIMUM: usize = 5;

/// 抜粋に列挙する語の数。
const WORDS: usize = 10;

/// 文末の述語の和語率。
pub struct FinalWagoRatio;

impl DocumentRule for FinalWagoRatio {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "G01"
    }

    /// 文末の述語が `MINIMUM` 個以上あり、和語率が `THRESHOLD` を超える文書。抜粋は
    /// 文末の和語の動詞を原形と件数で頻度順に列挙したもの。
    fn check(&self, sentences: &[Sentence], context: &Context) -> Vec<Finding> {
        let predicates = context.final_predicates();
        let exceeds = predicates.total() >= MINIMUM
            && predicates
                .wago_ratio()
                .is_some_and(|ratio| ratio > THRESHOLD);
        let Some(first) = sentences.first().filter(|_| exceeds) else {
            return Vec::new();
        };
        let words: Vec<String> = predicates
            .frequent(WORDS)
            .into_iter()
            .map(|(lemma, count)| format!("{lemma} {count}"))
            .collect();
        vec![Finding::new(
            ID,
            first.segment().origin.clone(),
            words.join("、"),
            HINT,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::document_excerpts(&FinalWagoRatio, text)
    }

    /// 文末が和語の動詞である文を `wago` 文、サ変動詞である文を `sahen` 文並べた文書。
    fn document(wago: usize, sahen: usize) -> String {
        format!(
            "{}{}",
            "設定を比べる。".repeat(wago),
            "設定を比較する。".repeat(sahen)
        )
    }

    #[test]
    fn a_document_of_wago_predicates_is_a_finding() {
        assert_eq!(excerpts(&document(5, 0)), ["比べる 5"]);
        assert_eq!(excerpts(&document(8, 2)), ["比べる 8"]);
    }

    #[test]
    fn a_context_without_the_measurement_is_not_judged() {
        assert!(
            harness::document_excerpts_with(&FinalWagoRatio, &harness::CONTEXT, &document(5, 0))
                .is_empty()
        );
    }

    #[test]
    fn a_ratio_within_the_threshold_is_not_a_finding() {
        assert!(excerpts(&document(7, 3)).is_empty());
        assert!(excerpts(&document(3, 7)).is_empty());
        assert!(excerpts(&document(0, 10)).is_empty());
    }

    #[test]
    fn a_document_below_the_minimum_count_is_not_judged() {
        assert!(excerpts(&document(4, 0)).is_empty());
        assert_eq!(excerpts(&document(5, 0)).len(), 1);
    }

    #[test]
    fn a_document_without_a_final_verb_is_not_judged() {
        assert!(excerpts(&"これは規則だ。".repeat(10)).is_empty());
    }

    #[test]
    fn the_excerpt_lists_the_wago_verbs_in_the_order_of_their_count() {
        assert_eq!(
            excerpts(
                "窓を開ける。設定を比べる。値を比べる。語を比べる。規則を数える。値を数える。"
            ),
            ["比べる 3、数える 2、開ける 1"]
        );
    }

    #[test]
    fn the_excerpt_keeps_the_most_frequent_words() {
        let verbs = [
            "比べる",
            "数える",
            "開ける",
            "下がる",
            "溜まる",
            "増える",
            "直る",
            "壊れる",
            "認める",
            "考える",
            "思う",
        ];
        let text: String = verbs
            .iter()
            .enumerate()
            .map(|(index, verb)| format!("値が{verb}。").repeat(verbs.len() - index))
            .collect();
        let excerpt = &excerpts(&text)[0];
        assert_eq!(excerpt.split('、').count(), WORDS);
        assert!(excerpt.starts_with("比べる 11、数える 10"), "{excerpt}");
        assert!(!excerpt.contains("思う"), "{excerpt}");
    }
}
