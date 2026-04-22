//! Port of `BalancedBracketSelectors` from `grammar.ts` (MIT, Microsoft).
//!
//! A matcher that decides whether a token's bracket characters should
//! count toward the editor's balanced-bracket tracking. The selector
//! string `"*"` is a wildcard "match anything" shortcut; everything
//! else passes through the scope-selector parser in [`crate::matcher`].

use crate::matcher::{create_matchers, BoxMatcher};

/// Scope-list matcher for balanced / unbalanced bracket tracking.
pub struct BalancedBracketSelectors {
    balanced: Vec<BoxMatcher<Vec<String>>>,
    unbalanced: Vec<BoxMatcher<Vec<String>>>,
    allow_any: bool,
}

impl BalancedBracketSelectors {
    #[must_use]
    pub fn new(balanced_selectors: &[&str], unbalanced_selectors: &[&str]) -> Self {
        let mut allow_any = false;
        let mut balanced: Vec<BoxMatcher<Vec<String>>> = Vec::new();
        for selector in balanced_selectors {
            if *selector == "*" {
                allow_any = true;
                continue;
            }
            for built in create_matchers(selector, name_matcher) {
                balanced.push(built.matcher);
            }
        }

        let mut unbalanced: Vec<BoxMatcher<Vec<String>>> = Vec::new();
        for selector in unbalanced_selectors {
            for built in create_matchers(selector, name_matcher) {
                unbalanced.push(built.matcher);
            }
        }

        Self {
            balanced,
            unbalanced,
            allow_any,
        }
    }

    /// True when every scope matches (wildcard enabled, no excluders).
    #[must_use]
    pub fn matches_always(&self) -> bool {
        self.allow_any && self.unbalanced.is_empty()
    }

    /// True when nothing will ever match (no includers, no wildcard).
    #[must_use]
    pub fn matches_never(&self) -> bool {
        self.balanced.is_empty() && !self.allow_any
    }

    /// Decides the match result for a concrete scope list. Excluders
    /// short-circuit to `false`; then includers → `true`; else returns
    /// the wildcard setting.
    #[must_use]
    pub fn matches(&self, scopes: &[String]) -> bool {
        let owned: Vec<String> = scopes.to_vec();
        for excluder in &self.unbalanced {
            if excluder(&owned) {
                return false;
            }
        }
        for includer in &self.balanced {
            if includer(&owned) {
                return true;
            }
        }
        self.allow_any
    }
}

/// Returns `true` when every identifier appears somewhere in the scope
/// list (outer→inner order preserved). Port of upstream's `nameMatcher`.
#[allow(clippy::ptr_arg)]
fn name_matcher(identifiers: &[String], scopes: &Vec<String>) -> bool {
    if scopes.len() < identifiers.len() {
        return false;
    }
    let mut last_index = 0;
    for ident in identifiers {
        let mut matched = false;
        for (i, scope) in scopes.iter().enumerate().skip(last_index) {
            if scopes_are_matching(scope, ident) {
                last_index = i + 1;
                matched = true;
                break;
            }
        }
        if !matched {
            return false;
        }
    }
    true
}

fn scopes_are_matching(this_scope: &str, scope: &str) -> bool {
    if this_scope.is_empty() {
        return false;
    }
    if this_scope == scope {
        return true;
    }
    let len = scope.len();
    this_scope.len() > len
        && this_scope.starts_with(scope)
        && this_scope.as_bytes().get(len) == Some(&b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matcher_traverses_scope_list_in_order() {
        let scopes = vec![
            "source.ts".to_string(),
            "meta.function".to_string(),
            "string.quoted".to_string(),
        ];
        assert!(name_matcher(
            &["meta.function".to_string(), "string".to_string()],
            &scopes
        ));
        // Wrong order.
        assert!(!name_matcher(
            &["string".to_string(), "meta.function".to_string()],
            &scopes
        ));
    }

    #[test]
    fn wildcard_selector_enables_match_always() {
        let sel = BalancedBracketSelectors::new(&["*"], &[]);
        assert!(sel.matches_always());
        assert!(sel.matches(&["source.ts".to_string()]));
    }

    #[test]
    fn excluder_overrides_includer() {
        let sel = BalancedBracketSelectors::new(&["source"], &["string"]);
        assert!(sel.matches(&["source.ts".to_string()]));
        assert!(!sel.matches(&["source.ts".to_string(), "string.quoted".to_string(),]));
    }

    #[test]
    fn matches_never_when_empty() {
        let sel = BalancedBracketSelectors::new(&[], &[]);
        assert!(sel.matches_never());
        assert!(!sel.matches(&["anything".to_string()]));
    }
}
