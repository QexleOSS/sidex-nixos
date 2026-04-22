//! Linked-list scope stack — matches `ScopeStack` in `theme.ts`.
//!
//! `Arc`-shared parent chain so cloning a handle never duplicates the
//! underlying nodes. `None` parent marks the root.

use std::sync::Arc;

pub type ScopePath = String;

/// Immutable linked-list of scope names. Cheap to share and compare.
///
/// Port of the upstream `ScopeStack` class. All construction goes
/// through [`Self::new`] / [`Self::push`] so parent pointers are never
/// mutated after the node is created.
#[derive(Debug, Clone)]
pub struct ScopeStack {
    parent: Option<Arc<ScopeStack>>,
    scope_name: String,
}

impl ScopeStack {
    #[must_use]
    pub fn new(parent: Option<Arc<ScopeStack>>, scope_name: impl Into<String>) -> Self {
        Self {
            parent,
            scope_name: scope_name.into(),
        }
    }

    /// Convenience constructor that builds a stack from a slice of scope
    /// names in outer-to-inner order. Mirrors the upstream `from` static.
    #[must_use]
    pub fn from_segments(segments: &[&str]) -> Option<Arc<Self>> {
        let mut current: Option<Arc<Self>> = None;
        for seg in segments {
            current = Some(Arc::new(Self::new(current, *seg)));
        }
        current
    }

    /// Appends a new leaf scope, returning a new [`Arc`] handle.
    #[must_use]
    pub fn push_segment(self: &Arc<Self>, scope_name: impl Into<String>) -> Arc<Self> {
        Arc::new(Self::new(Some(Arc::clone(self)), scope_name))
    }

    pub fn parent(&self) -> Option<&ScopeStack> {
        self.parent.as_deref()
    }

    pub fn scope_name(&self) -> &str {
        &self.scope_name
    }

    /// Walks the chain outer-to-inner, collecting the scope names.
    #[must_use]
    pub fn segments(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut cursor: Option<&ScopeStack> = Some(self);
        while let Some(node) = cursor {
            out.push(node.scope_name.clone());
            cursor = node.parent.as_deref();
        }
        out.reverse();
        out
    }

    /// Mirrors upstream's `toString`.
    #[must_use]
    pub fn as_path(&self) -> String {
        self.segments().join(" ")
    }

    /// Returns `true` when `self` extends `other` — i.e. `other` is a
    /// prefix of `self`. Direct port of `ScopeStack.extends`.
    pub fn extends(&self, other: &ScopeStack) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        match &self.parent {
            None => false,
            Some(parent) => parent.extends(other),
        }
    }

    /// If `self` extends `base`, returns the suffix scope names;
    /// otherwise `None`. Matches `getExtensionIfDefined`.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub fn extension_if_defined(&self, base: Option<&ScopeStack>) -> Option<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        let mut cursor: Option<&ScopeStack> = Some(self);
        loop {
            match (cursor, base) {
                (Some(c), Some(b)) if std::ptr::eq(c, b) => break,
                (Some(c), None) => {
                    out.push(c.scope_name.clone());
                    cursor = c.parent.as_deref();
                }
                (Some(c), Some(_)) => {
                    out.push(c.scope_name.clone());
                    cursor = c.parent.as_deref();
                }
                (None, None) => break,
                (None, Some(_)) => return None,
            }
        }
        out.reverse();
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_walk_outer_to_inner() {
        let stack = ScopeStack::from_segments(&["source.ts", "meta.function", "string"]).unwrap();
        assert_eq!(
            stack.segments(),
            vec![
                "source.ts".to_string(),
                "meta.function".to_string(),
                "string".to_string()
            ]
        );
        assert_eq!(stack.as_path(), "source.ts meta.function string");
    }

    #[test]
    fn push_segment_preserves_chain() {
        let base = ScopeStack::from_segments(&["source.ts"]).unwrap();
        let child = base.push_segment("string.quoted");
        assert_eq!(child.scope_name(), "string.quoted");
        assert_eq!(child.parent().unwrap().scope_name(), "source.ts");
    }

    #[test]
    fn extends_is_symmetric_by_pointer() {
        let a = ScopeStack::from_segments(&["source.ts", "comment"]).unwrap();
        assert!(a.extends(&a));
    }
}
