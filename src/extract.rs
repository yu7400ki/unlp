use std::iter::repeat_n;
use std::num::NonZeroU32;
use std::ops::Range;

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

use crate::document::{Document, LineRange, Origin, Segment, SegmentKind};
use crate::sentence::{BOLD, is_japanese};

mod source;
pub use source::{SourceLang, source_document};

/// 標準入力から読んだ文書の名前。
pub const STDIN_NAME: &str = "<stdin>";

/// 強調の記法の長さ。`*` と `_` のいずれでも 1 バイト。
const EMPHASIS: usize = 1;

/// テキスト全体を 1 つの Prose Segment とする文書。`name` が Segment の位置の path になる。
pub fn text_document(name: String, text: &str) -> Document {
    let origin = Origin {
        path: name.clone(),
        lines: whole_text(text),
        commit: None,
    };
    Document {
        name,
        segments: vec![Segment {
            text: text.to_string(),
            origin,
            kind: SegmentKind::Prose,
        }],
    }
}

/// Markdown の本文を Segment とする文書。段落、見出し、箇条書きの項目、引用ブロックの本文を
/// Prose、表のセルを TableCell にし、原文の順に並べる。コードブロックは Segment にしない。
/// インラインコード、インライン HTML、リンクの記法と URL、画像、行頭の引用記号は、同じ
/// バイト数の空白に置き換えて文字の位置を保つ。表のセルでは強調の記法と矢印も置き換える。
pub fn markdown_document(name: String, text: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS);
    let mut blocks = Blocks::new(name, text);
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        blocks.step(event, range);
    }
    blocks.finish()
}

/// 日本語の文字を含む Segment を持つ文書だけを残す。
pub fn with_japanese(document: Document) -> Option<Document> {
    document
        .segments
        .iter()
        .any(|segment| segment.text.chars().any(is_japanese))
        .then_some(document)
}

/// 1 行目から最終行までの範囲。
fn whole_text(text: &str) -> LineRange {
    let count = u32::try_from(text.lines().count()).unwrap_or(u32::MAX);
    let last = NonZeroU32::new(count).unwrap_or(NonZeroU32::MIN);
    LineRange::new(NonZeroU32::MIN, last).expect("最終行は 1 行目を下回らない")
}

/// Markdown の走査の途中の状態。
struct Blocks<'a> {
    name: String,
    text: &'a str,
    lines: Lines,
    /// 開いているブロック。内側のものが末尾。
    open: Vec<Block>,
    /// コードブロックの内側か。
    code_block: bool,
    /// 閉じた順の Segment。本文の始まりを鍵に原文の順へ戻す。
    closed: Vec<(usize, Segment)>,
}

/// 開いているブロック。`body` は内側の要素から定めた本文の範囲。
struct Block {
    kind: SegmentKind,
    body: Option<Range<usize>>,
    /// 本文のうち空白に置き換える範囲。
    blanks: Vec<Range<usize>>,
    /// 次の本文までを空白にする位置。
    held: Option<usize>,
}

impl<'a> Blocks<'a> {
    fn new(name: String, text: &'a str) -> Self {
        Self {
            name,
            text,
            lines: Lines::new(text),
            open: Vec::new(),
            code_block: false,
            closed: Vec::new(),
        }
    }

    fn step(&mut self, event: Event, range: Range<usize>) {
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag, range),
            _ if self.code_block => {}
            Event::Text(_) => self.body(range),
            Event::SoftBreak | Event::HardBreak => {
                self.body(range.clone());
                self.hold(range.end);
            }
            Event::Code(_) | Event::InlineHtml(_) => self.blank(range),
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag, range: Range<usize>) {
        match tag {
            Tag::CodeBlock(_) => self.code_block = true,
            _ if self.code_block => {}
            Tag::Paragraph | Tag::Heading { .. } | Tag::Item => self.open(SegmentKind::Prose),
            Tag::TableCell => self.open(SegmentKind::TableCell),
            Tag::Strong => self.notation(range.start..range.start + BOLD.len()),
            Tag::Emphasis => self.notation(range.start..range.start + EMPHASIS),
            Tag::Image { .. } => self.blank(range),
            Tag::Link { link_type, .. } => {
                if matches!(link_type, LinkType::Autolink | LinkType::Email) {
                    self.blank(range.clone());
                }
                self.hold(range.start);
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd, range: Range<usize>) {
        match tag {
            TagEnd::CodeBlock => self.code_block = false,
            _ if self.code_block => {}
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::TableCell => {
                self.close();
            }
            TagEnd::Strong => self.notation(range.end - BOLD.len()..range.end),
            TagEnd::Emphasis => self.notation(range.end - EMPHASIS..range.end),
            TagEnd::Link => self.blank_after_body(range.end),
            _ => {}
        }
    }

    fn open(&mut self, kind: SegmentKind) {
        self.open.push(Block {
            kind,
            body: None,
            blanks: Vec::new(),
            held: None,
        });
    }

    /// 開いているブロックを閉じ、空白でない本文があれば Segment にする。
    fn close(&mut self) {
        let Some(block) = self.open.pop() else {
            return;
        };
        let Some(body) = block.body else {
            return;
        };
        let mut blanks = block.blanks;
        if block.kind == SegmentKind::TableCell {
            blanks.extend(arrows(self.text, &body));
        }
        let text = blanked(self.text, &body, blanks);
        if text.trim().is_empty() {
            return;
        }
        let segment = Segment {
            text,
            origin: Origin {
                path: self.name.clone(),
                lines: self.lines.of(&body),
                commit: None,
            },
            kind: block.kind,
        };
        self.closed.push((body.start, segment));
    }

    /// 本文の範囲を広げる。保留した位置があれば、そこから本文の始まりまでを空白にする。
    fn body(&mut self, range: Range<usize>) {
        let Some(block) = self.open.last_mut() else {
            return;
        };
        if let Some(held) = block.held.take()
            && held < range.start
        {
            block.blanks.push(held..range.start);
        }
        block.body = Some(match block.body.take() {
            Some(body) => body.start.min(range.start)..body.end.max(range.end),
            None => range,
        });
    }

    fn blank(&mut self, range: Range<usize>) {
        if let Some(block) = self.open.last_mut() {
            block.blanks.push(range);
        }
    }

    /// 強調の記法の範囲。表のセルでは空白に置き換える。
    fn notation(&mut self, range: Range<usize>) {
        self.body(range.clone());
        if self.in_cell() {
            self.blank(range);
        }
    }

    fn in_cell(&self) -> bool {
        self.open
            .last()
            .is_some_and(|block| block.kind == SegmentKind::TableCell)
    }

    /// 本文の終わりから `end` までを空白にする。
    fn blank_after_body(&mut self, end: usize) {
        if let Some(block) = self.open.last_mut()
            && let Some(body) = &block.body
            && body.end < end
        {
            block.blanks.push(body.end..end);
        }
    }

    fn hold(&mut self, at: usize) {
        if let Some(block) = self.open.last_mut() {
            block.held.get_or_insert(at);
        }
    }

    fn finish(mut self) -> Document {
        self.closed.sort_by_key(|(at, _)| *at);
        Document {
            name: self.name,
            segments: self
                .closed
                .into_iter()
                .map(|(_, segment)| segment)
                .collect(),
        }
    }
}

/// 範囲に現れる矢印の範囲。
fn arrows(text: &str, body: &Range<usize>) -> Vec<Range<usize>> {
    text[body.clone()]
        .match_indices(crate::rule::ARROWS)
        .map(|(at, arrow)| body.start + at..body.start + at + arrow.len())
        .collect()
}

/// 範囲の原文を切り出し、`blanks` の各範囲を同じバイト数の空白に置き換えた文字列。
fn blanked(text: &str, body: &Range<usize>, mut blanks: Vec<Range<usize>>) -> String {
    blanks.sort_by_key(|blank| blank.start);
    let mut blanked = String::with_capacity(body.len());
    let mut at = body.start;
    for blank in blanks {
        let start = blank.start.max(at);
        let end = blank.end.min(body.end);
        if start >= end {
            continue;
        }
        blanked.push_str(&text[at..start]);
        blanked.extend(repeat_n(' ', end - start));
        at = end;
    }
    blanked.push_str(&text[at..body.end]);
    blanked
}

/// 原文の各行の始まりのバイト位置。
struct Lines(Vec<usize>);

impl Lines {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(at, _)| at + 1),
        );
        Self(starts)
    }

    /// バイト範囲が跨る行の範囲。
    fn of(&self, bytes: &Range<usize>) -> LineRange {
        let last = bytes.end.saturating_sub(1).max(bytes.start);
        LineRange::new(self.number(bytes.start), self.number(last))
            .expect("終了行は開始行を下回らない")
    }

    fn number(&self, at: usize) -> NonZeroU32 {
        let count = self.0.partition_point(|start| *start <= at);
        let count = u32::try_from(count).unwrap_or(u32::MAX);
        NonZeroU32::new(count).unwrap_or(NonZeroU32::MIN)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use super::*;
    use crate::morph::Analyzer;
    use crate::rule::Context;
    use crate::settings::Settings;

    static ANALYZER: LazyLock<Analyzer> = LazyLock::new(|| Analyzer::new().unwrap());

    /// 規則集の見出しの下に置く検出例と、それを数える規則。
    const EXAMPLES: [(&str, [&str; 2]); 3] = [
        ("S02", ["これは", "それらは"]),
        ("L03", ["失敗し始める", "一杯になった瞬間"]),
        ("S05", ["だけでなく", "静的ではなく"]),
    ];

    fn document(markdown: &str) -> Document {
        markdown_document("t".to_string(), markdown)
    }

    /// Markdown を抽出して規則を適用し、指摘の規則 ID を返す。
    fn rules(markdown: &str) -> Vec<String> {
        let document = document(markdown);
        let sentences = ANALYZER.analyze_document(&document);
        let settings = Settings::default();
        let context = Context::for_document(&sentences, &settings);
        crate::rule::check(&sentences, &context, settings.floor())
            .iter()
            .map(|finding| finding.rule().to_string())
            .collect()
    }

    fn texts(markdown: &str) -> Vec<String> {
        document(markdown)
            .segments
            .into_iter()
            .map(|segment| segment.text)
            .collect()
    }

    fn kinds(markdown: &str) -> Vec<SegmentKind> {
        document(markdown)
            .segments
            .iter()
            .map(|segment| segment.kind)
            .collect()
    }

    #[test]
    fn the_segment_spans_every_line() {
        let document = text_document("t".to_string(), "一行目。\n二行目。\n三行目。");
        assert_eq!(document.segments.len(), 1);
        let lines = document.segments[0].origin.lines;
        assert_eq!(lines.start().get(), 1);
        assert_eq!(lines.end().get(), 3);
    }

    #[test]
    fn an_empty_text_spans_the_first_line() {
        let document = text_document("t".to_string(), "");
        let lines = document.segments[0].origin.lines;
        assert_eq!(lines.start().get(), 1);
        assert_eq!(lines.end().get(), 1);
    }

    #[test]
    fn a_paragraph_and_a_heading_are_prose_segments() {
        assert_eq!(
            texts("# 見出しだ\n\n段落の本文だ。\n続く行だ。\n"),
            ["見出しだ", "段落の本文だ。\n続く行だ。"]
        );
        assert_eq!(kinds("# 見出しだ\n\n段落だ。\n"), [SegmentKind::Prose; 2]);
    }

    #[test]
    fn an_item_drops_its_marker_and_nests_on_its_own() {
        assert_eq!(
            texts("- 項目の一つ。\n- 項目の二つ。\n  - 入れ子の項目。\n"),
            ["項目の一つ。", "項目の二つ。", "入れ子の項目。"]
        );
        assert_eq!(
            texts("1. 一つ目だ。\n2. 二つ目だ。\n"),
            ["一つ目だ。", "二つ目だ。"]
        );
        assert_eq!(texts("1. **結論です。**\n"), ["**結論です。**"]);
    }

    #[test]
    fn a_task_list_marker_is_not_part_of_the_body() {
        assert_eq!(
            texts("- [ ] これは重要だ。\n- [x] 済みだ。\n"),
            ["これは重要だ。", "済みだ。"]
        );
        assert_eq!(rules("- [ ] これは重要だ。"), ["S02"]);
    }

    #[test]
    fn a_block_quote_drops_the_marker_of_its_lines() {
        assert_eq!(
            texts("> 引用の一行目。\n> 引用の二行目。\n"),
            ["引用の一行目。\n  引用の二行目。"]
        );
    }

    #[test]
    fn a_table_cell_is_its_own_segment() {
        let markdown = "| 見出し | 説明 |\n|---|---|\n| 語 | 文だ。 |\n";
        assert_eq!(texts(markdown), ["見出し", "説明", "語", "文だ。"]);
        assert_eq!(kinds(markdown), [SegmentKind::TableCell; 4]);
    }

    #[test]
    fn a_table_cell_drops_the_notation_of_bold_and_its_arrows() {
        let markdown = "| 語 | 説明 |\n|---|---|\n| **重要だ** | 入力 → 出力だ。 |\n";
        let cells = texts(markdown);
        assert_eq!(cells[2], format!("{}重要だ{}", blanks(BOLD), blanks(BOLD)));
        assert_eq!(cells[3], format!("入力 {} 出力だ。", blanks("→")));
        assert!(rules(markdown).is_empty(), "{:?}", rules(markdown));
        assert_eq!(
            texts("**重要だ**と入力 → 出力。\n"),
            ["**重要だ**と入力 → 出力。"]
        );
    }

    #[test]
    fn a_code_block_is_not_a_segment() {
        assert_eq!(
            texts("段落だ。\n\n```rust\nコードの文だ。\n```\n\n    字下げのコードだ。\n"),
            ["段落だ。"]
        );
        assert_eq!(
            texts("- 項目だ。\n\n  ```\n  コードの文だ。\n  ```\n"),
            ["項目だ。"]
        );
    }

    #[test]
    fn an_html_block_is_not_a_segment() {
        assert_eq!(
            texts("<div>HTML の中だ。</div>\n\n段落だ。\n"),
            ["段落だ。"]
        );
    }

    /// 記法と同じバイト数の空白。
    fn blanks(notation: &str) -> String {
        " ".repeat(notation.len())
    }

    #[test]
    fn a_code_span_keeps_the_place_of_the_text_around_it() {
        assert_eq!(
            texts("前に `コードの文。` と続く。\n"),
            [format!("前に {} と続く。", blanks("`コードの文。`"))]
        );
        assert!(texts("`コードだけ。`\n").is_empty());
    }

    #[test]
    fn a_link_keeps_its_text_without_the_url() {
        assert_eq!(
            texts("前に [リンクの文字](https://example.com/道) と続く。\n"),
            [format!(
                "前に {}リンクの文字{} と続く。",
                blanks("["),
                blanks("](https://example.com/道)")
            )]
        );
        assert_eq!(
            texts("自動リンク <https://example.com/道> だ。\n"),
            [format!(
                "自動リンク {} だ。",
                blanks("<https://example.com/道>")
            )]
        );
    }

    #[test]
    fn an_image_is_not_part_of_the_body() {
        assert_eq!(
            texts("前に ![画像の説明](img.png) と続く。\n"),
            [format!(
                "前に {} と続く。",
                blanks("![画像の説明](img.png)")
            )]
        );
        assert!(texts("![画像の説明](img.png)\n").is_empty());
    }

    #[test]
    fn the_notation_of_bold_survives_a_link_inside_it() {
        let text = &texts("**[太字の文字](https://example.com/道)**\n")[0];
        assert!(text.starts_with("**"), "{text}");
        assert!(text.ends_with("**"), "{text}");
        let bold = crate::sentence::bold(text);
        assert_eq!(bold.len(), 1, "{text}");
        assert_eq!(
            crate::sentence::inside_bold(text, &bold[0]).trim(),
            "太字の文字"
        );
    }

    #[test]
    fn a_code_span_keeps_an_example_out_of_the_findings() {
        for (rule, examples) in EXAMPLES {
            let line = format!("検出する例: `{}`、`{}`", examples[0], examples[1]);
            assert!(rules(&line).is_empty(), "{line}");
            for example in examples {
                assert_eq!(rules(example), [rule], "{example}");
            }
        }
    }

    #[test]
    fn the_lines_hold_the_place_of_the_segment() {
        let document = document("# 見出しだ\n\n段落の一行目。\n段落の二行目。\n\n- 項目だ。\n");
        let lines: Vec<(u32, u32)> = document
            .segments
            .iter()
            .map(|segment| {
                (
                    segment.origin.lines.start().get(),
                    segment.origin.lines.end().get(),
                )
            })
            .collect();
        assert_eq!(lines, [(1, 1), (3, 4), (6, 6)]);
        assert_eq!(document.segments[0].origin.path, "t");
    }
}
