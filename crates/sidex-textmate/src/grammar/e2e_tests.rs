//! End-to-end tokenizer tests with small, hand-built grammars.
//!
//! Proves the full pipeline — grammar compilation → injection
//! collection → scanner cache → rule-stack state machine → token
//! emission — works on real inputs. Each test builds a minimal
//! grammar in-memory, tokenizes a line, and asserts that known
//! scope-bearing tokens appear in the output.

#![cfg(test)]

use std::sync::Arc;

use crate::registry::Registry;
use crate::rule::{RawGrammar, RawRule};
use crate::theme::Theme;

use super::grammar_core::{EmbeddedLanguagesMap, Grammar};

fn empty_registry() -> Arc<Registry> {
    Arc::new(Registry::new(Theme::create_from_raw(&[], None)))
}

/// Builds a grammar that recognizes integers as `constant.numeric`
/// and bare identifiers as `variable.other`. Two simple match rules —
/// exercises the scanner-cache + match-rule dispatch path.
fn build_simple_grammar() -> Grammar {
    let raw = RawGrammar {
        scope_name: "source.demo".to_string(),
        patterns: vec![
            RawRule {
                match_: Some(r"\d+".to_string()),
                name: Some("constant.numeric".to_string()),
                ..RawRule::default()
            },
            RawRule {
                match_: Some(r"[A-Za-z_][A-Za-z0-9_]*".to_string()),
                name: Some("variable.other".to_string()),
                ..RawRule::default()
            },
        ],
        ..RawGrammar::default()
    };

    Grammar::new(
        "source.demo",
        &raw,
        1,
        &EmbeddedLanguagesMap::new(),
        None,
        None,
        empty_registry(),
    )
}

#[test]
fn tokenize_line_classifies_numbers_and_identifiers() {
    let grammar = build_simple_grammar();
    let result = grammar.tokenize_line("abc 123 xyz", None, None);

    // Collect scope names keyed by the token's first character, which
    // gives us a quick witness that the scanner ran.
    let found_numeric = result
        .tokens
        .iter()
        .any(|tok| tok.scopes.iter().any(|s| s == "constant.numeric"));
    let found_variable = result
        .tokens
        .iter()
        .any(|tok| tok.scopes.iter().any(|s| s == "variable.other"));

    assert!(
        found_numeric,
        "expected a constant.numeric token, got {:?}",
        result.tokens
    );
    assert!(
        found_variable,
        "expected a variable.other token, got {:?}",
        result.tokens
    );
    assert!(!result.stopped_early);
}

#[test]
fn tokenize_line_returns_root_scope_in_every_token() {
    let grammar = build_simple_grammar();
    let result = grammar.tokenize_line("hello", None, None);
    // Every token should include the grammar's root scope.
    for token in &result.tokens {
        assert!(
            token.scopes.iter().any(|s| s == "source.demo"),
            "token missing root scope: {token:?}"
        );
    }
}

#[test]
fn tokenize_line_binary_returns_non_empty_packed_output() {
    let grammar = build_simple_grammar();
    let result = grammar.tokenize_line_binary("value 42", None, None);
    assert!(
        !result.tokens.is_empty(),
        "expected packed tokens, got empty vec"
    );
    // Binary output is always (start, metadata) pairs.
    assert_eq!(result.tokens.len() % 2, 0);
}

/// Grammar with a begin/end rule for double-quoted strings; exercises
/// the scanner cache's end-pattern handling.
fn build_string_grammar() -> Grammar {
    let raw = RawGrammar {
        scope_name: "source.demo-strings".to_string(),
        patterns: vec![RawRule {
            begin: Some(r#"""#.to_string()),
            end: Some(r#"""#.to_string()),
            name: Some("string.quoted.double".to_string()),
            ..RawRule::default()
        }],
        ..RawGrammar::default()
    };

    Grammar::new(
        "source.demo-strings",
        &raw,
        1,
        &EmbeddedLanguagesMap::new(),
        None,
        None,
        empty_registry(),
    )
}

#[test]
fn tokenize_line_handles_begin_end_strings() {
    let grammar = build_string_grammar();
    let result = grammar.tokenize_line(r#"before "middle" after"#, None, None);
    let found_string = result
        .tokens
        .iter()
        .any(|tok| tok.scopes.iter().any(|s| s == "string.quoted.double"));
    assert!(
        found_string,
        "expected a string.quoted.double token, got {:?}",
        result.tokens
    );
}
