use std::borrow::Cow;
use std::result;

use lindera::dictionary::{DictionaryKind, load_embedded_dictionary};
use lindera::error::LinderaError;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::token::Token as LinderaToken;
use thiserror::Error;

use crate::sentence::Sentence;
use crate::token::{Goshu, Pos, Pos1, Token};

const POS1: &str = "part_of_speech";
const POS2: &str = "part_of_speech_subcategory_1";
const POS3: &str = "part_of_speech_subcategory_2";
const CTYPE: &str = "conjugation_type";
const CFORM: &str = "conjugation_form";
const LEMMA: &str = "lexeme";
const GOSHU: &str = "word_type";

/// Token の欄に写す辞書の列。
const COLUMNS: [&str; 7] = [POS1, POS2, POS3, CTYPE, CFORM, LEMMA, GOSHU];

/// 形態素解析で生じる誤り。
#[derive(Debug, Error)]
pub enum Error {
    #[error("同梱した辞書を読み込めない")]
    Dictionary(#[source] LinderaError),
    #[error("辞書のスキーマに列 {column} が無い")]
    MissingColumn { column: &'static str },
    #[error("文を解析できない: {text}")]
    Segment {
        text: String,
        #[source]
        source: LinderaError,
    },
}

pub type Result<T> = result::Result<T, Error>;

/// 同梱した UniDic で文を形態素解析する。
pub struct Analyzer {
    segmenter: Segmenter,
}

impl Analyzer {
    /// 同梱した辞書を読み込む。Token の欄に対応する列を持たないスキーマは誤りとする。
    pub fn new() -> Result<Self> {
        let dictionary =
            load_embedded_dictionary(DictionaryKind::UniDic).map_err(Error::Dictionary)?;
        let schema = &dictionary.metadata.dictionary_schema;
        for column in COLUMNS {
            if schema.get_field_index(column).is_none() {
                return Err(Error::MissingColumn { column });
            }
        }
        Ok(Self {
            segmenter: Segmenter::new(Mode::Normal, dictionary, None),
        })
    }

    /// 文を解析して Token 列を持たせる。語彙素を持たない語は表層形を `lemma` にする。
    pub fn analyze(&self, sentence: &mut Sentence) -> Result<()> {
        let text = sentence.text();
        let mut analyzed = self
            .segmenter
            .segment(Cow::Borrowed(text))
            .map_err(|source| Error::Segment {
                text: text.to_string(),
                source,
            })?;
        let tokens = analyzed.iter_mut().map(token).collect();
        sentence.set_tokens(tokens);
        Ok(())
    }
}

fn token(analyzed: &mut LinderaToken) -> Token {
    let surface = analyzed.surface.to_string();
    Token {
        lemma: column(analyzed, LEMMA).unwrap_or_else(|| surface.clone()),
        pos: Pos {
            pos1: pos1(column(analyzed, POS1).as_deref()),
            pos2: column(analyzed, POS2).unwrap_or_default(),
            pos3: column(analyzed, POS3).unwrap_or_default(),
        },
        ctype: column(analyzed, CTYPE),
        cform: column(analyzed, CFORM),
        goshu: goshu(column(analyzed, GOSHU).as_deref()),
        byte_range: analyzed.byte_start..analyzed.byte_end,
        surface,
    }
}

/// 列の値。辞書が値を持たない `*` は `None`。
fn column(analyzed: &mut LinderaToken, name: &str) -> Option<String> {
    analyzed
        .get(name)
        .filter(|value| *value != "*")
        .map(str::to_string)
}

fn pos1(value: Option<&str>) -> Pos1 {
    match value {
        Some("名詞") => Pos1::Noun,
        Some("代名詞") => Pos1::Pronoun,
        Some("形状詞") => Pos1::AdjectivalNoun,
        Some("連体詞") => Pos1::Adnominal,
        Some("副詞") => Pos1::Adverb,
        Some("接続詞") => Pos1::Conjunction,
        Some("感動詞") => Pos1::Interjection,
        Some("動詞") => Pos1::Verb,
        Some("形容詞") => Pos1::Adjective,
        Some("助動詞") => Pos1::AuxVerb,
        Some("助詞") => Pos1::Particle,
        Some("接頭辞") => Pos1::Prefix,
        Some("接尾辞") => Pos1::Suffix,
        Some("記号") => Pos1::Symbol,
        Some("補助記号") => Pos1::SupplementarySymbol,
        Some("空白") => Pos1::Whitespace,
        _ => Pos1::Other,
    }
}

fn goshu(value: Option<&str>) -> Goshu {
    match value {
        Some("和") => Goshu::Wago,
        Some("漢") => Goshu::Kango,
        Some("外") => Goshu::Gairai,
        Some("混") => Goshu::Konshu,
        Some("記号") => Goshu::Symbol,
        Some("固") => Goshu::Proper,
        _ => Goshu::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::sync::LazyLock;

    use super::*;
    use crate::document::{LineRange, Origin, Segment, SegmentKind};
    use crate::sentence::split_sentences;

    static ANALYZER: LazyLock<Analyzer> = LazyLock::new(|| Analyzer::new().unwrap());

    fn segment(text: &str) -> Segment {
        Segment {
            text: text.to_string(),
            origin: Origin {
                path: "t".to_string(),
                lines: LineRange::new(NonZeroU32::MIN, NonZeroU32::MIN).unwrap(),
                commit: None,
            },
            kind: SegmentKind::Prose,
        }
    }

    fn tokens(text: &str) -> Vec<Token> {
        let segment = segment(text);
        let mut sentences = split_sentences(&segment);
        for sentence in &mut sentences {
            ANALYZER.analyze(sentence).unwrap();
        }
        sentences
            .iter()
            .flat_map(|sentence| sentence.tokens().to_vec())
            .collect()
    }

    fn find<'a>(tokens: &'a [Token], surface: &str) -> &'a Token {
        let surfaces = || {
            tokens
                .iter()
                .map(|token| token.surface.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        tokens
            .iter()
            .find(|token| token.surface == surface)
            .unwrap_or_else(|| panic!("{surface} が無い: {}", surfaces()))
    }

    #[test]
    fn a_verb_carries_its_class_lemma_and_goshu() {
        let tokens = tokens("型の doc が名乗る。");
        let verb = find(&tokens, "名乗る");
        assert_eq!(verb.pos.pos1, Pos1::Verb);
        assert_eq!(verb.lemma, "名乗る");
        assert_eq!(verb.goshu, Goshu::Wago);

        let noun = find(&tokens, "型");
        assert_eq!(noun.pos.pos1, Pos1::Noun);
        assert_eq!(noun.goshu, Goshu::Wago);
    }

    #[test]
    fn goshu_separates_kango_from_gairai() {
        let tokens = tokens("設定のウィンドウを比べる。");
        let kango = find(&tokens, "設定");
        assert_eq!(kango.pos.pos1, Pos1::Noun);
        assert_eq!(kango.goshu, Goshu::Kango);
        assert_eq!(find(&tokens, "ウィンドウ").goshu, Goshu::Gairai);
    }

    #[test]
    fn a_conjugated_verb_carries_its_type_and_form() {
        let tokens = tokens("ウィンドウが表示され始めた。");
        let token = find(&tokens, "始め");
        assert_eq!(token.lemma, "始める");
        assert_eq!(token.ctype.as_deref(), Some("下一段-マ行"));
        assert!(token.cform.is_some(), "{:?}", token.cform);
    }

    #[test]
    fn a_word_outside_the_dictionary_keeps_its_surface_as_the_lemma() {
        let tokens = tokens("𩸽を焼く。");
        let unknown = find(&tokens, "𩸽");
        assert_eq!(unknown.lemma, "𩸽");
        assert_eq!(unknown.goshu, Goshu::Unknown);
        assert_eq!(unknown.ctype, None);
    }

    #[test]
    fn the_byte_range_points_into_the_sentence() {
        let segment = segment("設定のウィンドウを比べる。次の文だ。");
        let mut sentences = split_sentences(&segment);
        assert_eq!(sentences.len(), 2);
        for sentence in &mut sentences {
            ANALYZER.analyze(sentence).unwrap();
            let text = sentence.text();
            for token in sentence.tokens() {
                assert_eq!(&text[token.byte_range.clone()], token.surface);
            }
        }
    }

    #[test]
    fn missing_pos_levels_are_empty() {
        let tokens = tokens("型の doc が名乗る。");
        let particle = find(&tokens, "の");
        assert_eq!(particle.pos.pos1, Pos1::Particle);
        assert_eq!(particle.pos.pos2, "格助詞");
        assert_eq!(particle.pos.pos3, "");
    }
}
