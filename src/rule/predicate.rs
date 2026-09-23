use crate::token::{Pos1, Token};

/// 表層が一致する助詞。
pub fn is_particle(token: &Token, surface: &str) -> bool {
    token.pos.pos1 == Pos1::Particle && token.surface == surface
}

/// 表層が一致する格助詞。
pub fn is_case_particle(token: &Token, surface: &str) -> bool {
    is_particle(token, surface) && token.pos.pos2 == "格助詞"
}

/// 接続助詞であるか。
pub fn is_conjunctive_particle(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Particle && token.pos.pos2 == "接続助詞"
}

/// 読点。
pub fn is_comma(token: &Token) -> bool {
    token.pos.pos1 == Pos1::SupplementarySymbol && token.pos.pos2 == "読点"
}

/// サ変可能の名詞。
pub fn is_sahen_noun(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Noun && token.pos.pos3 == "サ変可能"
}

/// 原形が一致する名詞。
pub fn is_noun(token: &Token, lemma: &str) -> bool {
    token.pos.pos1 == Pos1::Noun && token.lemma == lemma
}

/// 原形が一致する動詞。
pub fn is_verb(token: &Token, lemma: &str) -> bool {
    token.pos.pos1 == Pos1::Verb && token.lemma == lemma
}

/// 原形が一致する助動詞。
pub fn is_aux_verb(token: &Token, lemma: &str) -> bool {
    token.pos.pos1 == Pos1::AuxVerb && token.lemma == lemma
}
