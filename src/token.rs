use std::ops::Range;

use serde::Serialize;

/// 形態素解析が返した 1 語。`ctype` は活用型、`cform` は活用形で、
/// `byte_range` は文の文字列の中の位置。
#[derive(Debug, Clone, Serialize)]
pub struct Token {
    pub surface: String,
    pub lemma: String,
    pub pos: Pos,
    pub ctype: Option<String>,
    pub cform: Option<String>,
    pub goshu: Goshu,
    pub byte_range: Range<usize>,
}

/// 品詞。`pos2` と `pos3` は辞書の細分類で、無い階層は空文字列。
#[derive(Debug, Clone, Serialize)]
pub struct Pos {
    pub pos1: Pos1,
    pub pos2: String,
    pub pos3: String,
}

/// 品詞の大分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pos1 {
    /// 名詞
    Noun,
    /// 代名詞
    Pronoun,
    /// 形状詞
    AdjectivalNoun,
    /// 連体詞
    Adnominal,
    /// 副詞
    Adverb,
    /// 接続詞
    Conjunction,
    /// 感動詞
    Interjection,
    /// 動詞
    Verb,
    /// 形容詞
    Adjective,
    /// 助動詞
    AuxVerb,
    /// 助詞
    Particle,
    /// 接頭辞
    Prefix,
    /// 接尾辞
    Suffix,
    /// 記号
    Symbol,
    /// 補助記号
    SupplementarySymbol,
    /// 空白
    Whitespace,
    /// その他
    Other,
}

/// 語種。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Goshu {
    /// 和語
    Wago,
    /// 漢語
    Kango,
    /// 外来語
    Gairai,
    /// 混種語
    Konshu,
    /// 記号
    Symbol,
    /// 固有名
    Proper,
    Unknown,
}
