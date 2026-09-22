use crate::rule::{Context, DocumentRule, Finding, Layer, RuleId, run};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Structure, 6);
const HINT: &str = "毎文に接続詞を置かない。主題の流れで繋がるなら接続詞を落とす";
const CONJUNCTIONS: &str = "conjunctions";

/// 抜粋で接続詞を隔てる読点。
const SEPARATOR: &str = "、";

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

/// 語が文の先頭に Token の境界で一致し、その直後で新しい節が始まるか。
fn is_leading(sentence: &Sentence, conjunction: &str) -> bool {
    if !sentence.text().starts_with(conjunction) {
        return false;
    }
    let tokens = sentence.tokens();
    let Some(length) = head_length(tokens, conjunction) else {
        return false;
    };
    tokens.get(length).is_none_or(|token| !continues(token))
}

/// 文頭から語を占める Token の数。語が Token の境界で終わらないときは `None`。
fn head_length(tokens: &[Token], conjunction: &str) -> Option<usize> {
    tokens
        .iter()
        .position(|token| token.byte_range.end == conjunction.len())
        .map(|index| index + 1)
}

/// 前の語に続いて 1 つの文節を作る Token。
fn continues(token: &Token) -> bool {
    matches!(
        token.pos.pos1,
        Pos1::Particle | Pos1::AuxVerb | Pos1::Verb | Pos1::Adjective | Pos1::Suffix
    )
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
                "さらに規則を数える。また、並べる。したがって規則が残る。規則を直す。なお、数える。ただし、並べる。その結果、規則が残る。"
            ),
            ["さらに、また、したがって", "なお、ただし、その結果"]
        );
    }

    /// 同じ文を 3 つ並べた文書が連打になるか。
    fn is_a_run(sentence: &str) -> bool {
        !excerpts(&sentence.repeat(3)).is_empty()
    }

    #[test]
    fn a_new_clause_after_the_word_makes_it_a_conjunction() {
        let missing: Vec<&str> = [
            "さらに、直す。",
            "これにより未返却の接続が溜まる。",
            "したがって、直す。",
            "また、直す。",
        ]
        .into_iter()
        .filter(|sentence| !is_a_run(sentence))
        .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn a_word_that_the_sentence_continues_is_not_a_conjunction() {
        let counted: Vec<&str> = [
            "さらに直す。",
            "さらに速い道を選ぶ。",
            "そのためです。",
            "その結果を表に書く。",
            "そのために必要な設定を数える。",
            "これにより得た値を使う。",
            "加えている項目を確認する。",
            "また来る。",
            "一方的に決める。",
        ]
        .into_iter()
        .filter(|sentence| is_a_run(sentence))
        .collect();
        assert!(counted.is_empty(), "{counted:?}");
    }

    #[test]
    fn a_conjunction_inside_the_sentence_is_not_at_its_head() {
        assert!(
            excerpts("規則をさらに数える。規則をまた、並べる。規則はしたがって残る。").is_empty()
        );
    }
}
