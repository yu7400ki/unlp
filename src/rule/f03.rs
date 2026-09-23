use std::ops::Range;

use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, surface};
use crate::sentence::{self, Sentence};

const ID: RuleId = RuleId::new(Layer::Formulaic, 3);
const HINT: &str = "本文の太字は外す。強調は語の選択と文の位置で行う";

/// 本文の太字。
pub struct BoldInProse;

impl SentenceRule for BoldInProse {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "F03"
    }

    /// 太字で囲んだ箇所。定義の列の見出し（文頭の太字に続くコロン）、見出しの代わりに
    /// 置いた太字だけの一区切り、中身が記法だけの太字は数えない。
    fn check(&self, sentence: &Sentence, _context: &Context) -> Vec<Finding> {
        if stands_for_a_heading(sentence) {
            return Vec::new();
        }
        let text = sentence.text();
        let ranges = sentence::bold(text)
            .into_iter()
            .filter(|range| !heads_a_definition(text, range))
            .filter(|range| !sentence::inside_bold(text, range).trim().is_empty())
            .collect();
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 文の属する一区切りが、句点で終わらない太字 1 つだけでできているか。
fn stands_for_a_heading(sentence: &Sentence) -> bool {
    let text = sentence.segment().text.trim();
    matches!(sentence::bold(text).as_slice(), [range]
        if *range == (0..text.len())
            && !sentence::inside_bold(text, range)
                .trim_end()
                .ends_with(sentence::TERMINATORS))
}

/// 文頭の太字が定義の見出しか。コロンは太字の直後にも内側の末尾にも置かれる。
fn heads_a_definition(text: &str, bold: &Range<usize>) -> bool {
    bold.start == 0
        && (text[bold.end..].starts_with([':', '：'])
            || sentence::inside_bold(text, bold)
                .trim_end()
                .ends_with([':', '：']))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&BoldInProse, text)
    }

    #[test]
    fn a_bold_range_is_a_finding() {
        assert_eq!(excerpts("本文の**強調**だ。"), ["**強調**"]);
        assert_eq!(
            excerpts("**一つ**と**二つ**を挙げる。"),
            ["**一つ**", "**二つ**"]
        );
    }

    #[test]
    fn the_bold_of_a_conclusion_is_also_counted() {
        assert_eq!(
            excerpts("**意図したものです。**エラーが出る。"),
            ["**意図したものです。**"]
        );
    }

    #[test]
    fn a_sentence_without_bold_is_not_a_finding() {
        assert!(excerpts("強調を外した文だ。").is_empty());
        assert!(excerpts("**閉じない文だ。").is_empty());
    }

    #[test]
    fn a_bold_heading_before_a_colon_is_not_a_finding() {
        assert!(excerpts("**ブランチの作成**: 作業ごとに切る。").is_empty());
        assert!(excerpts("**注意**：値を変える。").is_empty());
        assert!(excerpts("**注意:** 値を変える。").is_empty());
        assert!(excerpts("**注意：** 値を変える。").is_empty());
        assert_eq!(
            excerpts("値は **重要**: だと書く。"),
            ["**重要**"],
            "文頭でない太字はコロンが続いても数える"
        );
    }

    #[test]
    fn a_segment_of_a_bold_heading_alone_is_not_a_finding() {
        assert!(excerpts("**参考文献**").is_empty());
        assert!(excerpts(" **参考文献** ").is_empty());
    }

    #[test]
    fn a_segment_of_a_bold_sentence_alone_is_a_finding() {
        assert_eq!(excerpts("**結論です。**"), ["**結論です。**"]);
        assert_eq!(excerpts("**本当か？**"), ["**本当か？**"]);
    }

    #[test]
    fn a_bold_with_other_text_in_the_segment_is_a_finding() {
        assert_eq!(excerpts("**注意** この設定は無効です。"), ["**注意**"]);
        assert_eq!(excerpts("**参考文献**\n本文を読む。"), ["**参考文献**"]);
        assert_eq!(excerpts("**甲** と **乙**"), ["**甲**", "**乙**"]);
    }

    #[test]
    fn a_bold_around_a_blanked_code_span_is_not_a_finding() {
        assert!(excerpts("オプションは **      ** を渡す。").is_empty());
    }

    #[test]
    fn a_marker_in_a_code_span_is_not_a_finding() {
        assert!(excerpts("`/**` と `/**` の扱いを決める。").is_empty());
        assert!(excerpts("計算は `2**8` と `2**16` で行う。").is_empty());
    }
}
