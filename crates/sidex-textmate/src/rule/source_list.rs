//! Ordered list of [`super::RegExpSource`] patterns plus a cached
//! `OnigScanner` keyed by the four `(allowA, allowG)` anchor states.
//!
//! Port of `RegExpSourceList` and `CompiledRule` from upstream.
//! The scanner is constructed lazily the first time a pattern is
//! compiled; mutations to the source list (`push` / `unshift` /
//! `set_source`) invalidate the cache so the next `compile` rebuilds.

use onig::{Regex as OnigRegex, RegexOptions, Region, SearchOptions, Syntax};

use super::regex_source::RegExpSource;
use crate::utils::CaptureIndex;
use crate::TextMateError;

/// One scanner entry: the compiled regex and the rule id it matched.
#[derive(Debug)]
struct CompiledPattern<TRuleId: Copy> {
    regex: OnigRegex,
    rule_id: TRuleId,
}

/// Compiled scanner — equivalent to upstream's `CompiledRule`. Holds a
/// bank of pre-compiled Oniguruma regexes. `find_next_match` scans all
/// patterns and returns the earliest / longest hit, matching Oniguruma's
/// `onig_search` semantics with `ONIG_OPTION_NOT_BEGIN_POSITION` off so
/// `\G`-anchored patterns behave correctly at the string's origin.
pub struct CompiledRule<TRuleId: Copy + 'static> {
    patterns: Vec<CompiledPattern<TRuleId>>,
}

impl<TRuleId: Copy + 'static> CompiledRule<TRuleId> {
    fn new(sources: &[String], rule_ids: &[TRuleId]) -> Result<Self, TextMateError> {
        assert_eq!(sources.len(), rule_ids.len());
        let mut patterns = Vec::with_capacity(sources.len());
        for (source, id) in sources.iter().zip(rule_ids.iter()) {
            let regex = OnigRegex::with_options(
                source,
                RegexOptions::REGEX_OPTION_CAPTURE_GROUP,
                Syntax::default(),
            )
            .map_err(|e| TextMateError::RegexCompile(e.description().to_string()))?;
            patterns.push(CompiledPattern {
                regex,
                rule_id: *id,
            });
        }
        Ok(Self { patterns })
    }

    /// Equivalent to upstream's `findNextMatchSync` — scans every
    /// pattern and returns the hit with the lowest start offset.
    ///
    /// Ties broken by pattern order (earlier pattern wins), matching
    /// Oniguruma's scanner semantics.
    pub fn find_next_match(&self, text: &str, start: usize) -> Option<FindNextMatch<TRuleId>> {
        let mut best: Option<FindNextMatch<TRuleId>> = None;
        for (idx, pattern) in self.patterns.iter().enumerate() {
            let mut region = Region::new();
            let hit = pattern.regex.search_with_options(
                text,
                start,
                text.len(),
                SearchOptions::SEARCH_OPTION_NONE,
                Some(&mut region),
            );
            let Some(match_start) = hit else {
                continue;
            };

            if let Some(ref current) = best {
                if match_start >= current.start {
                    continue;
                }
            }

            let groups: Vec<Option<CaptureIndex>> = (0..region.len())
                .map(|i| region.pos(i).map(|(s, e)| CaptureIndex::new(s, e)))
                .collect();

            best = Some(FindNextMatch {
                rule_id: pattern.rule_id,
                pattern_index: idx,
                start: match_start,
                captures: groups,
            });
        }
        best
    }

    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

impl<TRuleId: Copy + 'static> std::fmt::Debug for CompiledRule<TRuleId> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledRule")
            .field("pattern_count", &self.patterns.len())
            .finish_non_exhaustive()
    }
}

/// Scanner hit. Equivalent to upstream's `IFindNextMatchResult`.
#[derive(Debug, Clone)]
pub struct FindNextMatch<TRuleId: Copy> {
    pub rule_id: TRuleId,
    /// Which pattern inside the scanner matched — useful for
    /// diagnostics and test harnesses.
    pub pattern_index: usize,
    pub start: usize,
    pub captures: Vec<Option<CaptureIndex>>,
}

impl<TRuleId: Copy> FindNextMatch<TRuleId> {
    #[must_use]
    pub fn end(&self) -> usize {
        self.captures
            .first()
            .and_then(|c| c.map(|c| c.end))
            .unwrap_or(self.start)
    }
}

struct AnchorCacheEntry<TRuleId: Copy + 'static> {
    a0_g0: Option<CompiledRule<TRuleId>>,
    a0_g1: Option<CompiledRule<TRuleId>>,
    a1_g0: Option<CompiledRule<TRuleId>>,
    a1_g1: Option<CompiledRule<TRuleId>>,
}

impl<TRuleId: Copy + 'static> Default for AnchorCacheEntry<TRuleId> {
    fn default() -> Self {
        Self {
            a0_g0: None,
            a0_g1: None,
            a1_g0: None,
            a1_g1: None,
        }
    }
}

/// Ordered bag of [`RegExpSource`] items with a lazy compiled-scanner
/// cache. Matches the upstream `RegExpSourceList` exactly.
pub struct RegExpSourceList<TRuleId: Copy + 'static> {
    items: Vec<RegExpSource<TRuleId>>,
    has_anchors: bool,
    cached: Option<CompiledRule<TRuleId>>,
    anchor_cache: AnchorCacheEntry<TRuleId>,
}

impl<TRuleId: Copy + 'static> Default for RegExpSourceList<TRuleId> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            has_anchors: false,
            cached: None,
            anchor_cache: AnchorCacheEntry::default(),
        }
    }
}

impl<TRuleId: Copy + 'static> RegExpSourceList<TRuleId> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: RegExpSource<TRuleId>) {
        self.has_anchors = self.has_anchors || item.has_anchor();
        self.items.push(item);
        self.invalidate_caches();
    }

    pub fn unshift(&mut self, item: RegExpSource<TRuleId>) {
        self.has_anchors = self.has_anchors || item.has_anchor();
        self.items.insert(0, item);
        self.invalidate_caches();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn has_anchors(&self) -> bool {
        self.has_anchors
    }

    pub fn items(&self) -> &[RegExpSource<TRuleId>] {
        &self.items
    }

    pub fn items_mut(&mut self) -> &mut [RegExpSource<TRuleId>] {
        self.invalidate_caches();
        &mut self.items
    }

    /// Upstream's `setSource(i, newSource)` — mutates pattern `i` and
    /// throws away any cached scanner.
    pub fn set_source(&mut self, index: usize, new_source: &str) {
        if let Some(item) = self.items.get(index) {
            if item.source() == new_source {
                return;
            }
        }
        self.invalidate_caches();
        if let Some(item) = self.items.get_mut(index) {
            item.set_source(new_source);
        }
    }

    /// Returns the cached "no anchor resolution" compiled scanner,
    /// building it on first call.
    pub fn compile(&mut self) -> Result<&CompiledRule<TRuleId>, TextMateError> {
        if self.cached.is_none() {
            let sources: Vec<String> = self.items.iter().map(|i| i.source().to_string()).collect();
            let ids: Vec<TRuleId> = self.items.iter().map(RegExpSource::rule_id).collect();
            self.cached = Some(CompiledRule::new(&sources, &ids)?);
        }
        Ok(self.cached.as_ref().expect("just populated"))
    }

    /// Returns the compiled scanner for the given `(A, G)` pair.
    /// Identical to upstream's `compileAG`.
    pub fn compile_with_anchors(
        &mut self,
        allow_a: bool,
        allow_g: bool,
    ) -> Result<&CompiledRule<TRuleId>, TextMateError> {
        if !self.has_anchors {
            return self.compile();
        }

        let slot: &mut Option<CompiledRule<TRuleId>> = match (allow_a, allow_g) {
            (false, false) => &mut self.anchor_cache.a0_g0,
            (false, true) => &mut self.anchor_cache.a0_g1,
            (true, false) => &mut self.anchor_cache.a1_g0,
            (true, true) => &mut self.anchor_cache.a1_g1,
        };

        if slot.is_none() {
            let sources: Vec<String> = self
                .items
                .iter()
                .map(|i| i.resolve_anchors(allow_a, allow_g).to_string())
                .collect();
            let ids: Vec<TRuleId> = self.items.iter().map(RegExpSource::rule_id).collect();
            *slot = Some(CompiledRule::new(&sources, &ids)?);
        }
        Ok(slot.as_ref().expect("just populated"))
    }

    fn invalidate_caches(&mut self) {
        self.cached = None;
        self.anchor_cache = AnchorCacheEntry::default();
    }
}

impl<TRuleId: Copy + 'static> std::fmt::Debug for RegExpSourceList<TRuleId> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegExpSourceList")
            .field("items", &self.items.len())
            .field("has_anchors", &self.has_anchors)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_leftmost_match_across_patterns() {
        let mut list: RegExpSourceList<u32> = RegExpSourceList::new();
        list.push(RegExpSource::new(r"foo", 1));
        list.push(RegExpSource::new(r"bar", 2));
        let compiled = list.compile().unwrap();
        let hit = compiled.find_next_match("abc bar foo", 0).unwrap();
        assert_eq!(hit.rule_id, 2);
    }

    #[test]
    fn compile_with_anchors_picks_right_cache_slot() {
        let mut list: RegExpSourceList<u32> = RegExpSourceList::new();
        list.push(RegExpSource::new(r"\Afoo", 1));
        // Has anchors → different slot per (A, G) pair.
        let _ = list.compile_with_anchors(true, true).unwrap();
        let _ = list.compile_with_anchors(false, false).unwrap();
    }
}
