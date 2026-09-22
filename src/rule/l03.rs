use crate::rule::predicate::{is_aux_verb, is_verb};
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Lexical, 3);
const HINT: &str = "「〜すると」「〜した時点で」に戻す";

/// 英語のリズム。
pub struct EnglishRhythm;

impl SentenceRule for EnglishRhythm {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "L03"
    }

    /// 動詞に「始める」が続く箇所と、動詞の過去に「瞬間」が続く箇所。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let tokens = sentence.tokens();
        let ranges = (0..tokens.len())
            .filter(|index| tokens[*index].pos.pos1 == Pos1::Verb)
            .filter_map(|index| {
                let end = begins(tokens, index).or_else(|| at_the_moment(tokens, index))?;
                Some(tokens[index].byte_range.start..tokens[end].byte_range.end)
            })
            .collect();
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 続く「始める」の位置。接続助詞を 1 つ挟める。
fn begins(tokens: &[Token], index: usize) -> Option<usize> {
    let mut next = index + 1;
    if tokens.get(next).is_some_and(is_conjunctive_particle) {
        next += 1;
    }
    tokens.get(next).filter(|token| is_beginning(token))?;
    Some(next)
}

/// 続く「た瞬間」の末尾の位置。
fn at_the_moment(tokens: &[Token], index: usize) -> Option<usize> {
    let end = index + 2;
    let window = tokens.get(index..=end)?;
    (is_past(&window[1]) && is_moment(&window[2])).then_some(end)
}

fn is_conjunctive_particle(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Particle && token.pos.pos2 == "接続助詞"
}

fn is_beginning(token: &Token) -> bool {
    is_verb(token, "始める") && token.pos.pos2 == "非自立可能"
}

fn is_past(token: &Token) -> bool {
    is_aux_verb(token, "た")
}

fn is_moment(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Noun && token.lemma == "瞬間"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&EnglishRhythm, text)
    }

    #[test]
    fn a_verb_that_begins_is_a_finding() {
        assert_eq!(excerpts("エラーを出さずに失敗し始めます。"), ["し始め"]);
        assert_eq!(excerpts("開始し始める。"), ["し始める"]);
    }

    #[test]
    fn a_conjunctive_particle_may_stand_between_the_verbs() {
        assert_eq!(excerpts("動いて始める。"), ["動いて始める"]);
    }

    #[test]
    fn a_past_verb_before_a_moment_is_a_finding() {
        assert_eq!(excerpts("実行した瞬間に落ちる。"), ["した瞬間"]);
        assert_eq!(excerpts("読み終えた瞬間だった。"), ["終えた瞬間"]);
    }

    #[test]
    fn a_verb_on_its_own_is_not_a_finding() {
        assert!(excerpts("作業を始める。").is_empty());
        assert!(excerpts("実行すると落ちる。").is_empty());
        assert!(excerpts("その瞬間に落ちる。").is_empty());
    }
}
