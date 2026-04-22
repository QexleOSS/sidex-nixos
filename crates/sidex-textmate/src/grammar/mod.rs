//! Grammar runtime — ports of `src/grammar/*.ts` from `vscode-textmate`
//! (MIT, Microsoft).
//!
//! Data-type backbone (shipped in the previous turn) plus the per-line
//! emitters [`LineTokens`] and [`LineFonts`]. The `Grammar` struct
//! itself (injection collection, tokenize entrypoints, scanner cache
//! wiring) lands in the follow-up.

pub mod attr_stack;
pub mod balanced_brackets;
pub mod basic_attrs;
pub mod grammar_core;
pub mod init;
pub mod line_fonts;
pub mod line_tokens;
pub mod state_stack;
pub mod tokenize;

#[cfg(test)]
mod e2e_tests;

pub use attr_stack::{AttributedScopeStack, AttributedScopeStackFrame, ScopeMetadataSource};
pub use balanced_brackets::BalancedBracketSelectors;
pub use basic_attrs::{BasicScopeAttributes, BasicScopeAttributesProvider};
pub use grammar_core::{EmbeddedLanguagesMap, Grammar, TokenTypeMap, TokenTypeMatcher};
pub use init::init_grammar;
pub use line_fonts::{FontInfo, LineFonts};
pub use line_tokens::{contains_rtl, LineTokens, Token, TokenEmitMode, TokenTypeOverride};
pub use state_stack::{StateStackFrame, StateStackImpl};
pub use tokenize::{TokenizeLineBinaryResult, TokenizeLineResult};

use std::sync::Arc;

use crate::theme::ScopeStack;
use crate::tokenizer::contracts::AttributedScopeStack as TokenizerAttributedScopeStack;
use crate::tokenizer::contracts::GrammarRuntime;
use crate::tokenizer::contracts::StateStack as TokenizerStateStack;

/// Implements the tokenizer's [`TokenizerAttributedScopeStack`] trait on
/// the `Arc`-wrapped concrete type.
///
/// We build a trait-object adapter that satisfies
/// [`ScopeMetadataSource`] by forwarding to the `GrammarRuntime`
/// methods, which lets us run the real `push_attributed` logic
/// without downcasting.
impl TokenizerAttributedScopeStack for Arc<AttributedScopeStack> {
    fn push_attributed(&self, scope: Option<&str>, grammar: &dyn GrammarRuntime) -> Self {
        let Some(scope) = scope else {
            return Arc::clone(self);
        };
        let adapter = GrammarScopeSource { grammar };
        AttributedScopeStack::push_attributed(self, Some(scope), &adapter)
    }

    fn scope_names(&self) -> Vec<String> {
        AttributedScopeStack::scope_names(self)
    }

    fn plain_stack(&self) -> Option<Arc<ScopeStack>> {
        None
    }
}

/// Adapter that satisfies [`ScopeMetadataSource`] by delegating to the
/// tokenizer's `GrammarRuntime` trait methods. Lets the generic
/// attributed stack build new frames without knowing the concrete
/// grammar type.
struct GrammarScopeSource<'a> {
    grammar: &'a dyn GrammarRuntime,
}

impl ScopeMetadataSource for GrammarScopeSource<'_> {
    fn basic_attributes(&self, scope_name: Option<&str>) -> BasicScopeAttributes {
        self.grammar.basic_scope_attributes(scope_name)
    }

    fn theme_match(&self, scope_path: &ScopeStack) -> Option<crate::theme::StyleAttributes> {
        self.grammar.theme_match(scope_path)
    }
}

/// Implements the tokenizer's [`TokenizerStateStack`] trait on the
/// `Arc<StateStackImpl>` used by `Grammar`.
impl TokenizerStateStack for Arc<StateStackImpl> {
    type Attr = Arc<AttributedScopeStack>;

    fn parent(&self) -> Option<Self> {
        StateStackImpl::parent(self).map(Arc::clone)
    }

    fn rule_id(&self) -> crate::rule::RuleId {
        StateStackImpl::rule_id(self)
    }

    fn anchor_pos(&self) -> i64 {
        StateStackImpl::anchor_pos(self)
    }

    fn enter_pos(&self) -> i64 {
        StateStackImpl::enter_pos(self)
    }

    fn end_rule(&self) -> Option<&str> {
        StateStackImpl::end_rule(self)
    }

    fn begin_rule_captured_eol(&self) -> bool {
        StateStackImpl::begin_rule_captured_eol(self)
    }

    fn name_scopes_list(&self) -> Option<&Self::Attr> {
        StateStackImpl::name_scopes_list(self)
    }

    fn content_name_scopes_list(&self) -> Option<&Self::Attr> {
        StateStackImpl::content_name_scopes_list(self)
    }

    fn push(
        &self,
        rule_id: crate::rule::RuleId,
        enter_pos: i64,
        anchor_pos: i64,
        begin_captured_eol: bool,
        end_rule: Option<String>,
        name_scopes: Option<Self::Attr>,
        content_scopes: Option<Self::Attr>,
    ) -> Self {
        StateStackImpl::push(
            self,
            rule_id,
            enter_pos,
            anchor_pos,
            begin_captured_eol,
            end_rule,
            name_scopes,
            content_scopes,
        )
    }

    fn pop(&self) -> Option<Self> {
        StateStackImpl::pop(self)
    }

    fn safe_pop(&self) -> Self {
        StateStackImpl::safe_pop(self)
    }

    fn with_content_name_scopes(&self, scopes: Self::Attr) -> Self {
        StateStackImpl::with_content_name_scopes_list(self, scopes)
    }

    fn with_end_rule(&self, end_rule: String) -> Self {
        StateStackImpl::with_end_rule(self, end_rule)
    }

    fn has_same_rule_as(&self, other: &Self) -> bool {
        StateStackImpl::has_same_rule_as(self, other)
    }
}
