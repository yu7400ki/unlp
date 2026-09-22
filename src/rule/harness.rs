use std::num::NonZeroU32;
use std::sync::LazyLock;

use crate::document::{Document, LineRange, Origin, Segment, SegmentKind};
use crate::morph::Analyzer;
use crate::rule::{Context, SentenceRule};

static ANALYZER: LazyLock<Analyzer> = LazyLock::new(|| Analyzer::new().unwrap());

/// 同梱した重みと語リスト。
pub static CONTEXT: LazyLock<Context> = LazyLock::new(Context::defaults);

/// 同梱した辞書で解析した文に規則を適用し、指摘の抜粋を返す。
pub fn excerpts(rule: &dyn SentenceRule, text: &str) -> Vec<String> {
    excerpts_with(rule, &CONTEXT, text)
}

/// `excerpts` の、語リストと重みを差し替える形。
pub fn excerpts_with(rule: &dyn SentenceRule, context: &Context, text: &str) -> Vec<String> {
    let document = Document {
        name: "t".to_string(),
        segments: vec![Segment {
            text: text.to_string(),
            origin: Origin {
                path: "t".to_string(),
                lines: LineRange::new(NonZeroU32::MIN, NonZeroU32::MIN).unwrap(),
                commit: None,
            },
            kind: SegmentKind::Prose,
        }],
    };
    ANALYZER
        .analyze_document(&document)
        .iter()
        .flat_map(|sentence| rule.check(sentence, context))
        .map(|finding| finding.excerpt().to_string())
        .collect()
}
