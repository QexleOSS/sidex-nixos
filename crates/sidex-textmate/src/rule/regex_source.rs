//! Anchor-aware regex source.
//!
//! Port of the `RegExpSource` class from upstream. Holds the original
//! grammar pattern and produces variants where `\A` / `\G` anchors are
//! selectively turned into unmatchable sentinels (`\uFFFF`) so a
//! single cached `OnigScanner` can answer queries under all four
//! `(allowA, allowG)` combinations.
//!
//! `\z` is rewritten to `$(?!\n)(?<!\n)` to match upstream's emulation
//! of Oniguruma's end-of-input anchor with a JavaScript-friendly form.

use crate::utils::{self, escape_regex_chars, LazyRegex};

/// A pre-computed set of four source strings, one per `(A, G)` combination.
#[derive(Debug, Clone)]
struct AnchorCache {
    a0_g0: String,
    a0_g1: String,
    a1_g0: String,
    a1_g1: String,
}

/// A compiled grammar regex fragment.
///
/// Generic over the rule-id value: most call sites use [`super::RuleId`]
/// but begin/end and begin/while rules use signed sentinels (`-1`, `-2`)
/// that upstream tracks in the same slot.
#[derive(Debug, Clone)]
pub struct RegExpSource<TRuleId: Copy> {
    source: String,
    rule_id: TRuleId,
    has_anchor: bool,
    has_back_references: bool,
    anchor_cache: Option<AnchorCache>,
}

impl<TRuleId: Copy> RegExpSource<TRuleId> {
    /// Constructor — applies upstream's `\z` rewrite and detects
    /// `\A` / `\G` anchors for later anchor-cache construction.
    pub fn new(regex_source: &str, rule_id: TRuleId) -> Self {
        let mut source = String::with_capacity(regex_source.len());
        let mut has_anchor = false;

        if regex_source.is_empty() {
            return Self {
                source: String::new(),
                rule_id,
                has_anchor: false,
                has_back_references: false,
                anchor_cache: None,
            };
        }

        let bytes = regex_source.as_bytes();
        let len = bytes.len();
        let mut last_pushed = 0usize;
        let mut rewrote_z = false;
        let mut pos = 0usize;
        while pos < len {
            let ch = bytes[pos];
            if ch == b'\\' && pos + 1 < len {
                let next = bytes[pos + 1];
                if next == b'z' {
                    source.push_str(&regex_source[last_pushed..pos]);
                    source.push_str("$(?!\\n)(?<!\\n)");
                    last_pushed = pos + 2;
                    rewrote_z = true;
                } else if next == b'A' || next == b'G' {
                    has_anchor = true;
                }
                pos += 2;
            } else {
                pos += 1;
            }
        }

        if rewrote_z {
            source.push_str(&regex_source[last_pushed..len]);
        } else {
            source.clear();
            source.push_str(regex_source);
        }

        let anchor_cache = if has_anchor {
            Some(build_anchor_cache(&source))
        } else {
            None
        };

        let has_back_references = has_back_reference(&source);

        Self {
            source,
            rule_id,
            has_anchor,
            has_back_references,
            anchor_cache,
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn rule_id(&self) -> TRuleId {
        self.rule_id
    }

    #[must_use]
    pub fn has_anchor(&self) -> bool {
        self.has_anchor
    }

    #[must_use]
    pub fn has_back_references(&self) -> bool {
        self.has_back_references
    }

    #[must_use]
    pub fn clone_inner(&self) -> Self {
        Self::new(&self.source, self.rule_id)
    }

    /// Overwrites the stored pattern. Invalidates the anchor cache when
    /// the source actually changed — matches `setSource` upstream.
    pub fn set_source(&mut self, new_source: &str) {
        if self.source == new_source {
            return;
        }
        self.source = new_source.to_string();
        if self.has_anchor {
            self.anchor_cache = Some(build_anchor_cache(&self.source));
        }
    }

    /// Substitutes `\N` back-references against captured spans from the
    /// triggering match. Uses upstream's escaping rules verbatim.
    pub fn resolve_back_references(
        &self,
        line_text: &str,
        captures: &[Option<utils::CaptureIndex>],
    ) -> String {
        static BACK_REF: LazyRegex = LazyRegex::new(r"\\(\d+)");
        let regex = BACK_REF.get();

        let captured_values: Vec<String> = captures
            .iter()
            .map(|c| match c {
                Some(c) => line_text.get(c.start..c.end).unwrap_or("").to_string(),
                None => String::new(),
            })
            .collect();

        let mut out = String::with_capacity(self.source.len());
        let mut last_end = 0usize;
        for caps in regex.captures_iter(&self.source) {
            let whole = caps.get(0).expect("group 0 present");
            out.push_str(&self.source[last_end..whole.start()]);
            let index = caps
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(usize::MAX);
            let value = captured_values.get(index).cloned().unwrap_or_default();
            out.push_str(&escape_regex_chars(&value));
            last_end = whole.end();
        }
        out.push_str(&self.source[last_end..]);
        out
    }

    /// Returns the anchor-resolved pattern for the given `(A, G)` pair.
    /// When the pattern has no anchors this is a no-op returning
    /// `self.source`.
    #[must_use]
    pub fn resolve_anchors(&self, allow_a: bool, allow_g: bool) -> &str {
        let Some(cache) = &self.anchor_cache else {
            return &self.source;
        };
        match (allow_a, allow_g) {
            (true, true) => &cache.a1_g1,
            (true, false) => &cache.a1_g0,
            (false, true) => &cache.a0_g1,
            (false, false) => &cache.a0_g0,
        }
    }
}

fn has_back_reference(source: &str) -> bool {
    static BACK_REF: LazyRegex = LazyRegex::new(r"\\(\d+)");
    BACK_REF.get().is_match(source)
}

/// Builds the four anchor-substitution variants upstream caches.
///
/// * `A0` → `\A` becomes unmatchable (`\uFFFF`)
/// * `A1` → `\A` is kept literal
/// * `G0` → `\G` becomes unmatchable
/// * `G1` → `\G` is kept literal
fn build_anchor_cache(source: &str) -> AnchorCache {
    let mut a0_g0 = String::with_capacity(source.len());
    let mut a0_g1 = String::with_capacity(source.len());
    let mut a1_g0 = String::with_capacity(source.len());
    let mut a1_g1 = String::with_capacity(source.len());

    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut pos = 0usize;
    while pos < len {
        let ch = chars[pos];
        a0_g0.push(ch);
        a0_g1.push(ch);
        a1_g0.push(ch);
        a1_g1.push(ch);

        if ch == '\\' && pos + 1 < len {
            let next = chars[pos + 1];
            match next {
                'A' => {
                    a0_g0.push('\u{FFFF}');
                    a0_g1.push('\u{FFFF}');
                    a1_g0.push('A');
                    a1_g1.push('A');
                }
                'G' => {
                    a0_g0.push('\u{FFFF}');
                    a0_g1.push('G');
                    a1_g0.push('\u{FFFF}');
                    a1_g1.push('G');
                }
                other => {
                    a0_g0.push(other);
                    a0_g1.push(other);
                    a1_g0.push(other);
                    a1_g1.push(other);
                }
            }
            pos += 2;
        } else {
            pos += 1;
        }
    }

    AnchorCache {
        a0_g0,
        a0_g1,
        a1_g0,
        a1_g1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_anchor_flags() {
        let src = RegExpSource::new(r"\Afoo\Gbar", 0u32);
        assert!(src.has_anchor());
    }

    #[test]
    fn rewrites_backslash_z_to_portable_end_anchor() {
        let src = RegExpSource::new(r"foo\z", 0u32);
        assert_eq!(src.source(), r"foo$(?!\n)(?<!\n)");
    }

    #[test]
    fn anchor_cache_zeroes_and_keeps_variants() {
        let src = RegExpSource::new(r"\Afoo\Gbar", 0u32);
        // A1_G1 = keep both literal
        assert!(src.resolve_anchors(true, true).contains(r"\A"));
        assert!(src.resolve_anchors(true, true).contains(r"\G"));
        // A0_G0 = both blocked with \uFFFF
        let a0_g0 = src.resolve_anchors(false, false);
        assert!(a0_g0.contains('\u{FFFF}'));
        assert!(!a0_g0.contains(r"\A"));
    }

    #[test]
    fn back_reference_substitution_uses_captured_text() {
        let src = RegExpSource::new(r"foo\1bar", 0u32);
        let captures = vec![None, Some(utils::CaptureIndex::new(0, 3))];
        let resolved = src.resolve_back_references("abc???", &captures);
        assert_eq!(resolved, "fooabcbar");
    }
}
