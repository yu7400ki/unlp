use std::ops::Range;

use crate::rule::WordList;
use crate::token::{Pos1, Token};

/// 原形が欄にある動詞の範囲。活用形は原形で照合する。
pub fn verbs(tokens: &[Token], list: &WordList, group: &str) -> Vec<Range<usize>> {
    tokens
        .iter()
        .filter(|token| token.pos.pos1 == Pos1::Verb && list.contains(group, &token.lemma))
        .map(|token| token.byte_range.clone())
        .collect()
}
