use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::rule::{Layer, RuleId, WordList};

const WEIGHTS: &str = include_str!("../data/weights.toml");

const LISTS: [(RuleId, &str); 9] = [
    (
        RuleId::new(Layer::Structure, 1),
        include_str!("../data/lists/S01.toml"),
    ),
    (
        RuleId::new(Layer::Structure, 3),
        include_str!("../data/lists/S03.toml"),
    ),
    (
        RuleId::new(Layer::Structure, 6),
        include_str!("../data/lists/S06.toml"),
    ),
    (
        RuleId::new(Layer::Lexical, 1),
        include_str!("../data/lists/L01.toml"),
    ),
    (
        RuleId::new(Layer::Lexical, 2),
        include_str!("../data/lists/L02.toml"),
    ),
    (
        RuleId::new(Layer::Register, 1),
        include_str!("../data/lists/R01.toml"),
    ),
    (
        RuleId::new(Layer::Register, 2),
        include_str!("../data/lists/R02.toml"),
    ),
    (
        RuleId::new(Layer::Formulaic, 1),
        include_str!("../data/lists/F01.toml"),
    ),
    (
        RuleId::new(Layer::Formulaic, 5),
        include_str!("../data/lists/F05.toml"),
    ),
];

/// 超過と判断する、1000 字あたりの点。
const THRESHOLD: f64 = 10.0;

/// 正規化した点で採点する日本語文字数の下限。
const FLOOR: usize = 300;

static DEFAULT_WEIGHTS: LazyLock<BTreeMap<RuleId, f64>> = LazyLock::new(|| {
    let table: BTreeMap<String, f64> =
        toml::from_str(WEIGHTS).expect("同梱した重みは規則 ID と数の表である");
    table
        .into_iter()
        .map(|(rule, weight)| {
            let rule = rule.parse().expect("同梱した重みの鍵は規則 ID である");
            (rule, weight)
        })
        .collect()
});

static DEFAULT_LISTS: LazyLock<BTreeMap<RuleId, WordList>> = LazyLock::new(|| {
    LISTS
        .into_iter()
        .map(|(rule, source)| {
            let list = toml::from_str(source).expect("同梱した語リストは欄の名前と語の表である");
            (rule, list)
        })
        .collect()
});

/// 採点の設定。しきい値、下限、規則ごとの重み、規則ごとの語リストを持つ。
#[derive(Debug, Clone)]
pub struct Settings {
    threshold: f64,
    floor: usize,
    weights: BTreeMap<RuleId, f64>,
    lists: BTreeMap<RuleId, WordList>,
}

impl Default for Settings {
    /// 同梱した重みと語リストによる設定。
    fn default() -> Self {
        Self {
            threshold: THRESHOLD,
            floor: FLOOR,
            weights: DEFAULT_WEIGHTS.clone(),
            lists: DEFAULT_LISTS.clone(),
        }
    }
}

impl Settings {
    /// 超過と判断する、1000 字あたりの点。
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// 正規化した点で採点する日本語文字数の下限。
    pub fn floor(&self) -> usize {
        self.floor
    }

    /// 規則 ID ごとの指摘 1 件の重み。
    pub fn weights(&self) -> &BTreeMap<RuleId, f64> {
        &self.weights
    }

    /// 規則 ID ごとの、規則が照合する語。
    pub fn lists(&self) -> &BTreeMap<RuleId, WordList> {
        &self.lists
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::registered;

    #[test]
    fn the_defaults_weigh_every_registered_rule() {
        let settings = Settings::default();
        assert_eq!(settings.weights().len(), 22);
        for (rule, _) in registered() {
            assert!(settings.weights().contains_key(&rule), "{rule}");
        }
        assert_eq!(settings.weights()[&RuleId::new(Layer::Structure, 1)], 3.0);
    }

    #[test]
    fn the_defaults_carry_the_words_of_the_rules() {
        let settings = Settings::default();
        let list = &settings.lists()[&RuleId::new(Layer::Structure, 1)];
        assert!(list.contains("person", "利用者"));
        assert!(list.contains("speech", "述べる"));
        assert!(!list.contains("speech", "言う"));
    }

    #[test]
    fn the_defaults_bound_the_point_and_the_ja_chars() {
        let settings = Settings::default();
        assert_eq!(settings.threshold(), 10.0);
        assert_eq!(settings.floor(), 300);
    }
}
