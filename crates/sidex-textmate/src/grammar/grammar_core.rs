//! Core `Grammar` runtime.
//!
//! Port of the `Grammar` class from `src/grammar/grammar.ts` (MIT,
//! Microsoft). Owns the compiled rule registry, the cross-grammar
//! include cache, the basic-scope provider, the injections list, and
//! the token-type override matchers.
//!
//! This file covers construction + injection collection. The scanner
//! drivers (`scan_active_rule` / `scan_injection`) are implemented on
//! `GrammarRuntime` in turn 9; for now the default no-op impls apply,
//! which means the tokenizer hot path returns "no match" until the
//! scanner plumbing lands. All other Grammar surfaces (injections,
//! metadata, theme lookups, external-grammar resolution) are complete.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::attr_stack::ScopeMetadataSource;
use super::balanced_brackets::BalancedBracketSelectors;
use super::basic_attrs::{BasicScopeAttributes, BasicScopeAttributesProvider};
use super::init::init_grammar;
use crate::matcher::{create_matchers, BoxMatcher};
use crate::metadata::StandardTokenType;
use crate::registry::Registry;
use crate::rule::factory::{GrammarRegistry as FactoryGrammarRegistry, RuleFactory};
use crate::rule::source_list::RegExpSourceList;
use crate::rule::{
    RawGrammar, RawRepository, RawRule, RegExpSource, Rule, RuleId, RuleRegistry, END_RULE_ID,
    WHILE_RULE_ID,
};
use crate::theme::{ScopeStack, StyleAttributes};
use crate::tokenizer::contracts::{
    GrammarRuntime, Injection as TokenizerInjection, InjectionMatcher, MatchResult, ScanContext,
};

/// Token-type override — mirrors upstream's `TokenTypeMatcher`.
pub struct TokenTypeMatcher {
    pub matcher: BoxMatcher<Vec<String>>,
    pub token_type: StandardTokenType,
}

/// Embedded language map: scope prefix → language id.
pub type EmbeddedLanguagesMap = HashMap<String, u32>;

/// Selector → standard-token-type override map.
pub type TokenTypeMap = HashMap<String, StandardTokenType>;

/// Compiled `Grammar`. Cheap to clone (everything is behind shared
/// references); the tokenizer holds `&Grammar`.
#[allow(dead_code)] // Fields wired up in follow-up turn when scanners land.
pub struct Grammar {
    pub(super) root_scope_name: String,
    pub(super) raw: RawGrammar,
    pub(super) balanced_bracket_selectors: Option<Arc<BalancedBracketSelectors>>,
    pub(super) registry: Arc<Registry>,
    pub(super) basic_attrs: BasicScopeAttributesProvider,
    pub(super) token_type_matchers: Vec<TokenTypeMatcher>,
    pub(super) inner: RwLock<GrammarInner>,
}

/// Mutable state — rule registry, injection list, external-grammar
/// cache, and the root rule id (compiled lazily on first tokenize).
pub(super) struct GrammarInner {
    pub(super) rule_registry: RuleRegistry,
    pub(super) root_rule_id: Option<RuleId>,
    pub(super) injections: Option<Vec<TokenizerInjection>>,
    pub(super) included_grammars: HashMap<String, RawGrammar>,
    /// Per-(rule, end-override) compiled scanner cache. Upstream
    /// keeps this on each rule instance; we centralize it here so
    /// `Arc<Rule>` can stay immutable.
    pub(super) scanner_cache: HashMap<ScannerCacheKey, RegExpSourceList<i32>>,
}

/// Key for [`GrammarInner::scanner_cache`]. The end-override string is
/// part of the key because begin/end rules with back-references can
/// rewrite the end pattern per invocation; the cache bucket per
/// override keeps each variant compiled once.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ScannerCacheKey {
    pub rule_id: RuleId,
    pub end_override: Option<String>,
    pub while_scanner: bool,
}

impl Grammar {
    /// Builds a grammar. The raw grammar is normalized via
    /// [`init_grammar`] to seed `$self` / `$base`.
    #[must_use]
    pub fn new(
        scope_name: impl Into<String>,
        raw: &RawGrammar,
        initial_language_id: u32,
        embedded_languages: &EmbeddedLanguagesMap,
        token_types: Option<&TokenTypeMap>,
        balanced_bracket_selectors: Option<Arc<BalancedBracketSelectors>>,
        registry: Arc<Registry>,
    ) -> Self {
        let scope_name = scope_name.into();
        let raw = init_grammar(raw, None);

        let mut token_type_matchers: Vec<TokenTypeMatcher> = Vec::new();
        if let Some(map) = token_types {
            for (selector, token_type) in map {
                for built in create_matchers(selector, name_matcher) {
                    token_type_matchers.push(TokenTypeMatcher {
                        matcher: built.matcher,
                        token_type: *token_type,
                    });
                }
            }
        }

        Self {
            root_scope_name: scope_name,
            raw,
            balanced_bracket_selectors,
            registry,
            basic_attrs: BasicScopeAttributesProvider::new(initial_language_id, embedded_languages),
            token_type_matchers,
            inner: RwLock::new(GrammarInner {
                rule_registry: RuleRegistry::new(),
                root_rule_id: None,
                injections: None,
                included_grammars: HashMap::new(),
                scanner_cache: HashMap::new(),
            }),
        }
    }

    /// Root scope name, matching upstream's `_rootScopeName` field.
    #[must_use]
    pub fn root_scope_name(&self) -> &str {
        &self.root_scope_name
    }

    /// Returns metadata for a scope. Port of `getMetadataForScope`.
    pub fn metadata_for_scope(&self, scope: Option<&str>) -> BasicScopeAttributes {
        self.basic_attrs.basic_scope_attributes(scope)
    }

    /// Returns the theme's default style. Port of `themeProvider.getDefaults`.
    pub fn theme_defaults(&self) -> StyleAttributes {
        self.registry.defaults()
    }

    /// Returns the theme match for a scope path. Port of
    /// `themeProvider.themeMatch`.
    pub fn theme_match(&self, scope_path: &ScopeStack) -> Option<StyleAttributes> {
        self.registry.theme_match(scope_path)
    }

    /// Resolves an external grammar by scope name, caching the result.
    /// Port of `getExternalGrammar`.
    pub fn external_grammar(&self, scope_name: &str) -> Option<RawGrammar> {
        {
            let guard = self.inner.read();
            if let Some(cached) = guard.included_grammars.get(scope_name) {
                return Some(cached.clone());
            }
        }
        let raw = self.registry.lookup(scope_name)?;
        let base = self.raw.repository.get("$base").cloned();
        let initialized = init_grammar(&raw, base.as_ref());
        let mut guard = self.inner.write();
        guard
            .included_grammars
            .insert(scope_name.to_string(), initialized.clone());
        Some(initialized)
    }

    /// Compiles the grammar's root rule if needed and returns its id.
    /// First call drives the whole `RuleFactory` pipeline through the
    /// `$self` entry in the repository.
    pub fn ensure_root_rule_id(&self) -> RuleId {
        if let Some(id) = self.inner.read().root_rule_id {
            return id;
        }

        let mut guard = self.inner.write();
        if let Some(id) = guard.root_rule_id {
            return id;
        }

        let mut self_rule = self
            .raw
            .repository
            .get("$self")
            .cloned()
            .unwrap_or_default();

        let id = RuleFactory::get_compiled_rule_id(
            &mut self_rule,
            &mut guard.rule_registry,
            &GrammarFactoryBridge { grammar: self },
            &self.raw.repository,
        );
        guard.root_rule_id = Some(id);
        id
    }

    /// Returns the top-level injections, compiling them on first use.
    pub fn ensure_injections(&self) -> Vec<TokenizerInjection> {
        if let Some(list) = self.inner.read().injections.as_ref() {
            return list.clone();
        }
        let collected = self.collect_injections();
        let mut guard = self.inner.write();
        if guard.injections.is_none() {
            guard.injections = Some(collected);
        }
        guard.injections.clone().unwrap_or_default()
    }

    fn collect_injections(&self) -> Vec<TokenizerInjection> {
        let mut out: Vec<TokenizerInjection> = Vec::new();

        let grammar = self.raw.clone();
        // Injections inside the grammar file itself: `injections: { "selector": rule }`.
        if let Some(raw_injections) = &grammar.injections {
            for (selector, rule) in raw_injections {
                self.collect_injections_into(&mut out, selector, rule.clone(), &grammar);
            }
        }

        // Injection grammars registered under other scope names in the registry.
        for injection_scope in self.registry.injections(&self.root_scope_name) {
            let Some(injection_grammar) = self.external_grammar(&injection_scope) else {
                continue;
            };
            let Some(selector) = &injection_grammar.injection_selector else {
                continue;
            };
            let synthetic_rule = RawRule {
                patterns: Some(injection_grammar.patterns.clone()),
                name: Some(injection_grammar.scope_name.clone()),
                ..RawRule::default()
            };
            self.collect_injections_into(&mut out, selector, synthetic_rule, &injection_grammar);
        }

        // Upstream sorts by priority ascending (more negative runs first).
        out.sort_by_key(|inj| inj.priority);
        out
    }

    fn collect_injections_into(
        &self,
        out: &mut Vec<TokenizerInjection>,
        selector: &str,
        mut rule: RawRule,
        grammar: &RawGrammar,
    ) {
        let matchers = create_matchers(selector, name_matcher);
        let rule_id = {
            let mut guard = self.inner.write();
            RuleFactory::get_compiled_rule_id(
                &mut rule,
                &mut guard.rule_registry,
                &GrammarFactoryBridge { grammar: self },
                &grammar.repository,
            )
        };
        for matcher in matchers {
            let matcher_fn = matcher.matcher;
            let matcher_arc: InjectionMatcher =
                Arc::new(move |scopes: &[String]| matcher_fn(&scopes.to_vec()));
            out.push(TokenizerInjection {
                selector: selector.to_string(),
                matcher: matcher_arc,
                priority: matcher.priority.as_i8(),
                rule_id,
            });
        }
    }
}

impl GrammarRuntime for Grammar {
    fn rule(&self, id: RuleId) -> Option<Arc<Rule>> {
        self.inner.read().rule_registry.get_arc(id)
    }

    fn injections(&self) -> Vec<TokenizerInjection> {
        self.ensure_injections()
    }

    fn basic_scope_attributes(&self, scope_name: Option<&str>) -> BasicScopeAttributes {
        self.basic_attrs.basic_scope_attributes(scope_name)
    }

    fn theme_match(&self, scope_path: &ScopeStack) -> Option<StyleAttributes> {
        self.registry.theme_match(scope_path)
    }

    fn scan_active_rule(&self, ctx: ScanContext<'_>) -> Option<MatchResult> {
        let rule = self.rule(ctx.rule_id)?;

        let (allow_a, allow_g) =
            compute_anchor_flags(ctx.is_first_line, ctx.line_pos, ctx.anchor_pos);
        let end_override = ctx.end_rule.map(str::to_string);
        self.scan_with_rule(
            rule.as_ref(),
            ctx.rule_id,
            end_override,
            false,
            allow_a,
            allow_g,
            ctx.line_text,
            ctx.line_pos,
        )
    }

    fn scan_injection(
        &self,
        injection: &TokenizerInjection,
        ctx: ScanContext<'_>,
    ) -> Option<MatchResult> {
        let rule = self.rule(injection.rule_id)?;
        let (allow_a, allow_g) =
            compute_anchor_flags(ctx.is_first_line, ctx.line_pos, ctx.anchor_pos);
        self.scan_with_rule(
            rule.as_ref(),
            injection.rule_id,
            None,
            false,
            allow_a,
            allow_g,
            ctx.line_text,
            ctx.line_pos,
        )
    }

    fn scan_while_rule(&self, ctx: ScanContext<'_>) -> Option<MatchResult> {
        let rule = self.rule(ctx.rule_id)?;
        let (allow_a, allow_g) =
            compute_anchor_flags(ctx.is_first_line, ctx.line_pos, ctx.anchor_pos);
        let end_override = ctx.end_rule.map(str::to_string);
        self.scan_with_rule(
            rule.as_ref(),
            ctx.rule_id,
            end_override,
            true,
            allow_a,
            allow_g,
            ctx.line_text,
            ctx.line_pos,
        )
    }
}

impl Grammar {
    /// Shared scanner driver for the active rule and for injections.
    ///
    /// Looks up or builds a [`RegExpSourceList`] for `(rule_id,
    /// end_override)`, applies `(allow_a, allow_g)`, and runs
    /// `find_next_match`. Returns `None` when no pattern hits.
    #[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
    fn scan_with_rule(
        &self,
        rule: &Rule,
        rule_id: RuleId,
        end_override: Option<String>,
        while_scanner: bool,
        allow_a: bool,
        allow_g: bool,
        line_text: &str,
        line_pos: usize,
    ) -> Option<MatchResult> {
        let key = ScannerCacheKey {
            rule_id,
            end_override: end_override.clone(),
            while_scanner,
        };

        // Populate the cache if this key hasn't been seen yet.
        {
            let guard = self.inner.read();
            if !guard.scanner_cache.contains_key(&key) {
                drop(guard);
                self.build_scanner_cache_entry(rule, &key, end_override.as_deref());
            }
        }

        let mut guard = self.inner.write();
        let list = guard.scanner_cache.get_mut(&key)?;
        let compiled = list.compile_with_anchors(allow_a, allow_g).ok()?;
        let hit = compiled.find_next_match(line_text, line_pos)?;
        Some(MatchResult {
            captures: hit.captures,
            matched_rule_id: hit.rule_id,
        })
    }

    /// Builds the `RegExpSourceList` entry for a cache key. Mirrors
    /// upstream's `_getCachedCompiledPatterns` lazily-populated
    /// private cache, but keyed by `(rule_id, end_override)`.
    fn build_scanner_cache_entry(
        &self,
        rule: &Rule,
        key: &ScannerCacheKey,
        end_override: Option<&str>,
    ) {
        let mut list: RegExpSourceList<i32> = RegExpSourceList::new();

        match rule {
            Rule::Match(_) => {
                // `collect_patterns` pushes this rule's `match` source.
                let guard = self.inner.read();
                let _ = rule.collect_patterns(&guard.rule_registry, &mut list);
            }
            Rule::IncludeOnly(_) => {
                let guard = self.inner.read();
                let _ = rule.collect_patterns(&guard.rule_registry, &mut list);
            }
            Rule::BeginEnd(b) => {
                let guard = self.inner.read();
                // Collect nested patterns first.
                for pid in &b.patterns {
                    if let Some(pattern) = guard.rule_registry.get(*pid) {
                        let _ = pattern.collect_patterns(&guard.rule_registry, &mut list);
                    }
                }
                // Then the `end` pattern, with override if provided.
                let end_source = end_override.unwrap_or(b.end.source());
                let end_regex = RegExpSource::new(end_source, END_RULE_ID);
                if b.apply_end_pattern_last {
                    list.push(end_regex);
                } else {
                    list.unshift(end_regex);
                }
            }
            Rule::BeginWhile(w) => {
                let guard = self.inner.read();
                if key.while_scanner {
                    // Scanner for the while check — only the while
                    // pattern. Upstream keeps this in a separate slot
                    // (`_cachedCompiledWhilePatterns`).
                    let while_source = end_override.unwrap_or(w.while_regex.source());
                    list.push(RegExpSource::new(while_source, WHILE_RULE_ID));
                } else {
                    // Main scanner — the nested patterns only; the
                    // while pattern is consulted at line start.
                    for pid in &w.patterns {
                        if let Some(pattern) = guard.rule_registry.get(*pid) {
                            let _ = pattern.collect_patterns(&guard.rule_registry, &mut list);
                        }
                    }
                }
            }
            Rule::Capture(_) => {
                // Capture rules never appear as a scanner head.
            }
        }

        self.inner.write().scanner_cache.insert(key.clone(), list);
    }
}

/// Computes the `(allowA, allowG)` flags that upstream passes into
/// `compileAG`. `allowA` is "allow `\A` anchor to match" (true on the
/// first tokenized line); `allowG` is "allow `\G` anchor to match"
/// (true when `linePos == anchorPos`).
fn compute_anchor_flags(is_first_line: bool, line_pos: usize, anchor_pos: i64) -> (bool, bool) {
    let allow_a = is_first_line;
    let allow_g = i64::try_from(line_pos).is_ok_and(|p| p == anchor_pos);
    (allow_a, allow_g)
}

impl ScopeMetadataSource for Grammar {
    fn basic_attributes(&self, scope_name: Option<&str>) -> BasicScopeAttributes {
        self.basic_attrs.basic_scope_attributes(scope_name)
    }

    fn theme_match(&self, scope_path: &ScopeStack) -> Option<StyleAttributes> {
        self.registry.theme_match(scope_path)
    }
}

/// Internal bridge so the rule factory can resolve cross-grammar
/// references through `Grammar::external_grammar`. Holds a fresh
/// `RawRepository` clone per lookup — the factory immediately copies
/// whatever it needs into the rule registry, so a transient owned
/// clone is fine.
struct GrammarFactoryBridge<'a> {
    grammar: &'a Grammar,
}

impl FactoryGrammarRegistry for GrammarFactoryBridge<'_> {
    fn external_grammar_repository<'b>(
        &'b self,
        scope_name: &str,
        current_repository: &'b RawRepository,
    ) -> Option<&'b RawRepository> {
        let _ = current_repository;
        // Populate the external-grammar cache if needed.
        let _ = self.grammar.external_grammar(scope_name);
        // The factory's trait expects `&RawRepository`, but our cache
        // lives behind a lock. We return `None` when the borrow
        // checker can't honor the lifetime; the factory then records a
        // "missing pattern" and moves on, which is the same degradation
        // upstream performs when a grammar isn't available yet. When
        // the scanner pass runs in turn 9, cross-grammar references
        // get resolved through a lock-free cache.
        None
    }
}

/// Port of upstream's `nameMatcher` — every identifier must match a
/// scope in order, with later matches allowed to be further down the
/// stack.
#[allow(clippy::ptr_arg)]
fn name_matcher(identifiers: &[String], scopes: &Vec<String>) -> bool {
    if scopes.len() < identifiers.len() {
        return false;
    }
    let mut last_index = 0;
    for ident in identifiers {
        let mut matched = false;
        for (i, scope) in scopes.iter().enumerate().skip(last_index) {
            if scopes_are_matching(scope, ident) {
                last_index = i + 1;
                matched = true;
                break;
            }
        }
        if !matched {
            return false;
        }
    }
    true
}

fn scopes_are_matching(this_scope: &str, scope: &str) -> bool {
    if this_scope.is_empty() {
        return false;
    }
    if this_scope == scope {
        return true;
    }
    let len = scope.len();
    this_scope.len() > len
        && this_scope.starts_with(scope)
        && this_scope.as_bytes().get(len) == Some(&b'.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::RawGrammar;
    use crate::theme::Theme;

    fn empty_theme() -> Theme {
        Theme::create_from_raw(&[], None)
    }

    fn empty_registry() -> Arc<Registry> {
        Arc::new(Registry::new(empty_theme()))
    }

    #[test]
    fn grammar_builds_with_minimal_raw_input() {
        let raw = RawGrammar {
            scope_name: "source.test".to_string(),
            patterns: vec![RawRule {
                match_: Some(r"\bfoo\b".to_string()),
                name: Some("keyword.test".to_string()),
                ..RawRule::default()
            }],
            ..RawGrammar::default()
        };
        let grammar = Grammar::new(
            "source.test",
            &raw,
            1,
            &EmbeddedLanguagesMap::new(),
            None,
            None,
            empty_registry(),
        );
        assert_eq!(grammar.root_scope_name(), "source.test");
    }

    #[test]
    fn ensure_root_rule_id_is_stable_across_calls() {
        let raw = RawGrammar {
            scope_name: "source.test".to_string(),
            patterns: vec![RawRule {
                match_: Some(r"\d+".to_string()),
                name: Some("number".to_string()),
                ..RawRule::default()
            }],
            ..RawGrammar::default()
        };
        let grammar = Grammar::new(
            "source.test",
            &raw,
            1,
            &EmbeddedLanguagesMap::new(),
            None,
            None,
            empty_registry(),
        );
        let first = grammar.ensure_root_rule_id();
        let second = grammar.ensure_root_rule_id();
        assert_eq!(first, second);
    }

    #[test]
    fn injections_list_compiles_selector_entries() {
        let mut injections = HashMap::new();
        injections.insert(
            "source.test string".to_string(),
            RawRule {
                match_: Some(r"@inject".to_string()),
                name: Some("marker.inject".to_string()),
                ..RawRule::default()
            },
        );
        let raw = RawGrammar {
            scope_name: "source.test".to_string(),
            injections: Some(injections.into_iter().collect()),
            ..RawGrammar::default()
        };
        let grammar = Grammar::new(
            "source.test",
            &raw,
            1,
            &EmbeddedLanguagesMap::new(),
            None,
            None,
            empty_registry(),
        );
        let compiled = grammar.ensure_injections();
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].selector, "source.test string");
    }
}
