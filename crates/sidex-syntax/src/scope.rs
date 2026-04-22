//! Token scope mapping from tree-sitter capture names to semantic highlight categories.
//!
//! Tree-sitter grammars use capture names like `@keyword`, `@string.special`,
//! and `@function.method`. This module maps those names to a fixed set of
//! semantic categories used by the theming layer.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A semantic highlight category derived from a tree-sitter capture name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HighlightName {
    Keyword,
    String,
    Comment,
    Function,
    Type,
    Variable,
    Number,
    Operator,
    Punctuation,
    Constant,
    Property,
    Tag,
    Attribute,
}

impl HighlightName {
    /// All standard highlight names in definition order.
    pub const ALL: &[Self] = &[
        Self::Keyword,
        Self::String,
        Self::Comment,
        Self::Function,
        Self::Type,
        Self::Variable,
        Self::Number,
        Self::Operator,
        Self::Punctuation,
        Self::Constant,
        Self::Property,
        Self::Tag,
        Self::Attribute,
    ];

    /// Returns the canonical string name for this category.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::String => "string",
            Self::Comment => "comment",
            Self::Function => "function",
            Self::Type => "type",
            Self::Variable => "variable",
            Self::Number => "number",
            Self::Operator => "operator",
            Self::Punctuation => "punctuation",
            Self::Constant => "constant",
            Self::Property => "property",
            Self::Tag => "tag",
            Self::Attribute => "attribute",
        }
    }
}

impl fmt::Display for HighlightName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolve a tree-sitter capture name (e.g. `"keyword"`, `"function.method"`)
/// to a [`HighlightName`].
///
/// The resolver strips a leading `@` if present, then matches on the first
/// dotted component (so `"string.special"` maps to [`HighlightName::String`]).
#[must_use]
pub fn resolve_highlight_name(capture: &str) -> Option<HighlightName> {
    let name = capture.strip_prefix('@').unwrap_or(capture);
    let base = name.split('.').next().unwrap_or(name);

    match base {
        "keyword" => Some(HighlightName::Keyword),
        "string" => Some(HighlightName::String),
        "comment" => Some(HighlightName::Comment),
        "function" | "method" => Some(HighlightName::Function),
        "type" | "constructor" => Some(HighlightName::Type),
        "variable" | "identifier" | "parameter" => Some(HighlightName::Variable),
        "number" | "float" => Some(HighlightName::Number),
        "operator" => Some(HighlightName::Operator),
        "punctuation" | "bracket" | "delimiter" => Some(HighlightName::Punctuation),
        "constant" | "boolean" => Some(HighlightName::Constant),
        "property" | "field" => Some(HighlightName::Property),
        "tag" | "label" => Some(HighlightName::Tag),
        "attribute" => Some(HighlightName::Attribute),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_basic_names() {
        assert_eq!(
            resolve_highlight_name("keyword"),
            Some(HighlightName::Keyword)
        );
        assert_eq!(
            resolve_highlight_name("string"),
            Some(HighlightName::String)
        );
        assert_eq!(
            resolve_highlight_name("comment"),
            Some(HighlightName::Comment)
        );
        assert_eq!(
            resolve_highlight_name("function"),
            Some(HighlightName::Function)
        );
        assert_eq!(resolve_highlight_name("type"), Some(HighlightName::Type));
        assert_eq!(
            resolve_highlight_name("variable"),
            Some(HighlightName::Variable)
        );
        assert_eq!(
            resolve_highlight_name("number"),
            Some(HighlightName::Number)
        );
        assert_eq!(
            resolve_highlight_name("operator"),
            Some(HighlightName::Operator)
        );
        assert_eq!(
            resolve_highlight_name("punctuation"),
            Some(HighlightName::Punctuation)
        );
        assert_eq!(
            resolve_highlight_name("constant"),
            Some(HighlightName::Constant)
        );
        assert_eq!(
            resolve_highlight_name("property"),
            Some(HighlightName::Property)
        );
        assert_eq!(resolve_highlight_name("tag"), Some(HighlightName::Tag));
        assert_eq!(
            resolve_highlight_name("attribute"),
            Some(HighlightName::Attribute)
        );
    }

    #[test]
    fn resolve_dotted_names() {
        assert_eq!(
            resolve_highlight_name("keyword.control"),
            Some(HighlightName::Keyword)
        );
        assert_eq!(
            resolve_highlight_name("string.special"),
            Some(HighlightName::String)
        );
        assert_eq!(
            resolve_highlight_name("function.method"),
            Some(HighlightName::Function)
        );
        assert_eq!(
            resolve_highlight_name("variable.builtin"),
            Some(HighlightName::Variable)
        );
    }

    #[test]
    fn resolve_at_prefix() {
        assert_eq!(
            resolve_highlight_name("@keyword"),
            Some(HighlightName::Keyword)
        );
        assert_eq!(
            resolve_highlight_name("@string.special"),
            Some(HighlightName::String)
        );
    }

    #[test]
    fn resolve_aliases() {
        assert_eq!(
            resolve_highlight_name("method"),
            Some(HighlightName::Function)
        );
        assert_eq!(
            resolve_highlight_name("constructor"),
            Some(HighlightName::Type)
        );
        assert_eq!(
            resolve_highlight_name("identifier"),
            Some(HighlightName::Variable)
        );
        assert_eq!(
            resolve_highlight_name("parameter"),
            Some(HighlightName::Variable)
        );
        assert_eq!(resolve_highlight_name("float"), Some(HighlightName::Number));
        assert_eq!(
            resolve_highlight_name("bracket"),
            Some(HighlightName::Punctuation)
        );
        assert_eq!(
            resolve_highlight_name("delimiter"),
            Some(HighlightName::Punctuation)
        );
        assert_eq!(
            resolve_highlight_name("boolean"),
            Some(HighlightName::Constant)
        );
        assert_eq!(
            resolve_highlight_name("field"),
            Some(HighlightName::Property)
        );
        assert_eq!(resolve_highlight_name("label"), Some(HighlightName::Tag));
    }

    #[test]
    fn resolve_unknown_returns_none() {
        assert_eq!(resolve_highlight_name("foobar"), None);
        assert_eq!(resolve_highlight_name(""), None);
    }

    #[test]
    fn highlight_name_as_str_roundtrip() {
        for name in HighlightName::ALL {
            let s = name.as_str();
            assert_eq!(resolve_highlight_name(s), Some(*name));
        }
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", HighlightName::Keyword), "keyword");
        assert_eq!(format!("{}", HighlightName::String), "string");
    }
}
