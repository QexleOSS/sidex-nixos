//! Theme trie — keyed by `.`-segmented scope name.
//!
//! Direct port of `ThemeTrieElement` from `theme.ts`. Lookups descend
//! the trie one segment at a time (`comment.line.double-slash` →
//! `comment` → `line` → `double-slash`). Matches are accumulated from
//! the deepest hit up, then sorted by the specificity rules documented
//! on `_cmpBySpecificity` upstream.

use std::collections::BTreeMap;

use crate::utils;

use super::rule::{FontStyle, ThemeTrieElementRule};

/// Single trie node. Mirrors the upstream class — the main rule is the
/// "no parent scopes" bucket and `rules_with_parent_scopes` holds
/// rules that require a parent selector match.
#[derive(Debug, Clone)]
pub struct ThemeTrieElement {
    main_rule: ThemeTrieElementRule,
    rules_with_parent_scopes: Vec<ThemeTrieElementRule>,
    children: BTreeMap<String, ThemeTrieElement>,
}

impl ThemeTrieElement {
    pub fn new(
        main_rule: ThemeTrieElementRule,
        rules_with_parent_scopes: Vec<ThemeTrieElementRule>,
    ) -> Self {
        Self {
            main_rule,
            rules_with_parent_scopes,
            children: BTreeMap::new(),
        }
    }

    /// Returns rules in precedence order for the given `.`-segmented
    /// scope. Matches the upstream `match(scope)` method.
    #[must_use]
    pub fn r#match(&self, scope: &str) -> Vec<ThemeTrieElementRule> {
        if !scope.is_empty() {
            let (head, tail) = match scope.find('.') {
                Some(idx) => (&scope[..idx], &scope[idx + 1..]),
                None => (scope, ""),
            };
            if let Some(child) = self.children.get(head) {
                return child.r#match(tail);
            }
        }

        let mut rules: Vec<ThemeTrieElementRule> = self.rules_with_parent_scopes.clone();
        rules.push(self.main_rule.clone());
        rules.sort_by(cmp_by_specificity);
        rules
    }

    /// Upserts a rule at the right depth. Mirrors `insert`.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &mut self,
        scope_depth: u32,
        scope: &str,
        parent_scopes: Option<&[String]>,
        font_style: FontStyle,
        foreground: u32,
        background: u32,
        font_family: &str,
        font_size: f64,
        line_height: f64,
    ) {
        if scope.is_empty() {
            self.do_insert_here(
                scope_depth,
                parent_scopes,
                font_style,
                foreground,
                background,
                font_family,
                font_size,
                line_height,
            );
            return;
        }

        let (head, tail) = match scope.find('.') {
            Some(idx) => (&scope[..idx], &scope[idx + 1..]),
            None => (scope, ""),
        };

        let child = self.children.entry(head.to_string()).or_insert_with(|| {
            ThemeTrieElement::new(
                self.main_rule.clone(),
                ThemeTrieElementRule::clone_arr(&self.rules_with_parent_scopes),
            )
        });
        child.insert(
            scope_depth + 1,
            tail,
            parent_scopes,
            font_style,
            foreground,
            background,
            font_family,
            font_size,
            line_height,
        );
    }

    #[allow(clippy::too_many_arguments, clippy::similar_names)]
    fn do_insert_here(
        &mut self,
        scope_depth: u32,
        parent_scopes: Option<&[String]>,
        font_style: FontStyle,
        foreground: u32,
        background: u32,
        font_family: &str,
        font_size: f64,
        line_height: f64,
    ) {
        let Some(parent) = parent_scopes else {
            self.main_rule.accept_overwrite(
                scope_depth,
                font_style,
                foreground,
                background,
                font_family,
                font_size,
                line_height,
            );
            return;
        };

        for existing in &mut self.rules_with_parent_scopes {
            if utils::str_arr_cmp(Some(&existing.parent_scopes), Some(parent)) == 0 {
                existing.accept_overwrite(
                    scope_depth,
                    font_style,
                    foreground,
                    background,
                    font_family,
                    font_size,
                    line_height,
                );
                return;
            }
        }

        // Inherit unset fields from main rule.
        let resolved_font_style = if font_style.is_not_set() {
            self.main_rule.font_style
        } else {
            font_style
        };
        let resolved_fg = if foreground == 0 {
            self.main_rule.foreground
        } else {
            foreground
        };
        let resolved_bg = if background == 0 {
            self.main_rule.background
        } else {
            background
        };
        let resolved_family = if font_family.is_empty() {
            self.main_rule.font_family.clone()
        } else {
            font_family.to_string()
        };
        let resolved_size = if font_size == 0.0 {
            self.main_rule.font_size
        } else {
            font_size
        };
        let resolved_line = if line_height == 0.0 {
            self.main_rule.line_height
        } else {
            line_height
        };

        self.rules_with_parent_scopes.push(ThemeTrieElementRule {
            scope_depth,
            parent_scopes: parent.to_vec(),
            font_style: resolved_font_style,
            foreground: resolved_fg,
            background: resolved_bg,
            font_family: resolved_family,
            font_size: resolved_size,
            line_height: resolved_line,
        });
    }
}

/// Matches `_cmpBySpecificity` exactly: scope depth desc, then parent-
/// scope length ties walked index-by-index (longer wins), finally
/// parent-scope count desc.
fn cmp_by_specificity(a: &ThemeTrieElementRule, b: &ThemeTrieElementRule) -> std::cmp::Ordering {
    if a.scope_depth != b.scope_depth {
        return b.scope_depth.cmp(&a.scope_depth);
    }

    let mut a_idx = 0usize;
    let mut b_idx = 0usize;
    loop {
        if a.parent_scopes.get(a_idx).map(String::as_str) == Some(">") {
            a_idx += 1;
        }
        if b.parent_scopes.get(b_idx).map(String::as_str) == Some(">") {
            b_idx += 1;
        }
        if a_idx >= a.parent_scopes.len() || b_idx >= b.parent_scopes.len() {
            break;
        }
        let diff: i64 = i64::try_from(b.parent_scopes[b_idx].len()).unwrap_or(i64::MAX)
            - i64::try_from(a.parent_scopes[a_idx].len()).unwrap_or(i64::MAX);
        if diff != 0 {
            return if diff < 0 {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        a_idx += 1;
        b_idx += 1;
    }

    b.parent_scopes.len().cmp(&a.parent_scopes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare(main_fg: u32) -> ThemeTrieElement {
        ThemeTrieElement::new(
            ThemeTrieElementRule {
                scope_depth: 0,
                parent_scopes: Vec::new(),
                font_style: FontStyle::NOT_SET,
                foreground: main_fg,
                background: 0,
                font_family: String::new(),
                font_size: 0.0,
                line_height: 0.0,
            },
            Vec::new(),
        )
    }

    #[test]
    fn insert_then_match_dotted_scope() {
        let mut trie = bare(7);
        trie.insert(
            0,
            "comment.line",
            None,
            FontStyle::ITALIC,
            42,
            0,
            "",
            0.0,
            0.0,
        );
        let matches = trie.r#match("comment.line.double-slash");
        assert!(matches.iter().any(|r| r.foreground == 42));
    }

    #[test]
    fn parent_scope_rule_is_stored_separately() {
        let mut trie = bare(7);
        trie.insert(
            0,
            "string",
            Some(&["meta.function".to_string()]),
            FontStyle::BOLD,
            99,
            0,
            "",
            0.0,
            0.0,
        );
        let matches = trie.r#match("string");
        // First rule should be the parent-scoped one (specificity wins).
        let with_parent = matches
            .iter()
            .find(|r| !r.parent_scopes.is_empty())
            .unwrap();
        assert_eq!(with_parent.foreground, 99);
    }
}
