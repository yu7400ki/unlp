use std::ops::Range;

use crate::rule::predicate::{is_case_particle, is_comma, is_verb};
use crate::rule::{Context, Finding, Layer, RuleId, SentenceRule, WordList, surface};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Lexical, 2);
const HINT: &str = "慣習語に置き換える: デフォルトでは、エラーにならずに失敗する、決済に失敗する、8 ワーカー構成のマシン";
const PHRASES: &str = "phrases";
const MACHINES: &str = "machines";

/// 「の」の次から格助詞「を」までに挟む Token の上限。
const GAP: usize = 6;

/// 辞書の第一義と逐語訳。
pub struct LiteralTranslation;

impl SentenceRule for LiteralTranslation {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "L02"
    }

    /// 語リストにある句が現れた箇所と、数えた物を持つ機械の形。
    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding> {
        let list = context.list(ID);
        let mut ranges = surface::matches(sentence.text(), list.words(PHRASES));
        ranges.extend(counted_possession(sentence.tokens(), list));
        surface::findings_at(ID, sentence, ranges, HINT)
    }
}

/// 数と助数詞に「の」が続き、名詞句の後に「を持つ」と語リストの名詞が並ぶ範囲。
fn counted_possession(tokens: &[Token], list: &WordList) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for index in 0..tokens.len() {
        if !is_count(&tokens[index])
            || !tokens.get(index + 1).is_some_and(is_counter)
            || !tokens
                .get(index + 2)
                .is_some_and(|token| is_case_particle(token, "の"))
        {
            continue;
        }
        if let Some(end) = possessed(tokens, index + 3, list) {
            ranges.push(tokens[index].byte_range.start..tokens[end].byte_range.end);
        }
    }
    ranges
}

/// 「を持つ」と語リストの名詞が並ぶ末尾の位置。`GAP` 個までの Token を挟み、読点をまたぐ
/// 先は見ない。
fn possessed(tokens: &[Token], from: usize, list: &WordList) -> Option<usize> {
    let last = (from + GAP).min(tokens.len().saturating_sub(1));
    for position in from..=last {
        if is_comma(&tokens[position]) {
            return None;
        }
        if is_case_particle(&tokens[position], "を") {
            let verb = tokens.get(position + 1)?;
            let noun = tokens.get(position + 2)?;
            return (is_verb(verb, "持つ") && list.contains(MACHINES, &noun.lemma))
                .then_some(position + 2);
        }
    }
    None
}

/// 数詞と「複数」。
fn is_count(token: &Token) -> bool {
    (token.pos.pos1 == Pos1::Noun && token.pos.pos2 == "数詞") || token.lemma == "複数"
}

/// 助数詞の「つ」と「個」。
fn is_counter(token: &Token) -> bool {
    matches!(token.surface.as_str(), "つ" | "個")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::excerpts(&LiteralTranslation, text)
    }

    #[test]
    fn a_phrase_of_the_list_is_a_finding() {
        assert_eq!(excerpts("既定では無効だ。"), ["既定では"]);
        assert_eq!(excerpts("書き込みは静かに失敗します。"), ["静かに失敗"]);
        assert_eq!(
            excerpts("すべてのリクエストは上流へのフォールバックとなります。"),
            ["フォールバックとなり"]
        );
        assert_eq!(excerpts("上限に丸められる。"), ["に丸められ"]);
        assert_eq!(excerpts("決済失敗を経験する。"), ["失敗を経験"]);
        assert_eq!(excerpts("速度を犠牲にして安全にする。"), ["を犠牲にし"]);
        assert_eq!(excerpts("値を明示的に指定します。"), ["明示的に指定し"]);
    }

    #[test]
    fn a_counted_thing_a_machine_holds_is_a_finding() {
        assert_eq!(
            excerpts("8つのワーカーを持つマシンでは8つのコピーができます。"),
            ["8つのワーカーを持つマシン"]
        );
        assert_eq!(
            excerpts("2 個の口を持つホストだ。"),
            ["2 個の口を持つホスト"]
        );
        assert_eq!(
            excerpts("複数個の接続を持つ環境で動作する。"),
            ["複数個の接続を持つ環境"]
        );
    }

    #[test]
    fn a_counted_thing_without_the_machine_is_not_a_finding() {
        assert!(excerpts("8つのワーカーを持つ設定だ。").is_empty());
        assert!(excerpts("8つのワーカーが動くマシンだ。").is_empty());
        assert!(excerpts("8つのワーカーを、マシンに割り当てる。").is_empty());
        assert!(excerpts("ワーカーを持つマシンだ。").is_empty());
    }

    #[test]
    fn an_ordinary_sentence_is_not_a_finding() {
        assert!(excerpts("鍵を回す。").is_empty());
        assert!(excerpts("窓を開ける。").is_empty());
        assert!(excerpts("電話を掛ける。").is_empty());
        assert!(excerpts("静かに歩く。").is_empty());
        assert!(excerpts("処理は明示的に記述する。").is_empty());
    }

    #[test]
    fn overlapping_phrases_are_one_finding() {
        assert_eq!(excerpts("直感的な意味論を導入する。"), ["直感的な意味論"]);
        assert_eq!(
            excerpts("8つの意味論を持つマシンだ。"),
            ["8つの意味論を持つマシン"]
        );
    }

    #[test]
    fn the_findings_follow_the_order_of_the_text() {
        assert_eq!(
            excerpts("既定では、8つの鍵を持つ環境で静かに失敗します。"),
            ["既定では", "8つの鍵を持つ環境", "静かに失敗"]
        );
    }
}
