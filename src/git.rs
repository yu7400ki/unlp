use std::num::NonZeroU32;
use std::path::Path;
use std::process::Command;
use std::{io, result};

use thiserror::Error;

use crate::document::{Document, LineRange, Origin, Segment, SegmentKind};
use crate::extract;
use crate::input::{self, Reading};

/// コミットメッセージから取り出した Segment の位置の path。
const COMMIT_PATH: &str = "<commit>";

/// メッセージのファイルから取り出した Segment の位置の path。
const MESSAGE_PATH: &str = "<commit-msg>";

/// 範囲を持たない指定で差分の内容を読むリビジョン。
const HEAD: &str = "HEAD";

/// 文書の名前に載せるハッシュの桁数。
const SHORT_HASH: usize = 7;

/// 文書の名前に載せる題名の文字数。
const SHORT_SUBJECT: usize = 20;

/// git の実行と出力の解析で生じる誤り。
#[derive(Debug, Error)]
pub enum Error {
    #[error("git を実行できない")]
    Spawn(#[source] io::Error),
    #[error("git {command} が失敗した: {message}")]
    Failed { command: String, message: String },
    #[error("git {command} の出力が UTF-8 で符号化されていない")]
    NotUtf8 { command: String },
}

pub type Result<T> = result::Result<T, Error>;

/// 採点するコミットの範囲。
#[derive(Debug, Clone)]
pub enum CommitRange {
    /// 直近の件数。
    Last(u32),
    /// git の範囲の指定。
    Spec(String),
}

impl CommitRange {
    fn spec(&self) -> String {
        match self {
            Self::Last(count) => format!("HEAD~{count}..HEAD"),
            Self::Spec(spec) => spec.clone(),
        }
    }
}

/// 範囲のコミットを 1 件 1 文書として読み込む。日本語を含まないコミットは文書にしない。
pub fn commit_documents(range: &CommitRange) -> Result<Vec<Document>> {
    let output = text(&["log", "--format=%H%x00%B%x00", &range.spec()])?;
    let mut documents = Vec::new();
    for (hash, message) in log_records(&output) {
        let segments = message_segments(message, COMMIT_PATH, Some(hash));
        let document = Document {
            name: commit_name(hash, &segments),
            segments,
        };
        documents.extend(extract::with_japanese(document));
    }
    Ok(documents)
}

/// 書きかけのコミットメッセージを 1 つの文書にする。日本語を含まなければ文書にしない。
pub fn message_document(message: &str) -> Option<Document> {
    let document = Document {
        name: MESSAGE_PATH.to_string(),
        segments: message_segments(message, MESSAGE_PATH, None),
    };
    extract::with_japanese(document)
}

/// 採点する差分の面。
#[derive(Debug, Clone)]
pub enum Diff {
    /// 索引に載せた変更。
    Staged,
    /// git の範囲の指定。
    Range(String),
}

/// 差分が追加・変更した行に触れる Segment だけを持つ、ファイルごとの文書。索引または範囲の
/// 右端のリビジョンにあるファイル全体を抽出し、触れていない Segment を落とす。抽出の書式を
/// 定めていない種類と、UTF-8 で符号化されていないファイルは飛ばす。
pub fn diff_documents(diff: &Diff) -> Result<Vec<Document>> {
    let mut args = vec![
        "diff",
        "-U0",
        "--diff-filter=AM",
        "--no-color",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ];
    let rev = match diff {
        Diff::Staged => {
            args.push("--cached");
            None
        }
        Diff::Range(spec) => {
            args.push(spec);
            Some(right_rev(spec))
        }
    };
    let output = text(&args)?;
    let commit = match &rev {
        Some(rev) => Some(text(&["rev-parse", rev])?.trim().to_string()),
        None => None,
    };
    let mut documents = Vec::new();
    for change in changes(&output) {
        documents.extend(change_document(&change, rev.as_deref(), commit.as_deref())?);
    }
    Ok(documents)
}

/// 範囲の右端のリビジョン。範囲でなければ `HEAD`。
fn right_rev(spec: &str) -> String {
    match spec.rsplit_once("..") {
        Some((_, right)) if !right.is_empty() => right.to_string(),
        _ => HEAD.to_string(),
    }
}

/// 差分が追加・変更した行を持つファイル。
struct Change {
    path: String,
    added: Vec<LineRange>,
}

/// 差分の出力を、ファイルごとの追加・変更行の範囲に分ける。
fn changes(diff: &str) -> Vec<Change> {
    let mut changes: Vec<Change> = Vec::new();
    let mut header = false;
    for line in diff.lines() {
        match line.strip_prefix("+++ b/") {
            Some(path) if header => changes.push(Change {
                path: path.to_string(),
                added: Vec::new(),
            }),
            _ => {
                if let Some(range) = added_range(line)
                    && let Some(change) = changes.last_mut()
                {
                    change.added.push(range);
                }
            }
        }
        header = line.starts_with("--- ");
    }
    changes.retain(|change| !change.added.is_empty());
    changes
}

/// hunk の見出しが示す、追加・変更後の行範囲。追加した行が無い hunk は `None`。
fn added_range(line: &str) -> Option<LineRange> {
    let (spec, _) = line.strip_prefix("@@ ")?.split_once(" @@")?;
    let added = spec
        .split_whitespace()
        .find_map(|part| part.strip_prefix('+'))?;
    let (start, count) = match added.split_once(',') {
        Some((start, count)) => (start.parse::<u32>().ok()?, count.parse::<u32>().ok()?),
        None => (added.parse::<u32>().ok()?, 1),
    };
    let start = NonZeroU32::new(start)?;
    let end = NonZeroU32::new(start.get() + count.checked_sub(1)?)?;
    LineRange::new(start, end)
}

/// 触れた Segment だけを残した 1 ファイルの文書。
fn change_document(
    change: &Change,
    rev: Option<&str>,
    commit: Option<&str>,
) -> Result<Option<Document>> {
    let path = Path::new(&change.path);
    if !input::has_format(path) {
        return Ok(None);
    }
    let spec = match rev {
        Some(rev) => format!("{rev}:{}", change.path),
        None => format!(":{}", change.path),
    };
    let Some(content) = blob(&spec)? else {
        return Ok(None);
    };
    let Reading::Document(mut document) =
        input::extract_document(change.path.clone(), path, &content)
    else {
        return Ok(None);
    };
    document
        .segments
        .retain(|segment| touches(&change.added, segment.origin.lines));
    for segment in &mut document.segments {
        segment.origin.commit = commit.map(str::to_string);
    }
    Ok(extract::with_japanese(document))
}

/// 索引または指定したリビジョンにあるファイルの内容。UTF-8 で符号化されていなければ `None`。
fn blob(spec: &str) -> Result<Option<String>> {
    Ok(String::from_utf8(run(&["show", spec])?).ok())
}

/// 追加・変更行のどれかが行範囲に重なるか。
fn touches(added: &[LineRange], lines: LineRange) -> bool {
    added
        .iter()
        .any(|added| added.start() <= lines.end() && lines.start() <= added.end())
}

/// `%H%x00%B%x00` の並びを、ハッシュとメッセージの対にする。
fn log_records(output: &str) -> Vec<(&str, &str)> {
    let mut records = Vec::new();
    let mut fields = output.split('\0');
    while let (Some(hash), Some(message)) = (fields.next(), fields.next()) {
        let hash = hash.trim();
        if !hash.is_empty() {
            records.push((hash, message));
        }
    }
    records
}

/// 短縮したハッシュと題名の先頭からなる文書の名前。
fn commit_name(hash: &str, segments: &[Segment]) -> String {
    let short: String = hash.chars().take(SHORT_HASH).collect();
    let subject = segments.first().map_or("", |segment| segment.text.as_str());
    let title: String = subject.chars().take(SHORT_SUBJECT).collect();
    if title.is_empty() {
        short
    } else {
        format!("{short} {title}")
    }
}

/// コミットメッセージの Segment。`#` で始まる行と鋏の行から下、末尾の段落のトレーラー行を除き、
/// 1 行目を題名、残りの段落を本文にする。
fn message_segments(message: &str, path: &str, commit: Option<&str>) -> Vec<Segment> {
    let lines: Vec<(NonZeroU32, &str)> = message
        .lines()
        .take_while(|line| !is_scissors(line))
        .enumerate()
        .map(|(at, line)| (line_number(at), line))
        .filter(|(_, line)| !line.starts_with('#'))
        .collect();
    let trailers = last_paragraph_start(&lines);
    let lines: Vec<(NonZeroU32, &str)> = lines
        .iter()
        .enumerate()
        .filter(|(at, (_, line))| {
            let in_last_paragraph = trailers.is_some_and(|start| *at >= start);
            !(in_last_paragraph && is_trailer(line))
        })
        .map(|(_, line)| *line)
        .collect();

    let mut segments = Vec::new();
    let mut lines = lines.into_iter().skip_while(|(_, line)| is_blank(line));
    let Some(subject) = lines.next() else {
        return segments;
    };
    segments.push(segment(
        &[subject],
        SegmentKind::CommitSubject,
        path,
        commit,
    ));
    let mut paragraph: Vec<(NonZeroU32, &str)> = Vec::new();
    for (number, line) in lines {
        if is_blank(line) {
            if !paragraph.is_empty() {
                segments.push(segment(&paragraph, SegmentKind::CommitBody, path, commit));
                paragraph.clear();
            }
        } else {
            paragraph.push((number, line));
        }
    }
    if !paragraph.is_empty() {
        segments.push(segment(&paragraph, SegmentKind::CommitBody, path, commit));
    }
    segments
}

/// 連続する行から 1 つの Segment を作る。
fn segment(
    lines: &[(NonZeroU32, &str)],
    kind: SegmentKind,
    path: &str,
    commit: Option<&str>,
) -> Segment {
    let (first, _) = *lines.first().expect("Segment は 1 行以上を持つ");
    let (last, _) = *lines.last().expect("Segment は 1 行以上を持つ");
    let text = lines
        .iter()
        .map(|(_, line)| *line)
        .collect::<Vec<&str>>()
        .join("\n");
    Segment {
        text,
        origin: Origin {
            path: path.to_string(),
            lines: LineRange::new(first, last).expect("終了行は開始行を下回らない"),
            commit: commit.map(str::to_string),
        },
        kind,
    }
}

/// 末尾の段落の最初の行の位置。段落が 1 つだけのときは `None`。
fn last_paragraph_start(lines: &[(NonZeroU32, &str)]) -> Option<usize> {
    let end = lines.iter().rposition(|(_, line)| !is_blank(line))?;
    let blank = lines[..end].iter().rposition(|(_, line)| is_blank(line))?;
    Some(blank + 1)
}

/// `Key: value` の形の行。Key は英字とハイフンからなる。
fn is_trailer(line: &str) -> bool {
    let Some((key, _)) = line.split_once(": ") else {
        return false;
    };
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
}

/// `git commit -v` がメッセージと差分を隔てる鋏の行。
fn is_scissors(line: &str) -> bool {
    line.starts_with('#') && line.contains(">8")
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn line_number(at: usize) -> NonZeroU32 {
    let number = u32::try_from(at + 1).unwrap_or(u32::MAX);
    NonZeroU32::new(number).unwrap_or(NonZeroU32::MIN)
}

/// git を実行し、標準出力を返す。
fn run(args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .output()
        .map_err(Error::Spawn)?;
    if !output.status.success() {
        return Err(Error::Failed {
            command: args.join(" "),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(output.stdout)
}

fn text(args: &[&str]) -> Result<String> {
    String::from_utf8(run(args)?).map_err(|_| Error::NotUtf8 {
        command: args.join(" "),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(message: &str) -> Vec<Segment> {
        message_segments(message, COMMIT_PATH, Some("0123456789abcdef"))
    }

    fn texts(message: &str) -> Vec<String> {
        segments(message)
            .into_iter()
            .map(|segment| segment.text)
            .collect()
    }

    #[test]
    fn the_records_of_the_log_are_the_hash_and_the_message() {
        let output = "aaa\u{0}題名だ\n\u{0}\nbbb\u{0}別の題名だ\n\n本文だ。\n\u{0}\n";
        assert_eq!(
            log_records(output),
            [("aaa", "題名だ\n"), ("bbb", "別の題名だ\n\n本文だ。\n")]
        );
        assert_eq!(log_records(""), []);
    }

    #[test]
    fn the_first_line_is_the_subject_and_the_paragraphs_are_the_body() {
        let message = "題名だ\n\n一つ目の段落だ。\n続く行だ。\n\n二つ目の段落だ。\n";
        assert_eq!(
            texts(message),
            ["題名だ", "一つ目の段落だ。\n続く行だ。", "二つ目の段落だ。"]
        );
        assert_eq!(
            segments(message)
                .iter()
                .map(|segment| segment.kind)
                .collect::<Vec<SegmentKind>>(),
            [
                SegmentKind::CommitSubject,
                SegmentKind::CommitBody,
                SegmentKind::CommitBody
            ]
        );
    }

    #[test]
    fn the_lines_hold_the_place_in_the_message() {
        let segments = segments("題名だ\n\n# 案内の行\n本文だ。\n続く行だ。\n");
        let lines: Vec<(u32, u32)> = segments
            .iter()
            .map(|segment| {
                (
                    segment.origin.lines.start().get(),
                    segment.origin.lines.end().get(),
                )
            })
            .collect();
        assert_eq!(lines, [(1, 1), (4, 5)]);
        assert_eq!(segments[0].origin.path, COMMIT_PATH);
        assert_eq!(
            segments[0].origin.commit.as_deref(),
            Some("0123456789abcdef")
        );
    }

    #[test]
    fn the_trailers_of_the_last_paragraph_are_not_segments() {
        assert_eq!(
            texts(
                "題名だ\n\n本文だ。\n\nCo-Authored-By: 手伝い <a@example.com>\nSigned-off-by: 誰か <b@example.com>\n"
            ),
            ["題名だ", "本文だ。"]
        );
        assert_eq!(
            texts("題名だ\n\n本文だ。\nCo-Authored-By: 手伝い <a@example.com>\n"),
            ["題名だ", "本文だ。"]
        );
        assert_eq!(texts("Refs: 番号だ\n"), ["Refs: 番号だ"]);
        assert_eq!(
            texts("題名だ\n\n注記: これは本文だ。\n"),
            ["題名だ", "注記: これは本文だ。"]
        );
    }

    #[test]
    fn the_diff_below_the_scissors_is_not_a_segment() {
        assert_eq!(
            texts(
                "題名だ\n\n# ------------------------ >8 ------------------------\n# 下は差分だ\n+日本語の行だ。\n"
            ),
            ["題名だ"]
        );
    }

    #[test]
    fn an_empty_message_has_no_segments() {
        assert!(texts("").is_empty());
        assert!(texts("# 案内の行だけだ\n\n").is_empty());
    }

    #[test]
    fn a_hunk_yields_the_range_of_the_added_lines() {
        let range =
            |line: &str| added_range(line).map(|range| (range.start().get(), range.end().get()));
        assert_eq!(range("@@ -1,0 +2 @@"), Some((2, 2)));
        assert_eq!(range("@@ -3 +4 @@ fn f()"), Some((4, 4)));
        assert_eq!(range("@@ -1,2 +5,3 @@"), Some((5, 7)));
        assert_eq!(range("@@ -2,1 +1,0 @@"), None);
        assert_eq!(range("+@@ -1 +1 @@"), None);
        assert_eq!(range("+行だ。"), None);
    }

    #[test]
    fn the_diff_lists_the_added_lines_of_each_file() {
        let diff = concat!(
            "diff --git a/a.md b/a.md\n",
            "index 1..2 100644\n",
            "--- a/a.md\n",
            "+++ b/a.md\n",
            "@@ -1,0 +2 @@\n",
            "+挿入だ。\n",
            "@@ -3 +4 @@\n",
            "-前の行だ。\n",
            "+直した行だ。\n",
            "diff --git a/b.md b/b.md\n",
            "--- a/b.md\n",
            "+++ b/b.md\n",
            "@@ -1 +1 @@\n",
            "-前だ。\n",
            "++++ b/c.md\n",
            "diff --git a/c.txt b/c.txt\n",
            "--- a/c.txt\n",
            "+++ b/c.txt\n",
            "@@ -1 +0,0 @@\n",
            "-消した行だ。\n",
        );
        let changes = changes(diff);
        let ranges: Vec<(&str, Vec<(u32, u32)>)> = changes
            .iter()
            .map(|change| {
                let added = change
                    .added
                    .iter()
                    .map(|range| (range.start().get(), range.end().get()))
                    .collect();
                (change.path.as_str(), added)
            })
            .collect();
        assert_eq!(
            ranges,
            [("a.md", vec![(2, 2), (4, 4)]), ("b.md", vec![(1, 1)])]
        );
    }

    #[test]
    fn the_right_side_of_the_range_holds_the_contents() {
        assert_eq!(right_rev("HEAD~1..HEAD"), "HEAD");
        assert_eq!(right_rev("main..topic"), "topic");
        assert_eq!(right_rev("main...topic"), "topic");
        assert_eq!(right_rev("HEAD~2.."), "HEAD");
        assert_eq!(right_rev("HEAD~2"), "HEAD");
    }

    #[test]
    fn the_name_carries_the_short_hash_and_the_subject() {
        let hash = "0123456789abcdef";
        let message = "題名はここでは二十字を超える長さで書いてある一文だ\n";
        let name = commit_name(hash, &segments(message));
        assert_eq!(name, "0123456 題名はここでは二十字を超える長さで書いて");
        assert_eq!(commit_name(hash, &[]), "0123456");
    }
}
