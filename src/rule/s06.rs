use crate::rule::{Context, DocumentRule, Finding, Layer, RuleId, run};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Structure, 6);
const HINT: &str = "毎文に接続詞を置かない。主題の流れで繋がるなら接続詞を落とす";
const CONJUNCTIONS: &str = "conjunctions";

/// 抜粋で接続詞を隔てる読点。
const COMMA: char = '、';

/// 文頭の接続詞の連打。
pub struct LeadingConjunction;

impl DocumentRule for LeadingConjunction {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "S06"
    }

    /// 語リストの接続詞で始まる文が続く範囲。抜粋はその接続詞を連ねた文字列。
    fn check(&self, sentences: &[Sentence], context: &Context) -> Vec<Finding> {
        run::runs(sentences, |sentence| leading(sentence, context).is_some())
            .into_iter()
            .map(|run| {
                let conjunctions: Vec<&str> = run
                    .iter()
                    .filter_map(|sentence| leading(sentence, context))
                    .collect();
                Finding::new(
                    ID,
                    run[0].segment().origin.clone(),
                    conjunctions.join(&COMMA.to_string()),
                    HINT,
                )
            })
            .collect()
    }
}

/// 文頭にある語リストの接続詞。読点を伴う語は読点まで照合し、読点を除いて返す。
fn leading<'a>(sentence: &Sentence, context: &'a Context) -> Option<&'a str> {
    context
        .list(ID)
        .words(CONJUNCTIONS)
        .find(|conjunction| sentence.text().starts_with(conjunction))
        .map(|conjunction| conjunction.trim_end_matches(COMMA))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::document_excerpts(&LeadingConjunction, text)
    }

    #[test]
    fn a_run_of_conjunctions_is_a_finding() {
        assert_eq!(
            excerpts("さらに規則を数える。また、規則を並べる。したがって規則が残る。"),
            ["さらに、また、したがって"]
        );
        assert_eq!(
            excerpts(
                "加えて規則を数える。これにより規則が減る。そのため規則を並べる。その結果、規則が残る。"
            ),
            ["加えて、これにより、そのため、その結果"]
        );
        assert_eq!(
            excerpts("一方、規則を数える。ただし、規則は残る。なお、規則を並べる。"),
            ["一方、ただし、なお"]
        );
    }

    #[test]
    fn a_run_below_three_sentences_is_not_a_finding() {
        assert!(excerpts("さらに規則を数える。また、規則を並べる。").is_empty());
        assert!(excerpts("規則を数える。規則を並べる。規則が残る。").is_empty());
    }

    #[test]
    fn a_sentence_without_a_conjunction_ends_the_run() {
        assert!(
            excerpts("さらに規則を数える。規則を並べる。また、規則が残る。したがって減る。")
                .is_empty()
        );
        assert_eq!(
            excerpts(
                "さらに数える。また、並べる。したがって残る。規則を直す。なお、数える。ただし、並べる。その結果、残る。"
            ),
            ["さらに、また、したがって", "なお、ただし、その結果"]
        );
    }

    #[test]
    fn a_conjunction_without_its_comma_is_not_a_conjunction() {
        assert!(
            excerpts("また規則を数える。一方で規則を並べる。なお規則が残る。ただし規則は減る。")
                .is_empty()
        );
    }

    #[test]
    fn a_conjunction_inside_the_sentence_is_not_at_its_head() {
        assert!(
            excerpts("規則をさらに数える。規則をまた、並べる。規則はしたがって残る。").is_empty()
        );
    }
}
