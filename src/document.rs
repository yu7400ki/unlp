use std::num::NonZeroU32;

use serde::Serialize;

/// 抽出した 1 つの入力。
#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub name: String,
    pub segments: Vec<Segment>,
}

/// 文書から取り出した文字列の一続き。`text` は原文の切り出しで、記法をそのまま保持する。
#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub text: String,
    pub origin: Origin,
    pub kind: SegmentKind,
}

/// Segment を取り出した位置。
#[derive(Debug, Clone, Serialize)]
pub struct Origin {
    pub path: String,
    pub lines: LineRange,
    pub commit: Option<String>,
}

/// 1 始まりの行範囲。両端を含む。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LineRange {
    start: NonZeroU32,
    end: NonZeroU32,
}

impl LineRange {
    /// `start` 行から `lines` 行を占める範囲。
    pub fn new(start: NonZeroU32, lines: NonZeroU32) -> Self {
        Self {
            start,
            end: start.saturating_add(lines.get() - 1),
        }
    }

    pub fn start(self) -> NonZeroU32 {
        self.start
    }

    pub fn end(self) -> NonZeroU32 {
        self.end
    }
}

/// Segment を取り出した面。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    Prose,
    TableCell,
    Comment,
    DocComment,
    StringLiteral,
    CommitSubject,
    CommitBody,
}
