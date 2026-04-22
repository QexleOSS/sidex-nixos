//! Utility helpers.
//!
//! Faithful Rust port of `src/utils.ts` from `vscode-textmate` (MIT,
//! Copyright Microsoft Corporation). Provides the handful of pure
//! helpers the rest of the crate depends on: regex-source substitution,
//! scope-path comparison, lazily-compiled static regexes, and a simple
//! memoizing cache used by the theme matcher.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::OnceLock;

use parking_lot::Mutex;
use regex::Regex;

/// Lazily-compiled static regex. Rust has no global mutable state, so we
/// wrap a `OnceLock<Regex>` behind a `const`-constructible type.
pub struct LazyRegex {
    pattern: &'static str,
    cell: OnceLock<Regex>,
}

impl LazyRegex {
    pub const fn new(pattern: &'static str) -> Self {
        Self {
            pattern,
            cell: OnceLock::new(),
        }
    }

    pub fn get(&self) -> &Regex {
        self.cell
            .get_or_init(|| Regex::new(self.pattern).expect("static regex must compile"))
    }
}

/// Capture-index pair. Mirrors `IOnigCaptureIndex` — byte offsets into
/// the original UTF-8 line.
#[derive(Debug, Clone, Copy)]
pub struct CaptureIndex {
    pub start: usize,
    pub end: usize,
    pub length: usize,
}

impl CaptureIndex {
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            length: end.saturating_sub(start),
        }
    }
}

/// Helpers mirroring the `RegexSource` namespace in `utils.ts`.
pub mod regex_source {
    use super::{CaptureIndex, LazyRegex};

    /// Pattern used by `hasCaptures` / `replaceCaptures`. Matches `$0..9`
    /// style references and the `${n:/upcase|downcase}` extended form.
    static CAPTURING: LazyRegex = LazyRegex::new(r"\$(\d+)|\$\{(\d+):/(downcase|upcase)\}");

    /// True if `source` contains any `$N` or `${N:/...}` capture reference.
    pub fn has_captures(source: Option<&str>) -> bool {
        match source {
            None => false,
            Some(s) => CAPTURING.get().is_match(s),
        }
    }

    /// Substitutes capture references against `capture_source`. The
    /// `/downcase` and `/upcase` commands are honored with ASCII
    /// case-folding semantics that match JavaScript's
    /// `toLowerCase` / `toUpperCase` for the Latin-1 subset
    /// (sufficient for grammar authors in practice).
    pub fn replace_captures(
        source: &str,
        capture_source: &str,
        captures: &[Option<CaptureIndex>],
    ) -> String {
        let regex = CAPTURING.get();
        let mut out = String::with_capacity(source.len());
        let mut last_end = 0;

        for caps in regex.captures_iter(source) {
            let whole = caps.get(0).expect("group 0 always present");
            out.push_str(&source[last_end..whole.start()]);

            let index_str = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str());
            let command = caps.get(3).map(|m| m.as_str());

            let Some(index) = index_str.and_then(|s| s.parse::<usize>().ok()) else {
                out.push_str(whole.as_str());
                last_end = whole.end();
                continue;
            };

            let resolved = captures.get(index).and_then(|c| c.as_ref()).and_then(|c| {
                let slice = capture_source.get(c.start..c.end)?;
                // Leading dots can create invalid scope selectors, so
                // strip them — same rule as upstream.
                let trimmed = slice.trim_start_matches('.');
                Some(match command {
                    Some("downcase") => trimmed.to_ascii_lowercase(),
                    Some("upcase") => trimmed.to_ascii_uppercase(),
                    _ => trimmed.to_string(),
                })
            });

            match resolved {
                Some(value) => out.push_str(&value),
                None => out.push_str(whole.as_str()),
            }
            last_end = whole.end();
        }

        out.push_str(&source[last_end..]);
        out
    }
}

/// Lexicographic compare of two strings, returning `-1 / 0 / 1` to match
/// upstream's `strcmp` ordering semantics.
pub fn strcmp(a: &str, b: &str) -> i32 {
    match a.cmp(b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Element-wise compare of two string arrays, with shorter arrays
/// sorting before longer ones (mirrors `strArrCmp`). `None` sorts before
/// `Some` — which is how upstream treats `null` arrays.
pub fn str_arr_cmp(a: Option<&[String]>, b: Option<&[String]>) -> i32 {
    match (a, b) {
        (None, None) => 0,
        (None, Some(_)) => -1,
        (Some(_), None) => 1,
        (Some(aa), Some(bb)) => {
            if aa.len() == bb.len() {
                for (x, y) in aa.iter().zip(bb.iter()) {
                    let res = strcmp(x, y);
                    if res != 0 {
                        return res;
                    }
                }
                0
            } else {
                i32::try_from(aa.len())
                    .unwrap_or(0)
                    .saturating_sub(i32::try_from(bb.len()).unwrap_or(0))
            }
        }
    }
}

/// Validates a CSS hex color string. Accepts `#rgb`, `#rgba`, `#rrggbb`,
/// or `#rrggbbaa`. Matches `isValidHexColor` exactly.
pub fn is_valid_hex_color(hex: &str) -> bool {
    static RGB6: LazyRegex = LazyRegex::new(r"^#[0-9a-fA-F]{6}$");
    static RGB8: LazyRegex = LazyRegex::new(r"^#[0-9a-fA-F]{8}$");
    static RGB3: LazyRegex = LazyRegex::new(r"^#[0-9a-fA-F]{3}$");
    static RGB4: LazyRegex = LazyRegex::new(r"^#[0-9a-fA-F]{4}$");

    RGB6.get().is_match(hex)
        || RGB8.get().is_match(hex)
        || RGB3.get().is_match(hex)
        || RGB4.get().is_match(hex)
}

/// Escapes regex-special characters, matching the upstream helper.
pub fn escape_regex_chars(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(
            ch,
            '\\' | '|' | '(' | '[' | '{' | '}' | ']' | ')' | '.' | '?' | '*' | '+' | '^' | '$'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Returns the last path segment of a `/`- or `\\`-separated path.
/// Mirrors upstream's `basename`.
pub fn basename(path: &str) -> &str {
    let last_slash = path.rfind('/');
    let last_back = path.rfind('\\');
    let idx = match (last_slash, last_back) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    };
    match idx {
        None => path,
        Some(i) if i == path.len() - 1 => basename(&path[..i]),
        Some(i) => &path[i + 1..],
    }
}

/// Generic memoizing cache. Port of `CachedFn<TKey, TValue>` — used by
/// the theme matcher to avoid recomputing for repeated scope names.
pub struct CachedFn<K: Eq + Hash, V: Clone> {
    inner: Mutex<HashMap<K, V>>,
}

impl<K: Eq + Hash, V: Clone> Default for CachedFn<K, V> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl<K: Eq + Hash + Clone, V: Clone> CachedFn<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get<F: FnOnce(&K) -> V>(&self, key: K, f: F) -> V {
        let mut guard = self.inner.lock();
        if let Some(v) = guard.get(&key) {
            return v.clone();
        }
        let value = f(&key);
        guard.insert(key, value.clone());
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strcmp_matches_ordering() {
        assert_eq!(strcmp("a", "b"), -1);
        assert_eq!(strcmp("b", "b"), 0);
        assert_eq!(strcmp("c", "b"), 1);
    }

    #[test]
    fn str_arr_cmp_orders_by_len_then_content() {
        let a = vec!["x".to_string()];
        let b = vec!["y".to_string(), "z".to_string()];
        assert!(str_arr_cmp(Some(&a), Some(&b)) < 0);
        let c = vec!["x".to_string()];
        assert_eq!(str_arr_cmp(Some(&a), Some(&c)), 0);
    }

    #[test]
    fn hex_colors_follow_upstream_rules() {
        assert!(is_valid_hex_color("#abc"));
        assert!(is_valid_hex_color("#abcd"));
        assert!(is_valid_hex_color("#aabbcc"));
        assert!(is_valid_hex_color("#aabbccdd"));
        assert!(!is_valid_hex_color("abc"));
        assert!(!is_valid_hex_color("#abcde"));
    }

    #[test]
    fn escape_regex_escapes_specials() {
        assert_eq!(escape_regex_chars("a.b+c*"), r"a\.b\+c\*");
        assert_eq!(escape_regex_chars("plain"), "plain");
    }

    #[test]
    fn basename_handles_trailing_and_windows_paths() {
        assert_eq!(basename("/foo/bar.tmLanguage"), "bar.tmLanguage");
        assert_eq!(basename(r"C:\grammars\x.json"), "x.json");
        assert_eq!(basename("foo/"), "foo");
        assert_eq!(basename("alone"), "alone");
    }

    #[test]
    fn replace_captures_substitutes_numbered_groups() {
        let captures = vec![Some(CaptureIndex::new(0, 5)), Some(CaptureIndex::new(6, 9))];
        let result = regex_source::replace_captures("$0-$1", "hello foo", &captures);
        assert_eq!(result, "hello-foo");
    }

    #[test]
    fn replace_captures_applies_case_commands() {
        let captures = vec![Some(CaptureIndex::new(0, 5))];
        let downcased = regex_source::replace_captures(r"${0:/downcase}", "HELLO", &captures);
        assert_eq!(downcased, "hello");
        let upcased = regex_source::replace_captures(r"${0:/upcase}", "world", &captures);
        assert_eq!(upcased, "WORLD");
    }

    #[test]
    fn cached_fn_memoizes_results() {
        let cache = CachedFn::<String, usize>::new();
        let calls = parking_lot::Mutex::new(0usize);
        let a = cache.get("key".to_string(), |_| {
            *calls.lock() += 1;
            42
        });
        let b = cache.get("key".to_string(), |_| {
            *calls.lock() += 1;
            99
        });
        assert_eq!(a, 42);
        assert_eq!(b, 42);
        assert_eq!(*calls.lock(), 1);
    }
}
