use crate::rule::predicate::is_particle;
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Structure, 2);
const HINT: &str = "指示語を省くか、受ける名詞を書く";

/// 文頭の指示代名詞。
pub struct LeadingDemonstrative;

impl SentenceRule for LeadingDemonstrative {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "S02"
    }

    /// 文頭の「これ」「それ」に「は」「も」が続く箇所。複数を表す接尾辞「ら」を挟む形も含む。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let tokens = sentence.tokens();
        let Some(head) = tokens.first().filter(|token| is_demonstrative(token)) else {
            return Vec::new();
        };
        let mut next = 1;
        if tokens.get(next).is_some_and(is_plural_suffix) {
            next += 1;
        }
        let range = tokens
            .get(next)
            .filter(|token| is_particle(token, "は") || is_particle(token, "も"))
            .map(|particle| head.byte_range.start..particle.byte_range.end);
        surface::findings_at(ID, sentence, range.into_iter().collect(), HINT)
    }
}

fn is_demonstrative(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Pronoun && matches!(token.lemma.as_str(), "これ" | "それ")
}

fn is_plural_suffix(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Suffix && token.lemma == "ら"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&LeadingDemonstrative, text)
    }

    #[test]
    fn a_demonstrative_at_the_head_with_a_topic_particle_is_a_finding() {
        assert_eq!(excerpts("これは意図的です。"), ["これは"]);
        assert_eq!(excerpts("それも同じだ。"), ["それも"]);
        assert_eq!(excerpts("これらは規則だ。"), ["これらは"]);
        assert_eq!(excerpts("それらも規則だ。"), ["それらも"]);
    }

    #[test]
    fn a_demonstrative_inside_the_sentence_is_not_a_finding() {
        assert!(excerpts("規則は、これは意図的だと述べる。").is_empty());
    }

    #[test]
    fn another_particle_after_the_demonstrative_is_not_a_finding() {
        assert!(excerpts("これが意図だ。").is_empty());
        assert!(excerpts("これを直す。").is_empty());
        assert!(excerpts("これと同じだ。").is_empty());
    }

    #[test]
    fn another_word_at_the_head_is_not_a_finding() {
        assert!(excerpts("規則は意図的です。").is_empty());
        assert!(excerpts("あれは意図的です。").is_empty());
    }

    #[test]
    fn an_unclosed_bold_marker_keeps_the_head_of_the_next_sentence() {
        assert_eq!(
            excerpts("**太字が閉じません。これは意図的です。"),
            ["これは"]
        );
    }

    #[test]
    fn the_findings_are_one_per_sentence() {
        assert_eq!(
            excerpts("これは意図的です。それも同じだ。"),
            ["これは", "それも"]
        );
    }
}
