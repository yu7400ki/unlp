use std::collections::{BTreeMap, BTreeSet};

use crate::rule::{Layer, RuleId};

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

    /// 敬体の文書か。敬体だけで数える規則がこれで自身の適用を決める。
    pub fn is_polite(&self) -> bool {
        self.polite_ratio >= POLITE
    }

    /// 文書の敬体率を持たせた Context。
    pub fn with_polite_ratio(&self, polite_ratio: f64) -> Self {
        Self {
            polite_ratio,
            ..self.clone()
        }
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
    fn the_words_of_a_group_are_read_back() {
        let context = Context::defaults();
        let list = context.list(RuleId::new(Layer::Structure, 3));
        assert!(list.words("phrases").any(|word| word == "つまり、"));
        assert_eq!(list.words("speech").count(), 0);
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
    fn a_document_is_plain_until_its_polite_ratio_is_measured() {
        assert!(!Context::defaults().is_polite());
    }

    #[test]
    fn the_polite_ratio_decides_the_register_of_the_document() {
        assert!(Context::defaults().with_polite_ratio(POLITE).is_polite());
        assert!(
            !Context::defaults()
                .with_polite_ratio(POLITE - 0.01)
                .is_polite()
        );
    }

    #[test]
    fn the_polite_ratio_of_a_document_keeps_the_words_and_the_weights() {
        let context = Context::defaults().with_polite_ratio(0.75);
        assert!(context.is_polite());
        assert!(
            context
                .list(RuleId::new(Layer::Structure, 1))
                .contains("person", "利用者")
        );
        assert_eq!(context.weights()[&RuleId::new(Layer::Structure, 1)], 3.0);
    }
}
