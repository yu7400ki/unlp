use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use crate::measure::{FinalPredicates, Measurement, Measures};
use crate::rule::{Layer, RuleId};
use crate::sentence::Sentence;

const WEIGHTS: &str = include_str!("../../data/weights.toml");

const LISTS: [(RuleId, &str); 9] = [
    (
        RuleId::new(Layer::Structure, 1),
        include_str!("../../data/lists/S01.toml"),
    ),
    (
        RuleId::new(Layer::Structure, 3),
        include_str!("../../data/lists/S03.toml"),
    ),
    (
        RuleId::new(Layer::Structure, 6),
        include_str!("../../data/lists/S06.toml"),
    ),
    (
        RuleId::new(Layer::Lexical, 1),
        include_str!("../../data/lists/L01.toml"),
    ),
    (
        RuleId::new(Layer::Lexical, 2),
        include_str!("../../data/lists/L02.toml"),
    ),
    (
        RuleId::new(Layer::Register, 1),
        include_str!("../../data/lists/R01.toml"),
    ),
    (
        RuleId::new(Layer::Register, 2),
        include_str!("../../data/lists/R02.toml"),
    ),
    (
        RuleId::new(Layer::Formulaic, 1),
        include_str!("../../data/lists/F01.toml"),
    ),
    (
        RuleId::new(Layer::Formulaic, 5),
        include_str!("../../data/lists/F05.toml"),
    ),
];

/// 敬体の文書として扱う敬体率の下限。
const POLITE: f64 = 0.5;

static EMPTY: WordList = WordList(BTreeMap::new());

static DEFAULT_WEIGHTS: LazyLock<BTreeMap<RuleId, f64>> = LazyLock::new(|| weights(WEIGHTS));

static DEFAULT_LISTS: LazyLock<BTreeMap<RuleId, WordList>> = LazyLock::new(|| {
    LISTS
        .into_iter()
        .map(|(rule, source)| (rule, word_list(source)))
        .collect()
});

/// 同梱した既定の、規則 ID ごとの指摘 1 件の重み。
pub fn default_weights() -> &'static BTreeMap<RuleId, f64> {
    &DEFAULT_WEIGHTS
}

/// 規則が照合する語。欄の名前ごとに語を保持する。
#[derive(Debug, Clone, Default)]
pub struct WordList(BTreeMap<String, BTreeSet<String>>);

impl WordList {
    /// 欄に語が含まれるか。
    pub fn contains(&self, group: &str, word: &str) -> bool {
        self.0.get(group).is_some_and(|words| words.contains(word))
    }

    /// 欄にある語。持たない欄は空。
    pub fn words(&self, group: &str) -> impl Iterator<Item = &str> {
        self.0.get(group).into_iter().flatten().map(String::as_str)
    }
}

/// 規則が参照する、文書と設定から決まる値。
#[derive(Debug, Clone)]
pub struct Context {
    weights: BTreeMap<RuleId, f64>,
    lists: BTreeMap<RuleId, WordList>,
    measurement: Measurement,
}

impl Context {
    /// 文書の文の列を計測し、同梱した既定の重みと語リストを添える。
    pub fn for_document(sentences: &[Sentence]) -> Self {
        Self {
            weights: DEFAULT_WEIGHTS.clone(),
            lists: DEFAULT_LISTS.clone(),
            measurement: Measurement::of(sentences),
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

    /// 文書の参考値。
    pub fn measures(&self) -> &Measures {
        self.measurement.measures()
    }

    /// 文書の文末の述語。
    pub fn final_predicates(&self) -> &FinalPredicates {
        self.measurement.final_predicates()
    }

    /// 敬体の文書か。敬体だけで数える規則がこれで自身の適用を決める。
    pub fn is_polite(&self) -> bool {
        self.measures()
            .polite_ratio
            .is_some_and(|ratio| ratio >= POLITE)
    }

    /// 語リストから語を 1 つ外した Context。
    #[cfg(test)]
    pub(crate) fn without_word(&self, rule: RuleId, group: &str, word: &str) -> Self {
        let mut context = self.clone();
        if let Some(words) = context
            .lists
            .get_mut(&rule)
            .and_then(|list| list.0.get_mut(group))
        {
            words.remove(word);
        }
        context
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
    use crate::rule::{harness, registered};

    /// 敬体の文を `polite` 文、常体の文を `plain` 文並べた文書の Context。
    fn document(polite: usize, plain: usize) -> Context {
        harness::context(&format!(
            "{}{}",
            "規則を数えます。".repeat(polite),
            "規則を数える。".repeat(plain)
        ))
    }

    #[test]
    fn the_defaults_weigh_every_registered_rule() {
        assert_eq!(default_weights().len(), 22);
        for (rule, _) in registered() {
            assert!(default_weights().contains_key(&rule), "{rule}");
        }
        assert_eq!(default_weights()[&RuleId::new(Layer::Structure, 1)], 3.0);
    }

    #[test]
    fn the_defaults_carry_the_words_of_the_rules() {
        let context = document(0, 1);
        let list = context.list(RuleId::new(Layer::Structure, 1));
        assert!(list.contains("person", "利用者"));
        assert!(list.contains("speech", "述べる"));
        assert!(!list.contains("speech", "言う"));
        assert!(!list.contains("person", "述べる"));
    }

    #[test]
    fn the_words_of_a_group_are_read_back() {
        let context = document(0, 1);
        let list = context.list(RuleId::new(Layer::Structure, 3));
        assert!(list.words("phrases").any(|word| word == "つまり、"));
        assert_eq!(list.words("speech").count(), 0);
    }

    #[test]
    fn a_rule_without_words_gets_an_empty_list() {
        assert!(
            !document(0, 1)
                .list(RuleId::new(Layer::Goshu, 1))
                .contains("person", "利用者")
        );
    }

    #[test]
    fn a_document_without_a_full_stop_is_plain() {
        let context = harness::context("見出しだ\n");
        assert_eq!(context.measures().polite_ratio, None);
        assert!(!context.is_polite());
    }

    #[test]
    fn the_polite_ratio_decides_the_register_of_the_document() {
        assert_eq!(document(1, 1).measures().polite_ratio, Some(POLITE));
        assert!(document(1, 1).is_polite());
        assert!(!document(2, 3).is_polite());
    }

    #[test]
    fn the_measurement_of_a_document_keeps_the_words_and_the_weights() {
        let context = document(3, 1);
        assert_eq!(context.measures().polite_ratio, Some(0.75));
        assert!(
            context
                .list(RuleId::new(Layer::Structure, 1))
                .contains("person", "利用者")
        );
        assert_eq!(context.weights()[&RuleId::new(Layer::Structure, 1)], 3.0);
    }
}
