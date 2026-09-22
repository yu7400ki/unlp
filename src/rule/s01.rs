use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, WordList};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Structure, 1);
const HINT: &str = "文書や型を語り手にしない。所在や手段で受ける";
const PERSON: &str = "person";
const SPEECH: &str = "speech";

/// 格助詞「が」と発話動詞の間に置く Token の上限。
const WINDOW: usize = 6;

/// 無生物主語と発話動詞。
pub struct InanimateSpeaker;

impl SentenceRule for InanimateSpeaker {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "S01"
    }

    /// 人を表さない名詞に格助詞「が」が続き、その後の発話動詞が受け身でも引用でもない箇所。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        let tokens = sentence.tokens();
        let list = context.list(ID);
        let mut findings = Vec::new();
        for (index, subject) in tokens.iter().enumerate() {
            if !is_noun(subject) || list.contains(PERSON, &subject.lemma) {
                continue;
            }
            if !tokens.get(index + 1).is_some_and(is_case_ga) {
                continue;
            }
            let start = index + 2;
            let end = (start + WINDOW).min(tokens.len());
            for position in start..end {
                let verb = &tokens[position];
                if !is_speech(verb, list) || passive_follows(tokens, position) {
                    continue;
                }
                if quote_follows(tokens, position) {
                    continue;
                }
                let excerpt = &sentence.text()[subject.byte_range.start..verb.byte_range.end];
                findings.push(Finding::new(
                    ID,
                    sentence.segment().origin.clone(),
                    excerpt.to_string(),
                    HINT,
                ));
                break;
            }
        }
        findings
    }
}

fn is_noun(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Noun && matches!(token.pos.pos2.as_str(), "普通名詞" | "固有名詞")
}

fn is_case_ga(token: &Token) -> bool {
    is_case_particle(token, "が")
}

fn is_speech(token: &Token, list: &WordList) -> bool {
    if !list.contains(SPEECH, &token.lemma) {
        return false;
    }
    token.pos.pos1 == Pos1::Verb
        || (token.pos.pos1 == Pos1::Noun
            && token.pos.pos2 == "普通名詞"
            && token.pos.pos3 == "サ変可能")
}

fn is_case_particle(token: &Token, surface: &str) -> bool {
    token.pos.pos1 == Pos1::Particle && token.pos.pos2 == "格助詞" && token.surface == surface
}

/// 「される」「語られる」のように受け身が続くか。
fn passive_follows(tokens: &[Token], position: usize) -> bool {
    let Some(next) = tokens.get(position + 1) else {
        return false;
    };
    if next.pos.pos1 == Pos1::AuxVerb && matches!(next.lemma.as_str(), "れる" | "られる") {
        return true;
    }
    next.surface == "さ"
        && tokens
            .get(position + 2)
            .is_some_and(|token| token.surface == "れ")
}

/// 「述べると書く」のように引用の「と」が続くか。
fn quote_follows(tokens: &[Token], position: usize) -> bool {
    tokens
        .get(position + 1)
        .is_some_and(|token| is_case_particle(token, "と"))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::sync::LazyLock;

    use super::*;
    use crate::document::{Document, LineRange, Origin, Segment, SegmentKind};
    use crate::morph::Analyzer;

    static ANALYZER: LazyLock<Analyzer> = LazyLock::new(|| Analyzer::new().unwrap());
    static CONTEXT: LazyLock<Context> = LazyLock::new(Context::defaults);

    fn excerpts(text: &str) -> Vec<String> {
        let document = Document {
            name: "t".to_string(),
            segments: vec![Segment {
                text: text.to_string(),
                origin: Origin {
                    path: "t".to_string(),
                    lines: LineRange::new(NonZeroU32::MIN, NonZeroU32::MIN).unwrap(),
                    commit: None,
                },
                kind: SegmentKind::Prose,
            }],
        };
        ANALYZER
            .analyze_document(&document)
            .iter()
            .flat_map(|sentence| InanimateSpeaker.check(sentence, &CONTEXT))
            .map(|finding| finding.excerpt().to_string())
            .collect()
    }

    #[test]
    fn an_inanimate_subject_with_a_speech_verb_is_a_finding() {
        assert_eq!(excerpts("型の doc が名乗る。"), ["doc が名乗る"]);
        assert_eq!(excerpts("README が述べる。"), ["README が述べる"]);
        assert_eq!(excerpts("README が主張する。"), ["README が主張"]);
    }

    #[test]
    fn the_speech_verb_is_within_six_tokens_of_the_particle() {
        assert_eq!(
            excerpts("文書がとても丁寧に詳しく述べる。"),
            ["文書がとても丁寧に詳しく述べる"]
        );
        assert!(excerpts("文書がとても丁寧に何度も述べる。").is_empty());
    }

    #[test]
    fn a_person_as_the_subject_is_not_a_finding() {
        assert!(excerpts("チームが述べる。").is_empty());
        assert!(excerpts("筆者が主張する。").is_empty());
        assert!(excerpts("彼が答えると言う。").is_empty());
        assert!(excerpts("利用者が述べる。").is_empty());
    }

    #[test]
    fn a_passive_speech_verb_is_not_a_finding() {
        assert!(excerpts("型が宣言された。").is_empty());
        assert!(excerpts("設計文書が語られる。").is_empty());
    }

    #[test]
    fn a_quoted_speech_verb_is_not_a_finding() {
        assert!(excerpts("文書が述べると書いた。").is_empty());
    }

    #[test]
    fn a_verb_outside_the_list_is_not_a_finding() {
        assert!(excerpts("仕様が要求する。").is_empty());
        assert!(excerpts("設定が変わる。").is_empty());
    }

    #[test]
    fn the_findings_are_one_per_subject() {
        assert_eq!(
            excerpts("型が名乗る。README が述べる。"),
            ["型が名乗る", "README が述べる"]
        );
    }
}
