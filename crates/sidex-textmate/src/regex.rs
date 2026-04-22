//! Oniguruma regex wrapper.
//!
//! `vscode-textmate` calls into Oniguruma via an asm.js/WASM build
//! (`vscode-oniguruma`) because every `TextMate` grammar depends on
//! Oniguruma-specific features: `\G`, `\A`, possessive quantifiers with
//! backreferences, and POSIX bracket classes. Ordinary `regex`
//! expressions can't execute the grammars correctly.
//!
//! This module exposes a thin, safe wrapper around `onig::Regex` with the
//! options matching those `vscode-textmate` uses when compiling grammar
//! patterns: `ONIG_OPTION_CAPTURE_GROUP` is always on, and `ONIG_SYNTAX_DEFAULT`
//! is used (Oniguruma's Ruby-compatible dialect).

use crate::TextMateError;
use onig::{Regex, RegexOptions, Syntax};

/// Compiled Oniguruma regex usable from the tokenizer hot path.
pub struct OnigRegex {
    inner: Regex,
    source: String,
}

impl std::fmt::Debug for OnigRegex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnigRegex")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl OnigRegex {
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the byte offset of the first match on or after `start`.
    pub fn search(&self, text: &str, start: usize) -> Option<MatchResult> {
        let region = onig::Region::new();
        let opts = onig::SearchOptions::SEARCH_OPTION_NONE;
        let hit = self.inner.search_with_options(
            text,
            start,
            text.len(),
            opts,
            Some(&mut region.clone()),
        )?;

        // onig returns `Some(match_start)` when the leftmost match begins
        // at `hit` (byte offset). We also need the full group capture table.
        let mut new_region = onig::Region::new();
        self.inner
            .search_with_options(text, start, text.len(), opts, Some(&mut new_region))?;

        let groups = (0..new_region.len())
            .map(|idx| new_region.pos(idx))
            .collect();
        Some(MatchResult { start: hit, groups })
    }
}

/// A single match with captured groups.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub start: usize,
    /// Byte ranges for each capture group. Group `0` is the whole match.
    pub groups: Vec<Option<(usize, usize)>>,
}

impl MatchResult {
    pub fn end(&self) -> usize {
        self.groups
            .first()
            .and_then(|g| g.map(|(_, e)| e))
            .unwrap_or(self.start)
    }
}

/// Builder for [`OnigRegex`], following `vscode-textmate`'s flags.
pub struct RegexBuilder {
    pattern: String,
    case_insensitive: bool,
    extended: bool,
    multiline: bool,
}

impl RegexBuilder {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            case_insensitive: false,
            extended: false,
            multiline: false,
        }
    }

    #[must_use]
    pub fn case_insensitive(mut self, v: bool) -> Self {
        self.case_insensitive = v;
        self
    }

    #[must_use]
    pub fn multiline(mut self, v: bool) -> Self {
        self.multiline = v;
        self
    }

    #[must_use]
    pub fn extended(mut self, v: bool) -> Self {
        self.extended = v;
        self
    }

    pub fn build(self) -> Result<OnigRegex, TextMateError> {
        let mut options = RegexOptions::REGEX_OPTION_CAPTURE_GROUP;
        if self.case_insensitive {
            options |= RegexOptions::REGEX_OPTION_IGNORECASE;
        }
        if self.extended {
            options |= RegexOptions::REGEX_OPTION_EXTEND;
        }
        if self.multiline {
            options |= RegexOptions::REGEX_OPTION_MULTILINE;
        }
        let inner = Regex::with_options(&self.pattern, options, Syntax::default())
            .map_err(|e| TextMateError::RegexCompile(e.description().to_string()))?;
        Ok(OnigRegex {
            inner,
            source: self.pattern,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_basic_regex() {
        let regex = RegexBuilder::new(r"\b(if|else|for)\b").build().unwrap();
        let hit = regex.search("let x = if true 1 else 2;", 0).unwrap();
        assert!(hit.start > 0);
    }

    #[test]
    fn rejects_invalid_regex() {
        let err = RegexBuilder::new("(").build().unwrap_err();
        matches!(err, TextMateError::RegexCompile(_));
    }
}
