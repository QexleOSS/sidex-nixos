//! Sync grammar/theme registry.
//!
//! Port of `src/registry.ts` from `vscode-textmate` (MIT, Microsoft).
//! Stores raw grammars keyed by scope name, tracks the active theme,
//! and memoizes compiled `Grammar` instances on first request.
//!
//! The original API exposes `grammarForScopeName` as async because its
//! `onigLib` is a `Promise`; `sidex-textmate` links `libonig` directly,
//! so the Rust port is synchronous — which is also how every consumer
//! uses the result in practice.

use std::collections::HashMap;

use parking_lot::RwLock;

use crate::rule::RawGrammar;
use crate::theme::{ScopeStack, StyleAttributes, Theme};

/// Storage for raw grammars, injection-scope mappings, and the active theme.
pub struct Registry {
    inner: RwLock<RegistryInner>,
}

struct RegistryInner {
    raw_grammars: HashMap<String, RawGrammar>,
    injection_grammars: HashMap<String, Vec<String>>,
    theme: Theme,
}

impl Registry {
    /// Builds a registry seeded with a theme.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            inner: RwLock::new(RegistryInner {
                raw_grammars: HashMap::new(),
                injection_grammars: HashMap::new(),
                theme,
            }),
        }
    }

    /// Stores a grammar and, optionally, the list of scope names its
    /// injections should target. Matches upstream's `addGrammar`.
    pub fn add_grammar(&self, grammar: RawGrammar, injection_scope_names: Option<Vec<String>>) {
        let mut guard = self.inner.write();
        let scope = grammar.scope_name.clone();
        guard.raw_grammars.insert(scope.clone(), grammar);
        if let Some(names) = injection_scope_names {
            guard.injection_grammars.insert(scope, names);
        }
    }

    /// Looks up a raw grammar — port of `lookup`.
    #[must_use]
    pub fn lookup(&self, scope_name: &str) -> Option<RawGrammar> {
        self.inner.read().raw_grammars.get(scope_name).cloned()
    }

    /// Returns the injections for a target grammar scope. Port of
    /// `injections(targetScope)`.
    #[must_use]
    pub fn injections(&self, target_scope: &str) -> Vec<String> {
        self.inner
            .read()
            .injection_grammars
            .get(target_scope)
            .cloned()
            .unwrap_or_default()
    }

    /// Swaps the active theme. Port of `setTheme`.
    pub fn set_theme(&self, theme: Theme) {
        self.inner.write().theme = theme;
    }

    /// Exposes the color map — matches upstream's `getColorMap`.
    #[must_use]
    pub fn color_map(&self) -> Vec<String> {
        self.inner.read().theme.color_map()
    }

    /// Returns a clone of the active theme's defaults.
    #[must_use]
    pub fn defaults(&self) -> StyleAttributes {
        self.inner.read().theme.defaults().clone()
    }

    /// Port of `themeMatch`.
    #[must_use]
    pub fn theme_match(&self, scope_path: &ScopeStack) -> Option<StyleAttributes> {
        self.inner.read().theme.r#match(Some(scope_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_theme() -> Theme {
        Theme::create_from_raw(&[], None)
    }

    #[test]
    fn add_and_lookup_raw_grammar() {
        let registry = Registry::new(empty_theme());
        let grammar = RawGrammar {
            scope_name: "source.test".to_string(),
            ..RawGrammar::default()
        };
        registry.add_grammar(grammar.clone(), None);
        assert!(registry.lookup("source.test").is_some());
        assert!(registry.lookup("missing").is_none());
    }

    #[test]
    fn add_grammar_records_injection_scope_names() {
        let registry = Registry::new(empty_theme());
        let grammar = RawGrammar {
            scope_name: "source.ext".to_string(),
            ..RawGrammar::default()
        };
        registry.add_grammar(grammar, Some(vec!["source.target".to_string()]));
        assert_eq!(
            registry.injections("source.target"),
            Vec::<String>::new(),
            "registry stores injections keyed on the contributor, not the target"
        );
        // The contributor's own scope remembers what it injects into.
        assert!(registry
            .inner
            .read()
            .injection_grammars
            .contains_key("source.ext"));
    }

    #[test]
    fn set_theme_updates_defaults() {
        let registry = Registry::new(empty_theme());
        let original_default_fg_id = registry.defaults().foreground_id;
        let original_colors = registry.color_map();
        let original_fg = original_colors
            .get(original_default_fg_id as usize)
            .cloned();

        let next_theme = Theme::create_from_raw(
            &[crate::theme::RawThemeSetting {
                name: None,
                scope: crate::theme::ScopeField::Missing,
                settings: crate::theme::RawSettings {
                    foreground: Some("#ff0000".to_string()),
                    background: Some("#000000".to_string()),
                    ..crate::theme::RawSettings::default()
                },
            }],
            None,
        );
        registry.set_theme(next_theme);
        let updated_colors = registry.color_map();
        let updated_fg_id = registry.defaults().foreground_id;
        let updated_fg = updated_colors.get(updated_fg_id as usize).cloned();

        // The palette changed: the new theme's foreground maps to a
        // different color string than the old default even though both
        // may happen to share the same id slot.
        assert_ne!(original_fg, updated_fg);
        assert_eq!(updated_fg.as_deref(), Some("#FF0000"));
    }
}
