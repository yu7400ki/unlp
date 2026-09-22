use std::fmt;

use serde::{Serialize, Serializer};

use crate::document::Origin;

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
