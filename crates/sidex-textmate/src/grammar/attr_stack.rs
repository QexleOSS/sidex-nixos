//! Port of `AttributedScopeStack` from `grammar.ts` (MIT, Microsoft).
//!
//! An [`AttributedScopeStack`] is the "theme-aware" counterpart to
//! [`crate::theme::ScopeStack`]: at every frame it carries the packed
//! encoded-token metadata and optional font attributes resolved by the
//! theme, so emitting a token becomes a simple read off the top of the
//! stack instead of a theme lookup on the hot path.
//!
//! The tokenizer pushes attributed frames on every scope it encounters;
//! popping happens implicitly by holding an `Arc` to an earlier node.

use std::sync::Arc;

use crate::metadata::{self, EncodedTokenAttributes, FontStyle};
use crate::theme::{ScopeStack, StyleAttributes};

/// Trait providing the theme + basic-scope lookups [`AttributedScopeStack`]
/// needs when it builds child frames.
///
/// The full `Grammar` struct will implement this in the next turn; the
/// trait lets us port the data structure independently.
pub trait ScopeMetadataSource {
    /// Basic-scope metadata (language id + token type) for a scope
    /// segment. `None` signals "no scope".
    fn basic_attributes(
        &self,
        scope_name: Option<&str>,
    ) -> crate::grammar::basic_attrs::BasicScopeAttributes;

    /// Runs the theme matcher against a scope path, returning an
    /// optional style.
    fn theme_match(&self, scope_path: &ScopeStack) -> Option<StyleAttributes>;
}

/// One frame in the attributed scope stack.
///
/// Matches upstream's fields: `parent`, `scopePath`, `tokenAttributes`,
/// `fontAttributes` (kept `None` until the font-attribute plumbing
/// lands in turn 6 — the upstream implementation allows `null` too),
/// `styleAttributes`.
#[derive(Debug, Clone)]
pub struct AttributedScopeStack {
    parent: Option<Arc<AttributedScopeStack>>,
    scope_path: Arc<ScopeStack>,
    token_attributes: EncodedTokenAttributes,
    style_attributes: Option<StyleAttributes>,
}

impl AttributedScopeStack {
    /// Low-level constructor — prefer [`Self::create_root`] or
    /// [`Self::create_root_with_lookup`] for initial roots.
    pub(crate) fn new(
        parent: Option<Arc<AttributedScopeStack>>,
        scope_path: Arc<ScopeStack>,
        token_attributes: EncodedTokenAttributes,
        style_attributes: Option<StyleAttributes>,
    ) -> Self {
        Self {
            parent,
            scope_path,
            token_attributes,
            style_attributes,
        }
    }

    /// Root frame without theme lookup. Matches `createRoot`.
    pub fn create_root(scope_name: &str, token_attributes: EncodedTokenAttributes) -> Self {
        Self::new(
            None,
            Arc::new(ScopeStack::new(None, scope_name)),
            token_attributes,
            None,
        )
    }

    /// Root frame that runs theme + basic-scope lookups against the
    /// supplied source. Port of `createRootAndLookUpScopeName`.
    pub fn create_root_with_lookup<S: ScopeMetadataSource>(
        scope_name: &str,
        token_attributes: EncodedTokenAttributes,
        source: &S,
    ) -> Self {
        let basic = source.basic_attributes(Some(scope_name));
        let path = Arc::new(ScopeStack::new(None, scope_name));
        let style = source.theme_match(&path);
        let merged = merge_attributes(token_attributes, basic, style.as_ref());
        Self::new(None, path, merged, style)
    }

    #[must_use]
    pub fn token_attributes(&self) -> EncodedTokenAttributes {
        self.token_attributes
    }

    #[must_use]
    pub fn style_attributes(&self) -> Option<&StyleAttributes> {
        self.style_attributes.as_ref()
    }

    #[must_use]
    pub fn scope_path(&self) -> &ScopeStack {
        &self.scope_path
    }

    #[must_use]
    pub fn scope_names(&self) -> Vec<String> {
        self.scope_path.segments()
    }

    #[must_use]
    pub fn scope_name(&self) -> &str {
        self.scope_path.scope_name()
    }

    #[must_use]
    pub fn parent(&self) -> Option<&AttributedScopeStack> {
        self.parent.as_deref()
    }

    /// Push a new scope-name segment (optionally space-separated) and
    /// return the new attributed frame. Port of `pushAttributed`.
    #[must_use]
    pub fn push_attributed<S: ScopeMetadataSource>(
        self: &Arc<Self>,
        scope_path: Option<&str>,
        source: &S,
    ) -> Arc<Self> {
        let Some(scope_path) = scope_path else {
            return Arc::clone(self);
        };

        if !scope_path.contains(' ') {
            // Common, fast path.
            return Arc::new(Self::push_one(self, scope_path, source));
        }

        let mut current: Arc<Self> = Arc::clone(self);
        for scope in scope_path.split(' ') {
            if scope.is_empty() {
                continue;
            }
            current = Arc::new(Self::push_one(&current, scope, source));
        }
        current
    }

    fn push_one<S: ScopeMetadataSource>(target: &Arc<Self>, scope_name: &str, source: &S) -> Self {
        let basic = source.basic_attributes(Some(scope_name));
        let new_path = target.scope_path.push_segment(scope_name);
        let style = source.theme_match(&new_path);
        let metadata = merge_attributes(target.token_attributes, basic, style.as_ref());
        Self::new(Some(Arc::clone(target)), new_path, metadata, style)
    }

    /// Structural equality matching upstream's `AttributedScopeStack.equals`.
    pub fn equals(a: Option<&AttributedScopeStack>, b: Option<&AttributedScopeStack>) -> bool {
        let mut a_ptr = a;
        let mut b_ptr = b;
        loop {
            match (a_ptr, b_ptr) {
                (None, None) => return true,
                (Some(x), Some(y)) if std::ptr::eq(x, y) => return true,
                (Some(_), None) | (None, Some(_)) => return false,
                (Some(x), Some(y)) => {
                    if x.scope_path.scope_name() != y.scope_path.scope_name()
                        || x.token_attributes != y.token_attributes
                    {
                        return false;
                    }
                    a_ptr = x.parent.as_deref();
                    b_ptr = y.parent.as_deref();
                }
            }
        }
    }

    /// Collects frames above `base`, outer→inner. Returns `None` when
    /// `self` does not extend `base`. Port of `getExtensionIfDefined`.
    #[must_use]
    pub fn extension_if_defined(
        &self,
        base: Option<&AttributedScopeStack>,
    ) -> Option<Vec<AttributedScopeStackFrame>> {
        let mut out: Vec<AttributedScopeStackFrame> = Vec::new();
        let mut current: Option<&AttributedScopeStack> = Some(self);
        loop {
            match (current, base) {
                (Some(c), Some(b)) if std::ptr::eq(c, b) => break,
                (None, None) => break,
                (None, Some(_)) => return None,
                (Some(c), _) => {
                    let parent_path = c.parent.as_deref().map(|p| p.scope_path.as_ref());
                    let extension = c.scope_path.extension_if_defined(parent_path)?;
                    out.push(AttributedScopeStackFrame {
                        encoded_token_attributes: c.token_attributes,
                        scope_names: extension,
                    });
                    current = c.parent.as_deref();
                }
            }
        }
        out.reverse();
        Some(out)
    }

    /// Inverse of [`Self::extension_if_defined`] — rehydrates an
    /// attributed stack from frames. Port of `fromExtension`.
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_extension(
        names_scope_list: Option<Arc<AttributedScopeStack>>,
        frames: &[AttributedScopeStackFrame],
    ) -> Option<Arc<AttributedScopeStack>> {
        let mut current: Option<Arc<AttributedScopeStack>> = names_scope_list.clone();
        let mut scope_path: Option<Arc<ScopeStack>> =
            names_scope_list.as_ref().map(|s| Arc::clone(&s.scope_path));
        for frame in frames {
            for segment in &frame.scope_names {
                scope_path = Some(match scope_path.as_ref() {
                    Some(p) => p.push_segment(segment.as_str()),
                    None => Arc::new(ScopeStack::new(None, segment)),
                });
            }
            let path = scope_path.clone()?;
            let style = current.as_ref().and_then(|c| c.style_attributes.clone());
            current = Some(Arc::new(AttributedScopeStack::new(
                current.clone(),
                path,
                frame.encoded_token_attributes,
                style,
            )));
        }
        current
    }
}

/// Serializable frame used by [`AttributedScopeStack::extension_if_defined`].
#[derive(Debug, Clone)]
pub struct AttributedScopeStackFrame {
    pub encoded_token_attributes: EncodedTokenAttributes,
    pub scope_names: Vec<String>,
}

fn merge_attributes(
    existing: EncodedTokenAttributes,
    basic: crate::grammar::basic_attrs::BasicScopeAttributes,
    style: Option<&StyleAttributes>,
) -> EncodedTokenAttributes {
    let (font_style, foreground, background) = match style {
        Some(s) => (s.font_style, s.foreground_id, s.background_id),
        None => (FontStyle::NOT_SET, 0, 0),
    };
    metadata::set(
        existing,
        basic.language_id,
        basic.token_type,
        None,
        font_style,
        foreground,
        background,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::StandardTokenType;

    struct StubSource;

    impl ScopeMetadataSource for StubSource {
        fn basic_attributes(
            &self,
            _scope_name: Option<&str>,
        ) -> crate::grammar::basic_attrs::BasicScopeAttributes {
            crate::grammar::basic_attrs::BasicScopeAttributes {
                language_id: 0,
                token_type: crate::metadata::OptionalStandardTokenType::NotSet,
            }
        }

        fn theme_match(&self, _scope_path: &ScopeStack) -> Option<StyleAttributes> {
            None
        }
    }

    #[test]
    fn create_root_starts_with_single_scope() {
        let root = AttributedScopeStack::create_root("source.ts", 0);
        assert_eq!(root.scope_name(), "source.ts");
        assert_eq!(root.scope_names(), vec!["source.ts".to_string()]);
    }

    #[test]
    fn push_attributed_handles_space_separated_paths() {
        let root = Arc::new(AttributedScopeStack::create_root("source.ts", 0));
        let pushed = root.push_attributed(Some("meta.function string.quoted"), &StubSource);
        assert_eq!(
            pushed.scope_names(),
            vec![
                "source.ts".to_string(),
                "meta.function".to_string(),
                "string.quoted".to_string(),
            ]
        );
    }

    #[test]
    fn equals_walks_parent_chain() {
        let base = AttributedScopeStack::create_root(
            "source.ts",
            metadata::pack(1, StandardTokenType::Other, false, FontStyle::NONE, 0, 0),
        );
        let copy = base.clone();
        assert!(AttributedScopeStack::equals(Some(&base), Some(&copy)));

        let other = AttributedScopeStack::create_root(
            "source.js",
            metadata::pack(1, StandardTokenType::Other, false, FontStyle::NONE, 0, 0),
        );
        assert!(!AttributedScopeStack::equals(Some(&base), Some(&other)));
    }

    #[test]
    fn extension_round_trips_via_from_extension() {
        let root = Arc::new(AttributedScopeStack::create_root("source.ts", 0));
        let deep = root.push_attributed(Some("meta.function"), &StubSource);
        let frames = deep.extension_if_defined(Some(&root)).unwrap();
        let restored = AttributedScopeStack::from_extension(Some(root.clone()), &frames).unwrap();
        assert_eq!(restored.scope_names(), deep.scope_names());
    }
}
