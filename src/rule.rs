use std::fmt;
use std::str::FromStr;

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::document::Origin;
use crate::sentence::{self, Sentence};

mod context;
mod d01;
mod d02;
mod f01;
mod f02;
mod f03;
mod f04;
pub(crate) use f04::ARROWS;
mod f05;
mod f06;
mod g01;
#[cfg(test)]
pub(crate) mod harness;
mod l01;
mod l02;
mod l03;
pub(crate) mod predicate;
mod r02;
mod r03;
mod run;
mod s01;
mod s02;
mod s03;
mod s04;
mod s05;
mod s06;
mod surface;

pub use context::{Context, WordList};

/// 規則集。
const RULES: &str = include_str!("../skills/unlp/reference/rules.md");

/// 規則の層。指摘の重みと、どの条件で数えるかを決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// 構造
    Structure,
    /// 語彙
    Lexical,
    /// 密度
    Density,
    /// レジスター
    Register,
    /// 定型句
    Formulaic,
    /// 語種
    Goshu,
}

impl Layer {
    const ALL: [Self; 6] = [
        Self::Structure,
        Self::Lexical,
        Self::Density,
        Self::Register,
        Self::Formulaic,
        Self::Goshu,
    ];

    /// 規則 ID の先頭に置く文字。
    pub fn letter(self) -> char {
        match self {
            Self::Structure => 'S',
            Self::Lexical => 'L',
            Self::Density => 'D',
            Self::Register => 'R',
            Self::Formulaic => 'F',
            Self::Goshu => 'G',
        }
    }

    /// その文字を `letter` が返す層。
    pub fn from_letter(letter: char) -> Option<Self> {
        Self::ALL.into_iter().find(|layer| layer.letter() == letter)
    }

    /// 層の名前。
    pub fn name(self) -> &'static str {
        match self {
            Self::Structure => "構造",
            Self::Lexical => "語彙",
            Self::Density => "密度",
            Self::Register => "レジスター",
            Self::Formulaic => "定型句",
            Self::Goshu => "語種",
        }
    }
}

/// 規則の識別子。層の文字と 2 桁の番号で表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleId {
    layer: Layer,
    number: u8,
}

impl RuleId {
    /// 層と 1〜99 の番号から作る。
    pub const fn new(layer: Layer, number: u8) -> Self {
        assert!(matches!(number, 1..=99));
        Self { layer, number }
    }

    pub fn layer(self) -> Layer {
        self.layer
    }
}

/// 規則 ID として解釈できない文字列。
#[derive(Debug, Error)]
#[error("規則 ID の形ではない: {0}")]
pub struct ParseRuleIdError(String);

impl FromStr for RuleId {
    type Err = ParseRuleIdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || ParseRuleIdError(text.to_string());
        let mut chars = text.chars();
        let layer = chars
            .next()
            .and_then(Layer::from_letter)
            .ok_or_else(error)?;
        let number = chars.as_str();
        if number.len() != 2 {
            return Err(error());
        }
        match number.parse() {
            Ok(number @ 1..=99) => Ok(Self::new(layer, number)),
            _ => Err(error()),
        }
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{:02}", self.layer.letter(), self.number)
    }
}

impl Serialize for RuleId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// 規則に一致した箇所。
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    rule: RuleId,
    layer: Layer,
    origin: Origin,
    excerpt: String,
    hint: &'static str,
}

impl Finding {
    pub fn new(rule: RuleId, origin: Origin, excerpt: String, hint: &'static str) -> Self {
        Self {
            layer: rule.layer(),
            rule,
            origin,
            excerpt,
            hint,
        }
    }

    pub fn rule(&self) -> RuleId {
        self.rule
    }

    pub fn layer(&self) -> Layer {
        self.layer
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }

    pub fn hint(&self) -> &'static str {
        self.hint
    }
}

/// 1 文を対象とする規則。
pub trait SentenceRule {
    fn id(&self) -> RuleId;

    /// 規則集の見出しを指す anchor。
    fn doc_anchor(&self) -> &'static str;

    fn check(&self, sentence: &Sentence, context: &Context) -> Vec<Finding>;
}

/// 文書の文の列を対象とする規則。
pub trait DocumentRule {
    fn id(&self) -> RuleId;

    /// 規則集の見出しを指す anchor。
    fn doc_anchor(&self) -> &'static str;

    fn check(&self, sentences: &[Sentence], context: &Context) -> Vec<Finding>;
}

/// 文の規則の一覧。
pub fn sentence_rules() -> Vec<Box<dyn SentenceRule>> {
    vec![
        Box::new(s01::InanimateSpeaker),
        Box::new(s02::LeadingDemonstrative),
        Box::new(s03::Scaffolding),
        Box::new(s04::FirstPerson),
        Box::new(s05::NegativeContrast),
        Box::new(l01::NativizedTerm),
        Box::new(l02::LiteralTranslation),
        Box::new(l03::EnglishRhythm),
        Box::new(d02::PredicatelessFragment),
        Box::new(r02::PhysicalMetaphor),
        Box::new(f01::ClosingFormula),
        Box::new(f02::BoldConclusion),
        Box::new(f03::BoldInProse),
        Box::new(f04::DashAndArrow),
        Box::new(f05::EmptyEmphasis),
        Box::new(f06::Circumlocution),
    ]
}

/// 文書の規則の一覧。
pub fn document_rules() -> Vec<Box<dyn DocumentRule>> {
    vec![
        Box::new(s06::LeadingConjunction),
        Box::new(d01::ShortSentenceRun),
        Box::new(r03::MixedRegister),
        Box::new(g01::FinalWagoRatio),
    ]
}

/// 一覧にある規則の ID と anchor。
pub fn registered() -> Vec<(RuleId, &'static str)> {
    sentence_rules()
        .iter()
        .map(|rule| (rule.id(), rule.doc_anchor()))
        .chain(
            document_rules()
                .iter()
                .map(|rule| (rule.id(), rule.doc_anchor())),
        )
        .collect()
}

/// anchor が指す規則集の見出し。ID に続く一文を返す。
pub fn doc_heading(anchor: &str) -> Option<&'static str> {
    RULES
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .find_map(|heading| heading.strip_prefix(anchor)?.strip_prefix(' '))
        .map(str::trim)
}

/// 一覧にある規則を適用した指摘。文の順、規則の順に並ぶ。日本語文字数が `floor` 未満の
/// ときは、割合で評価する層の `DocumentRule` を適用しない。
pub fn check(sentences: &[Sentence], context: &Context, floor: usize) -> Vec<Finding> {
    let mut findings = Vec::new();
    let rules = sentence_rules();
    for sentence in sentences {
        for rule in &rules {
            findings.extend(rule.check(sentence, context));
        }
    }
    let below_floor = crate::score::below_floor(sentence::ja_chars(sentences), floor);
    for rule in document_rules() {
        if below_floor && !applies_below_floor(rule.id().layer()) {
            continue;
        }
        findings.extend(rule.check(sentences, context));
    }
    findings
}

/// 下限未満の入力にも `DocumentRule` を適用する層か。密度、レジスター、語種は文書全体の
/// 割合で評価するため、下限以上の入力でだけ適用する。
fn applies_below_floor(layer: Layer) -> bool {
    match layer {
        Layer::Structure | Layer::Lexical | Layer::Formulaic => true,
        Layer::Density | Layer::Register | Layer::Goshu => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_id_is_read_back_from_its_text() {
        assert_eq!(
            "S01".parse::<RuleId>().unwrap(),
            RuleId::new(Layer::Structure, 1)
        );
        assert_eq!(
            "G01".parse::<RuleId>().unwrap(),
            RuleId::new(Layer::Goshu, 1)
        );
        for text in ["", "S", "S0", "S00", "S1", "S001", "X01", "s01", "SAB"] {
            assert!(text.parse::<RuleId>().is_err(), "{text}");
        }
    }

    #[test]
    fn every_rule_points_at_a_heading_of_the_rule_book() {
        for (rule, anchor) in registered() {
            assert!(
                doc_heading(anchor).is_some_and(|heading| !heading.is_empty()),
                "{rule} の anchor {anchor} に対応する見出しが無い"
            );
        }
    }

    #[test]
    fn every_weighted_rule_has_a_heading() {
        for rule in Context::defaults().weights().keys() {
            assert!(doc_heading(&rule.to_string()).is_some(), "{rule}");
        }
    }

    #[test]
    fn a_heading_is_read_without_its_id() {
        assert_eq!(doc_heading("S01"), Some("文書・型・検査を語り手にしない"));
        assert_eq!(doc_heading("S0"), None);
        assert_eq!(doc_heading("Z99"), None);
    }

    #[test]
    fn the_layers_of_a_ratio_wait_for_the_floor() {
        for layer in [Layer::Structure, Layer::Lexical, Layer::Formulaic] {
            assert!(applies_below_floor(layer), "{}", layer.name());
        }
        for layer in [Layer::Density, Layer::Register, Layer::Goshu] {
            assert!(!applies_below_floor(layer), "{}", layer.name());
        }
    }

    #[test]
    fn every_layer_letter_maps_back_to_its_layer() {
        for layer in Layer::ALL {
            assert_eq!(Layer::from_letter(layer.letter()), Some(layer));
        }
        assert_eq!(Layer::from_letter('X'), None);
    }
}
