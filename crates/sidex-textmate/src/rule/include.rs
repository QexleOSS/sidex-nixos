//! Include-reference parser.
//!
//! Port of the `parseInclude` function and reference types from
//! `src/grammar/grammarDependencies.ts` upstream. A `TextMate` grammar can
//! reference other rules via `include` strings; this module classifies
//! them into the five forms documented on [`IncludeReference`].

use super::super::theme::ScopePath;

/// One of the five include-reference forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeReference {
    /// `"include": "$base"` — the grammar currently driving tokenization.
    Base,

    /// `"include": "$self"` — the current grammar's top-level rule.
    SelfRef,

    /// `"include": "#name"` — a rule in the current grammar's repository.
    Relative { rule_name: String },

    /// `"include": "source.x"` — the top-level rule of another grammar.
    TopLevel { scope_name: String },

    /// `"include": "source.x#name"` — a named rule in another grammar.
    TopLevelRepository {
        scope_name: String,
        rule_name: String,
    },
}

impl IncludeReference {
    /// Stable lookup key used by the dependency collector.
    #[must_use]
    pub fn to_key(&self) -> String {
        match self {
            Self::Base => "$base".to_string(),
            Self::SelfRef => "$self".to_string(),
            Self::Relative { rule_name } => format!("#{rule_name}"),
            Self::TopLevel { scope_name } => scope_name.clone(),
            Self::TopLevelRepository {
                scope_name,
                rule_name,
            } => {
                format!("{scope_name}#{rule_name}")
            }
        }
    }

    /// Returns the scope name if this reference targets another grammar.
    #[must_use]
    pub fn scope_name(&self) -> Option<&str> {
        match self {
            Self::TopLevel { scope_name } | Self::TopLevelRepository { scope_name, .. } => {
                Some(scope_name.as_str())
            }
            _ => None,
        }
    }
}

/// Parses an include string. Direct port of upstream's `parseInclude`.
#[must_use]
pub fn parse_include(include: &str) -> IncludeReference {
    if include == "$base" {
        return IncludeReference::Base;
    }
    if include == "$self" {
        return IncludeReference::SelfRef;
    }
    match include.find('#') {
        None => IncludeReference::TopLevel {
            scope_name: include.to_string(),
        },
        Some(0) => IncludeReference::Relative {
            rule_name: include[1..].to_string(),
        },
        Some(idx) => IncludeReference::TopLevelRepository {
            scope_name: include[..idx].to_string(),
            rule_name: include[idx + 1..].to_string(),
        },
    }
}

/// Re-exported alias kept so downstream modules that want a strong type
/// for scope names don't have to reach into `theme` directly.
pub type GrammarScopeName = ScopePath;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_and_self_are_recognized() {
        assert_eq!(parse_include("$base"), IncludeReference::Base);
        assert_eq!(parse_include("$self"), IncludeReference::SelfRef);
    }

    #[test]
    fn relative_reference_strips_hash() {
        assert_eq!(
            parse_include("#entity"),
            IncludeReference::Relative {
                rule_name: "entity".to_string(),
            }
        );
    }

    #[test]
    fn top_level_reference_keeps_scope() {
        assert_eq!(
            parse_include("source.ts"),
            IncludeReference::TopLevel {
                scope_name: "source.ts".to_string(),
            }
        );
    }

    #[test]
    fn top_level_repository_reference_splits_at_hash() {
        assert_eq!(
            parse_include("source.ts#string"),
            IncludeReference::TopLevelRepository {
                scope_name: "source.ts".to_string(),
                rule_name: "string".to_string(),
            }
        );
    }
}
