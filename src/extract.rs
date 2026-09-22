use std::num::NonZeroU32;

use crate::document::{Document, LineRange, Origin, Segment, SegmentKind};

/// 標準入力から読んだ文書の名前。
pub const STDIN_NAME: &str = "<stdin>";

/// テキスト全体を 1 つの Prose Segment とする文書。`name` が Segment の位置の path になる。
pub fn text_document(name: String, text: &str) -> Document {
    let lines = u32::try_from(text.lines().count()).unwrap_or(u32::MAX);
    let origin = Origin {
        path: name.clone(),
        lines: LineRange::new(
            NonZeroU32::MIN,
            NonZeroU32::new(lines).unwrap_or(NonZeroU32::MIN),
        ),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
