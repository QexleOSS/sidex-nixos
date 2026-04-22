//! Auto-indentation rules for computing indent actions based on line content.
//!
//! Mirrors Monaco's `IndentAction`/`IndentRule` system so that the editor can
//! automatically indent or outdent when the user types or presses Enter.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// What the editor should do with indentation after a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndentAction {
    /// Keep the current indentation.
    None,
    /// Increase indentation by one level.
    Indent,
    /// Increase indentation for the content between begin/end tokens, then
    /// outdent the closing token (e.g. `{` with `}` on the next line).
    IndentOutdent,
    /// Decrease indentation by one level.
    Outdent,
}

/// A rule that matches line text to determine an indentation action.
#[derive(Debug, Clone)]
pub struct IndentRule {
    /// Regex that must match the text **before** the cursor (typically the
    /// previous line or the portion before a newline).
    pub before_text: Regex,
    /// Optional regex that must match the text **after** the cursor
    /// (typically the current line or the portion after a newline).
    pub after_text: Option<Regex>,
    /// The indentation action to apply when the rule matches.
    pub action: IndentAction,
}

impl IndentRule {
    /// Creates a new indent rule from pattern strings.
    ///
    /// Returns `None` if `before_text` is not a valid regex.
    pub fn new(before_text: &str, after_text: Option<&str>, action: IndentAction) -> Option<Self> {
        let before = Regex::new(before_text).ok()?;
        let after = match after_text {
            Some(pat) => Some(Regex::new(pat).ok()?),
            None => None,
        };
        Some(Self {
            before_text: before,
            after_text: after,
            action,
        })
    }
}

/// Rules evaluated when the user presses Enter.
#[derive(Debug, Clone)]
pub struct OnEnterRule {
    /// Regex that must match the line text before the cursor.
    pub before_text: Regex,
    /// Optional regex that must match the text after the cursor on the same
    /// line.
    pub after_text: Option<Regex>,
    /// The action to apply.
    pub action: IndentAction,
}

impl OnEnterRule {
    /// Creates a new on-enter rule from pattern strings.
    pub fn new(before_text: &str, after_text: Option<&str>, action: IndentAction) -> Option<Self> {
        let before = Regex::new(before_text).ok()?;
        let after = match after_text {
            Some(pat) => Some(Regex::new(pat).ok()?),
            None => None,
        };
        Some(Self {
            before_text: before,
            after_text: after,
            action,
        })
    }
}

/// Configuration for marker-based folding (e.g. `#region` / `#endregion`).
#[derive(Debug, Clone)]
pub struct FoldingRules {
    /// Regex matching the start of a foldable region marker.
    pub markers_start: Option<Regex>,
    /// Regex matching the end of a foldable region marker.
    pub markers_end: Option<Regex>,
}

impl FoldingRules {
    pub fn new(start: Option<&str>, end: Option<&str>) -> Option<Self> {
        let s = match start {
            Some(pat) => Some(Regex::new(pat).ok()?),
            None => None,
        };
        let e = match end {
            Some(pat) => Some(Regex::new(pat).ok()?),
            None => None,
        };
        Some(Self {
            markers_start: s,
            markers_end: e,
        })
    }
}

/// Computes the indent action for the current line given the previous line's
/// text, the current line's text, and a set of indent rules.
///
/// Returns the first matching rule's action, or [`IndentAction::None`] if no
/// rules match.
#[must_use]
pub fn compute_indent(prev_line: &str, current_line: &str, rules: &[IndentRule]) -> IndentAction {
    for rule in rules {
        if rule.before_text.is_match(prev_line) {
            if let Some(ref after) = rule.after_text {
                if after.is_match(current_line) {
                    return rule.action;
                }
            } else {
                return rule.action;
            }
        }
    }
    IndentAction::None
}

/// Returns the default indentation rules for common bracket patterns.
///
/// These handle `{`, `(`, `[` for indent and `}`, `)`, `]` for outdent.
#[must_use]
pub fn default_indent_rules() -> Vec<IndentRule> {
    let mut rules = Vec::new();

    if let Some(r) = IndentRule::new(
        r"[{(\[]\s*$",
        Some(r"^\s*[})\]]"),
        IndentAction::IndentOutdent,
    ) {
        rules.push(r);
    }
    if let Some(r) = IndentRule::new(r"[{(\[]\s*$", None, IndentAction::Indent) {
        rules.push(r);
    }
    if let Some(r) = IndentRule::new(r"^\s*[})\]]", None, IndentAction::Outdent) {
        rules.push(r);
    }

    rules
}

/// Returns default on-enter rules for common patterns.
#[must_use]
pub fn default_on_enter_rules() -> Vec<OnEnterRule> {
    let mut rules = Vec::new();

    if let Some(r) = OnEnterRule::new(
        r"[{(\[]\s*$",
        Some(r"^\s*[})\]]"),
        IndentAction::IndentOutdent,
    ) {
        rules.push(r);
    }
    if let Some(r) = OnEnterRule::new(r"[{(\[]\s*$", None, IndentAction::Indent) {
        rules.push(r);
    }
    if let Some(r) = OnEnterRule::new(r"^\s*\*\s", None, IndentAction::None) {
        rules.push(r);
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_indent_open_brace() {
        let rules = default_indent_rules();
        let action = compute_indent("fn main() {", "    ", &rules);
        assert_eq!(action, IndentAction::Indent);
    }

    #[test]
    fn compute_indent_close_brace() {
        let rules = default_indent_rules();
        let action = compute_indent("    let x = 1;", "}", &rules);
        assert_eq!(action, IndentAction::None);
    }

    #[test]
    fn compute_indent_outdent_on_prev_line() {
        let rules = default_indent_rules();
        let action = compute_indent("}", "fn next() {", &rules);
        assert_eq!(action, IndentAction::Outdent);
    }

    #[test]
    fn compute_indent_no_match() {
        let rules = default_indent_rules();
        let action = compute_indent("let x = 1;", "let y = 2;", &rules);
        assert_eq!(action, IndentAction::None);
    }

    #[test]
    fn compute_indent_open_paren() {
        let rules = default_indent_rules();
        let action = compute_indent("foo(", "", &rules);
        assert_eq!(action, IndentAction::Indent);
    }

    #[test]
    fn compute_indent_open_bracket() {
        let rules = default_indent_rules();
        let action = compute_indent("let arr = [", "", &rules);
        assert_eq!(action, IndentAction::Indent);
    }

    #[test]
    fn indent_outdent_brace_pair() {
        let rules = default_indent_rules();
        let action = compute_indent("fn main() {", "}", &rules);
        assert_eq!(action, IndentAction::IndentOutdent);
    }

    #[test]
    fn indent_rule_new_invalid_regex() {
        let rule = IndentRule::new("[invalid", None, IndentAction::Indent);
        assert!(rule.is_none());
    }

    #[test]
    fn indent_rule_new_valid() {
        let rule = IndentRule::new(r"\{", Some(r"\}"), IndentAction::IndentOutdent);
        assert!(rule.is_some());
    }

    #[test]
    fn on_enter_rule_new() {
        let rule = OnEnterRule::new(r"\{\s*$", Some(r"^\s*\}"), IndentAction::IndentOutdent);
        assert!(rule.is_some());
    }

    #[test]
    fn default_indent_rules_non_empty() {
        let rules = default_indent_rules();
        assert!(!rules.is_empty());
    }

    #[test]
    fn default_on_enter_rules_non_empty() {
        let rules = default_on_enter_rules();
        assert!(!rules.is_empty());
    }

    #[test]
    fn folding_rules_new() {
        let rules = FoldingRules::new(Some(r"#region"), Some(r"#endregion"));
        assert!(rules.is_some());
        let rules = rules.unwrap();
        assert!(rules.markers_start.is_some());
        assert!(rules.markers_end.is_some());
    }

    #[test]
    fn folding_rules_none_patterns() {
        let rules = FoldingRules::new(None, None);
        assert!(rules.is_some());
        let rules = rules.unwrap();
        assert!(rules.markers_start.is_none());
        assert!(rules.markers_end.is_none());
    }

    #[test]
    fn indent_action_serialize() {
        let json = serde_json::to_string(&IndentAction::Indent).unwrap();
        let back: IndentAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back, IndentAction::Indent);
    }

    #[test]
    fn empty_rules_returns_none() {
        let action = compute_indent("anything", "anything", &[]);
        assert_eq!(action, IndentAction::None);
    }
}
