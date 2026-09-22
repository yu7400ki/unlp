use crate::rule::predicate::{is_case_particle, is_sahen_noun, is_verb};
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;
use crate::token::Token;

const ID: RuleId = RuleId::new(Layer::Formulaic, 6);
const HINT: &str = "「できる」「する」に戻す";

/// 迂言。
pub struct Circumlocution;

impl SentenceRule for Circumlocution {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "F06"
    }

    /// 「することができる」と、「を行う」「を実施する」「を実現する」の並び。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let tokens = sentence.tokens();
        let ranges = (0..tokens.len())
            .filter_map(|index| {
                let end = able_to(tokens, index).or_else(|| performs(tokens, index))?;
                Some(tokens[index].byte_range.start..tokens[end].byte_range.end)
            })
            .collect();
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 「サ変名詞＋する＋ことができる」の末尾の位置。
fn able_to(tokens: &[Token], index: usize) -> Option<usize> {
    let end = index + 4;
    let window = tokens.get(index..=end)?;
    (is_sahen_noun(&window[0])
        && is_verb(&window[1], "する")
        && window[2].lemma == "こと"
        && is_case_particle(&window[3], "が")
        && is_verb(&window[4], "できる"))
    .then_some(end)
}

/// 「を行う」「を実施する」「を実現する」の末尾の位置。
fn performs(tokens: &[Token], index: usize) -> Option<usize> {
    if !is_case_particle(tokens.get(index)?, "を") {
        return None;
    }
    let next = tokens.get(index + 1)?;
    if is_verb(next, "行う") {
        return Some(index + 1);
    }
    let follows_suru = matches!(next.lemma.as_str(), "実施" | "実現")
        && tokens
            .get(index + 2)
            .is_some_and(|token| is_verb(token, "する"));
    follows_suru.then_some(index + 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&Circumlocution, text)
    }

    #[test]
    fn a_roundabout_ability_is_a_finding() {
        assert_eq!(excerpts("動作することができる。"), ["動作することができる"]);
        assert_eq!(
            excerpts("設定を比較することができます。"),
            ["比較することができ"]
        );
    }

    #[test]
    fn a_roundabout_verb_is_a_finding() {
        assert_eq!(excerpts("検証を行う。"), ["を行う"]);
        assert_eq!(excerpts("移行を実施する。"), ["を実施する"]);
        assert_eq!(excerpts("機能を実現する。"), ["を実現する"]);
    }

    #[test]
    fn the_plain_form_is_not_a_finding() {
        assert!(excerpts("動作できる。").is_empty());
        assert!(excerpts("検証する。").is_empty());
        assert!(excerpts("実現する。").is_empty());
        assert!(excerpts("検証をする。").is_empty());
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(excerpts("設定を行い、検証を行う。"), ["を行い", "を行う"]);
    }
}
