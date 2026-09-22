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
    /// 開始行と終了行から作る。終了が開始を下回るときは `None`。
    pub fn new(start: NonZeroU32, end: NonZeroU32) -> Option<Self> {
        (end >= start).then_some(Self { start, end })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(number: u32) -> NonZeroU32 {
        NonZeroU32::new(number).unwrap()
    }

    #[test]
    fn the_range_keeps_both_ends() {
        let range = LineRange::new(line(2), line(5)).unwrap();
        assert_eq!(range.start().get(), 2);
        assert_eq!(range.end().get(), 5);
        assert_eq!(LineRange::new(line(3), line(3)).unwrap().end().get(), 3);
    }

    #[test]
    fn an_end_before_the_start_is_rejected() {
        assert_eq!(LineRange::new(line(3), line(2)), None);
    }
}
