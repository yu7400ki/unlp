use crate::rule::{Context, DocumentRule, Finding, Layer, RuleId, run, surface};
use crate::sentence::Sentence;

const ID: RuleId = RuleId::new(Layer::Density, 1);
const HINT: &str = "短い文を 3 つ以上続けない";

/// 短文として数える日本語の文字数の上限。
const JA_CHARS: usize = 15;

/// 短文の連打。
pub struct ShortSentenceRun;

impl DocumentRule for ShortSentenceRun {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "D01"
    }

    /// 短文が続く範囲。抜粋はその範囲の最初の文。
    fn check(&self, sentences: &[Sentence], _context: &Context) -> Vec<Finding> {
        run::runs(sentences, is_short)
            .into_iter()
            .flat_map(|run| {
                let first = &run[0];
                let whole = 0..first.text().len();
                surface::findings_at(ID, first, vec![whole], HINT)
            })
            .collect()
    }
}

/// 句点で終わり、`JA_CHARS` 字までの日本語で書かれた文。
fn is_short(sentence: &Sentence) -> bool {
    sentence.is_terminated() && sentence.ja_chars() <= JA_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;

    fn excerpts(text: &str) -> Vec<String> {
        harness::document_excerpts(&ShortSentenceRun, text)
    }

    #[test]
    fn a_run_of_short_sentences_is_a_finding() {
        assert_eq!(excerpts("窓が開く。鍵が回る。値が減る。"), ["窓が開く。"]);
        assert_eq!(excerpts("暗い。鳴る。最後だ。終わる。"), ["暗い。"]);
    }

    #[test]
    fn a_run_below_three_sentences_is_not_a_finding() {
        assert!(excerpts("窓が開く。鍵が回る。").is_empty());
    }

    #[test]
    fn a_sentence_beyond_the_length_ends_the_run() {
        assert!(
            excerpts("窓が開く。鍵が回る。この文は十五字を超える長さで書いてある。値が減る。")
                .is_empty()
        );
        assert_eq!(
            excerpts(
                "窓が開く。鍵が回る。値が減る。この文は十五字を超える長さで書いてある。日が沈む。風が吹く。雨が降る。"
            ),
            ["窓が開く。", "日が沈む。"]
        );
    }

    #[test]
    fn the_length_counts_only_the_japanese_chars() {
        let sentence = "JSON と CSV と TOML の値を返す。";
        assert_eq!(excerpts(&sentence.repeat(3)), [sentence]);
    }

    #[test]
    fn a_sentence_at_the_upper_length_is_still_short() {
        let short = format!("{}。", "あ".repeat(JA_CHARS));
        let long = format!("{}。", "あ".repeat(JA_CHARS + 1));
        assert_eq!(excerpts(&short.repeat(3)), [short.as_str()]);
        assert!(excerpts(&long.repeat(3)).is_empty());
    }

    #[test]
    fn a_sentence_without_a_full_stop_ends_the_run() {
        assert!(excerpts("窓が開く。鍵が回る\n値が減る。").is_empty());
    }

    #[test]
    fn short_items_of_a_list_are_not_a_run() {
        let items = ["窓が開く。", "鍵が回る。", "値が減る。"];
        assert!(
            harness::document_excerpts_of(&ShortSentenceRun, &items).is_empty(),
            "{items:?}"
        );
    }

    #[test]
    fn short_sentences_of_one_paragraph_are_a_run() {
        assert_eq!(
            harness::document_excerpts_of(
                &ShortSentenceRun,
                &["窓が開く。鍵が回る。値が減る。", "日が沈む。"]
            ),
            ["窓が開く。"]
        );
    }
}
