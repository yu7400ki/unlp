use std::collections::{BTreeMap, BTreeSet};

use crate::rule::{Layer, RuleId};

const WEIGHTS: &str = include_str!("../../data/weights.toml");

const LISTS: [(RuleId, &str); 1] = [(
    RuleId::new(Layer::Structure, 1),
    include_str!("../../data/lists/S01.toml"),
)];

static EMPTY: WordList = WordList(BTreeMap::new());

/// 規則が照合する語。欄の名前ごとに語を保持する。
#[derive(Debug, Clone, Default)]
pub struct WordList(BTreeMap<String, BTreeSet<String>>);

impl WordList {
    /// 欄に語が含まれるか。
    pub fn contains(&self, group: &str, word: &str) -> bool {
        self.0.get(group).is_some_and(|words| words.contains(word))
    }
}

/// 規則が参照する、文書と設定から決まる値。
#[derive(Debug, Clone)]
pub struct Context {
    weights: BTreeMap<RuleId, f64>,
    lists: BTreeMap<RuleId, WordList>,
    polite_ratio: f64,
}

impl Context {
    /// 同梱した既定の重みと語リストを読み込む。
    pub fn defaults() -> Self {
        Self {
            weights: weights(WEIGHTS),
            lists: LISTS
                .into_iter()
                .map(|(rule, source)| (rule, word_list(source)))
                .collect(),
            polite_ratio: 0.0,
        }
    }

    /// 規則 ID ごとの指摘 1 件の重み。
    pub fn weights(&self) -> &BTreeMap<RuleId, f64> {
        &self.weights
    }

    /// 規則が照合する語。持たない規則は空の語リスト。
    pub fn list(&self, rule: RuleId) -> &WordList {
        self.lists.get(&rule).unwrap_or(&EMPTY)
    }

    /// 文のうち敬体で終わるものの割合。
    pub fn polite_ratio(&self) -> f64 {
        self.polite_ratio
    }
}

fn weights(source: &str) -> BTreeMap<RuleId, f64> {
    let table: BTreeMap<String, f64> =
        toml::from_str(source).expect("同梱した重みは規則 ID と数の表である");
    table
        .into_iter()
        .map(|(rule, weight)| {
            let rule = rule.parse().expect("同梱した重みの鍵は規則 ID である");
            (rule, weight)
        })
        .collect()
}

fn word_list(source: &str) -> WordList {
    WordList(toml::from_str(source).expect("同梱した語リストは欄の名前と語の表である"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::registered;

    #[test]
    fn the_defaults_weigh_every_registered_rule() {
        let context = Context::defaults();
        assert_eq!(context.weights().len(), 22);
        for (rule, _) in registered() {
            assert!(context.weights().contains_key(&rule), "{rule}");
        }
        assert_eq!(context.weights()[&RuleId::new(Layer::Structure, 1)], 3.0);
    }

    #[test]
    fn the_defaults_carry_the_words_of_the_rules() {
        let context = Context::defaults();
        let list = context.list(RuleId::new(Layer::Structure, 1));
        assert!(list.contains("person", "利用者"));
        assert!(list.contains("speech", "述べる"));
        assert!(!list.contains("speech", "言う"));
        assert!(!list.contains("person", "述べる"));
    }

    #[test]
    fn a_rule_without_words_gets_an_empty_list() {
        let context = Context::defaults();
        assert!(
            !context
                .list(RuleId::new(Layer::Goshu, 1))
                .contains("person", "利用者")
        );
    }

    #[test]
    fn the_polite_ratio_starts_at_zero() {
        assert_eq!(Context::defaults().polite_ratio(), 0.0);
    }
}
