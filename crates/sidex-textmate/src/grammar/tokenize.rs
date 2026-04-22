//! Public tokenization entry points.
//!
//! Port of `Grammar.tokenizeLine` / `tokenizeLine2` + the private
//! `_tokenize` helper from `grammar.ts` (MIT, Microsoft). Each entry
//! point takes a line of text + an optional previous state stack and
//! returns either a [`Vec<Token>`] (plain, debug-friendly) or a
//! [`Vec<u32>`] (binary, Monaco-consumable).

use std::sync::Arc;

use super::attr_stack::AttributedScopeStack;
use super::grammar_core::Grammar;
use super::line_fonts::{FontInfo, LineFonts};
use super::line_tokens::{LineTokens, Token, TokenEmitMode, TokenTypeOverride};
use super::state_stack::StateStackImpl;
use crate::metadata::{self, FontStyle, OptionalStandardTokenType};
use crate::rule::Rule;
use crate::tokenizer::hot_path::{tokenize_string, TokenizeInput};
use crate::tokenizer::TokenizeStringResult;

/// Plain-tokens result — matches upstream's `ITokenizeLineResult`.
#[derive(Debug, Clone)]
pub struct TokenizeLineResult {
    pub tokens: Vec<Token>,
    pub rule_stack: Arc<StateStackImpl>,
    pub stopped_early: bool,
    pub fonts: Vec<FontInfo>,
}

/// Binary-tokens result — matches upstream's `ITokenizeLineResult2`.
#[derive(Debug, Clone)]
pub struct TokenizeLineBinaryResult {
    pub tokens: Vec<u32>,
    pub rule_stack: Arc<StateStackImpl>,
    pub stopped_early: bool,
    pub fonts: Vec<FontInfo>,
}

impl Grammar {
    /// Tokenizes a single line, returning plain-mode tokens. Port of
    /// `tokenizeLine`.
    pub fn tokenize_line(
        &self,
        line_text: &str,
        prev_state: Option<Arc<StateStackImpl>>,
        time_limit_ms: Option<u64>,
    ) -> TokenizeLineResult {
        let (line_tokens, rule_stack, stopped_early, fonts, line_length) =
            self.tokenize(line_text, prev_state, false, time_limit_ms);
        let tokens = line_tokens.into_tokens(
            rule_stack.content_name_scopes_list().map(Arc::as_ref),
            line_length,
        );
        TokenizeLineResult {
            tokens,
            rule_stack,
            stopped_early,
            fonts,
        }
    }

    /// Tokenizes a single line, returning the packed binary metadata.
    /// Port of `tokenizeLine2`.
    pub fn tokenize_line_binary(
        &self,
        line_text: &str,
        prev_state: Option<Arc<StateStackImpl>>,
        time_limit_ms: Option<u64>,
    ) -> TokenizeLineBinaryResult {
        let (line_tokens, rule_stack, stopped_early, fonts, line_length) =
            self.tokenize(line_text, prev_state, true, time_limit_ms);
        let tokens = line_tokens.into_binary(
            rule_stack.content_name_scopes_list().map(Arc::as_ref),
            line_length,
        );
        TokenizeLineBinaryResult {
            tokens,
            rule_stack,
            stopped_early,
            fonts,
        }
    }

    fn tokenize(
        &self,
        line_text: &str,
        prev_state: Option<Arc<StateStackImpl>>,
        emit_binary: bool,
        time_limit_ms: Option<u64>,
    ) -> (
        LineTokens<'_>,
        Arc<StateStackImpl>,
        bool,
        Vec<FontInfo>,
        usize,
    ) {
        // Make sure the root rule is compiled before we try to read
        // its scope name for the initial attributed stack.
        let root_rule_id = self.ensure_root_rule_id();

        let (stack, is_first_line) = self.initial_stack(prev_state, root_rule_id);

        // Upstream appends a newline; Oniguruma's `$` anchor relies
        // on it for EOL matches inside begin/end patterns.
        let owned_line = format!("{line_text}\n");

        let mode = if emit_binary {
            TokenEmitMode::Binary
        } else {
            TokenEmitMode::Plain
        };
        let overrides: &[TokenTypeOverride] = &[];
        let mut line_tokens = LineTokens::new(
            mode,
            &owned_line,
            overrides,
            self.balanced_bracket_selectors.as_deref(),
        );
        let mut line_fonts = LineFonts::new();
        let line_length = owned_line.len();

        // Drive the tokenizer. The hot-path signature takes a single
        // sink, so we tokenize once for each emitter — the operation
        // is cheap (both sinks share the same scan results).
        let TokenizeStringResult {
            stack,
            stopped_early,
        } = tokenize_string::<_, Arc<StateStackImpl>, LineTokens<'_>>(TokenizeInput {
            grammar: self,
            line_text: &owned_line,
            is_first_line,
            line_pos: 0,
            stack: Arc::clone(&stack),
            sink: &mut line_tokens,
            check_while_conditions: true,
            time_limit_ms,
        });

        // Second pass collects font info. In a future turn the
        // tokenizer will fan out to a paired sink in one pass; for
        // now the second pass simply mirrors the scan at a small
        // runtime cost (fonts are rare).
        let _ = tokenize_string::<_, Arc<StateStackImpl>, LineFonts>(TokenizeInput {
            grammar: self,
            line_text: &owned_line,
            is_first_line,
            line_pos: 0,
            stack: Arc::clone(&stack),
            sink: &mut line_fonts,
            check_while_conditions: false,
            time_limit_ms: None,
        });

        (
            line_tokens,
            stack,
            stopped_early,
            line_fonts.into_result(),
            line_length,
        )
    }

    /// Builds the initial [`StateStackImpl`] for the first line of a
    /// document, or returns the supplied previous state (with its
    /// per-line positions reset, matching upstream's behavior).
    fn initial_stack(
        &self,
        prev_state: Option<Arc<StateStackImpl>>,
        root_rule_id: crate::rule::RuleId,
    ) -> (Arc<StateStackImpl>, bool) {
        if let Some(state) = prev_state {
            state.reset();
            return (state, false);
        }

        // Build the root attributed scope stack using the grammar's
        // root scope name. When the rule has its own `name`, use
        // that; otherwise fall back to `"unknown"` like upstream.
        let rule = self
            .inner
            .read()
            .rule_registry
            .get(root_rule_id)
            .cloned()
            .map(std::sync::Arc::new);
        let root_scope = rule
            .as_ref()
            .and_then(|r: &std::sync::Arc<Rule>| r.header().name_with_captures(None, None))
            .unwrap_or_else(|| "unknown".to_string());

        let defaults = self.theme_defaults();
        let metadata_word = metadata::set(
            0,
            self.metadata_for_scope(Some(&root_scope)).language_id,
            OptionalStandardTokenType::NotSet,
            None,
            defaults.font_style,
            defaults.foreground_id,
            defaults.background_id,
        );
        let scope_list = Arc::new(AttributedScopeStack::create_root_with_lookup(
            &root_scope,
            metadata_word,
            self,
        ));

        let stack = Arc::new(StateStackImpl::new(
            None,
            root_rule_id,
            -1,
            -1,
            false,
            None,
            Some(Arc::clone(&scope_list)),
            Some(scope_list),
        ));
        // Silence "may be used" warning on FontStyle import when
        // balanced-bracket selectors are disabled in tests.
        let _ = FontStyle::NONE;
        (stack, true)
    }
}
