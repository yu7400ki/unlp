use std::cmp::Reverse;
use std::iter::repeat_n;
use std::ops::Range;
use std::path::Path;

use ast_grep_core::matcher::KindMatcher;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{Language, Node};
use ast_grep_language::{LanguageExt, SupportLang};

use super::Lines;
use crate::document::{Document, Origin, Segment, SegmentKind};
use crate::sentence::is_japanese;

/// 構文木からコメントと文字列リテラルを取り出せる言語と、取り出すノード種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLang {
    lang: SupportLang,
    kinds: &'static [(&'static str, Face)],
}

impl SourceLang {
    /// パスの拡張子が指す言語。取り出すノード種別を定めていない言語は `None`。
    pub fn from_path(path: &Path) -> Option<Self> {
        let lang = SupportLang::from_path(path)?;
        Some(Self {
            lang,
            kinds: kinds(lang)?,
        })
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
    let SourceLang { lang, kinds } = lang;
    let root = lang.ast_grep(text);
    let mut parts = Vec::new();
    for (kind, face) in kinds {
        let matcher = KindMatcher::new(kind, lang);
        for node in root.root().find_all(&matcher) {
            parts.push(Part {
                range: node.range(),
                face: *face,
                kind: segment_kind(lang, &node, *face),
                body: body(&node, *face),
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

/// ノードから取り出す本文。
fn body(node: &Node<StrDoc<SupportLang>>, face: Face) -> String {
    match face {
        Face::Comment => comment_body(&node.text()),
        Face::StringLiteral => blank_placeholders(&literal_body(node)),
    }
}

/// 文字列リテラルの値。区切り文字を除き、エスケープを復元し、補間を同じバイト長の空白にする。
fn literal_body(node: &Node<StrDoc<SupportLang>>) -> String {
    let mut body = String::new();
    let mut continued = false;
    for child in node.children() {
        let continues = child.kind() == ESCAPE && is_continuation(&child.text());
        match child.kind().as_ref() {
            ESCAPE if continues => {}
            ESCAPE => body.push_str(&unescape(&child.text())),
            kind if is_content(kind) => {
                let value = content(&child);
                body.push_str(after_continuation(&value, continued));
            }
            "string_start" | "string_end" => {}
            _ if !child.is_named() => {}
            _ => blanks(&mut body, child.range().len()),
        }
        continued = continues;
    }
    body
}

/// 行を継続するエスケープか。
fn is_continuation(escape: &str) -> bool {
    escape
        .strip_prefix('\\')
        .is_some_and(|rest| rest.starts_with(['\n', '\r']))
}

/// 行を継続した後の値。続きの行の字下げを除く。
fn after_continuation(value: &str, continued: bool) -> &str {
    if continued { value.trim_start() } else { value }
}

/// エスケープを表すノードの種別。
const ESCAPE: &str = "escape_sequence";

/// 文字列リテラルの中身を表すノードの種別か。
fn is_content(kind: &str) -> bool {
    kind == "string_fragment" || kind.ends_with("_content")
}

/// 中身のノードの値。中のエスケープを復元し、他のノードを同じバイト長の空白にする。
fn content(node: &Node<StrDoc<SupportLang>>) -> String {
    let text = node.text();
    let start = node.range().start;
    let mut value = String::with_capacity(text.len());
    let mut at = 0;
    let mut continued = false;
    for child in node.children() {
        let range = child.range();
        let (from, to) = (range.start - start, range.end - start);
        value.push_str(after_continuation(&text[at..from], continued));
        continued = false;
        if child.kind() == ESCAPE {
            let escape = child.text();
            if is_continuation(&escape) {
                continued = true;
            } else {
                value.push_str(&unescape(&escape));
            }
        } else {
            blanks(&mut value, to - from);
        }
        at = to;
    }
    value.push_str(after_continuation(&text[at..], continued));
    value
}

/// エスケープが表す文字。文字を表さないものは空になる。
fn unescape(escape: &str) -> String {
    let mut chars = escape.chars();
    if chars.next() != Some('\\') {
        return escape.to_string();
    }
    match chars.next() {
        Some('n') => "\n".to_string(),
        Some('t') => "\t".to_string(),
        Some('r') => "\r".to_string(),
        Some('u' | 'U' | 'x') => code_point(chars.as_str()),
        Some('0') | Some('\n') | None => String::new(),
        Some(c) => c.to_string(),
    }
}

/// 16 進の符号位置が表す文字。
fn code_point(digits: &str) -> String {
    let digits = digits.trim_start_matches('{').trim_end_matches('}');
    u32::from_str_radix(digits, 16)
        .ok()
        .and_then(char::from_u32)
        .map(String::from)
        .unwrap_or_default()
}

/// プレースホルダーを同じバイト長の空白に置き換えた文字列。
fn blank_placeholders(text: &str) -> String {
    let mut blanked = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['{', '$', '%']) {
        blanked.push_str(&rest[..at]);
        rest = &rest[at..];
        let length = match placeholder(rest) {
            Some(length) => {
                blanks(&mut blanked, length);
                length
            }
            None => {
                let head = rest.chars().next().expect("探した文字がある");
                blanked.push(head);
                head.len_utf8()
            }
        };
        rest = &rest[length..];
    }
    blanked.push_str(rest);
    blanked
}

/// 先頭にあるプレースホルダーのバイト長。
fn placeholder(text: &str) -> Option<usize> {
    match text.strip_prefix('$') {
        Some(rest) => braced(rest).map(|length| length + 1),
        None => braced(text).or_else(|| conversion(text)),
    }
}

/// `{` で囲んだ差し込みのバイト長。括弧、空白、引用符を挟むものは差し込みにしない。
fn braced(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('{')?;
    let end = rest.find('}')?;
    let inside = &rest[..end];
    let plain =
        !inside.contains(['{', '}', '"', '\'', '`']) && !inside.contains(char::is_whitespace);
    plain.then_some(end + 2)
}

/// `%` で始まる変換指定のバイト長。
fn conversion(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('%')?;
    let letter = rest.trim_start_matches(|c: char| matches!(c, '-' | '+' | '#' | '.' | '0'..='9'));
    letter
        .starts_with(|c: char| c.is_ascii_alphabetic())
        .then_some(rest.len() - letter.len() + 2)
}

/// バイト長と同じ数の空白を足す。
fn blanks(text: &mut String, length: usize) {
    text.extend(repeat_n(' ', length));
}

/// コメントの記号を除いた本文。行をまたぐコメントは 1 行ずつ記号を除いて連ねる。
fn comment_body(text: &str) -> String {
    let inner = match text.strip_prefix("/*") {
        Some(inner) => {
            let inner = inner.strip_prefix('!').unwrap_or(inner);
            inner.strip_suffix("*/").unwrap_or(inner)
        }
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

/// 行を連ねる。本文の無い行は段落の区切りとして改行を残し、他の行は境目の両側が非 ASCII の
/// 文字なら詰め、そうでなければ空白 1 つを挟む。
fn join(body: &mut String, line: &str) {
    if line.is_empty() {
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        return;
    }
    let glued = body.is_empty()
        || body.ends_with('\n')
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
        let mut text = part.body;
        text.truncate(text.trim_end().len());
        segments.push(Segment {
            text,
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
            doc_or(
                ["///", "//!", "/**", "/*!"]
                    .iter()
                    .any(|marker| text.starts_with(marker)),
                SegmentKind::Comment,
            )
        }
        (SupportLang::TypeScript | SupportLang::Tsx | SupportLang::JavaScript, Face::Comment) => {
            doc_or(node.text().starts_with("/**"), SegmentKind::Comment)
        }
        (SupportLang::Python, Face::StringLiteral) => {
            doc_or(is_docstring(node), SegmentKind::StringLiteral)
        }
        (_, Face::Comment) => SegmentKind::Comment,
        (_, Face::StringLiteral) => SegmentKind::StringLiteral,
    }
}

/// モジュール、関数、クラスの先頭に置いた文字列か。
fn is_docstring(node: &Node<StrDoc<SupportLang>>) -> bool {
    let Some(statement) = node.parent() else {
        return false;
    };
    if statement.kind() != "expression_statement" || statement.prev().is_some() {
        return false;
    }
    match statement.parent() {
        Some(body) if body.kind() == "module" => true,
        Some(body) if body.kind() == "block" => body.parent().is_some_and(|owner| {
            matches!(
                owner.kind().as_ref(),
                "function_definition" | "class_definition"
            )
        }),
        _ => false,
    }
}

/// doc コメントであるか、そうでなければ与えた面。
fn doc_or(is_doc: bool, plain: SegmentKind) -> SegmentKind {
    if is_doc {
        SegmentKind::DocComment
    } else {
        plain
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
                "文字列だ。",
                "生の文字列だ。",
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
    fn a_comment_line_without_a_body_is_a_paragraph_break() {
        assert_eq!(
            texts("a.rs", "/// 窓が開く。\n///\n/// 値が減る。\nmod a {}\n"),
            ["窓が開く。\n値が減る。"]
        );
        assert_eq!(
            texts("a.rs", "/* 一段目だ。\n *\n * 二段目だ。\n */\nmod a {}\n"),
            ["一段目だ。\n二段目だ。"]
        );
        assert_eq!(
            texts("a.py", "# 一段目だ。\n#\n# 二段目だ。\n"),
            ["一段目だ。\n二段目だ。"]
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

    /// 記法と同じバイト数の空白。
    fn blanked(notation: &str) -> String {
        " ".repeat(notation.len())
    }

    #[test]
    fn a_string_inside_an_interpolation_stays_in_the_outer_segment() {
        assert_eq!(
            texts("a.ts", "const t = `外 ${\"内の文字列\"} 外`;\n"),
            [format!("外 {} 外", blanked("${\"内の文字列\"}"))]
        );
        assert_eq!(
            texts("a.py", "w = f\"{d['キー']} だ\"\n"),
            [format!("{} だ", blanked("{d['キー']}"))]
        );
    }

    #[test]
    fn a_string_keeps_its_value_without_the_delimiters() {
        assert_eq!(
            texts("a.rs", "fn f() { let s = \"一行目。\\n二行目。\"; }\n"),
            ["一行目。\n二行目。"]
        );
        assert_eq!(
            texts("a.rs", "fn f() { let s = \"引用の \\\"中\\\" だ。\"; }\n"),
            ["引用の \"中\" だ。"]
        );
        assert_eq!(
            texts("a.py", "s = '''三重の\n文字列だ。'''\n"),
            ["三重の\n文字列だ。"]
        );
        assert_eq!(
            texts("a.py", "s = r\"生の\\n文字列だ。\"\n"),
            ["生の\\n文字列だ。"]
        );
        assert_eq!(
            texts("a.go", "var x = \"符号位置の \\u3042 だ。\"\n"),
            ["符号位置の あ だ。"]
        );
    }

    #[test]
    fn a_line_that_continues_stays_one_sentence() {
        assert_eq!(
            texts(
                "a.rs",
                "fn f() { let s = \"改行を\\\n             続ける一文だ。\"; }\n"
            ),
            ["改行を続ける一文だ。"]
        );
        assert_eq!(
            texts("a.py", "s = \"改行を\\\n    続ける一文だ。\"\n"),
            ["改行を続ける一文だ。"]
        );
    }

    #[test]
    fn a_placeholder_becomes_blanks_of_the_same_length() {
        assert_eq!(
            texts("a.rs", "fn f() { format!(\"{name} を読み込めない\"); }\n"),
            [format!("{} を読み込めない", blanked("{name}"))]
        );
        assert_eq!(
            texts("a.rs", "fn f() { format!(\"{} と {:?} だ。\", 1, 2); }\n"),
            [format!("{} と {} だ。", blanked("{}"), blanked("{:?}"))]
        );
        assert_eq!(
            texts("a.py", "s = \"%s を %-3d 回だ。\"\n"),
            [format!("{} を {} 回だ。", blanked("%s"), blanked("%-3d"))]
        );
    }

    #[test]
    fn a_sign_that_opens_no_placeholder_stays_in_the_body() {
        assert_eq!(
            texts("a.rs", "fn f() { let s = \"100% の値だ。\"; }\n"),
            ["100% の値だ。"]
        );
        assert_eq!(
            texts(
                "a.rs",
                "fn f() { let s = \"{ 空白を挟む } のは差し込みでない。\"; }\n"
            ),
            ["{ 空白を挟む } のは差し込みでない。"]
        );
    }

    #[test]
    fn a_block_comment_of_rust_is_a_doc_when_it_opens_with_a_star_or_a_bang() {
        let source = concat!(
            "/*! 内側の doc だ。 */\n",
            "\n",
            "/** 外側の doc だ。 */\n",
            "mod a {}\n",
            "\n",
            "/* ただのブロックだ。 */\n",
            "mod b {}\n",
        );
        assert_eq!(
            texts("a.rs", source),
            ["内側の doc だ。", "外側の doc だ。", "ただのブロックだ。"]
        );
        assert_eq!(
            kinds_of("a.rs", source),
            [
                SegmentKind::DocComment,
                SegmentKind::DocComment,
                SegmentKind::Comment,
            ]
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
    fn a_string_at_the_head_of_a_body_is_a_doc_comment() {
        let source = concat!(
            "\"\"\"モジュールの説明だ。\"\"\"\n",
            "class C:\n",
            "    \"\"\"クラスの説明だ。\"\"\"\n",
            "def f(s):\n",
            "    \"\"\"関数の説明だ。\"\"\"\n",
            "    s = \"ただの文字列だ。\"\n",
            "    if s:\n",
            "        \"式の文字列だ。\"\n",
        );
        assert_eq!(
            kinds_of("a.py", source),
            [
                SegmentKind::DocComment,
                SegmentKind::DocComment,
                SegmentKind::DocComment,
                SegmentKind::StringLiteral,
                SegmentKind::StringLiteral,
            ]
        );
    }

    #[test]
    fn go_and_python_keep_their_own_kinds() {
        assert_eq!(
            texts(
                "a.go",
                "// 説明だ。\nvar x = \"値だ。\"\nvar y = `生の値だ。`\n"
            ),
            ["説明だ。", "値だ。", "生の値だ。"]
        );
        assert_eq!(
            texts("a.py", "# 説明だ。\ns = \"値だ。\"\n"),
            ["説明だ。", "値だ。"]
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
