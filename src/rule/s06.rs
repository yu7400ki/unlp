use crate::rule::predicate::is_comma;
use crate::rule::{Context, DocumentRule, Finding, Layer, RuleId, run};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Structure, 6);
const HINT: &str = "毎文に接続詞を置かない。主題の流れで繋がるなら接続詞を落とす";
const CONJUNCTIONS: &str = "conjunctions";

/// 抜粋で接続詞を隔てる読点。
const SEPARATOR: &str = "、";

/// 直後の読点を伴うときだけ接続詞として数える語。
const WITH_COMMA: [&str; 4] = ["また", "一方", "ただし", "なお"];

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
                    conjunctions.join(SEPARATOR),
                    HINT,
                )
            })
            .collect()
    }
}

/// 文頭にある語リストの接続詞。
fn leading<'a>(sentence: &Sentence, context: &'a Context) -> Option<&'a str> {
    context
        .list(ID)
        .words(CONJUNCTIONS)
        .find(|conjunction| is_leading(sentence, conjunction))
}

/// 文頭の接続詞であるか。`WITH_COMMA` の語は直後の Token が読点のときだけ、1 つの Token に
/// なる語はその品詞が接続詞か副詞のときだけ、複数の Token に分かれる語は後ろに別の形を
/// 作る Token が続かないときだけ接続詞とする。
fn is_leading(sentence: &Sentence, conjunction: &str) -> bool {
    if !sentence.text().starts_with(conjunction) {
        return false;
    }
    let tokens = sentence.tokens();
    let Some(length) = head_length(tokens, conjunction) else {
        return false;
    };
    let next = tokens.get(length);
    if WITH_COMMA.contains(&conjunction) {
        return next.is_some_and(is_comma);
    }
    if length == 1 {
        return matches!(tokens[0].pos.pos1, Pos1::Conjunction | Pos1::Adverb);
    }
    next.is_none_or(|token| !continues(token))
}

/// 文頭から語を占める Token の数。語が Token の境界で終わらないときは `None`。
fn head_length(tokens: &[Token], conjunction: &str) -> Option<usize> {
    tokens
        .iter()
        .position(|token| token.byte_range.end == conjunction.len())
        .map(|index| index + 1)
}

/// 前の語に続いて別の形を作る Token。助詞は名詞句にし、動詞は継続の形にする。
fn continues(token: &Token) -> bool {
    matches!(token.pos.pos1, Pos1::Particle | Pos1::Verb)
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
                "さらに数える。また、並べる。したがって規則が残る。規則を直す。なお、数える。ただし、並べる。その結果、規則が残る。"
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
    fn a_noun_phrase_at_the_head_is_not_a_conjunction() {
        assert!(excerpts(&"その結果を表に書く。".repeat(3)).is_empty());
        assert!(excerpts(&"そのために必要な設定を数える。".repeat(3)).is_empty());
    }

    #[test]
    fn a_continuous_form_at_the_head_is_not_a_conjunction() {
        assert!(excerpts(&"加えている項目を確認する。".repeat(3)).is_empty());
    }

    #[test]
    fn a_conjunction_inside_the_sentence_is_not_at_its_head() {
        assert!(
            excerpts("規則をさらに数える。規則をまた、並べる。規則はしたがって残る。").is_empty()
        );
    }
}
