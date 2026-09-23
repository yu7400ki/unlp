use std::cmp::Reverse;
use std::ops::Range;
use std::path::Path;

use ast_grep_core::matcher::KindMatcher;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{Language, Node};
use ast_grep_language::{LanguageExt, SupportLang};

use super::Lines;
use crate::document::{Document, Origin, Segment, SegmentKind};
use crate::sentence::is_japanese;

/// 構文木からコメントと文字列リテラルを取り出せる言語。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLang(SupportLang);

impl SourceLang {
    /// パスの拡張子が指す言語。取り出すノード種別を定めていない言語は `None`。
    pub fn from_path(path: &Path) -> Option<Self> {
        let lang = SupportLang::from_path(path)?;
        kinds(lang).is_some().then_some(Self(lang))
    }
}

/// 構文木から取り出すノードの面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    Comment,
    StringLiteral,
}

/// 言語ごとの、日本語を取り出すノード種別。
fn kinds(lang: SupportLang) -> Option<&'static [(&'static str, Face)]> {
    use Face::{Comment, StringLiteral};
    Some(match lang {
        SupportLang::Rust => &[
            ("line_comment", Comment),
            ("block_comment", Comment),
            ("string_literal", StringLiteral),
            ("raw_string_literal", StringLiteral),
        ],
        SupportLang::Python => &[("comment", Comment), ("string", StringLiteral)],
        SupportLang::TypeScript | SupportLang::Tsx | SupportLang::JavaScript => &[
            ("comment", Comment),
            ("string", StringLiteral),
            ("template_string", StringLiteral),
        ],
        SupportLang::Go => &[
            ("comment", Comment),
            ("interpreted_string_literal", StringLiteral),
            ("raw_string_literal", StringLiteral),
        ],
        _ => return None,
    })
}

/// ソースコードのコメントと文字列リテラルを Segment とする文書。日本語を含まない Segment は
/// 持たない。
pub fn source_document(name: String, text: &str, lang: SourceLang) -> Document {
    let SourceLang(lang) = lang;
    let root = lang.ast_grep(text);
    let mut parts = Vec::new();
    for (kind, face) in kinds(lang).unwrap_or_default() {
        let matcher = KindMatcher::new(kind, lang);
        for node in root.root().find_all(&matcher) {
            parts.push(Part {
                range: node.range(),
                face: *face,
                kind: segment_kind(lang, &node, *face),
                body: body(&node.text(), *face),
            });
        }
    }
    let lines = Lines::new(text);
    Document {
        name: name.clone(),
        segments: segments(merged(outermost(parts), &lines), &name, &lines),
    }
}

/// Segment になる前の 1 つのノード。
struct Part {
    range: Range<usize>,
    face: Face,
    kind: SegmentKind,
    body: String,
}

/// ノードの文字列から取り出す本文。
fn body(text: &str, face: Face) -> String {
    match face {
        Face::Comment => comment_body(text),
        Face::StringLiteral => text.to_string(),
    }
}

/// コメントの記号を除いた本文。行をまたぐコメントは 1 行ずつ記号を除いて連ねる。
fn comment_body(text: &str) -> String {
    let inner = match text.strip_prefix("/*") {
        Some(inner) => inner.strip_suffix("*/").unwrap_or(inner),
        None => text,
    };
    let mut body = String::with_capacity(inner.len());
    for line in inner.lines() {
        join(&mut body, strip_marker(line.trim()).trim());
    }
    body
}

/// 行の先頭のコメントの記号を除いた部分。
fn strip_marker(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix("//").or_else(|| line.strip_prefix('#')) {
        let rest = rest.trim_start_matches(['/', '#']);
        return rest.strip_prefix('!').unwrap_or(rest);
    }
    match line.strip_prefix('*') {
        Some(rest) if rest.is_empty() || rest.starts_with(' ') => rest,
        _ => line,
    }
}

/// 行を連ねる。境目の両側が非 ASCII の文字なら詰め、そうでなければ空白 1 つを挟む。
fn join(body: &mut String, line: &str) {
    if line.is_empty() {
        return;
    }
    let glued = body.is_empty()
        || (body.ends_with(|c: char| !c.is_ascii()) && line.starts_with(|c: char| !c.is_ascii()));
    if !glued {
        body.push(' ');
    }
    body.push_str(line);
}

/// 連続する行の同じ面のコメントを 1 つに統合する。
fn merged(parts: Vec<Part>, lines: &Lines) -> Vec<Part> {
    let mut merged: Vec<Part> = Vec::new();
    for part in parts {
        match merged.last_mut() {
            Some(last) if continues(last, &part, lines) => {
                join(&mut last.body, &part.body);
                last.range.end = part.range.end;
            }
            _ => merged.push(part),
        }
    }
    merged
}

/// 前のコメントの次の行から続くコメントか。
fn continues(last: &Part, part: &Part, lines: &Lines) -> bool {
    last.face == Face::Comment
        && part.face == Face::Comment
        && last.kind == part.kind
        && lines.of(&part.range).start().get() == lines.of(&last.range).end().get() + 1
}

/// 原文の順に並べ、他のノードの内側にあるものを除く。文字列の補間の中の文字列は、外側の
/// ノードが持つ。
fn outermost(mut parts: Vec<Part>) -> Vec<Part> {
    parts.sort_by_key(|part| (part.range.start, Reverse(part.range.end)));
    let mut covered = 0;
    parts.retain(|part| {
        let outer = part.range.end > covered;
        covered = covered.max(part.range.end);
        outer
    });
    parts
}

/// Part を Segment にする。日本語を含まないものは返さない。
fn segments(parts: Vec<Part>, name: &str, lines: &Lines) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    for part in parts {
        segments.push(Segment {
            text: part.body,
            origin: Origin {
                path: name.to_string(),
                lines: lines.of(&part.range),
                commit: None,
            },
            kind: part.kind,
        });
    }
    segments.retain(|segment| segment.text.chars().any(is_japanese));
    segments
}

/// ノードを取り出す面。コメントと Python の docstring は doc コメントかを判別する。
fn segment_kind(lang: SupportLang, node: &Node<StrDoc<SupportLang>>, face: Face) -> SegmentKind {
    match (lang, face) {
        (SupportLang::Rust, Face::Comment) => {
            let text = node.text();
            doc_or_comment(text.starts_with("///") || text.starts_with("//!"))
        }
        (SupportLang::TypeScript | SupportLang::Tsx | SupportLang::JavaScript, Face::Comment) => {
            doc_or_comment(node.text().starts_with("/**"))
        }
        (_, Face::Comment) => SegmentKind::Comment,
        (_, Face::StringLiteral) => SegmentKind::StringLiteral,
    }
}

fn doc_or_comment(is_doc: bool) -> SegmentKind {
    if is_doc {
        SegmentKind::DocComment
    } else {
        SegmentKind::Comment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(name: &str, text: &str) -> Document {
        let lang = SourceLang::from_path(Path::new(name)).expect("対象の言語");
        source_document(name.to_string(), text, lang)
    }

    fn texts(name: &str, text: &str) -> Vec<String> {
        document(name, text)
            .segments
            .into_iter()
            .map(|segment| segment.text)
            .collect()
    }

    fn kinds_of(name: &str, text: &str) -> Vec<SegmentKind> {
        document(name, text)
            .segments
            .iter()
            .map(|segment| segment.kind)
            .collect()
    }

    #[test]
    fn the_extension_chooses_the_language() {
        for name in ["a.rs", "a.py", "a.ts", "a.tsx", "a.js", "a.jsx", "a.go"] {
            assert!(SourceLang::from_path(Path::new(name)).is_some(), "{name}");
        }
        for name in ["a.md", "a.txt", "a.toml", "a.json", "a", "a.java"] {
            assert!(SourceLang::from_path(Path::new(name)).is_none(), "{name}");
        }
    }

    #[test]
    fn rust_comments_and_strings_are_segments() {
        let source = concat!(
            "//! 内側の doc だ。\n",
            "\n",
            "/// 外側の doc を\n",
            "/// 二行に分ける。\n",
            "// 行のコメントだ。\n",
            "/* ブロックのコメントだ。 */\n",
            "fn 名前() {\n",
            "    let s = \"文字列だ。\";\n",
            "    let r = r#\"生の文字列だ。\"#;\n",
            "}\n",
        );
        assert_eq!(
            texts("a.rs", source),
            [
                "内側の doc だ。",
                "外側の doc を二行に分ける。",
                "行のコメントだ。ブロックのコメントだ。",
                "\"文字列だ。\"",
                "r#\"生の文字列だ。\"#",
            ]
        );
        assert_eq!(
            kinds_of("a.rs", source),
            [
                SegmentKind::DocComment,
                SegmentKind::DocComment,
                SegmentKind::Comment,
                SegmentKind::StringLiteral,
                SegmentKind::StringLiteral,
            ]
        );
    }

    #[test]
    fn a_comment_of_several_lines_becomes_one_segment() {
        assert_eq!(
            texts("a.rs", "/* 一行目だ。\n * 二行目だ。\n */\n"),
            ["一行目だ。二行目だ。"]
        );
        assert_eq!(
            texts(
                "a.py",
                "# 一行目だ。\n# 二行目だ。\ns = 1\n# 離れた行だ。\n"
            ),
            ["一行目だ。二行目だ。", "離れた行だ。"]
        );
    }

    #[test]
    fn a_line_break_beside_an_ascii_word_keeps_a_blank() {
        assert_eq!(
            texts("a.rs", "/// 型の名前は\n/// Segment だ。\n"),
            ["型の名前は Segment だ。"]
        );
    }

    #[test]
    fn the_notation_of_bold_survives_the_comment_marker() {
        assert_eq!(texts("a.rs", "/// **結論です。**\n"), ["**結論です。**"]);
    }

    #[test]
    fn a_string_inside_an_interpolation_stays_in_the_outer_segment() {
        assert_eq!(
            texts("a.ts", "const t = `外 ${\"内の文字列\"} 外`;\n"),
            ["`外 ${\"内の文字列\"} 外`"]
        );
        assert_eq!(
            texts("a.py", "w = f\"{d['キー']} だ\"\n"),
            ["f\"{d['キー']} だ\""]
        );
    }

    #[test]
    fn a_comment_of_a_script_is_a_doc_when_it_opens_with_two_stars() {
        assert_eq!(
            kinds_of("a.ts", "/** doc だ。 */\n// 行だ。\n"),
            [SegmentKind::DocComment, SegmentKind::Comment]
        );
    }

    #[test]
    fn go_and_python_keep_their_own_kinds() {
        assert_eq!(
            texts(
                "a.go",
                "// 説明だ。\nvar x = \"値だ。\"\nvar y = `生の値だ。`\n"
            ),
            ["説明だ。", "\"値だ。\"", "`生の値だ。`"]
        );
        assert_eq!(
            texts("a.py", "# 説明だ。\ns = \"値だ。\"\n"),
            ["説明だ。", "\"値だ。\""]
        );
    }

    #[test]
    fn the_lines_hold_the_place_of_the_segment() {
        let document = document("a.rs", "fn f() {\n    // 二行目のコメントだ。\n}\n");
        let lines = document.segments[0].origin.lines;
        assert_eq!((lines.start().get(), lines.end().get()), (2, 2));
        assert_eq!(document.segments[0].origin.path, "a.rs");
    }

    #[test]
    fn a_node_without_japanese_is_not_a_segment() {
        assert!(texts("a.rs", "// no japanese\nfn f() { let s = \"ascii\"; }\n").is_empty());
    }
}
