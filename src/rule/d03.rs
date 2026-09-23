use crate::document::SegmentKind;
use crate::rule::predicate::{
    is_case_particle, is_conjunctive_particle, is_noun, is_particle, is_verb,
};
use crate::rule::{Context, DocumentRule, Finding, Layer, RuleId};
use crate::sentence::Sentence;
use crate::token::{Pos1, Token};

const ID: RuleId = RuleId::new(Layer::Density, 3);
const HINT: &str =
    "名詞と記号で済ませた説明を、例を挙げ、語を定義し、変化と対比を述べる文に書き直す";

/// 指摘とする 1000 字あたりの件数のしきい値。
const THRESHOLD: f64 = 1.00;

/// 規則を適用する日本語の文字数の下限。
const MINIMUM_JA_CHARS: usize = 1000;

/// 人間の文に普通の表現の欠如。
pub struct MissingCommonPhrase;

impl DocumentRule for MissingCommonPhrase {
    fn id(&self) -> RuleId {
        ID
    }

    fn doc_anchor(&self) -> &'static str {
        "D03"
    }

    /// 本文と表のセルの日本語が `MINIMUM_JA_CHARS` 字以上あり、その 1000 字あたりの表現の
    /// 件数が `THRESHOLD` を下回る文書。抜粋にはその件数を小数第 2 位までの切り捨てで載せる。
    fn check(&self, sentences: &[Sentence], _context: &Context) -> Vec<Finding> {
        let prose: Vec<&Sentence> = sentences.iter().filter(|s| is_prose(s)).collect();
        let ja_chars: usize = prose.iter().map(|sentence| sentence.ja_chars()).sum();
        if ja_chars < MINIMUM_JA_CHARS {
            return Vec::new();
        }
        let count: usize = prose
            .iter()
            .map(|sentence| occurrences(sentence.tokens()))
            .sum();
        if 1000.0 * count as f64 / ja_chars as f64 >= THRESHOLD {
            return Vec::new();
        }
        let hundredths = count * 100_000 / ja_chars;
        vec![Finding::new(
            ID,
            prose[0].segment().origin.clone(),
            format!(
                "1000 字あたり {}.{:02} 件",
                hundredths / 100,
                hundredths % 100
            ),
            HINT,
        )]
    }
}

/// Token 列の位置から始まる表現か。
type Phrase = fn(&[Token], usize) -> bool;

const PHRASES: [Phrase; 9] = [
    becomes,
    goes_on,
    about,
    for_example,
    is_called,
    however,
    often,
    difficult_or_easy,
    occasion,
];

fn is_prose(sentence: &Sentence) -> bool {
    matches!(
        sentence.segment().kind,
        SegmentKind::Prose | SegmentKind::TableCell
    )
}

/// Token 列に現れる表現の件数。
fn occurrences(tokens: &[Token]) -> usize {
    (0..tokens.len())
        .map(|index| {
            PHRASES
                .iter()
                .filter(|phrase| phrase(tokens, index))
                .count()
        })
        .sum()
}

/// 位置に「〜ようになる」があるか。
fn becomes(tokens: &[Token], index: usize) -> bool {
    let Some(window) = tokens.get(index..index + 3) else {
        return false;
    };
    is_lemma_of(&window[0], Pos1::AdjectivalNoun, &["よう", "様"])
        && window[1].surface == "に"
        && is_verb(&window[2], "なる")
}

/// 位置に「〜ていく」があるか。
fn goes_on(tokens: &[Token], index: usize) -> bool {
    let Some(window) = tokens.get(index..index + 2) else {
        return false;
    };
    is_conjunctive_particle(&window[0])
        && matches!(window[0].surface.as_str(), "て" | "で")
        && matches!(window[1].lemma.as_str(), "いく" | "行く")
        && window[1].pos.pos2 == "非自立可能"
}

/// 位置に「について」があるか。
fn about(tokens: &[Token], index: usize) -> bool {
    let Some(window) = tokens.get(index..index + 3) else {
        return false;
    };
    is_particle(&window[0], "に")
        && matches!(window[1].lemma.as_str(), "つく" | "就く")
        && matches!(window[1].surface.as_str(), "つい" | "就い")
        && window[2].surface == "て"
}

/// 位置に「例えば」があるか。
fn for_example(tokens: &[Token], index: usize) -> bool {
    matches!(tokens[index].lemma.as_str(), "例えば" | "たとえば")
}

/// 位置に「と呼ぶ」か「と呼ばれる」があるか。
fn is_called(tokens: &[Token], index: usize) -> bool {
    let Some(window) = tokens.get(index..index + 2) else {
        return false;
    };
    is_case_particle(&window[0], "と") && window[1].lemma == "呼ぶ"
}

/// 位置に接続詞の「しかし」があるか。
fn however(tokens: &[Token], index: usize) -> bool {
    is_lemma_of(&tokens[index], Pos1::Conjunction, &["しかし"])
}

/// 位置に副詞の「よく」があるか。
fn often(tokens: &[Token], index: usize) -> bool {
    is_lemma_of(&tokens[index], Pos1::Adverb, &["よく", "良く"])
}

/// 位置に形容詞の「難しい」か形状詞の「簡単」があるか。
fn difficult_or_easy(tokens: &[Token], index: usize) -> bool {
    let token = &tokens[index];
    is_lemma_of(token, Pos1::Adjective, &["難しい", "むずかしい"])
        || is_lemma_of(token, Pos1::AdjectivalNoun, &["簡単", "かんたん"])
}

/// 位置に名詞の「際」があるか。
fn occasion(tokens: &[Token], index: usize) -> bool {
    is_noun(&tokens[index], "際")
}

/// 品詞が一致し、原形がいずれかの語である Token。
fn is_lemma_of(token: &Token, pos1: Pos1, lemmas: &[&str]) -> bool {
    token.pos.pos1 == pos1 && lemmas.contains(&token.lemma.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::harness;
    use crate::token::{Goshu, Pos};

    /// 表現を含まない日本語 5 字の文。
    const FILLER: &str = "値を数える。";

    /// 表現を 1 つ含む日本語 5 字の文。
    const PHRASE: &str = "例えば読む。";

    /// 例の 1 文の日本語の字数。
    const SENTENCE_JA_CHARS: usize = 5;

    fn excerpts(text: &str) -> Vec<String> {
        harness::document_excerpts(&MissingCommonPhrase, text)
    }

    fn excerpts_of_kinds(segments: &[(SegmentKind, &str)]) -> Vec<String> {
        harness::document_excerpts_of_kinds(&MissingCommonPhrase, segments)
    }

    fn occurrences_in(text: &str) -> usize {
        harness::with_sentences(text, |sentences| {
            sentences
                .iter()
                .map(|sentence| occurrences(sentence.tokens()))
                .sum()
        })
    }

    /// 表現を含む文を `phrases` 文、含まない文を残りに並べた、日本語 `ja_chars` 字の文書。
    fn document(phrases: usize, ja_chars: usize) -> String {
        let sentences = ja_chars / SENTENCE_JA_CHARS;
        format!(
            "{}{}",
            PHRASE.repeat(phrases),
            FILLER.repeat(sentences - phrases)
        )
    }

    fn excerpt(value: f64) -> String {
        format!("1000 字あたり {value:.2} 件")
    }

    #[test]
    fn each_phrase_is_counted() {
        for text in [
            "値を読めるようになる。",
            "値を数える様になる。",
            "値が増えていく。",
            "値を読んでいく。",
            "規則について書く。",
            "例えば読む。",
            "たとえば読む。",
            "この値を閾値と呼ぶ。",
            "閾値と呼ばれる値を読む。",
            "しかし値は変わらない。",
            "よく使う。",
            "良く使う。",
            "設定は難しい。",
            "設定は簡単だ。",
            "読む際に確かめる。",
        ] {
            assert_eq!(occurrences_in(text), 1, "{text}");
        }
    }

    #[test]
    fn a_similar_sequence_is_not_counted() {
        for text in [
            "登録すると呼ばれる。",
            "性能がよくなる。",
            "性能が良くなる。",
            "駅に着いて書く。",
            "実際に読む。",
            "国際の規則を読む。",
            "車で行く。",
            "東京に行く。",
            "歩きながらいく。",
            "値を分けていくつかにする。",
            "職につきて働く。",
            "静かについて歩く。",
            "お客様になる。",
            FILLER,
        ] {
            assert_eq!(occurrences_in(text), 0, "{text}");
        }
    }

    /// 原形が `lemma` で、品詞の大分類が `pos1` の Token。
    fn token(lemma: &str, pos1: Pos1) -> Token {
        Token {
            surface: lemma.to_string(),
            lemma: lemma.to_string(),
            pos: Pos {
                pos1,
                pos2: String::new(),
                pos3: String::new(),
            },
            ctype: None,
            cform: None,
            goshu: Goshu::Wago,
            byte_range: 0..lemma.len(),
        }
    }

    #[test]
    fn a_word_is_counted_only_in_its_part_of_speech() {
        for (lemma, counted, other) in [
            ("よく", Pos1::Adverb, Pos1::Noun),
            ("良く", Pos1::Adverb, Pos1::Noun),
            ("しかし", Pos1::Conjunction, Pos1::Adverb),
            ("際", Pos1::Noun, Pos1::Suffix),
            ("難しい", Pos1::Adjective, Pos1::Noun),
            ("簡単", Pos1::AdjectivalNoun, Pos1::Noun),
        ] {
            assert_eq!(occurrences(&[token(lemma, counted)]), 1, "{lemma}");
            assert_eq!(occurrences(&[token(lemma, other)]), 0, "{lemma}");
        }
    }

    #[test]
    fn a_space_between_the_words_keeps_the_phrase() {
        assert_eq!(occurrences_in("規則に ついて書く。"), 1);
        assert_eq!(occurrences_in("値が増えて いく。"), 1);
    }

    #[test]
    fn the_excerpt_truncates_the_value() {
        assert_eq!(excerpts(&document(2, 2005)), ["1000 字あたり 0.99 件"]);
    }

    #[test]
    fn the_phrases_of_one_sentence_are_counted_each() {
        assert_eq!(occurrences_in("例えば規則について書く際は難しい。"), 4);
    }

    #[test]
    fn a_document_without_the_phrases_is_a_finding() {
        assert_eq!(excerpts(&document(0, MINIMUM_JA_CHARS)), [excerpt(0.0)]);
    }

    #[test]
    fn a_document_below_the_minimum_is_not_judged() {
        assert!(excerpts(&document(0, MINIMUM_JA_CHARS - SENTENCE_JA_CHARS)).is_empty());
    }

    #[test]
    fn a_value_from_the_threshold_up_is_not_a_finding() {
        let ja_chars = 2 * MINIMUM_JA_CHARS;
        let per_mille = ja_chars as f64 / 1000.0;
        let at = (THRESHOLD * per_mille).ceil() as usize;
        assert!(excerpts(&document(at, ja_chars)).is_empty());
        assert_eq!(
            excerpts(&document(at - 1, ja_chars)),
            [excerpt((at - 1) as f64 / per_mille)]
        );
    }

    #[test]
    fn a_table_cell_is_counted_with_the_prose() {
        let half = document(0, MINIMUM_JA_CHARS / 2);
        assert_eq!(
            excerpts_of_kinds(&[(SegmentKind::TableCell, &half), (SegmentKind::Prose, &half)]),
            [excerpt(0.0)]
        );
        let phrases = PHRASE.repeat(MINIMUM_JA_CHARS / SENTENCE_JA_CHARS / 2);
        assert!(
            excerpts_of_kinds(&[
                (SegmentKind::TableCell, &phrases),
                (SegmentKind::Prose, &half)
            ])
            .is_empty()
        );
    }

    #[test]
    fn a_segment_outside_the_prose_is_not_counted() {
        let phrases = PHRASE.repeat(MINIMUM_JA_CHARS / SENTENCE_JA_CHARS);
        let prose = document(0, MINIMUM_JA_CHARS);
        let long = document(0, 2 * MINIMUM_JA_CHARS);
        for kind in [
            SegmentKind::Comment,
            SegmentKind::DocComment,
            SegmentKind::StringLiteral,
            SegmentKind::CommitSubject,
            SegmentKind::CommitBody,
        ] {
            assert_eq!(
                excerpts_of_kinds(&[(kind, &phrases), (SegmentKind::Prose, &prose)]),
                [excerpt(0.0)],
                "{kind:?}"
            );
            assert!(excerpts_of_kinds(&[(kind, &long)]).is_empty(), "{kind:?}");
        }
    }
}
