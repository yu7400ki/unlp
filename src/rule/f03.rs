use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Formulaic, 3);
const HINT: &str = "括弧か句点で分ける。矢印は表とコードに限る";

/// 2 倍ダッシュ。
const DOUBLE_DASH: &str = "——";

/// 1 つでも数えるダッシュ。
const DASH: char = '—';

/// 地の文で数える矢印。表のセルではこの矢印を抽出の段で空白にする。
pub(crate) const ARROWS: [char; 1] = ['→'];

/// 記号に添える前後の文字数。
const AROUND: usize = 10;

/// 2 倍ダッシュと矢印。
pub struct DashAndArrow;

impl SentenceRule for DashAndArrow {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "F03"
    }

    /// ダッシュと矢印が現れた箇所。2 倍ダッシュは 1 件として数える。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        let text = sentence.text();
        let dashes = surface::matches(text, [DOUBLE_DASH]);
        let marks = text
            .match_indices(|c: char| c == DASH || ARROWS.contains(&c))
            .map(|(index, mark)| index..index + mark.len())
            .filter(|mark| !dashes.iter().any(|dash| dash.contains(&mark.start)));
        let ranges = dashes.iter().cloned().chain(marks).collect();
        surface::findings_around(ID, sentence, ranges, AROUND, HINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&DashAndArrow, text)
    }

    #[test]
    fn a_dash_or_an_arrow_is_a_finding() {
        assert_eq!(excerpts("設計——実装の順だ。"), ["設計——実装の順だ。"]);
        assert_eq!(excerpts("設計—実装の順だ。"), ["設計—実装の順だ。"]);
        assert_eq!(excerpts("入力→出力に変える。"), ["入力→出力に変える。"]);
    }

    #[test]
    fn a_double_dash_is_counted_once() {
        assert_eq!(excerpts("設計——実装の順だ。").len(), 1);
        assert_eq!(excerpts("設計—実装—検証の順だ。").len(), 2);
    }

    #[test]
    fn a_sentence_without_the_marks_is_not_a_finding() {
        assert!(excerpts("設計から実装へ進む。").is_empty());
    }

    #[test]
    fn the_excerpt_keeps_ten_chars_on_each_side() {
        let text = format!("{}→{}。", "あ".repeat(20), "い".repeat(20));
        let excerpts = excerpts(&text);
        assert_eq!(excerpts.len(), 1);
        assert_eq!(
            excerpts[0],
            format!("{}→{}", "あ".repeat(AROUND), "い".repeat(AROUND))
        );
    }
}
