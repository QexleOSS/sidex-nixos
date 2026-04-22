//! In-memory fixtures that exercise the generic tokenizer.
//!
//! Turn 5 ports the real `Grammar` + `StateStackImpl` types; until
//! then, these fixtures let us confirm the tokenizer plumbing works
//! end-to-end: `tokenize_string` is driven with a trivial grammar that
//! yields no matches, and we assert it emits exactly one line-covering
//! token and reports `stopped_early = false`.

#![cfg(test)]

use std::sync::Arc;

use crate::rule::{Rule, RuleId, RuleRegistry};
use crate::theme::ScopeStack;
use crate::tokenizer::contracts::{
    AttributedScopeStack, GrammarRuntime, Injection, StateStack, TokenSink,
};
use crate::tokenizer::hot_path::{tokenize_string, TokenizeInput};

#[derive(Clone)]
struct FakeAttrStack;

impl AttributedScopeStack for FakeAttrStack {
    fn push_attributed(&self, _scope: Option<&str>, _grammar: &dyn GrammarRuntime) -> Self {
        Self
    }
    fn scope_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn plain_stack(&self) -> Option<Arc<ScopeStack>> {
        None
    }
}

#[derive(Clone)]
struct FakeStack;

impl StateStack for FakeStack {
    type Attr = FakeAttrStack;
    fn parent(&self) -> Option<Self> {
        None
    }
    fn rule_id(&self) -> RuleId {
        RuleId(0)
    }
    fn anchor_pos(&self) -> i64 {
        -1
    }
    fn enter_pos(&self) -> i64 {
        -1
    }
    fn end_rule(&self) -> Option<&str> {
        None
    }
    fn begin_rule_captured_eol(&self) -> bool {
        false
    }
    fn name_scopes_list(&self) -> Option<&Self::Attr> {
        None
    }
    fn content_name_scopes_list(&self) -> Option<&Self::Attr> {
        None
    }
    fn push(
        &self,
        _rule_id: RuleId,
        _enter_pos: i64,
        _anchor_pos: i64,
        _begin_captured_eol: bool,
        _end_rule: Option<String>,
        _name_scopes: Option<Self::Attr>,
        _content_scopes: Option<Self::Attr>,
    ) -> Self {
        Self
    }
    fn pop(&self) -> Option<Self> {
        None
    }
    fn safe_pop(&self) -> Self {
        Self
    }
    fn with_content_name_scopes(&self, _scopes: Self::Attr) -> Self {
        Self
    }
    fn with_end_rule(&self, _end_rule: String) -> Self {
        Self
    }
    fn has_same_rule_as(&self, _other: &Self) -> bool {
        true
    }
}

struct FakeGrammar {
    registry: RuleRegistry,
    injections: Vec<Injection>,
}

impl GrammarRuntime for FakeGrammar {
    fn rule(&self, id: RuleId) -> Option<std::sync::Arc<Rule>> {
        self.registry.get_arc(id)
    }
    fn injections(&self) -> Vec<Injection> {
        self.injections.clone()
    }
}

#[derive(Default)]
struct CountingSink {
    produced: Vec<usize>,
}

impl TokenSink<FakeAttrStack> for CountingSink {
    fn produce(&mut self, _scopes: Option<&FakeAttrStack>, end_index: usize) {
        self.produced.push(end_index);
    }
    fn produce_from_scopes(&mut self, _scopes: Option<&FakeAttrStack>, end_index: usize) {
        self.produced.push(end_index);
    }
}

#[test]
fn tokenize_empty_grammar_emits_single_line_token() {
    let grammar = FakeGrammar {
        registry: RuleRegistry::new(),
        injections: Vec::new(),
    };
    let mut sink = CountingSink::default();
    let line_text = "const x = 1;";

    let result = tokenize_string(TokenizeInput {
        grammar: &grammar,
        line_text,
        is_first_line: true,
        line_pos: 0,
        stack: FakeStack,
        sink: &mut sink,
        check_while_conditions: false,
        time_limit_ms: None,
    });

    assert!(!result.stopped_early);
    assert_eq!(sink.produced, vec![line_text.len()]);
}

#[test]
fn tokenize_respects_time_limit() {
    // Tiny budget plus an infinite loop would crash in turn 5's real
    // scanner; with the stub scanner we just assert the API accepts the
    // budget without panicking.
    let grammar = FakeGrammar {
        registry: RuleRegistry::new(),
        injections: Vec::new(),
    };
    let mut sink = CountingSink::default();
    let result = tokenize_string(TokenizeInput {
        grammar: &grammar,
        line_text: "",
        is_first_line: true,
        line_pos: 0,
        stack: FakeStack,
        sink: &mut sink,
        check_while_conditions: false,
        time_limit_ms: Some(1),
    });
    assert!(!result.stopped_early);
}
