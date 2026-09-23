use std::num::NonZeroU32;
use std::sync::LazyLock;

use crate::document::{Document, LineRange, Origin, Segment, SegmentKind};
use crate::morph::Analyzer;
use crate::rule::{Context, DocumentRule, Finding, SentenceRule};
use crate::sentence::Sentence;
use crate::settings::Settings;

static ANALYZER: LazyLock<Analyzer> = LazyLock::new(|| Analyzer::new().unwrap());

/// 敬体の文書にするために添える文。
const POLITE: &str = "規則を数えます。";

/// 同梱した辞書で解析した文から作った Context。
pub fn context(text: &str) -> Context {
    with_sentences(text, |sentences| {
        Context::for_document(sentences, &Settings::default())
    })
}

/// 同梱した辞書で解析した文に規則を適用し、指摘の抜粋を返す。
pub fn excerpts(rule: &dyn SentenceRule, text: &str) -> Vec<String> {
    with_sentences(text, |sentences| {
        let context = Context::for_document(sentences, &Settings::default());
        excerpts_of_sentences(rule, sentences, &context)
    })
}

/// `excerpts` の、敬体の文を添えて敬体の文書にする形。
pub fn polite_excerpts(rule: &dyn SentenceRule, text: &str) -> Vec<String> {
    excerpts(rule, &format!("{POLITE}{text}"))
}

/// `excerpts` の、語リストと重みを差し替える形。
pub fn excerpts_with(rule: &dyn SentenceRule, context: &Context, text: &str) -> Vec<String> {
    with_sentences(text, |sentences| {
        excerpts_of_sentences(rule, sentences, context)
    })
}

/// 同梱した辞書で解析した文に文書の規則を適用し、指摘の抜粋を返す。
pub fn document_excerpts(rule: &dyn DocumentRule, text: &str) -> Vec<String> {
    document_excerpts_of(rule, &[text])
}

/// `document_excerpts` の、Segment を分けて渡す形。
pub fn document_excerpts_of(rule: &dyn DocumentRule, texts: &[&str]) -> Vec<String> {
    with_segments(texts, |sentences| {
        let context = Context::for_document(sentences, &Settings::default());
        excerpts_of(rule.check(sentences, &context))
    })
}

fn excerpts_of_sentences(
    rule: &dyn SentenceRule,
    sentences: &[Sentence],
    context: &Context,
) -> Vec<String> {
    excerpts_of(
        sentences
            .iter()
            .flat_map(|sentence| rule.check(sentence, context))
            .collect(),
    )
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
