//! Trait interfaces the tokenizer depends on.
//!
//! `tokenizeString.ts` references `Grammar`, `StateStackImpl`,
//! `AttributedScopeStack`, `LineTokens`, `LineFonts`, `Injection`. Those
//! types live in `grammar.ts` (port arrives in turn 5). We express their
//! minimal contracts as traits so the tokenizer can be written,
//! reviewed, and tested independently of the grammar compiler.
//!
//! Every method on these traits matches the exact shape upstream calls.
//! When the grammar port lands, its types will implement these traits
//! and the tokenizer lights up with zero changes.

#![allow(clippy::return_self_not_must_use)]

use crate::rule::{Rule, RuleId};
use crate::theme::ScopeStack;
use crate::utils::CaptureIndex;
use std::sync::Arc;

/// Minimal contract for a rule + grammar registry. Upstream's
/// `Grammar` implements both `IRuleRegistry` and `IOnigLib`; the
/// tokenizer only touches these few entry points.
pub trait GrammarRuntime {
    /// Returns the compiled rule for `id`, cloned out behind an
    /// `Arc`. `None` should never happen in a well-formed grammar.
    /// The returned handle is cheap to clone and `Send + Sync`.
    fn rule(&self, id: RuleId) -> Option<Arc<Rule>>;

    /// Top-level injections in priority order. Empty when the grammar
    /// has none. Returned by value (cloned) so the caller can hold
    /// the list across `&self` method calls on the grammar.
    fn injections(&self) -> Vec<Injection>;

    /// Resolves basic-scope metadata for a scope name. Port of
    /// `grammar.getMetadataForScope`. Used by the attributed scope
    /// stack when it pushes a new frame. Default implementation
    /// returns null metadata so fixtures can exercise the hot path.
    fn basic_scope_attributes(
        &self,
        _scope_name: Option<&str>,
    ) -> crate::grammar::basic_attrs::BasicScopeAttributes {
        crate::grammar::basic_attrs::BasicScopeAttributes::NULL
    }

    /// Runs the theme matcher against a scope path. Port of
    /// `grammar.themeProvider.themeMatch`. Default implementation
    /// returns `None` so fixtures can exercise the hot path without a
    /// live theme.
    fn theme_match(
        &self,
        _scope_path: &crate::theme::ScopeStack,
    ) -> Option<crate::theme::StyleAttributes> {
        None
    }

    /// Runs the scanner for the active rule at the top of the stack.
    /// Equivalent to upstream's `prepareRuleSearch(stack.getRule(...))`
    /// followed by `findNextMatchSync`.
    ///
    /// Returns the leftmost hit or `None`. Default implementation
    /// yields `None` so simple fixtures can still exercise the hot
    /// path without a real scanner.
    fn scan_active_rule(&self, _ctx: ScanContext<'_>) -> Option<MatchResult> {
        None
    }

    /// Runs the scanner for a single injection, returning the leftmost
    /// hit or `None`. Default implementation yields `None`.
    fn scan_injection(&self, _injection: &Injection, _ctx: ScanContext<'_>) -> Option<MatchResult> {
        None
    }

    /// Runs the while-scanner for a specific `begin/while` rule
    /// frame. Port of upstream's `prepareRuleWhileSearch` +
    /// `findNextMatchSync`. Returns the leftmost hit or `None` when
    /// the while condition fails — a failure triggers the tokenizer
    /// to pop the frame.
    fn scan_while_rule(&self, _ctx: ScanContext<'_>) -> Option<MatchResult> {
        None
    }
}

/// Input context for [`GrammarRuntime::scan_active_rule`] and
/// [`GrammarRuntime::scan_injection`].
pub struct ScanContext<'a> {
    pub line_text: &'a str,
    pub line_pos: usize,
    pub is_first_line: bool,
    pub anchor_pos: i64,
    /// Active-rule end-regex override when the current frame is a
    /// begin/end or begin/while rule. `None` selects the static end
    /// pattern the grammar shipped with.
    pub end_rule: Option<&'a str>,
    /// Current rule id at the top of the stack (matches
    /// `stack.getRule(grammar).id`).
    pub rule_id: RuleId,
}

/// Attributed scope stack — upstream's `AttributedScopeStack`. Tracks
/// the chain of scope names plus a pre-resolved theme metadata word.
/// Implementations live in turn 5; here we expose the minimum API
/// surface the tokenizer hot path calls.
pub trait AttributedScopeStack: Clone + Send + Sync {
    /// Pushes a new scope onto the stack, returning a new attributed
    /// stack handle. Upstream's `pushAttributed`.
    fn push_attributed(&self, scope: Option<&str>, grammar: &dyn GrammarRuntime) -> Self;

    /// Returns the ordered list of scope names (outer-most first).
    fn scope_names(&self) -> Vec<String>;

    /// Underlying plain scope stack used by [`super::Registry`] for
    /// theme matching.
    fn plain_stack(&self) -> Option<Arc<ScopeStack>>;
}

/// The rule-stack node — upstream's `StateStackImpl`. Implemented in
/// turn 5; the trait covers what `tokenizeString` needs.
pub trait StateStack: Clone {
    type Attr: AttributedScopeStack;

    /// Returns the parent node (`None` at root).
    fn parent(&self) -> Option<Self>;

    /// The rule id driving the current frame.
    fn rule_id(&self) -> RuleId;

    /// Anchor position recorded when this frame was pushed.
    fn anchor_pos(&self) -> i64;

    /// Position the current frame was entered at.
    fn enter_pos(&self) -> i64;

    /// Override end regex (for begin/end rules with back-references).
    fn end_rule(&self) -> Option<&str>;

    /// Was the begin pattern captured just before the end of line?
    fn begin_rule_captured_eol(&self) -> bool;

    /// Attributed scope stack covering the rule's `name` scope.
    fn name_scopes_list(&self) -> Option<&Self::Attr>;

    /// Attributed scope stack covering the rule's `contentName` scope.
    fn content_name_scopes_list(&self) -> Option<&Self::Attr>;

    /// Pushes a new frame. Parameters match upstream's `push` exactly.
    #[allow(clippy::too_many_arguments)]
    fn push(
        &self,
        rule_id: RuleId,
        enter_pos: i64,
        anchor_pos: i64,
        begin_captured_eol: bool,
        end_rule: Option<String>,
        name_scopes: Option<Self::Attr>,
        content_scopes: Option<Self::Attr>,
    ) -> Self;

    /// Pops the top frame. Upstream returns `StateStackImpl | null`; we
    /// return `Option<Self>` so callers handle root gracefully.
    fn pop(&self) -> Option<Self>;

    /// Pop that never yields `None` — used in error-recovery paths.
    fn safe_pop(&self) -> Self;

    /// Overrides the content-name scope list in place (returns a new
    /// node with the override applied).
    fn with_content_name_scopes(&self, scopes: Self::Attr) -> Self;

    /// Override the stored end-rule string (when back-references were
    /// resolved).
    fn with_end_rule(&self, end_rule: String) -> Self;

    /// Upstream's `hasSameRuleAs`.
    fn has_same_rule_as(&self, other: &Self) -> bool;
}

/// Injection entry — matches upstream's `Injection` interface.
#[derive(Clone)]
pub struct Injection {
    /// `debugSelector` upstream.
    pub selector: String,
    /// Pre-compiled predicate; returns `true` when the injection
    /// should activate for the given scope list.
    pub matcher: InjectionMatcher,
    /// Priority: `-1` = left, `0` = normal, `1` = right. Higher wins
    /// on ties; negative `-1` means "preempt the grammar rule".
    pub priority: i8,
    /// Rule to invoke when the injection fires.
    pub rule_id: RuleId,
}

impl std::fmt::Debug for Injection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Injection")
            .field("selector", &self.selector)
            .field("priority", &self.priority)
            .field("rule_id", &self.rule_id)
            .finish_non_exhaustive()
    }
}

/// Shareable closure used by [`Injection::matcher`]. `Arc`-wrapped so
/// cloning an `Injection` stays cheap.
pub type InjectionMatcher = Arc<dyn Fn(&[String]) -> bool + Send + Sync>;

/// Token/font sink — the tokenizer emits tokens here as scopes open
/// and close. Upstream has two separate sinks (`LineTokens` +
/// `LineFonts`); the trait keeps them paired because every emission
/// site calls both in lockstep.
pub trait TokenSink<A: AttributedScopeStack> {
    /// Emits a token covering `[previous_end, end_index)` with the
    /// scopes currently on `stack`.
    fn produce(&mut self, stack_scopes: Option<&A>, end_index: usize);

    /// Emits a token using an ad-hoc scope list instead of the stack's
    /// current frame. Mirrors upstream's `produceFromScopes`.
    fn produce_from_scopes(&mut self, scopes: Option<&A>, end_index: usize);
}

/// Where a match came from during a single `scanNext` step.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub captures: Vec<Option<CaptureIndex>>,
    pub matched_rule_id: i32,
}

/// Returned by [`super::hot_path::tokenize_string`]. Tracks whether
/// the time budget was hit so the editor can reschedule.
#[derive(Debug, Clone)]
pub struct TokenizeStringResult<S: StateStack> {
    pub stack: S,
    pub stopped_early: bool,
}
