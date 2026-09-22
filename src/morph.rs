use std::borrow::Cow;
use std::{error, result};

use lindera::dictionary::{DictionaryKind, load_embedded_dictionary};
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::token::Token as LinderaToken;
use thiserror::Error;

use crate::document::{Document, SegmentKind};
use crate::sentence::{self, Sentence};
use crate::token::{Goshu, Pos, Pos1, Token};

const POS1: &str = "part_of_speech";
const POS2: &str = "part_of_speech_subcategory_1";
const POS3: &str = "part_of_speech_subcategory_2";
const CTYPE: &str = "conjugation_type";
const CFORM: &str = "conjugation_form";
const LEMMA: &str = "orthographic_base_form";
const GOSHU: &str = "word_type";

/// Token の欄に写す辞書の列。
const COLUMNS: [&str; 7] = [POS1, POS2, POS3, CTYPE, CFORM, LEMMA, GOSHU];

/// 形態素解析で生じる誤り。
#[derive(Debug, Error)]
pub enum Error {
    #[error("同梱した辞書を読み込めない")]
    Dictionary(#[source] Box<dyn error::Error + Send + Sync>),
    #[error("辞書のスキーマに列 {column} が無い")]
    MissingColumn { column: &'static str },
}

pub type Result<T> = result::Result<T, Error>;

/// 同梱した UniDic で文を形態素解析する。
pub struct Analyzer {
    segmenter: Segmenter,
}

impl Analyzer {
    /// 同梱した辞書を読み込む。辞書を読めないとき、および Token の欄に写す列の名前が
    /// 辞書のスキーマに無いときは、ここで誤りを返す。
    pub fn new() -> Result<Self> {
        let dictionary = load_embedded_dictionary(DictionaryKind::UniDic)
            .map_err(|error| Error::Dictionary(Box::new(error)))?;
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

    /// 文書を文に分割し、解析した文を返す。表のセルの文は、述語を持つものだけを返す。
    pub fn analyze_document<'a>(&self, document: &'a Document) -> Vec<Sentence<'a>> {
        let mut sentences = sentence::split_document(document);
        for sentence in &mut sentences {
            self.analyze(sentence);
        }
        sentences.retain(is_scored);
        sentences
    }

    /// 文を解析して Token 列を持たせる。書字形基本形を持たない語は表層形を `lemma` にし、
    /// 隣り合う記号-文字の並びは 1 つの名詞にする。
    fn analyze(&self, sentence: &mut Sentence) {
        let mut analyzed = self
            .segmenter
            .segment(Cow::Borrowed(sentence.text()))
            .expect("読み込めた辞書での文の解析は成功する");
        let tokens = analyzed.iter_mut().map(token).collect();
        sentence.set_tokens(join_letters(tokens));
    }
}

/// 採点する文か。表のセルは語だけを並べることがあり、句点で終わるか述語を持つ文だけを採点する。
fn is_scored(sentence: &Sentence) -> bool {
    sentence.segment().kind != SegmentKind::TableCell
        || sentence.is_terminated()
        || sentence.has_predicate()
}

/// 隣り合う記号-文字の Token を 1 つの名詞にする。
fn join_letters(tokens: Vec<Token>) -> Vec<Token> {
    let mut joined: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut after_letter = false;
    for token in tokens {
        let letter = is_letter(&token);
        match joined.last_mut() {
            Some(word)
                if after_letter && letter && word.byte_range.end == token.byte_range.start =>
            {
                absorb(word, token);
            }
            _ => joined.push(token),
        }
        after_letter = letter;
    }
    joined
}

fn is_letter(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Symbol && token.pos.pos2 == "文字"
}

/// 後続の文字を取り込み、綴りを 1 語の名詞として持たせる。
fn absorb(word: &mut Token, letter: Token) {
    word.surface.push_str(&letter.surface);
    word.byte_range.end = letter.byte_range.end;
    word.lemma = word.surface.clone();
    word.pos = Pos {
        pos1: Pos1::Noun,
        pos2: "普通名詞".to_string(),
        pos3: "一般".to_string(),
    };
    word.ctype = None;
    word.cform = None;
    word.goshu = Goshu::Unknown;
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
            ANALYZER.analyze(sentence);
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

    fn cell(text: &str) -> Segment {
        Segment {
            kind: SegmentKind::TableCell,
            ..segment(text)
        }
    }

    #[test]
    fn a_table_cell_keeps_only_the_sentences_that_predicate() {
        let document = Document {
            name: "t".to_string(),
            segments: vec![
                cell("表のセル"),
                cell("失敗する"),
                cell("名詞の列挙。"),
                segment("本文だ。"),
            ],
        };

        let sentences = ANALYZER.analyze_document(&document);
        let texts: Vec<&str> = sentences.iter().map(Sentence::text).collect();
        assert_eq!(texts, ["失敗する", "名詞の列挙。", "本文だ。"]);
        assert_eq!(sentence::ja_chars(&sentences), 12);
    }

    #[test]
    fn a_document_yields_analyzed_sentences() {
        let document = Document {
            name: "t".to_string(),
            segments: vec![segment("型が名乗る。"), segment("次の文だ。設定を比べる。")],
        };

        let sentences = ANALYZER.analyze_document(&document);
        assert_eq!(sentences.len(), 3);
        for sentence in &sentences {
            assert!(!sentence.tokens().is_empty(), "{}", sentence.text());
        }
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
    fn the_lemma_is_the_base_form_as_it_is_written() {
        let tokens = tokens("ウィンドウを表示した。東京でできることをする。");
        assert_eq!(find(&tokens, "ウィンドウ").lemma, "ウィンドウ");
        assert_eq!(find(&tokens, "東京").lemma, "東京");
        assert_eq!(find(&tokens, "できる").lemma, "できる");
        assert_eq!(find(&tokens, "し").lemma, "する");
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

    /// 辞書が返す品詞と `Pos1` の対応。空白は `Segmenter` が空白の Token を落とすので
    /// 生じず、16 分類のうち残る 15 をここで突き合わせる。
    #[test]
    fn every_pos_of_the_dictionary_has_its_own_variant() {
        let tokens = tokens(concat!(
            "これはとても静かな部屋だ。",
            "しかしこの本は高い。",
            "ああ、そうします。",
            "ご説明は第一章の内容的なものです。",
            "α を並べる。",
        ));
        let expected = [
            ("部屋", Pos1::Noun),
            ("これ", Pos1::Pronoun),
            ("静か", Pos1::AdjectivalNoun),
            ("この", Pos1::Adnominal),
            ("とても", Pos1::Adverb),
            ("しかし", Pos1::Conjunction),
            ("ああ", Pos1::Interjection),
            ("並べる", Pos1::Verb),
            ("高い", Pos1::Adjective),
            ("だ", Pos1::AuxVerb),
            ("は", Pos1::Particle),
            ("第", Pos1::Prefix),
            ("的", Pos1::Suffix),
            ("α", Pos1::Symbol),
            ("。", Pos1::SupplementarySymbol),
        ];
        for (surface, pos1) in expected {
            assert_eq!(find(&tokens, surface).pos.pos1, pos1, "{surface}");
        }
    }

    /// 辞書が返す語種と `Goshu` の対応。
    #[test]
    fn every_goshu_of_the_dictionary_has_its_own_variant() {
        let tokens = tokens("田中さんが本とゴムを買った。サボることが README に載る。");
        let expected = [
            ("が", Goshu::Wago),
            ("本", Goshu::Kango),
            ("ゴム", Goshu::Gairai),
            ("サボる", Goshu::Konshu),
            ("。", Goshu::Symbol),
            ("田中", Goshu::Proper),
            ("README", Goshu::Unknown),
        ];
        for (surface, goshu) in expected {
            assert_eq!(find(&tokens, surface).goshu, goshu, "{surface}");
        }
    }

    #[test]
    fn a_run_of_letters_is_one_noun() {
        let tokens = tokens("README と doc と api と CLI と Claude を読む。");
        for surface in ["README", "doc", "api", "CLI", "Claude"] {
            let token = find(&tokens, surface);
            assert_eq!(token.pos.pos1, Pos1::Noun, "{surface}");
            assert_eq!(token.lemma, surface, "{surface}");
        }

        let merged = find(&tokens, "api");
        assert_eq!(merged.pos, find(&tokens, "README").pos);
        assert_eq!(merged.goshu, Goshu::Unknown);
        assert_eq!(merged.ctype, None);
        assert_eq!(merged.cform, None);
    }

    #[test]
    fn letters_separated_by_a_space_stay_apart() {
        let tokens = tokens("a b が並ぶ。");
        assert_eq!(find(&tokens, "a").pos.pos1, Pos1::Symbol);
        assert_eq!(find(&tokens, "b").pos.pos1, Pos1::Symbol);
    }

    #[test]
    fn a_lone_letter_stays_a_symbol() {
        let tokens = tokens("α を並べる。");
        assert_eq!(find(&tokens, "α").pos.pos1, Pos1::Symbol);
    }

    #[test]
    fn the_byte_range_points_into_the_sentence() {
        let segment = segment("設定の doc を比べる。次の文だ。");
        let mut sentences = split_sentences(&segment);
        assert_eq!(sentences.len(), 2);
        for sentence in &mut sentences {
            ANALYZER.analyze(sentence);
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
