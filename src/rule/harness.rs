use std::num::NonZeroU32;
use std::sync::LazyLock;

use crate::document::{Document, LineRange, Origin, Segment, SegmentKind};
use crate::measure::Measurement;
use crate::morph::Analyzer;
use crate::rule::{Context, DocumentRule, Finding, SentenceRule};
use crate::sentence::Sentence;

static ANALYZER: LazyLock<Analyzer> = LazyLock::new(|| Analyzer::new().unwrap());

/// 同梱した重みと語リスト。
pub static CONTEXT: LazyLock<Context> = LazyLock::new(Context::defaults);

/// 同梱した辞書で解析した文に規則を適用し、指摘の抜粋を返す。
pub fn excerpts(rule: &dyn SentenceRule, text: &str) -> Vec<String> {
    excerpts_with(rule, &CONTEXT, text)
}

/// `excerpts` の、語リストと重みを差し替える形。
pub fn excerpts_with(rule: &dyn SentenceRule, context: &Context, text: &str) -> Vec<String> {
    with_sentences(text, |sentences| {
        excerpts_of(
            sentences
                .iter()
                .flat_map(|sentence| rule.check(sentence, context))
                .collect(),
        )
    })
}

/// 同梱した辞書で解析した文に文書の規則を適用し、指摘の抜粋を返す。Context には
/// その文書の計測を持たせる。
pub fn document_excerpts(rule: &dyn DocumentRule, text: &str) -> Vec<String> {
    document_excerpts_of(rule, &[text])
}

/// `document_excerpts` の、Segment を分けて渡す形。
pub fn document_excerpts_of(rule: &dyn DocumentRule, texts: &[&str]) -> Vec<String> {
    with_segments(texts, |sentences| {
        let context = CONTEXT.for_document(Measurement::of(sentences));
        excerpts_of(rule.check(sentences, &context))
    })
}

/// `document_excerpts` の、Context を差し替える形。
pub fn document_excerpts_with(
    rule: &dyn DocumentRule,
    context: &Context,
    text: &str,
) -> Vec<String> {
    with_sentences(text, |sentences| {
        excerpts_of(rule.check(sentences, context))
    })
}

/// 敬体率だけを計測した Context。
pub fn polite_context(ratio: f64) -> Context {
    CONTEXT.for_document(Measurement::of_polite_ratio(ratio))
}

fn excerpts_of(findings: Vec<Finding>) -> Vec<String> {
    findings
        .iter()
        .map(|finding| finding.excerpt().to_string())
        .collect()
}

/// 同梱した辞書で解析した文を渡す。
pub fn with_sentences<T>(text: &str, read: impl FnOnce(&[Sentence]) -> T) -> T {
    with_segments(&[text], read)
}

/// 文字列ごとに Segment を分けた文書を解析し、その文を渡す。
pub fn with_segments<T>(texts: &[&str], read: impl FnOnce(&[Sentence]) -> T) -> T {
    let document = Document {
        name: "t".to_string(),
        segments: texts.iter().map(|text| segment(text)).collect(),
    };
    read(&ANALYZER.analyze_document(&document))
}

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
