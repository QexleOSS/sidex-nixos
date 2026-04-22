//! Theme rule value types.
//!
//! Ports `FontStyle`, `StyleAttributes`, `ParsedThemeRule`, and
//! `ThemeTrieElementRule` from `theme.ts`. These are plain-data
//! structures; the behavioral logic lives in the parent module.

use crate::metadata;

pub use metadata::FontStyle;

/// Resolved style for a matched scope. Matches the upstream class.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleAttributes {
    pub font_style: FontStyle,
    pub foreground_id: u32,
    pub background_id: u32,
    pub font_family: String,
    pub font_size: f64,
    pub line_height: f64,
}

/// Intermediate parse output — one per split scope entry. Port of the
/// upstream `ParsedThemeRule` class (positional ctor args preserved as
/// named fields for clarity).
#[derive(Debug, Clone)]
pub struct ParsedThemeRule {
    pub scope: String,
    pub parent_scopes: Option<Vec<String>>,
    pub index: u32,
    pub font_style: FontStyle,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub font_family: String,
    pub font_size: f64,
    pub line_height: f64,
}

/// Rule stored inside a [`super::ThemeTrieElement`]. Direct port of
/// `ThemeTrieElementRule` — mutable so upstream's `acceptOverwrite`
/// merge-in-place semantics carry over.
#[derive(Debug, Clone)]
pub struct ThemeTrieElementRule {
    pub scope_depth: u32,
    pub parent_scopes: Vec<String>,
    pub font_style: FontStyle,
    pub foreground: u32,
    pub background: u32,
    pub font_family: String,
    pub font_size: f64,
    pub line_height: f64,
}

impl ThemeTrieElementRule {
    /// Merges a newly-observed rule into `self` in-place. Port of
    /// `acceptOverwrite`.
    #[allow(clippy::too_many_arguments)]
    pub fn accept_overwrite(
        &mut self,
        scope_depth: u32,
        font_style: FontStyle,
        foreground: u32,
        background: u32,
        font_family: &str,
        font_size: f64,
        line_height: f64,
    ) {
        if self.scope_depth > scope_depth {
            // Upstream logs "how did this happen?"; preserve that note
            // for anyone chasing the same edge case.
            log::debug!(
                "theme trie accept_overwrite received shallower depth (have {} got {})",
                self.scope_depth,
                scope_depth
            );
        } else {
            self.scope_depth = scope_depth;
        }
        if !font_style.is_not_set() {
            self.font_style = font_style;
        }
        if foreground != 0 {
            self.foreground = foreground;
        }
        if background != 0 {
            self.background = background;
        }
        if !font_family.is_empty() {
            self.font_family = font_family.to_string();
        }
        if font_size != 0.0 {
            self.font_size = font_size;
        }
        if line_height != 0.0 {
            self.line_height = line_height;
        }
    }

    pub(super) fn clone_arr(rules: &[ThemeTrieElementRule]) -> Vec<ThemeTrieElementRule> {
        rules.to_vec()
    }
}
