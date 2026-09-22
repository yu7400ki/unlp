use crate::token::{Pos1, Token};

/// 表層が一致する格助詞。
pub fn is_case_particle(token: &Token, surface: &str) -> bool {
    is_particle(token, "格助詞", surface)
}

/// 表層が一致する係助詞。
pub fn is_topic_particle(token: &Token, surface: &str) -> bool {
    is_particle(token, "係助詞", surface)
}

fn is_particle(token: &Token, pos2: &str, surface: &str) -> bool {
    token.pos.pos1 == Pos1::Particle && token.pos.pos2 == pos2 && token.surface == surface
}

/// サ変可能の名詞。
pub fn is_sahen_noun(token: &Token) -> bool {
    token.pos.pos1 == Pos1::Noun && token.pos.pos3 == "サ変可能"
}

/// 原形が一致する動詞。
pub fn is_verb(token: &Token, lemma: &str) -> bool {
    token.pos.pos1 == Pos1::Verb && token.lemma == lemma
}

/// 原形が一致する助動詞。
pub fn is_aux_verb(token: &Token, lemma: &str) -> bool {
    token.pos.pos1 == Pos1::AuxVerb && token.lemma == lemma
}
