use std::ops::Range;

use crate::document::{Document, Segment};
use crate::token::Token;

/// 太字の記法。
pub(crate) const BOLD: &str = "**";

/// Segment から切り出した 1 文。`byte_range` は Segment の文字列の中の位置。
#[derive(Debug, Clone)]
pub struct Sentence<'a> {
    segment: &'a Segment,
    byte_range: Range<usize>,
    tokens: Vec<Token>,
}

impl<'a> Sentence<'a> {
    pub fn segment(&self) -> &'a Segment {
        self.segment
    }

    pub fn byte_range(&self) -> Range<usize> {
        self.byte_range.clone()
    }

    /// 文の文字列。
    pub fn text(&self) -> &'a str {
        &self.segment.text[self.byte_range.clone()]
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub(crate) fn set_tokens(&mut self, tokens: Vec<Token>) {
        self.tokens = tokens;
    }

    /// 文に含まれる日本語の文字数。
    pub fn ja_chars(&self) -> usize {
        self.text().chars().filter(|c| is_japanese(*c)).count()
    }
}

/// ひらがな、カタカナ、漢字、繰り返し記号の々と〆のいずれかであるか。
pub fn is_japanese(c: char) -> bool {
    matches!(c, '\u{3005}' | '\u{3006}' | '\u{3041}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}' | '\u{4e00}'..='\u{9fff}')
}

/// 文書のすべての Segment を文に分割する。
pub(crate) fn split_document(document: &Document) -> Vec<Sentence<'_>> {
    document.segments.iter().flat_map(split_sentences).collect()
}

/// 文の日本語文字数の合計。
pub fn ja_chars(sentences: &[Sentence]) -> usize {
    sentences.iter().map(Sentence::ja_chars).sum()
}

/// Segment の文字列を `。！？` と改行で分割する。鉤括弧・丸括弧・バッククォート・
/// 太字の内側では分割せず、閉じていないものは空行で解消する。日本語の文字を含まない文は
/// 返さない。
pub(crate) fn split_sentences(segment: &Segment) -> Vec<Sentence<'_>> {
    let text = &segment.text;
    let mut sentences = Vec::new();
    let mut closers: Vec<char> = Vec::new();
    let mut in_code_span = false;
    let mut in_bold = false;
    let mut after_marker = false;
    let mut start = 0;
    let mut line_start = 0;
    for (index, c) in text.char_indices() {
        if after_marker {
            after_marker = false;
            continue;
        }
        if c == '\n' {
            let blank_line = text[line_start..index].trim().is_empty();
            line_start = index + 1;
            if blank_line {
                closers.clear();
                in_code_span = false;
                in_bold = false;
            } else if !closers.is_empty() || in_code_span || in_bold {
                continue;
            }
            push_sentence(&mut sentences, segment, start..index);
            start = index + 1;
            continue;
        }
        match c {
            '`' => in_code_span = !in_code_span,
            _ if in_code_span => {}
            '*' if text[index..].starts_with(BOLD) => {
                in_bold = !in_bold;
                after_marker = true;
            }
            _ if in_bold => {}
            '「' => closers.push('」'),
            '『' => closers.push('』'),
            '（' => closers.push('）'),
            _ if closers.last() == Some(&c) => {
                closers.pop();
            }
            _ if !closers.is_empty() => {}
            '。' | '！' | '？' => {
                let end = index + c.len_utf8();
                push_sentence(&mut sentences, segment, start..end);
                start = end;
            }
            _ => {}
        }
    }
    push_sentence(&mut sentences, segment, start..text.len());
    sentences
}

fn push_sentence<'a>(sentences: &mut Vec<Sentence<'a>>, segment: &'a Segment, range: Range<usize>) {
    let trimmed = trim(&segment.text, range);
    if !segment.text[trimmed.clone()].chars().any(is_japanese) {
        return;
    }
    sentences.push(Sentence {
        segment,
        byte_range: trimmed,
        tokens: Vec::new(),
    });
}

fn trim(text: &str, range: Range<usize>) -> Range<usize> {
    let slice = &text[range.clone()];
    let start = range.start + (slice.len() - slice.trim_start().len());
    let trimmed = slice.trim();
    start..start + trimmed.len()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::document::{LineRange, Origin, SegmentKind};

    fn segment(text: &str) -> Segment {
        Segment {
            text: text.to_string(),
            origin: Origin {
                path: "t".to_string(),
                lines: LineRange::new(NonZeroU32::MIN, NonZeroU32::MIN).unwrap(),
                commit: None,
            },
            kind: SegmentKind::Prose,
        }
    }

    fn texts(text: &str) -> Vec<String> {
        let segment = segment(text);
        split_sentences(&segment)
            .iter()
            .map(|sentence| sentence.text().to_string())
            .collect()
    }

    #[test]
    fn splits_on_terminators() {
        assert_eq!(
            texts("型が名乗る。本当か！ そうか？"),
            ["型が名乗る。", "本当か！", "そうか？"]
        );
    }

    #[test]
    fn splits_on_newlines() {
        assert_eq!(texts("一つ目\r\n二つ目\n"), ["一つ目", "二つ目"]);
    }

    #[test]
    fn keeps_brackets_whole() {
        assert_eq!(
            texts("彼は「行く。\n帰る。」と言った。次だ。"),
            ["彼は「行く。\n帰る。」と言った。", "次だ。"]
        );
        assert_eq!(
            texts("「外『内。』外。」終わり。"),
            ["「外『内。』外。」終わり。"]
        );
        assert_eq!(texts("（補足。）続く。"), ["（補足。）続く。"]);
    }

    #[test]
    fn keeps_code_spans_whole() {
        assert_eq!(texts("`a。b` は識別子だ。"), ["`a。b` は識別子だ。"]);
    }

    #[test]
    fn keeps_bold_whole() {
        assert_eq!(
            texts("**意図したものです。**エラーが出る。"),
            ["**意図したものです。**エラーが出る。"]
        );
        assert_eq!(texts("太字の**強調**だ。"), ["太字の**強調**だ。"]);
    }

    #[test]
    fn a_blank_line_closes_what_is_left_open() {
        assert_eq!(texts("彼は「行く。\n\n次だ。"), ["彼は「行く。", "次だ。"]);
        assert_eq!(texts("`コード\n\n文だ。"), ["`コード", "文だ。"]);
        assert_eq!(texts("**太字。\n\n文だ。"), ["**太字。", "文だ。"]);
    }

    #[test]
    fn counts_repetition_marks_as_japanese() {
        let segment = segment("日々の時々。");
        assert_eq!(ja_chars(&split_sentences(&segment)), 5);
        assert_eq!(ja_chars(&split_sentences(&self::segment("〆だ。"))), 2);
    }

    #[test]
    fn discards_sentences_without_japanese() {
        assert_eq!(texts("abc def.\n---\n日本語だ。"), ["日本語だ。"]);
    }

    #[test]
    fn counts_only_japanese_chars() {
        let segment = segment("型の doc が名乗る。設定を比べると動作が変わる。");
        let sentences = split_sentences(&segment);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].ja_chars(), 6);
        assert_eq!(ja_chars(&sentences), 19);
    }

    #[test]
    fn splits_every_segment_of_the_document() {
        let document = Document {
            name: "t".to_string(),
            segments: vec![segment("一つ目。"), segment("二つ目。三つ目。")],
        };
        assert_eq!(split_document(&document).len(), 3);
    }
}
