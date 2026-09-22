use std::ops::{Range, RangeInclusive};

use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::{self, BOLD, Sentence};
use crate::token::Pos1;

const ID: RuleId = RuleId::new(Layer::Formulaic, 2);
const HINT: &str = "太字で結論を先出ししない。文の順序で示す";

/// 結論として数える太字の内側の文字数。
const INNER: RangeInclusive<usize> = 2..=30;

/// 冒頭の太字の結論。
pub struct BoldConclusion;

impl SentenceRule for BoldConclusion {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "F02"
    }

    /// 文頭の太字が短く言い切っている箇所。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let text = sentence.text();
        let range = sentence::bold(text)
            .into_iter()
            .next()
            .filter(|range| range.start == 0)
            .filter(|range| {
                let inner = sentence::inside_bold(text, range);
                INNER.contains(&inner.chars().count()) && concludes(sentence, range)
            });
        surface::findings_at(ID, sentence, range.into_iter().collect(), HINT)
    }
}

/// 太字の内側が文として終わるか。句点で閉じるか、末尾の Token が助動詞であるもの。
fn concludes(sentence: &Sentence, bold: &Range<usize>) -> bool {
    let text = sentence.text();
    let inner = sentence::inside_bold(text, bold);
    if inner.ends_with('。') {
        return true;
    }
    let end = bold.end - BOLD.len();
    sentence
        .tokens()
        .iter()
        .rfind(|token| token.byte_range.end <= end)
        .is_some_and(|token| token.pos.pos1 == Pos1::AuxVerb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&BoldConclusion, text)
    }

    #[test]
    fn a_bold_conclusion_at_the_head_is_a_finding() {
        assert_eq!(
            excerpts("**意図したものです。**エラーが出る。"),
            ["**意図したものです。**"]
        );
        assert_eq!(excerpts("**直します**。次に進む。"), ["**直します**"]);
        assert_eq!(excerpts("**直りません**。"), ["**直りません**"]);
        assert_eq!(excerpts("**そうでした**。"), ["**そうでした**"]);
        assert_eq!(excerpts("**直しました**"), ["**直しました**"]);
        assert_eq!(excerpts("**直した**。"), ["**直した**"]);
    }

    #[test]
    fn a_plain_conclusion_at_the_head_is_a_finding() {
        assert_eq!(
            excerpts("**これは意図した挙動だ。**"),
            ["**これは意図した挙動だ。**"]
        );
        assert_eq!(excerpts("**必要である。**"), ["**必要である。**"]);
    }

    #[test]
    fn a_bold_inside_the_sentence_is_not_a_finding() {
        assert!(excerpts("結論は**意図したものです**。").is_empty());
    }

    #[test]
    fn a_bold_without_a_conclusion_is_not_a_finding() {
        assert!(excerpts("**名詞の列挙**だ。").is_empty());
        assert!(excerpts("**注意**").is_empty());
        assert!(excerpts("**重要な点**").is_empty());
    }

    #[test]
    fn the_inner_text_stays_between_two_and_thirty_chars() {
        assert_eq!(
            excerpts(&format!("**{}です**。", "あ".repeat(28))),
            [format!("**{}です**", "あ".repeat(28))]
        );
        assert!(excerpts(&format!("**{}です**。", "あ".repeat(29))).is_empty());
    }

    #[test]
    fn a_marker_without_its_pair_is_not_a_finding() {
        assert!(excerpts("**意図したものです。").is_empty());
    }
}
