use std::collections::BTreeMap;

use serde::Serialize;

use crate::rule::{Finding, Layer, RuleId};

/// 正規化した点で採点する日本語文字数の下限。
pub const DEFAULT_FLOOR: usize = 300;

/// 入力全体の結果。
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub documents: Vec<DocumentScore>,
    pub total: Score,
}

/// 文書 1 つの結果。
#[derive(Debug, Clone, Serialize)]
pub struct DocumentScore {
    pub name: String,
    pub score: Score,
}

/// 採点結果。
#[derive(Debug, Clone, Serialize)]
pub struct Score {
    pub ja_chars: usize,
    pub mode: ScoreMode,
    pub by_rule: BTreeMap<RuleId, usize>,
    pub measures: Measures,
    pub findings: Vec<Finding>,
}

/// 点の表し方。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreMode {
    Normalized {
        per_1000: f64,
        by_layer: BTreeMap<Layer, f64>,
    },
    CountOnly,
}

/// 参考値。計測していない欄は `None`。
#[derive(Debug, Clone, Default, Serialize)]
pub struct Measures {
    pub polite_ratio: Option<f64>,
    pub plain_ratio: Option<f64>,
    pub wago_noun_ratio: Option<f64>,
    pub wago_verb_ratio: Option<f64>,
    pub ga_per_sentence: Option<f64>,
}

impl Score {
    /// 日本語文字数が `floor` 以上なら正規化した点、未満なら件数で採点する。
    pub fn new(ja_chars: usize, findings: Vec<Finding>, measures: Measures, floor: usize) -> Self {
        let mut by_rule = BTreeMap::new();
        for finding in &findings {
            *by_rule.entry(finding.rule()).or_insert(0) += 1;
        }
        Self {
            ja_chars,
            mode: mode_for(ja_chars, floor),
            by_rule,
            measures,
            findings,
        }
    }

    /// 点がしきい値を超えているか。件数で採点したときは構造か語彙の指摘があれば超過とする。
    pub fn exceeds(&self, threshold: f64) -> bool {
        match &self.mode {
            ScoreMode::Normalized { per_1000, .. } => *per_1000 > threshold,
            ScoreMode::CountOnly => self
                .by_rule
                .keys()
                .any(|rule| matches!(rule.layer(), Layer::Structure | Layer::Lexical)),
        }
    }

    /// 指摘の件数。
    pub fn finding_count(&self) -> usize {
        self.by_rule.values().sum()
    }
}

impl Report {
    /// 文書ごとの結果と、日本語文字数で加重した全体の結果をまとめる。
    pub fn new(documents: Vec<DocumentScore>, floor: usize) -> Self {
        let ja_chars = documents
            .iter()
            .map(|document| document.score.ja_chars)
            .sum();
        let mut by_rule: BTreeMap<RuleId, usize> = BTreeMap::new();
        for document in &documents {
            for (rule, count) in &document.score.by_rule {
                *by_rule.entry(*rule).or_insert(0) += count;
            }
        }
        let total = Score {
            ja_chars,
            mode: mode_for(ja_chars, floor),
            by_rule,
            measures: Measures::default(),
            findings: Vec::new(),
        };
        Self { documents, total }
    }

    /// 全体の点がしきい値を超えているか。
    pub fn exceeds(&self, threshold: f64) -> bool {
        self.total.exceeds(threshold)
    }
}

fn mode_for(ja_chars: usize, floor: usize) -> ScoreMode {
    if ja_chars >= floor {
        ScoreMode::Normalized {
            per_1000: 0.0,
            by_layer: BTreeMap::new(),
        }
    } else {
        ScoreMode::CountOnly
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::document::{LineRange, Origin};

    fn origin() -> Origin {
        Origin {
            path: "t".to_string(),
            lines: LineRange::new(NonZeroU32::MIN, NonZeroU32::MIN),
            commit: None,
        }
    }

    fn finding(layer: Layer, number: u8) -> Finding {
        Finding::new(RuleId::new(layer, number), origin(), "x".to_string(), "h")
    }

    fn score(ja_chars: usize, findings: Vec<Finding>) -> Score {
        Score::new(ja_chars, findings, Measures::default(), DEFAULT_FLOOR)
    }

    #[test]
    fn floor_switches_mode() {
        assert!(matches!(
            score(DEFAULT_FLOOR - 1, Vec::new()).mode,
            ScoreMode::CountOnly
        ));
        assert!(matches!(
            score(DEFAULT_FLOOR, Vec::new()).mode,
            ScoreMode::Normalized { .. }
        ));
    }

    #[test]
    fn by_rule_counts_findings() {
        let score = score(
            10,
            vec![
                finding(Layer::Structure, 1),
                finding(Layer::Structure, 1),
                finding(Layer::Density, 2),
            ],
        );
        assert_eq!(score.by_rule[&RuleId::new(Layer::Structure, 1)], 2);
        assert_eq!(score.by_rule[&RuleId::new(Layer::Density, 2)], 1);
        assert_eq!(score.finding_count(), 3);
    }

    #[test]
    fn count_only_exceeds_on_structure_or_lexical() {
        assert!(score(10, vec![finding(Layer::Structure, 1)]).exceeds(f64::MAX));
        assert!(score(10, vec![finding(Layer::Lexical, 1)]).exceeds(f64::MAX));
        assert!(!score(10, vec![finding(Layer::Density, 1)]).exceeds(f64::MAX));
        assert!(!score(10, Vec::new()).exceeds(0.0));
    }

    #[test]
    fn normalized_compares_the_point_with_the_threshold() {
        let score = score(DEFAULT_FLOOR, vec![finding(Layer::Structure, 1)]);
        assert!(!score.exceeds(0.0));
    }

    #[test]
    fn total_sums_chars_and_findings() {
        let report = Report::new(
            vec![
                DocumentScore {
                    name: "a".to_string(),
                    score: score(150, vec![finding(Layer::Structure, 1)]),
                },
                DocumentScore {
                    name: "b".to_string(),
                    score: score(200, vec![finding(Layer::Structure, 1)]),
                },
            ],
            DEFAULT_FLOOR,
        );
        assert!(matches!(
            report.documents[0].score.mode,
            ScoreMode::CountOnly
        ));
        assert_eq!(report.total.ja_chars, 350);
        assert_eq!(report.total.by_rule[&RuleId::new(Layer::Structure, 1)], 2);
        assert!(matches!(report.total.mode, ScoreMode::Normalized { .. }));
    }

    #[test]
    fn rule_id_and_layer_are_json_keys() {
        let score = score(DEFAULT_FLOOR, vec![finding(Layer::Structure, 1)]);
        let json = serde_json::to_string(&score).unwrap();
        assert!(json.contains(r#""by_rule":{"S01":1}"#), "{json}");
        assert!(json.contains(r#""normalized":{"per_1000":0.0"#), "{json}");

        let by_layer = BTreeMap::from([(Layer::Structure, 1.5)]);
        assert_eq!(
            serde_json::to_string(&by_layer).unwrap(),
            r#"{"structure":1.5}"#
        );
    }
}
