use std::collections::BTreeMap;

use serde::Serialize;

use crate::measure::Measures;
use crate::rule::{Finding, Layer, RuleId};
use crate::sentence::{self, Sentence};

/// 正規化した点で採点する日本語文字数の下限。
pub const DEFAULT_FLOOR: usize = 300;

/// 超過と判断する既定のしきい値。
pub const DEFAULT_THRESHOLD: f64 = 10.0;

/// 入力全体の結果。
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    documents: Vec<DocumentScore>,
    total: Total,
}

/// 文書 1 つの結果。
#[derive(Debug, Clone, Serialize)]
pub struct DocumentScore {
    pub name: String,
    pub score: Score,
}

/// 文書 1 つの採点結果。
#[derive(Debug, Clone, Serialize)]
pub struct Score {
    ja_chars: usize,
    sentences: usize,
    mode: ScoreMode,
    by_rule: BTreeMap<RuleId, usize>,
    measures: Measures,
    /// 指摘の列。集計だけを残したときは `None`。
    findings: Option<Vec<Finding>>,
}

/// 文書ごとの結果を日本語文字数で加重した集計。
#[derive(Debug, Clone, Serialize)]
pub struct Total {
    ja_chars: usize,
    sentences: usize,
    mode: ScoreMode,
    by_rule: BTreeMap<RuleId, usize>,
}

/// 点の表し方。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScoreMode {
    Normalized {
        per_1000: f64,
        by_layer: BTreeMap<Layer, f64>,
    },
    CountOnly,
}

impl Score {
    /// 文と指摘から採点する。日本語文字数が `floor` 以上なら重みで正規化した点、未満なら
    /// 件数で判断する。
    pub fn new(
        sentences: &[Sentence],
        findings: Vec<Finding>,
        measures: Measures,
        floor: usize,
        weights: &BTreeMap<RuleId, f64>,
    ) -> Self {
        let ja_chars = sentence::ja_chars(sentences);
        let mut by_rule = BTreeMap::new();
        for finding in &findings {
            *by_rule.entry(finding.rule()).or_insert(0) += 1;
        }
        Self {
            ja_chars,
            sentences: sentences.len(),
            mode: mode_for(ja_chars, floor, &by_rule, weights),
            by_rule,
            measures,
            findings: Some(findings),
        }
    }

    pub fn ja_chars(&self) -> usize {
        self.ja_chars
    }

    pub fn sentences(&self) -> usize {
        self.sentences
    }

    pub fn mode(&self) -> &ScoreMode {
        &self.mode
    }

    pub fn by_rule(&self) -> &BTreeMap<RuleId, usize> {
        &self.by_rule
    }

    pub fn measures(&self) -> &Measures {
        &self.measures
    }

    /// 指摘の列。集計だけを残したときは `None`。
    pub fn findings(&self) -> Option<&[Finding]> {
        self.findings.as_deref()
    }

    /// 指摘の件数。
    pub fn finding_count(&self) -> usize {
        self.by_rule.values().sum()
    }

    /// 点がしきい値を超えているか。
    pub fn exceeds(&self, threshold: f64) -> bool {
        exceeds(&self.mode, &self.by_rule, threshold)
    }

    /// 指摘の列を落とし、集計だけを残す。
    pub fn forget_findings(&mut self) {
        self.findings = None;
    }

    /// 指摘と参考値を除いた集計。
    pub fn total(&self) -> Total {
        Total {
            ja_chars: self.ja_chars,
            sentences: self.sentences,
            mode: self.mode.clone(),
            by_rule: self.by_rule.clone(),
        }
    }
}

impl Total {
    pub fn ja_chars(&self) -> usize {
        self.ja_chars
    }

    pub fn sentences(&self) -> usize {
        self.sentences
    }

    pub fn mode(&self) -> &ScoreMode {
        &self.mode
    }

    pub fn by_rule(&self) -> &BTreeMap<RuleId, usize> {
        &self.by_rule
    }

    /// 指摘の件数。
    pub fn finding_count(&self) -> usize {
        self.by_rule.values().sum()
    }

    /// 点がしきい値を超えているか。
    pub fn exceeds(&self, threshold: f64) -> bool {
        exceeds(&self.mode, &self.by_rule, threshold)
    }
}

impl Report {
    /// 文書ごとの結果と、それを合算した集計をまとめる。
    pub fn new(
        documents: Vec<DocumentScore>,
        floor: usize,
        weights: &BTreeMap<RuleId, f64>,
    ) -> Self {
        let ja_chars = documents
            .iter()
            .map(|document| document.score.ja_chars)
            .sum();
        let sentences = documents
            .iter()
            .map(|document| document.score.sentences)
            .sum();
        let mut by_rule: BTreeMap<RuleId, usize> = BTreeMap::new();
        for document in &documents {
            for (rule, count) in &document.score.by_rule {
                *by_rule.entry(*rule).or_insert(0) += count;
            }
        }
        let total = Total {
            ja_chars,
            sentences,
            mode: mode_for(ja_chars, floor, &by_rule, weights),
            by_rule,
        };
        Self { documents, total }
    }

    pub fn documents(&self) -> &[DocumentScore] {
        &self.documents
    }

    pub fn total(&self) -> &Total {
        &self.total
    }

    /// 全体の点がしきい値を超えているか。
    pub fn exceeds(&self, threshold: f64) -> bool {
        self.total.exceeds(threshold)
    }

    /// 文書ごとの指摘の列を落とし、集計だけを残す。
    pub fn forget_findings(&mut self) {
        for document in &mut self.documents {
            document.score.forget_findings();
        }
    }
}

/// 日本語文字数が下限以上なら、指摘の件数と重みから 1000 字あたりの点を層ごとに求める。
fn mode_for(
    ja_chars: usize,
    floor: usize,
    by_rule: &BTreeMap<RuleId, usize>,
    weights: &BTreeMap<RuleId, f64>,
) -> ScoreMode {
    if below_floor(ja_chars, floor) {
        return ScoreMode::CountOnly;
    }
    let mut by_layer: BTreeMap<Layer, f64> = BTreeMap::new();
    for (rule, count) in by_rule {
        let weight = weights.get(rule).copied().unwrap_or_default();
        *by_layer.entry(rule.layer()).or_default() +=
            *count as f64 * weight * 1000.0 / ja_chars as f64;
    }
    ScoreMode::Normalized {
        // f64 の `sum` は空の列で -0.0 を返すため、0.0 から畳む。
        per_1000: by_layer.values().fold(0.0, |total, point| total + point),
        by_layer,
    }
}

/// 正規化した点を出さずに件数で判断する入力か。日本語の文字が無い入力も含む。
pub fn below_floor(ja_chars: usize, floor: usize) -> bool {
    ja_chars == 0 || ja_chars < floor
}

/// 正規化した点はしきい値との比較で、件数だけのときは構造か語彙の指摘の有無で判断する。
fn exceeds(mode: &ScoreMode, by_rule: &BTreeMap<RuleId, usize>, threshold: f64) -> bool {
    match mode {
        ScoreMode::Normalized { per_1000, .. } => *per_1000 > threshold,
        ScoreMode::CountOnly => by_rule
            .keys()
            .any(|rule| matches!(rule.layer(), Layer::Structure | Layer::Lexical)),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::document::{LineRange, Origin, Segment, SegmentKind};
    use crate::sentence::split_sentences;

    fn segment(text: &str) -> Segment {
        Segment {
            text: text.to_string(),
            origin: origin(),
            kind: SegmentKind::Prose,
        }
    }

    fn origin() -> Origin {
        Origin {
            path: "t".to_string(),
            lines: LineRange::new(NonZeroU32::MIN, NonZeroU32::MIN).unwrap(),
            commit: None,
        }
    }

    fn finding(layer: Layer, number: u8) -> Finding {
        Finding::new(RuleId::new(layer, number), origin(), "x".to_string(), "h")
    }

    /// 構造の 1 番を 3.0、密度の 2 番を 0.5 とする重み。
    fn weights() -> BTreeMap<RuleId, f64> {
        BTreeMap::from([
            (RuleId::new(Layer::Structure, 1), 3.0),
            (RuleId::new(Layer::Density, 2), 0.5),
        ])
    }

    fn score(text: &str, findings: Vec<Finding>) -> Score {
        let segment = segment(text);
        Score::new(
            &split_sentences(&segment),
            findings,
            Measures::default(),
            DEFAULT_FLOOR,
            &weights(),
        )
    }

    fn short(findings: Vec<Finding>) -> Score {
        score("短い文だ。", findings)
    }

    #[test]
    fn counts_chars_and_sentences_from_the_given_sentences() {
        let score = score(
            "型の doc が名乗る。設定を比べると動作が変わる。",
            Vec::new(),
        );
        assert_eq!(score.ja_chars(), 19);
        assert_eq!(score.sentences(), 2);
    }

    #[test]
    fn floor_switches_mode() {
        let below = "あ".repeat(DEFAULT_FLOOR - 1);
        let at = "あ".repeat(DEFAULT_FLOOR);
        assert!(matches!(
            score(&below, Vec::new()).mode(),
            ScoreMode::CountOnly
        ));
        assert!(matches!(
            score(&at, Vec::new()).mode(),
            ScoreMode::Normalized { .. }
        ));
    }

    #[test]
    fn by_rule_counts_findings() {
        let score = short(vec![
            finding(Layer::Structure, 1),
            finding(Layer::Structure, 1),
            finding(Layer::Density, 2),
        ]);
        assert_eq!(score.by_rule()[&RuleId::new(Layer::Structure, 1)], 2);
        assert_eq!(score.by_rule()[&RuleId::new(Layer::Density, 2)], 1);
        assert_eq!(score.finding_count(), 3);
    }

    #[test]
    fn count_only_exceeds_on_structure_or_lexical() {
        assert!(short(vec![finding(Layer::Structure, 1)]).exceeds(f64::MAX));
        assert!(short(vec![finding(Layer::Lexical, 1)]).exceeds(f64::MAX));
        assert!(!short(vec![finding(Layer::Density, 1)]).exceeds(f64::MAX));
        assert!(!short(Vec::new()).exceeds(0.0));
    }

    #[test]
    fn normalized_weighs_the_findings_per_1000_ja_chars() {
        let text = "あ".repeat(500);
        let score = score(
            &text,
            vec![finding(Layer::Structure, 1), finding(Layer::Density, 2)],
        );
        let ScoreMode::Normalized { per_1000, by_layer } = score.mode() else {
            panic!("{:?}", score.mode());
        };
        assert_eq!(*per_1000, 7.0);
        assert_eq!(by_layer[&Layer::Structure], 6.0);
        assert_eq!(by_layer[&Layer::Density], 1.0);
        assert_eq!(by_layer.len(), 2);
    }

    #[test]
    fn a_document_without_findings_holds_a_positive_zero() {
        let score = score(&"あ".repeat(DEFAULT_FLOOR), Vec::new());
        let ScoreMode::Normalized { per_1000, by_layer } = score.mode() else {
            panic!("{:?}", score.mode());
        };
        assert_eq!(format!("{per_1000:.1}"), "0.0");
        assert!(by_layer.is_empty());
    }

    #[test]
    fn normalized_compares_the_point_with_the_threshold() {
        let text = "あ".repeat(DEFAULT_FLOOR);
        assert!(!score(&text, Vec::new()).exceeds(0.0));

        let score = score(&text, vec![finding(Layer::Structure, 1)]);
        assert!(score.exceeds(9.9));
        assert!(!score.exceeds(10.0));
    }

    #[test]
    fn an_empty_finding_list_is_still_a_list() {
        let json = serde_json::to_string(&short(Vec::new())).unwrap();
        assert!(json.contains(r#""findings":[]"#), "{json}");
    }

    #[test]
    fn forgetting_findings_keeps_the_counts() {
        let mut score = short(vec![finding(Layer::Structure, 1)]);
        assert!(serde_json::to_string(&score).unwrap().contains("findings"));

        score.forget_findings();
        assert!(score.findings().is_none());
        assert_eq!(score.by_rule()[&RuleId::new(Layer::Structure, 1)], 1);
        assert!(
            serde_json::to_string(&score)
                .unwrap()
                .contains(r#""findings":null"#)
        );
    }

    #[test]
    fn the_total_sums_the_documents() {
        let report = Report::new(
            vec![
                DocumentScore {
                    name: "a".to_string(),
                    score: score(&"あ".repeat(100), vec![finding(Layer::Structure, 1)]),
                },
                DocumentScore {
                    name: "b".to_string(),
                    score: score(&"い".repeat(200), vec![finding(Layer::Structure, 1)]),
                },
            ],
            DEFAULT_FLOOR,
            &weights(),
        );
        assert!(matches!(
            report.documents[0].score.mode(),
            ScoreMode::CountOnly
        ));
        assert_eq!(report.total.ja_chars(), 300);
        assert_eq!(report.total.sentences(), 2);
        assert_eq!(report.total.by_rule()[&RuleId::new(Layer::Structure, 1)], 2);
        assert_eq!(report.total.finding_count(), 2);
        let ScoreMode::Normalized { per_1000, .. } = report.total.mode() else {
            panic!("{:?}", report.total.mode());
        };
        assert_eq!(*per_1000, 20.0);
    }

    #[test]
    fn rule_id_and_layer_are_json_keys() {
        let json = serde_json::to_string(&short(vec![finding(Layer::Structure, 1)])).unwrap();
        assert!(json.contains(r#""by_rule":{"S01":1}"#), "{json}");
        assert!(json.contains(r#""mode":{"kind":"count_only"}"#), "{json}");

        let by_layer = BTreeMap::from([(Layer::Structure, 1.5)]);
        assert_eq!(
            serde_json::to_string(&by_layer).unwrap(),
            r#"{"structure":1.5}"#
        );
    }
}
