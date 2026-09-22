use std::cmp::Reverse;
use std::collections::BTreeMap;

use serde::Serialize;

use crate::rule::predicate::{is_case_particle, is_sahen_noun, is_verb};
use crate::sentence::Sentence;
use crate::token::{Goshu, Pos1, Token};

/// 文書の計測値。分母を持たない欄は `None`。
#[derive(Debug, Clone, Default, Serialize)]
pub struct Measures {
    pub polite_ratio: Option<f64>,
    pub plain_ratio: Option<f64>,
    pub wago_noun_ratio: Option<f64>,
    pub wago_verb_ratio: Option<f64>,
    pub final_wago_ratio: Option<f64>,
    pub ga_per_sentence: Option<f64>,
}

impl Measures {
    /// 解析済みの文から計測する。
    pub fn of(sentences: &[Sentence]) -> Self {
        Self {
            polite_ratio: polite_ratio(sentences),
            plain_ratio: plain_ratio(sentences),
            wago_noun_ratio: wago_noun_ratio(sentences),
            wago_verb_ratio: wago_verb_ratio(sentences),
            final_wago_ratio: FinalPredicates::of(sentences).wago_ratio(),
            ga_per_sentence: ga_per_sentence(sentences),
        }
    }
}

/// 句点で終わる文のうち敬体で終わるものの割合。
pub fn polite_ratio(sentences: &[Sentence]) -> Option<f64> {
    ratio(
        terminated(sentences)
            .filter(|sentence| is_polite(sentence))
            .count(),
        terminated(sentences).count(),
    )
}

/// 句点で終わる文のうち、述語を持ち敬体でないものの割合。
pub fn plain_ratio(sentences: &[Sentence]) -> Option<f64> {
    ratio(
        terminated(sentences)
            .filter(|sentence| sentence.has_predicate() && !is_polite(sentence))
            .count(),
        terminated(sentences).count(),
    )
}

/// 普通名詞に占める和語の割合。
pub fn wago_noun_ratio(sentences: &[Sentence]) -> Option<f64> {
    let nouns = tokens(sentences).filter(|token| is_common_noun(token));
    let (wago, all) = nouns.fold((0, 0), |(wago, all), noun| {
        (wago + usize::from(noun.goshu == Goshu::Wago), all + 1)
    });
    ratio(wago, all)
}

/// 和語の一般動詞と サ変可能名詞＋する の合計に占める和語の動詞の割合。
pub fn wago_verb_ratio(sentences: &[Sentence]) -> Option<f64> {
    let mut wago = 0;
    let mut sahen = 0;
    for sentence in sentences {
        let tokens = sentence.tokens();
        for index in 0..tokens.len() {
            if is_wago_verb(&tokens[index]) {
                wago += 1;
            } else if is_sahen_verb(tokens, index) {
                sahen += 1;
            }
        }
    }
    ratio(wago, wago + sahen)
}

/// 1 文あたりの格助詞「が」の数。
pub fn ga_per_sentence(sentences: &[Sentence]) -> Option<f64> {
    let ga = tokens(sentences)
        .filter(|token| is_case_particle(token, "が"))
        .count();
    ratio(ga, sentences.len())
}

/// 句点で終わる文の文末の述語。動詞と サ変可能名詞＋する だけを数える。
#[derive(Debug, Clone, Default)]
pub struct FinalPredicates {
    wago: BTreeMap<String, usize>,
    sahen: usize,
}

impl FinalPredicates {
    /// 句点で終わる文の、末尾から助動詞・助詞・記号と非自立可能の Token を飛ばした先を
    /// 述語として数える。
    pub fn of(sentences: &[Sentence]) -> Self {
        let mut predicates = Self::default();
        for sentence in terminated(sentences) {
            let tokens = sentence.tokens();
            let Some(index) = final_predicate(tokens) else {
                continue;
            };
            let token = &tokens[index];
            if token.pos.pos1 == Pos1::Verb {
                if token.goshu == Goshu::Wago {
                    *predicates.wago.entry(token.lemma.clone()).or_default() += 1;
                }
            } else if is_sahen_verb(tokens, index) {
                predicates.sahen += 1;
            }
        }
        predicates
    }

    /// 文末が和語の動詞か サ変可能名詞＋する である文の数。
    pub fn total(&self) -> usize {
        self.wago_count() + self.sahen
    }

    /// 文末の述語に占める和語の動詞の割合。
    pub fn wago_ratio(&self) -> Option<f64> {
        ratio(self.wago_count(), self.total())
    }

    /// 文末の和語の動詞の原形と出現数。出現数の多い順、同数なら原形の順に並ぶ。
    pub fn frequent(&self, limit: usize) -> Vec<(&str, usize)> {
        let mut words: Vec<(&str, usize)> = self
            .wago
            .iter()
            .map(|(lemma, count)| (lemma.as_str(), *count))
            .collect();
        words.sort_by_key(|(lemma, count)| (Reverse(*count), *lemma));
        words.truncate(limit);
        words
    }

    fn wago_count(&self) -> usize {
        self.wago.values().sum()
    }
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then(|| numerator as f64 / denominator as f64)
}

fn terminated<'a, 's>(
    sentences: &'a [Sentence<'s>],
) -> impl Iterator<Item = &'a Sentence<'s>> + Clone {
    sentences.iter().filter(|sentence| sentence.is_terminated())
}

fn tokens<'a>(sentences: &'a [Sentence]) -> impl Iterator<Item = &'a Token> {
    sentences.iter().flat_map(Sentence::tokens)
}

/// 文末の助動詞の連なりに です または ます があるか、文末の動詞が くださる であるか。
/// 記号と助詞は飛ばす。
fn is_polite(sentence: &Sentence) -> bool {
    let mut tail = sentence
        .tokens()
        .iter()
        .rev()
        .skip_while(|token| is_trailing(token))
        .peekable();
    if tail.peek().is_some_and(|token| is_verb(token, "くださる")) {
        return true;
    }
    tail.take_while(|token| token.pos.pos1 == Pos1::AuxVerb)
        .any(|token| matches!(token.lemma.as_str(), "です" | "ます"))
}

/// 述語の後ろに続く記号と助詞か。
fn is_trailing(token: &Token) -> bool {
    matches!(
        token.pos.pos1,
        Pos1::Particle | Pos1::SupplementarySymbol | Pos1::Symbol
    )
}

fn is_common_noun(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Noun && token.pos.pos2 == "普通名詞"
}

fn is_wago_verb(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Verb && token.pos.pos2 == "一般" && token.goshu == Goshu::Wago
}

/// サ変可能名詞に する が続く位置か。
fn is_sahen_verb(tokens: &[Token], index: usize) -> bool {
    is_sahen_noun(&tokens[index])
        && tokens
            .get(index + 1)
            .is_some_and(|next| is_verb(next, "する") || is_verb(next, "できる"))
}

/// 文末から助動詞・助詞・記号と非自立可能の Token を飛ばした先の位置。
fn final_predicate(tokens: &[Token]) -> Option<usize> {
    tokens.iter().rposition(|token| !is_skipped_at_end(token))
}

fn is_skipped_at_end(token: &Token) -> bool {
    matches!(
        token.pos.pos1,
        Pos1::AuxVerb | Pos1::Particle | Pos1::SupplementarySymbol | Pos1::Symbol
    ) || token.pos.pos2 == "非自立可能"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn measures(text: &str) -> Measures {
        harness::with_sentences(text, Measures::of)
    }

    fn final_predicates(text: &str) -> FinalPredicates {
        harness::with_sentences(text, FinalPredicates::of)
    }

    #[test]
    fn the_polite_ratio_counts_the_sentences_that_end_with_desu_or_masu() {
        assert_eq!(measures("動作します。").polite_ratio, Some(1.0));
        assert_eq!(measures("これは規則です。").polite_ratio, Some(1.0));
        assert_eq!(measures("動作しました。").polite_ratio, Some(1.0));
        assert_eq!(measures("動作しません。").polite_ratio, Some(1.0));
        assert_eq!(measures("動作しますか。").polite_ratio, Some(1.0));
        assert_eq!(measures("動作する。").polite_ratio, Some(0.0));
        assert_eq!(measures("これは規則だ。").polite_ratio, Some(0.0));
        assert_eq!(measures("動作します。動作する。").polite_ratio, Some(0.5));
    }

    #[test]
    fn a_request_is_polite() {
        assert_eq!(measures("設定を確認してください。").polite_ratio, Some(1.0));
        assert_eq!(measures("ご確認ください。").polite_ratio, Some(1.0));
        assert_eq!(measures("設定を確認してください。").plain_ratio, Some(0.0));
    }

    #[test]
    fn a_volitional_polite_form_is_polite() {
        assert_eq!(measures("設定を直しましょう。").polite_ratio, Some(1.0));
    }

    #[test]
    fn a_sentence_without_a_full_stop_is_outside_the_register_ratios() {
        let measures = measures("動作します。見出しです\n");
        assert_eq!(measures.polite_ratio, Some(1.0));
        assert_eq!(measures.plain_ratio, Some(0.0));
    }

    #[test]
    fn the_plain_ratio_counts_the_sentences_with_a_predicate_outside_the_polite_form() {
        assert_eq!(measures("動作する。").plain_ratio, Some(1.0));
        assert_eq!(measures("動作します。").plain_ratio, Some(0.0));
        assert_eq!(measures("動作します。動作する。").plain_ratio, Some(0.5));
    }

    #[test]
    fn a_fragment_belongs_to_neither_register() {
        let measures = measures("名詞の列挙。動作します。");
        assert_eq!(measures.polite_ratio, Some(0.5));
        assert_eq!(measures.plain_ratio, Some(0.0));
    }

    #[test]
    fn the_register_ratios_are_empty_without_a_full_stop() {
        let measures = measures("見出しだ\n");
        assert_eq!(measures.polite_ratio, None);
        assert_eq!(measures.plain_ratio, None);
    }

    #[test]
    fn the_wago_noun_ratio_counts_the_common_nouns() {
        assert_eq!(measures("窓と鍵を数える。").wago_noun_ratio, Some(1.0));
        assert_eq!(measures("設定と規則を数える。").wago_noun_ratio, Some(0.0));
        assert_eq!(measures("窓と規則を数える。").wago_noun_ratio, Some(0.5));
        assert_eq!(measures("数える。").wago_noun_ratio, None);
    }

    #[test]
    fn the_wago_verb_ratio_weighs_the_wago_verbs_against_the_sahen_verbs() {
        assert_eq!(measures("設定を比べる。").wago_verb_ratio, Some(1.0));
        assert_eq!(measures("設定を比較する。").wago_verb_ratio, Some(0.0));
        assert_eq!(
            measures("設定を比べる。設定を比較する。").wago_verb_ratio,
            Some(0.5)
        );
        assert_eq!(measures("これは規則だ。").wago_verb_ratio, None);
    }

    #[test]
    fn the_ga_is_counted_for_each_sentence() {
        assert_eq!(measures("動作が変わる。").ga_per_sentence, Some(1.0));
        assert_eq!(
            measures("動作が変わる。設定が変わる。").ga_per_sentence,
            Some(1.0)
        );
        assert_eq!(
            measures("動作が変わる。設定を変える。").ga_per_sentence,
            Some(0.5)
        );
        assert_eq!(measures("設定を変える。").ga_per_sentence, Some(0.0));
    }

    #[test]
    fn the_final_predicate_is_the_last_verb_of_a_sentence() {
        let predicates = final_predicates("設定を比べる。値を追加する。");
        assert_eq!(predicates.total(), 2);
        assert_eq!(predicates.wago_ratio(), Some(0.5));
        assert_eq!(predicates.frequent(10), [("比べる", 1)]);
    }

    #[test]
    fn the_final_predicate_skips_the_auxiliaries_and_the_particles() {
        assert_eq!(
            final_predicates("設定を比べました。").wago_ratio(),
            Some(1.0)
        );
        assert_eq!(
            final_predicates("設定を比較していません。").wago_ratio(),
            Some(0.0)
        );
    }

    #[test]
    fn a_sentence_that_does_not_end_with_a_verb_is_outside_the_final_predicates() {
        assert_eq!(final_predicates("これは規則だ。").total(), 0);
        assert_eq!(final_predicates("窓は静かだ。").total(), 0);
        assert_eq!(final_predicates("設定を比べる").total(), 0);
        assert_eq!(final_predicates("これは規則だ。").wago_ratio(), None);
    }

    #[test]
    fn the_frequent_wago_verbs_come_in_the_order_of_their_count() {
        let predicates = final_predicates(
            "窓を開ける。設定を比べる。値を比べる。規則を数える。語を比べる。値を数える。",
        );
        assert_eq!(
            predicates.frequent(10),
            [("比べる", 3), ("数える", 2), ("開ける", 1)]
        );
        assert_eq!(predicates.frequent(2), [("比べる", 3), ("数える", 2)]);
        assert_eq!(predicates.total(), 6);
    }

    #[test]
    fn the_measures_read_the_final_wago_ratio() {
        assert_eq!(measures("設定を比べる。").final_wago_ratio, Some(1.0));
        assert_eq!(measures("設定を比較する。").final_wago_ratio, Some(0.0));
        assert_eq!(measures("これは規則だ。").final_wago_ratio, None);
    }
}
