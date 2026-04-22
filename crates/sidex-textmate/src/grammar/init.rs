//! Grammar initialization helper.
//!
//! Port of the `initGrammar` function from `grammar.ts` (MIT, Microsoft).
//! Seeds the `$self` / `$base` sentinels in the grammar's repository so
//! the rule factory can resolve `"include": "$self"` / `"$base"` by the
//! same lookup path as any other named entry.

use crate::rule::{RawGrammar, RawRule};

/// Clones `grammar` and injects `$self` / `$base` entries into its
/// repository. `base` is the `$base` grammar's top-level rule (or
/// `None`, in which case `$base` defaults to the same as `$self`).
#[must_use]
pub fn init_grammar(grammar: &RawGrammar, base: Option<&RawRule>) -> RawGrammar {
    let mut grammar = grammar.clone();

    let self_rule = RawRule {
        location: grammar.location.clone(),
        patterns: Some(grammar.patterns.clone()),
        name: Some(grammar.scope_name.clone()),
        ..RawRule::default()
    };

    grammar
        .repository
        .insert("$self".to_string(), self_rule.clone());

    let base_rule = base.cloned().unwrap_or(self_rule);
    grammar.repository.insert("$base".to_string(), base_rule);

    grammar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_seeds_self_and_base_from_patterns() {
        let grammar = RawGrammar {
            scope_name: "source.test".to_string(),
            patterns: vec![RawRule {
                match_: Some(r"\bfoo\b".to_string()),
                ..RawRule::default()
            }],
            ..RawGrammar::default()
        };

        let init = init_grammar(&grammar, None);
        assert!(init.repository.contains_key("$self"));
        assert!(init.repository.contains_key("$base"));
        assert_eq!(
            init.repository["$self"].name.as_deref(),
            Some("source.test")
        );
    }

    #[test]
    fn explicit_base_overrides_default() {
        let grammar = RawGrammar {
            scope_name: "source.child".to_string(),
            ..RawGrammar::default()
        };
        let base = RawRule {
            name: Some("source.parent".to_string()),
            ..RawRule::default()
        };
        let init = init_grammar(&grammar, Some(&base));
        assert_eq!(
            init.repository["$base"].name.as_deref(),
            Some("source.parent")
        );
    }
}
