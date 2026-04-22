//! Scope-selector matcher.
//!
//! Faithful Rust port of `src/matcher.ts` from `vscode-textmate` (MIT,
//! Copyright Microsoft Corporation). Parses a `TextMate` scope-selector
//! expression (e.g. `source.ts string.quoted, -comment`) into a list of
//! boolean matchers with priority flags. Used for theme rules (matching
//! against a scope stack) and injection-selector evaluation.
//!
//! The parser follows the upstream grammar exactly:
//!
//! ```text
//!   selector    = (priority? conjunction (',' priority? conjunction)*)
//!   priority    = 'L:' | 'R:'
//!   conjunction = operand+
//!   operand     = '-' operand
//!               | '(' inner ')'
//!               | identifier+
//!   inner       = conjunction (('|' | ',')+ conjunction)*
//! ```

use regex::Regex;

use crate::utils::LazyRegex;

/// Type alias for a compiled scope-selector predicate.
pub type BoxMatcher<T> = Box<dyn Fn(&T) -> bool + Send + Sync>;

/// Priority of a scope-selector branch.
///
/// `L:` → `Left` (`-1`), `R:` → `Right` (`1`), no prefix → `Normal` (`0`).
/// Matches upstream's `-1 | 0 | 1` values precisely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Left = -1,
    Normal = 0,
    Right = 1,
}

impl Priority {
    pub const fn as_i8(self) -> i8 {
        self as i8
    }
}

/// A compiled matcher bundled with its priority marker.
pub struct MatcherWithPriority<T> {
    pub matcher: BoxMatcher<T>,
    pub priority: Priority,
}

impl<T> std::fmt::Debug for MatcherWithPriority<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MatcherWithPriority")
            .field("priority", &self.priority)
            .finish_non_exhaustive()
    }
}

/// Compiles a selector into one or more priority-tagged matchers.
///
/// `matches_name` is invoked with the tokenized identifier segments for
/// each leaf and the caller-supplied context value (typically a scope
/// stack slice). Port of the `createMatchers<T>` upstream function.
pub fn create_matchers<T, F>(selector: &str, matches_name: F) -> Vec<MatcherWithPriority<T>>
where
    T: 'static,
    F: Fn(&[String], &T) -> bool + Send + Sync + Clone + 'static,
{
    let mut results: Vec<MatcherWithPriority<T>> = Vec::new();
    let mut tokenizer = Tokenizer::new(selector);
    let mut token = tokenizer.next();

    loop {
        let mut priority = Priority::Normal;
        if let Some(tok) = token.as_deref() {
            if tok.len() == 2 && tok.as_bytes()[1] == b':' {
                match tok.as_bytes()[0] {
                    b'R' => priority = Priority::Right,
                    b'L' => priority = Priority::Left,
                    _ => log::debug!("unknown priority `{tok}` in scope selector"),
                }
                token = tokenizer.next();
            }
        }

        let matcher = parse_conjunction(&mut tokenizer, &mut token, matches_name.clone());
        results.push(MatcherWithPriority { matcher, priority });

        match token.as_deref() {
            Some(",") => {
                token = tokenizer.next();
            }
            _ => break,
        }
    }

    results
}

fn parse_operand<T, F>(
    tokenizer: &mut Tokenizer<'_>,
    token: &mut Option<String>,
    matches_name: F,
) -> Option<BoxMatcher<T>>
where
    T: 'static,
    F: Fn(&[String], &T) -> bool + Send + Sync + Clone + 'static,
{
    if token.as_deref() == Some("-") {
        *token = tokenizer.next();
        let inner = parse_operand::<T, _>(tokenizer, token, matches_name);
        return Some(Box::new(move |input: &T| match inner.as_ref() {
            Some(f) => !f(input),
            None => false,
        }));
    }

    if token.as_deref() == Some("(") {
        *token = tokenizer.next();
        let inner = parse_inner_expression::<T, _>(tokenizer, token, matches_name);
        if token.as_deref() == Some(")") {
            *token = tokenizer.next();
        }
        return Some(inner);
    }

    if is_identifier(token.as_deref()) {
        let mut idents: Vec<String> = Vec::new();
        while is_identifier(token.as_deref()) {
            if let Some(t) = token.take() {
                idents.push(t);
            }
            *token = tokenizer.next();
        }
        let matcher = matches_name.clone();
        return Some(Box::new(move |input: &T| matcher(&idents, input)));
    }

    None
}

fn parse_conjunction<T, F>(
    tokenizer: &mut Tokenizer<'_>,
    token: &mut Option<String>,
    matches_name: F,
) -> BoxMatcher<T>
where
    T: 'static,
    F: Fn(&[String], &T) -> bool + Send + Sync + Clone + 'static,
{
    let mut matchers: Vec<BoxMatcher<T>> = Vec::new();
    while let Some(m) = parse_operand::<T, _>(tokenizer, token, matches_name.clone()) {
        matchers.push(m);
    }
    Box::new(move |input: &T| matchers.iter().all(|m| m(input)))
}

fn parse_inner_expression<T, F>(
    tokenizer: &mut Tokenizer<'_>,
    token: &mut Option<String>,
    matches_name: F,
) -> BoxMatcher<T>
where
    T: 'static,
    F: Fn(&[String], &T) -> bool + Send + Sync + Clone + 'static,
{
    let mut matchers: Vec<BoxMatcher<T>> = Vec::new();
    matchers.push(parse_conjunction::<T, _>(
        tokenizer,
        token,
        matches_name.clone(),
    ));
    while matches!(token.as_deref(), Some("|" | ",")) {
        while matches!(token.as_deref(), Some("|" | ",")) {
            *token = tokenizer.next();
        }
        matchers.push(parse_conjunction::<T, _>(
            tokenizer,
            token,
            matches_name.clone(),
        ));
    }
    Box::new(move |input: &T| matchers.iter().any(|m| m(input)))
}

fn is_identifier(token: Option<&str>) -> bool {
    static IDENTIFIER: LazyRegex = LazyRegex::new(r"[\w\.:]+");
    token.is_some_and(|t| IDENTIFIER.get().is_match(t))
}

struct Tokenizer<'a> {
    input: &'a str,
    regex: &'static Regex,
    cursor: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        static SELECTOR: LazyRegex = LazyRegex::new(r"([LR]:|[\w\.:][\w\.:\-]*|[,|\-()])");
        Self {
            input,
            regex: SELECTOR.get(),
            cursor: 0,
        }
    }

    fn next(&mut self) -> Option<String> {
        if self.cursor >= self.input.len() {
            return None;
        }
        let hay = &self.input[self.cursor..];
        let m = self.regex.find(hay)?;
        self.cursor += m.end();
        Some(m.as_str().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::ptr_arg)]
    fn name_matcher(names: &[String], input: &Vec<String>) -> bool {
        let scopes: &[String] = input.as_slice();
        names.iter().all(|n| {
            scopes.iter().any(|scope| {
                scope == n || (scope.starts_with(n) && scope.as_bytes().get(n.len()) == Some(&b'.'))
            })
        })
    }

    #[test]
    fn single_identifier_matches() {
        let matchers = create_matchers::<Vec<String>, _>("string", name_matcher);
        assert_eq!(matchers.len(), 1);
        assert_eq!(matchers[0].priority, Priority::Normal);
        assert!((matchers[0].matcher)(&vec!["string".to_string()]));
        assert!((matchers[0].matcher)(&vec!["string.quoted".to_string()]));
        assert!(!(matchers[0].matcher)(&vec!["comment".to_string()]));
    }

    #[test]
    fn comma_produces_multiple_matchers() {
        let matchers = create_matchers::<Vec<String>, _>("string, comment", name_matcher);
        assert_eq!(matchers.len(), 2);
    }

    #[test]
    fn priority_prefixes_are_parsed() {
        let matchers = create_matchers::<Vec<String>, _>("R:string, L:comment", name_matcher);
        assert_eq!(matchers[0].priority, Priority::Right);
        assert_eq!(matchers[1].priority, Priority::Left);
    }

    #[test]
    fn negation_operator_works() {
        let matchers = create_matchers::<Vec<String>, _>("-comment", name_matcher);
        assert!((matchers[0].matcher)(&vec!["string".to_string()]));
        assert!(!(matchers[0].matcher)(&vec!["comment.line".to_string()]));
    }

    #[test]
    fn parens_allow_alternation() {
        let matchers = create_matchers::<Vec<String>, _>("(string | comment)", name_matcher);
        assert!((matchers[0].matcher)(&vec!["string".to_string()]));
        assert!((matchers[0].matcher)(&vec!["comment".to_string()]));
        assert!(!(matchers[0].matcher)(&vec!["keyword".to_string()]));
    }

    #[test]
    fn conjunction_requires_all_operands() {
        let matchers = create_matchers::<Vec<String>, _>("meta.function string", name_matcher);
        assert!((matchers[0].matcher)(&vec![
            "meta.function".into(),
            "string".into(),
        ]));
        assert!(!(matchers[0].matcher)(&vec!["string".into()]));
    }
}
