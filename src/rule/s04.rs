use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Structure, 4);
const HINT: &str = "書き手を省く。主体を示すなら「こちらで」「今回は」";

/// 技術文の「私」。
pub struct FirstPerson;

impl SentenceRule for FirstPerson {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "S04"
    }

    /// 一人称の代名詞に助詞か「自身」が続く箇所。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let tokens = sentence.tokens();
        let ranges = tokens
            .iter()
            .zip(tokens.iter().skip(1))
            .filter(|(pronoun, next)| is_first_person(pronoun) && follows_the_writer(next))
            .map(|(pronoun, next)| pronoun.byte_range.start..next.byte_range.end)
            .collect();
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

fn is_first_person(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Pronoun && matches!(token.lemma.as_str(), "私" | "わたし")
}

/// 主体を示す助詞か、「私自身」の「自身」であるか。
fn follows_the_writer(token: &Token) -> bool {
    match token.pos.pos1 {
        Pos1::Particle => matches!(token.surface.as_str(), "は" | "が" | "の" | "も"),
        Pos1::Noun | Pos1::Suffix => token.lemma == "自身",
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&FirstPerson, text)
    }

    #[test]
    fn the_writer_with_a_particle_is_a_finding() {
        assert_eq!(excerpts("私が設計しました。"), ["私が"]);
        assert_eq!(excerpts("私は設計しました。"), ["私は"]);
        assert_eq!(excerpts("私の案を通す。"), ["私の"]);
        assert_eq!(excerpts("私も見ます。"), ["私も"]);
        assert_eq!(excerpts("わたしの案を通す。"), ["わたしの"]);
    }

    #[test]
    fn the_writer_with_jishin_is_a_finding() {
        assert_eq!(excerpts("私自身が確かめた。"), ["私自身"]);
    }

    #[test]
    fn a_sentence_without_the_writer_is_not_a_finding() {
        assert!(excerpts("こちらで設計しました。").is_empty());
        assert!(excerpts("彼が設計しました。").is_empty());
    }

    #[test]
    fn another_word_after_the_writer_is_not_a_finding() {
        assert!(excerpts("「私」を省く。").is_empty());
        assert!(excerpts("私と彼が話す。").is_empty());
    }

    #[test]
    fn the_findings_are_one_per_pronoun() {
        assert_eq!(excerpts("私は私の案を通す。"), ["私は", "私の"]);
    }
}
