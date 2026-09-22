use std::ops::Range;

use crate::rule::predicate::{is_aux_verb, is_case_particle, is_sahen_noun, is_verb};
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, WordList, surface};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Structure, 1);
const HINT: &str = "文書や型を語り手にしない。所在や手段で受ける";
const PERSON: &str = "person";
const SPEECH: &str = "speech";

/// 格助詞「が」と発話動詞の間に挟む Token の上限。
const GAP: usize = 6;

/// 無生物主語と発話動詞。
pub struct InanimateSpeaker;

impl SentenceRule for InanimateSpeaker {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "S01"
    }

    /// 人を表さない名詞句に格助詞「が」が続き、その後の発話動詞が受け身でも引用でもない箇所。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        let tokens = sentence.tokens();
        let list = context.list(ID);
        let mut ranges = Vec::new();
        for (index, particle) in tokens.iter().enumerate() {
            if !is_case_particle(particle, "が") {
                continue;
            }
            let Some(subject) = subject_phrase(tokens, index) else {
                continue;
            };
            if tokens[subject.clone()]
                .iter()
                .any(|token| is_person(token, list))
            {
                continue;
            }
            let Some(verb) = speech_verb(tokens, index, list) else {
                continue;
            };
            ranges.push(
                tokens[subject.start].byte_range.start
                    ..tokens[verb_end(tokens, verb)].byte_range.end,
            );
        }
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 「が」の直前から遡った名詞と接辞の連なり。普通名詞も固有名詞も含まない連なりは主語にしない。
fn subject_phrase(tokens: &[Token], particle: usize) -> Option<Range<usize>> {
    let mut start = particle;
    while start > 0 && is_phrase_token(&tokens[start - 1]) {
        start -= 1;
    }
    let phrase = start..particle;
    tokens[phrase.clone()]
        .iter()
        .any(is_common_noun)
        .then_some(phrase)
}

/// 「が」から数えて `GAP` 個までの Token を挟む発話動詞。読点をまたぐ先は見ない。
fn speech_verb(tokens: &[Token], particle: usize, list: &WordList) -> Option<usize> {
    let last = (particle + 1 + GAP).min(tokens.len().saturating_sub(1));
    for position in particle + 1..=last {
        if is_comma(&tokens[position]) {
            return None;
        }
        if is_speech(tokens, position, list)
            && !passive_follows(tokens, position)
            && !quote_follows(tokens, position)
        {
            return Some(position);
        }
    }
    None
}

/// 人名の固有名詞か、語リストにある人を表す語。
fn is_person(token: &Token, list: &WordList) -> bool {
    (token.pos.pos1 == Pos1::Noun && token.pos.pos3 == "人名")
        || list.contains(PERSON, &token.lemma)
}

fn is_phrase_token(token: &Token) -> bool {
    matches!(token.pos.pos1, Pos1::Noun | Pos1::Prefix | Pos1::Suffix)
}

fn is_common_noun(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Noun && matches!(token.pos.pos2.as_str(), "普通名詞" | "固有名詞")
}

fn is_comma(token: &Token) -> bool {
    token.pos.pos1 == Pos1::SupplementarySymbol && token.pos.pos2 == "読点"
}

/// 語リストにある発話動詞。サ変可能の名詞は「する」が続くときに限る。
fn is_speech(tokens: &[Token], position: usize, list: &WordList) -> bool {
    let token = &tokens[position];
    if !list.contains(SPEECH, &token.lemma) {
        return false;
    }
    match token.pos.pos1 {
        Pos1::Verb => true,
        Pos1::Noun => is_sahen_noun(token) && is_suru(tokens.get(position + 1)),
        _ => false,
    }
}

/// 発話動詞の末尾の Token。サ変可能の名詞は続く「する」までを 1 語とする。
fn verb_end(tokens: &[Token], position: usize) -> usize {
    if is_suru(tokens.get(position + 1)) {
        position + 1
    } else {
        position
    }
}

/// 「語られる」「宣言される」のように受け身が続くか。
fn passive_follows(tokens: &[Token], position: usize) -> bool {
    let next = tokens.get(verb_end(tokens, position) + 1);
    next.is_some_and(|token| is_aux_verb(token, "れる") || is_aux_verb(token, "られる"))
}

/// 「述べると書く」のように引用の「と」が続くか。
fn quote_follows(tokens: &[Token], position: usize) -> bool {
    tokens
        .get(verb_end(tokens, position) + 1)
        .is_some_and(|token| is_case_particle(token, "と"))
}

fn is_suru(token: Option<&Token>) -> bool {
    token.is_some_and(|token| is_verb(token, "する"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness::{self, CONTEXT};

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&InanimateSpeaker, text)
    }

    fn excerpts_with(context: &Context, text: &str) -> Vec<String> {
        harness::excerpts_with(&InanimateSpeaker, context, text)
    }

    #[test]
    fn an_inanimate_subject_with_a_speech_verb_is_a_finding() {
        assert_eq!(excerpts("型の doc が名乗る。"), ["doc が名乗る"]);
        assert_eq!(excerpts("README が述べる。"), ["README が述べる"]);
        assert_eq!(excerpts("README が主張する。"), ["README が主張する"]);
    }

    #[test]
    fn the_subject_reaches_back_over_affixes() {
        assert_eq!(excerpts("設計書が述べる。"), ["設計書が述べる"]);
        assert_eq!(excerpts("自動化が述べる。"), ["自動化が述べる"]);
        assert_eq!(excerpts("本規則が述べる。"), ["本規則が述べる"]);
    }

    #[test]
    fn a_person_in_the_subject_is_not_a_finding() {
        assert!(excerpts("チームが述べる。").is_empty());
        assert!(excerpts("筆者が主張する。").is_empty());
        assert!(excerpts("彼が答えると言う。").is_empty());
        assert!(excerpts("利用者が述べる。").is_empty());
        assert!(excerpts("田中さんが教えてくれた。").is_empty());
        assert!(excerpts("家康が訴える。").is_empty());

        let context = CONTEXT.without_word(ID, PERSON, "者");
        assert_eq!(
            excerpts_with(&context, "利用者が述べる。"),
            ["利用者が述べる"]
        );
    }

    #[test]
    fn a_passive_speech_verb_is_not_a_finding() {
        assert!(excerpts("型が宣言された。").is_empty());
        assert!(excerpts("設計文書が語られる。").is_empty());
        assert!(excerpts("文書が主張される。").is_empty());
        assert!(excerpts("型が宣言されます。").is_empty());
    }

    #[test]
    fn a_quoted_speech_verb_is_not_a_finding() {
        assert!(excerpts("文書が述べると書いた。").is_empty());
        assert!(excerpts("仕様が主張すると書いた。").is_empty());
    }

    #[test]
    fn a_sahen_noun_without_suru_is_not_a_finding() {
        assert!(excerpts("型が宣言を持つ。").is_empty());
        assert!(excerpts("仕様が主張を含む。").is_empty());
        assert!(excerpts("設定が約束の値を返す。").is_empty());
    }

    #[test]
    fn a_verb_outside_the_list_is_not_a_finding() {
        assert!(excerpts("仕様が要求する。").is_empty());
        assert!(excerpts("設定が変わる。").is_empty());
    }

    #[test]
    fn at_most_six_tokens_stand_between_the_particle_and_the_verb() {
        assert_eq!(
            excerpts("文書がとても丁寧に何度も述べる。"),
            ["文書がとても丁寧に何度も述べる"]
        );
        assert!(excerpts("文書がとても丁寧に何度も繰り返し述べる。").is_empty());
    }

    #[test]
    fn a_comma_ends_the_search_for_the_verb() {
        assert!(excerpts("保証が無く、誤った宣言は規則を誤検出させる。").is_empty());
        assert!(excerpts("文書が古く、規則を述べる。").is_empty());
    }

    #[test]
    fn the_findings_are_one_per_subject() {
        assert_eq!(
            excerpts("型が名乗る。README が述べる。"),
            ["型が名乗る", "README が述べる"]
        );
    }
}
