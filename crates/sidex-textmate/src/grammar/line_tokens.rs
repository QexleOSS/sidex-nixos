//! Plain-token + binary-token output for a single tokenized line.
//!
//! Faithful Rust port of the `LineTokens` class from `grammar.ts` (MIT,
//! Microsoft). The emitter is called from the tokenizer's hot path via
//! the [`crate::tokenizer::TokenSink`] contract; it produces either a
//! [`Vec<Token>`] (plain, debuggable) or a packed [`Vec<u32>`] stream
//! (`[startIndex, metadata, startIndex, metadata, ...]`) that Monaco
//! consumes directly as a `Uint32Array`.

use std::sync::Arc;

use crate::grammar::attr_stack::AttributedScopeStack;
use crate::grammar::balanced_brackets::BalancedBracketSelectors;
use crate::matcher::BoxMatcher;
use crate::metadata::{self, FontStyle, OptionalStandardTokenType, StandardTokenType};
use crate::tokenizer::TokenSink;

/// Token emitted in plain mode. Mirrors upstream's `IToken` interface.
#[derive(Debug, Clone)]
pub struct Token {
    pub start_index: usize,
    pub end_index: usize,
    pub scopes: Vec<String>,
}

/// Override mapping an injection/theme selector to a standard token type.
pub struct TokenTypeOverride {
    pub matcher: BoxMatcher<Vec<String>>,
    pub token_type: StandardTokenType,
}

/// RTL detection for the "merge consecutive tokens with equal metadata"
/// fast path. Upstream refuses the merge when the line contains any
/// RTL character because `BiDi` relies on per-token ordering.
///
/// The upstream regex enumerates every Unicode RTL block; Rust's
/// `regex` crate doesn't support surrogate pairs cleanly, so we instead
/// scan for characters in the known RTL scalar ranges. This is
/// slightly more Unicode-correct than upstream (no surrogate pitfalls)
/// while still returning `true` for all the same inputs upstream does.
#[must_use]
pub fn contains_rtl(s: &str) -> bool {
    for ch in s.chars() {
        let c = ch as u32;
        // Hebrew + Arabic + Syriac + Thaana + N'Ko + Samaritan + etc.
        if (0x05BE..=0x08C9).contains(&c)
            || c == 0x200F
            || (0xFB1D..=0xFDFC).contains(&c)
            || (0xFE70..=0xFEFC).contains(&c)
            || (0x10800..=0x10FFF).contains(&c)
            || (0x1E800..=0x1EFFF).contains(&c)
        {
            return true;
        }
    }
    false
}

/// Mode flag — plain `Vec<Token>` vs packed `Vec<u32>` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenEmitMode {
    Plain,
    Binary,
}

/// Collects tokens as the tokenizer produces them.
pub struct LineTokens<'b> {
    emit_mode: TokenEmitMode,
    tokens: Vec<Token>,
    binary_tokens: Vec<u32>,
    last_token_end: usize,
    token_type_overrides: &'b [TokenTypeOverride],
    balanced_bracket_selectors: Option<&'b BalancedBracketSelectors>,
    merge_equal_metadata: bool,
}

impl<'b> LineTokens<'b> {
    /// Constructs a new collector for a single line.
    #[must_use]
    pub fn new(
        emit_mode: TokenEmitMode,
        line_text: &str,
        token_type_overrides: &'b [TokenTypeOverride],
        balanced_bracket_selectors: Option<&'b BalancedBracketSelectors>,
    ) -> Self {
        Self {
            emit_mode,
            tokens: Vec::new(),
            binary_tokens: Vec::new(),
            last_token_end: 0,
            token_type_overrides,
            balanced_bracket_selectors,
            merge_equal_metadata: !contains_rtl(line_text),
        }
    }

    fn emit_plain(&mut self, scopes: Vec<String>, end_index: usize) {
        if self.last_token_end >= end_index {
            return;
        }
        self.tokens.push(Token {
            start_index: self.last_token_end,
            end_index,
            scopes,
        });
        self.last_token_end = end_index;
    }

    fn emit_binary(&mut self, scopes_list: Option<&AttributedScopeStack>, end_index: usize) {
        if self.last_token_end >= end_index {
            return;
        }

        let mut metadata = scopes_list.map_or(0u32, AttributedScopeStack::token_attributes);
        let mut contains_balanced = self
            .balanced_bracket_selectors
            .is_some_and(BalancedBracketSelectors::matches_always);

        let needs_scopes = !self.token_type_overrides.is_empty()
            || self
                .balanced_bracket_selectors
                .is_some_and(|sel| !sel.matches_always() && !sel.matches_never());

        if needs_scopes {
            let scopes: Vec<String> = scopes_list
                .map(AttributedScopeStack::scope_names)
                .unwrap_or_default();

            for override_entry in self.token_type_overrides {
                if (override_entry.matcher)(&scopes) {
                    metadata = metadata::set(
                        metadata,
                        0,
                        OptionalStandardTokenType::from(override_entry.token_type),
                        None,
                        FontStyle::NOT_SET,
                        0,
                        0,
                    );
                }
            }

            if let Some(sel) = self.balanced_bracket_selectors {
                contains_balanced = sel.matches(&scopes);
            }
        }

        if contains_balanced {
            metadata = metadata::set(
                metadata,
                0,
                OptionalStandardTokenType::NotSet,
                Some(true),
                FontStyle::NOT_SET,
                0,
                0,
            );
        }

        if self.merge_equal_metadata {
            if let Some(&last_meta) = self.binary_tokens.last() {
                if last_meta == metadata {
                    self.last_token_end = end_index;
                    return;
                }
            }
        }

        let start = u32::try_from(self.last_token_end).unwrap_or(u32::MAX);
        self.binary_tokens.push(start);
        self.binary_tokens.push(metadata);
        self.last_token_end = end_index;
    }

    /// Finalizes plain-mode output. Port of `getResult`. Pops the
    /// trailing newline token and guarantees the returned `Vec` is
    /// non-empty (inserts a covering token when needed).
    pub fn into_tokens(
        mut self,
        stack_scopes: Option<&AttributedScopeStack>,
        line_length: usize,
    ) -> Vec<Token> {
        if let Some(last) = self.tokens.last() {
            if last.start_index == line_length.saturating_sub(1) {
                self.tokens.pop();
            }
        }
        if self.tokens.is_empty() {
            self.last_token_end = usize::MAX;
            self.emit_binary_or_plain(stack_scopes, line_length);
            if let Some(last) = self.tokens.last_mut() {
                last.start_index = 0;
            }
        }
        self.tokens
    }

    /// Finalizes binary-mode output. Port of `getBinaryResult`. Emits
    /// the final token covering up to `line_length` if needed.
    pub fn into_binary(
        mut self,
        stack_scopes: Option<&AttributedScopeStack>,
        line_length: usize,
    ) -> Vec<u32> {
        // Each binary token is two u32s; the newline-trim check looks at
        // the second-to-last slot (start index).
        if self.binary_tokens.len() >= 2 {
            let start_at = self.binary_tokens.len() - 2;
            if self.binary_tokens[start_at] as usize == line_length.saturating_sub(1) {
                self.binary_tokens.pop();
                self.binary_tokens.pop();
            }
        }
        if self.binary_tokens.is_empty() {
            self.last_token_end = usize::MAX;
            self.emit_binary_or_plain(stack_scopes, line_length);
            if let Some(slot) = self.binary_tokens.get_mut(0) {
                *slot = 0;
            }
        }
        self.binary_tokens
    }

    fn emit_binary_or_plain(
        &mut self,
        scopes_list: Option<&AttributedScopeStack>,
        end_index: usize,
    ) {
        match self.emit_mode {
            TokenEmitMode::Binary => self.emit_binary(scopes_list, end_index),
            TokenEmitMode::Plain => {
                let scopes = scopes_list
                    .map(AttributedScopeStack::scope_names)
                    .unwrap_or_default();
                self.emit_plain(scopes, end_index);
            }
        }
    }
}

impl TokenSink<Arc<AttributedScopeStack>> for LineTokens<'_> {
    fn produce(&mut self, stack_scopes: Option<&Arc<AttributedScopeStack>>, end_index: usize) {
        self.emit_binary_or_plain(stack_scopes.map(std::convert::AsRef::as_ref), end_index);
    }

    fn produce_from_scopes(
        &mut self,
        scopes: Option<&Arc<AttributedScopeStack>>,
        end_index: usize,
    ) {
        self.emit_binary_or_plain(scopes.map(std::convert::AsRef::as_ref), end_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::pack;

    #[test]
    fn plain_tokens_populate_start_and_end() {
        let mut sink = LineTokens::new(TokenEmitMode::Plain, "const x = 1;", &[], None);
        sink.produce(None, 5);
        sink.produce(None, 12);
        let tokens = sink.into_tokens(None, 12);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].start_index, 0);
        assert_eq!(tokens[0].end_index, 5);
        assert_eq!(tokens[1].start_index, 5);
    }

    #[test]
    fn binary_tokens_emit_start_plus_metadata_pairs() {
        let root = Arc::new(AttributedScopeStack::create_root(
            "source.ts",
            pack(1, StandardTokenType::Other, false, FontStyle::NONE, 5, 0),
        ));
        let mut sink = LineTokens::new(TokenEmitMode::Binary, "abc", &[], None);
        sink.produce(Some(&root), 3);
        let packed = sink.into_binary(Some(&root), 3);
        assert_eq!(packed.len() % 2, 0);
        assert!(!packed.is_empty());
    }

    #[test]
    fn merge_skips_duplicate_metadata_runs() {
        let root = Arc::new(AttributedScopeStack::create_root("source.ts", 0));
        let mut sink = LineTokens::new(TokenEmitMode::Binary, "abc", &[], None);
        sink.produce(Some(&root), 1);
        sink.produce(Some(&root), 2);
        sink.produce(Some(&root), 3);
        let packed = sink.into_binary(Some(&root), 3);
        // Three same-metadata pushes collapse to one binary token pair.
        assert_eq!(packed.len(), 2);
    }

    #[test]
    fn rtl_detection_disables_merge() {
        let mut sink = LineTokens::new(TokenEmitMode::Binary, "שלום", &[], None);
        assert!(!sink.merge_equal_metadata);
        sink.produce(None, 4);
        let out = sink.into_binary(None, 4);
        assert!(!out.is_empty());
    }
}
