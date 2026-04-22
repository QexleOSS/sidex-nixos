//! Faithful Rust port of `src/grammar/basicScopesAttributeProvider.ts` from
//! `vscode-textmate` (MIT, Microsoft).
//!
//! Maps a `TextMate` scope name to two pieces of basic metadata:
//!
//! * A language id (for embedded-language regions, e.g. `source.css`
//!   inside an HTML grammar should light up as `css`).
//! * A [`StandardTokenType`] if the scope matches `comment`, `string`,
//!   `regex`, or `meta.embedded`.
//!
//! Both lookups are memoized through [`CachedFn`] because they fire on
//! every scope push and every scope segment of every push.

use std::collections::HashMap;

use regex::Regex;

use crate::metadata::OptionalStandardTokenType;
use crate::utils::{escape_regex_chars, CachedFn, LazyRegex};

/// Basic metadata for a scope — mirrors upstream's `BasicScopeAttributes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasicScopeAttributes {
    pub language_id: u32,
    pub token_type: OptionalStandardTokenType,
}

impl BasicScopeAttributes {
    /// Used for `null` scope lookups — matches upstream's `_NULL_SCOPE_METADATA`.
    pub const NULL: Self = Self {
        language_id: 0,
        token_type: OptionalStandardTokenType::Other,
    };
}

/// Maps embedded-language scope prefixes to their language ids, plus
/// classifies scopes into [`OptionalStandardTokenType`] categories.
pub struct BasicScopeAttributesProvider {
    default_attributes: BasicScopeAttributes,
    embedded_matcher: ScopeMatcher<u32>,
    cache: CachedFn<String, BasicScopeAttributes>,
}

impl BasicScopeAttributesProvider {
    /// Builds a provider. `initial_language_id` is the language id used
    /// for the grammar's root scope. `embedded_languages` maps scope
    /// prefix → language id for injected grammars.
    pub fn new(initial_language_id: u32, embedded_languages: &HashMap<String, u32>) -> Self {
        Self {
            default_attributes: BasicScopeAttributes {
                language_id: initial_language_id,
                token_type: OptionalStandardTokenType::NotSet,
            },
            embedded_matcher: ScopeMatcher::new(embedded_languages),
            cache: CachedFn::new(),
        }
    }

    #[must_use]
    pub fn default_attributes(&self) -> BasicScopeAttributes {
        self.default_attributes
    }

    /// Returns memoized metadata for `scope_name`. `None` maps to
    /// [`BasicScopeAttributes::NULL`], matching upstream.
    pub fn basic_scope_attributes(&self, scope_name: Option<&str>) -> BasicScopeAttributes {
        let Some(name) = scope_name else {
            return BasicScopeAttributes::NULL;
        };
        let matcher = &self.embedded_matcher;
        self.cache
            .get(name.to_string(), |key| BasicScopeAttributes {
                language_id: matcher.lookup(key).unwrap_or(0),
                token_type: classify_standard_token(key),
            })
    }
}

fn classify_standard_token(scope_name: &str) -> OptionalStandardTokenType {
    static STANDARD_TOKEN_TYPE: LazyRegex =
        LazyRegex::new(r"\b(comment|string|regex|meta\.embedded)\b");
    let Some(caps) = STANDARD_TOKEN_TYPE.get().captures(scope_name) else {
        return OptionalStandardTokenType::NotSet;
    };
    match caps.get(1).map(|m| m.as_str()) {
        Some("comment") => OptionalStandardTokenType::Comment,
        Some("string") => OptionalStandardTokenType::String,
        Some("regex") => OptionalStandardTokenType::RegEx,
        Some("meta.embedded") => OptionalStandardTokenType::Other,
        _ => OptionalStandardTokenType::NotSet,
    }
}

/// Prefix-based scope matcher — returns the most-specific registered
/// value whose scope prefix matches the input. Direct port of the
/// upstream `ScopeMatcher` inner class.
struct ScopeMatcher<TValue: Copy> {
    values: HashMap<String, TValue>,
    regex: Option<Regex>,
}

impl<TValue: Copy> ScopeMatcher<TValue> {
    fn new(entries: &HashMap<String, TValue>) -> Self {
        if entries.is_empty() {
            return Self {
                values: HashMap::new(),
                regex: None,
            };
        }

        let mut escaped: Vec<String> = entries
            .keys()
            .map(|scope| escape_regex_chars(scope))
            .collect();
        // Longest-scope first: upstream sorts ascending then reverses;
        // doing a direct reverse-sort produces the same ordering.
        escaped.sort();
        escaped.reverse();

        let joined = escaped
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(")|(");
        let pattern = format!(r"^(({joined}))($|\.)");
        let regex = Regex::new(&pattern).ok();

        Self {
            values: entries.clone(),
            regex,
        }
    }

    fn lookup(&self, scope: &str) -> Option<TValue> {
        let regex = self.regex.as_ref()?;
        let caps = regex.captures(scope)?;
        // Group 1 = the whole matched scope prefix.
        let key = caps.get(1)?.as_str();
        self.values.get(key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_scope_uses_initial_language_id() {
        let provider = BasicScopeAttributesProvider::new(5, &HashMap::new());
        let attrs = provider.default_attributes();
        assert_eq!(attrs.language_id, 5);
        assert_eq!(attrs.token_type, OptionalStandardTokenType::NotSet);
    }

    #[test]
    fn null_scope_returns_null_metadata() {
        let provider = BasicScopeAttributesProvider::new(1, &HashMap::new());
        assert_eq!(
            provider.basic_scope_attributes(None),
            BasicScopeAttributes::NULL
        );
    }

    #[test]
    fn classify_picks_known_standard_tokens() {
        assert_eq!(
            classify_standard_token("comment.line.double-slash"),
            OptionalStandardTokenType::Comment
        );
        assert_eq!(
            classify_standard_token("string.quoted.double.js"),
            OptionalStandardTokenType::String
        );
        assert_eq!(
            classify_standard_token("meta.embedded.block.css"),
            OptionalStandardTokenType::Other
        );
        assert_eq!(
            classify_standard_token("source.ts"),
            OptionalStandardTokenType::NotSet
        );
    }

    #[test]
    fn embedded_language_matcher_picks_longest_prefix() {
        let mut embedded = HashMap::new();
        embedded.insert("source.css".to_string(), 2);
        embedded.insert("source.css.embedded".to_string(), 3);
        let provider = BasicScopeAttributesProvider::new(1, &embedded);
        let nested = provider.basic_scope_attributes(Some("source.css.embedded.html"));
        assert_eq!(nested.language_id, 3);
        let shorter = provider.basic_scope_attributes(Some("source.css"));
        assert_eq!(shorter.language_id, 2);
    }
}
