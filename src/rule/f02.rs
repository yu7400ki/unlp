use std::ops::RangeInclusive;

use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Formulaic, 2);
const HINT: &str = "太字で結論を先出ししない。文の順序で示す";

/// 結論として数える太字の内側の文字数。
const INNER: RangeInclusive<usize> = 2..=30;

/// 言い切りの文末。
const ENDINGS: [&str; 4] = ["です", "ます", "ません", "でした"];

/// 冒頭の太字の結論。
pub struct BoldConclusion;

impl SentenceRule for BoldConclusion {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "F02"
    }

    /// 文頭の太字が短く言い切っている箇所。箇条書きの番号は文頭の手前に置ける。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let text = sentence.text();
        let head = text.len() - without_ordinal(text).len();
        let range = surface::bold(text)
            .into_iter()
            .next()
            .filter(|range| range.start == head)
            .filter(|range| {
                let inner = surface::inside_bold(text, range);
                INNER.contains(&inner.chars().count()) && concludes(inner)
            });
        surface::findings_at(ID, sentence, range.into_iter().collect(), HINT)
    }
}

/// 言い切りで終わるか。句点は在ってもよい。
fn concludes(inner: &str) -> bool {
    let stem = inner.strip_suffix('。').unwrap_or(inner);
    ENDINGS.iter().any(|ending| stem.ends_with(ending))
}

/// 箇条書きの番号を除いた残り。
fn without_ordinal(text: &str) -> &str {
    let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() == text.len() {
        return text;
    }
    rest.strip_prefix('.').map_or(text, str::trim_start)
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
    }

    #[test]
    fn an_ordinal_may_stand_before_the_bold() {
        assert_eq!(
            excerpts("1. **結論です。**理由を書く。"),
            ["**結論です。**"]
        );
    }

    #[test]
    fn a_bold_inside_the_sentence_is_not_a_finding() {
        assert!(excerpts("結論は**意図したものです**。").is_empty());
    }

    #[test]
    fn a_bold_without_a_conclusion_is_not_a_finding() {
        assert!(excerpts("**名詞の列挙**だ。").is_empty());
        assert!(excerpts("**直した**。").is_empty());
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
