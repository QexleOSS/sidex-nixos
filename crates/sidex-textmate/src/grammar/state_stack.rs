//! Port of `StateStackImpl` + `StateStackFrame` from `grammar.ts` (MIT,
//! Microsoft).
//!
//! The per-frame state the tokenizer carries across line boundaries.
//! Each frame tracks the active rule, the name/content scope lists,
//! the dynamic end-rule override (for begin/end rules with
//! back-references), and per-line anchor/enter positions used by the
//! endless-loop guards in `tokenizeString`.

use std::sync::Arc;

use parking_lot::Mutex;

use super::attr_stack::{AttributedScopeStack, AttributedScopeStackFrame};
use crate::rule::RuleId;

/// Linked-list stack frame. `Arc<Mutex<_>>` for the per-frame
/// line-local positions (`enter_pos` / `anchor_pos`) because upstream
/// mutates them in-place via `reset` while keeping other fields frozen.
#[derive(Clone)]
pub struct StateStackImpl {
    parent: Option<Arc<StateStackImpl>>,
    rule_id: RuleId,
    positions: Arc<Mutex<StackPositions>>,
    begin_rule_captured_eol: bool,
    end_rule: Option<String>,
    name_scopes_list: Option<Arc<AttributedScopeStack>>,
    content_name_scopes_list: Option<Arc<AttributedScopeStack>>,
    depth: u32,
}

#[derive(Clone, Copy)]
struct StackPositions {
    enter_pos: i64,
    anchor_pos: i64,
}

impl std::fmt::Debug for StateStackImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let positions = *self.positions.lock();
        f.debug_struct("StateStackImpl")
            .field("rule_id", &self.rule_id)
            .field("depth", &self.depth)
            .field("enter_pos", &positions.enter_pos)
            .field("anchor_pos", &positions.anchor_pos)
            .field("begin_rule_captured_eol", &self.begin_rule_captured_eol)
            .field("end_rule", &self.end_rule)
            .finish_non_exhaustive()
    }
}

impl StateStackImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parent: Option<Arc<StateStackImpl>>,
        rule_id: RuleId,
        enter_pos: i64,
        anchor_pos: i64,
        begin_rule_captured_eol: bool,
        end_rule: Option<String>,
        name_scopes_list: Option<Arc<AttributedScopeStack>>,
        content_name_scopes_list: Option<Arc<AttributedScopeStack>>,
    ) -> Self {
        let depth = parent.as_ref().map_or(1, |p| p.depth + 1);
        Self {
            parent,
            rule_id,
            positions: Arc::new(Mutex::new(StackPositions {
                enter_pos,
                anchor_pos,
            })),
            begin_rule_captured_eol,
            end_rule,
            name_scopes_list,
            content_name_scopes_list,
            depth,
        }
    }

    /// Sentinel for the top-of-stack node used before any rule is
    /// pushed. Matches upstream's `StateStackImpl.NULL`.
    pub fn null() -> Arc<Self> {
        Arc::new(Self::new(None, RuleId(0), -1, -1, false, None, None, None))
    }

    pub fn parent(&self) -> Option<&Arc<StateStackImpl>> {
        self.parent.as_ref()
    }

    pub fn rule_id(&self) -> RuleId {
        self.rule_id
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn begin_rule_captured_eol(&self) -> bool {
        self.begin_rule_captured_eol
    }

    pub fn end_rule(&self) -> Option<&str> {
        self.end_rule.as_deref()
    }

    pub fn name_scopes_list(&self) -> Option<&Arc<AttributedScopeStack>> {
        self.name_scopes_list.as_ref()
    }

    pub fn content_name_scopes_list(&self) -> Option<&Arc<AttributedScopeStack>> {
        self.content_name_scopes_list.as_ref()
    }

    pub fn enter_pos(&self) -> i64 {
        self.positions.lock().enter_pos
    }

    pub fn anchor_pos(&self) -> i64 {
        self.positions.lock().anchor_pos
    }

    /// Resets every frame's line-local positions. Called at the start
    /// of `tokenizeLine` so the `_enterPos == linePos` endless-loop
    /// check can't false-positive carry over from the previous line.
    pub fn reset(self: &Arc<Self>) {
        let mut cursor: Option<Arc<StateStackImpl>> = Some(Arc::clone(self));
        while let Some(node) = cursor {
            {
                let mut pos = node.positions.lock();
                pos.enter_pos = -1;
                pos.anchor_pos = -1;
            }
            cursor = node.parent.as_ref().map(Arc::clone);
        }
    }

    /// Pop the top frame. Returns `None` only at the root.
    pub fn pop(self: &Arc<Self>) -> Option<Arc<StateStackImpl>> {
        self.parent.as_ref().map(Arc::clone)
    }

    /// Pop that stays on root if there's no parent.
    pub fn safe_pop(self: &Arc<Self>) -> Arc<StateStackImpl> {
        self.parent
            .as_ref()
            .map_or_else(|| Arc::clone(self), Arc::clone)
    }

    /// Push a new frame.
    #[allow(clippy::too_many_arguments)]
    pub fn push(
        self: &Arc<Self>,
        rule_id: RuleId,
        enter_pos: i64,
        anchor_pos: i64,
        begin_rule_captured_eol: bool,
        end_rule: Option<String>,
        name_scopes_list: Option<Arc<AttributedScopeStack>>,
        content_name_scopes_list: Option<Arc<AttributedScopeStack>>,
    ) -> Arc<StateStackImpl> {
        Arc::new(Self::new(
            Some(Arc::clone(self)),
            rule_id,
            enter_pos,
            anchor_pos,
            begin_rule_captured_eol,
            end_rule,
            name_scopes_list,
            content_name_scopes_list,
        ))
    }

    /// Replace the content-name scope list on the top frame, returning
    /// the new frame (or `self` if unchanged). Port of
    /// `withContentNameScopesList`.
    pub fn with_content_name_scopes_list(
        self: &Arc<Self>,
        content_scopes: Arc<AttributedScopeStack>,
    ) -> Arc<StateStackImpl> {
        if let Some(existing) = &self.content_name_scopes_list {
            if Arc::ptr_eq(existing, &content_scopes) {
                return Arc::clone(self);
            }
        }
        let parent = self
            .parent
            .as_ref()
            .expect("root has no content scope to replace");
        let positions = *self.positions.lock();
        parent.push(
            self.rule_id,
            positions.enter_pos,
            positions.anchor_pos,
            self.begin_rule_captured_eol,
            self.end_rule.clone(),
            self.name_scopes_list.clone(),
            Some(content_scopes),
        )
    }

    /// Overwrite the dynamic end-rule string. Port of `withEndRule`.
    pub fn with_end_rule(self: &Arc<Self>, end_rule: String) -> Arc<StateStackImpl> {
        if self.end_rule.as_deref() == Some(end_rule.as_str()) {
            return Arc::clone(self);
        }
        let positions = *self.positions.lock();
        Arc::new(Self::new(
            self.parent.clone(),
            self.rule_id,
            positions.enter_pos,
            positions.anchor_pos,
            self.begin_rule_captured_eol,
            Some(end_rule),
            self.name_scopes_list.clone(),
            self.content_name_scopes_list.clone(),
        ))
    }

    /// Walk back looking for a frame with the same `ruleId` whose
    /// `enter_pos` also equals `other.enter_pos`. Direct port of
    /// upstream's endless-loop guard helper.
    pub fn has_same_rule_as(&self, other: &StateStackImpl) -> bool {
        let other_enter = other.positions.lock().enter_pos;
        let mut cursor: Option<&StateStackImpl> = Some(self);
        while let Some(node) = cursor {
            if node.positions.lock().enter_pos != other_enter {
                return false;
            }
            if node.rule_id == other.rule_id {
                return true;
            }
            cursor = node.parent.as_deref();
        }
        false
    }

    /// Structural equality (scope content included). Port of
    /// `StateStackImpl._equals`.
    pub fn equals(a: &StateStackImpl, b: &StateStackImpl) -> bool {
        if std::ptr::eq(a, b) {
            return true;
        }
        if !Self::structural_equals(Some(a), Some(b)) {
            return false;
        }
        AttributedScopeStack::equals(
            a.content_name_scopes_list.as_deref(),
            b.content_name_scopes_list.as_deref(),
        )
    }

    fn structural_equals(mut a: Option<&StateStackImpl>, mut b: Option<&StateStackImpl>) -> bool {
        loop {
            match (a, b) {
                (None, None) => return true,
                (Some(x), Some(y)) if std::ptr::eq(x, y) => return true,
                (Some(_), None) | (None, Some(_)) => return false,
                (Some(x), Some(y)) => {
                    if x.depth != y.depth || x.rule_id != y.rule_id || x.end_rule != y.end_rule {
                        return false;
                    }
                    a = x.parent.as_deref();
                    b = y.parent.as_deref();
                }
            }
        }
    }

    /// Converts this frame (top of stack) into the serializable form
    /// [`StateStackFrame`]. Used by the renderer/worker IPC. Port of
    /// `toStateStackFrame`.
    pub fn to_frame(&self) -> StateStackFrame {
        let parent_name_scope = self
            .parent
            .as_ref()
            .and_then(|p| p.name_scopes_list.clone());
        let name_frames = self
            .name_scopes_list
            .as_ref()
            .and_then(|s| s.extension_if_defined(parent_name_scope.as_deref()))
            .unwrap_or_default();

        let content_frames = self
            .content_name_scopes_list
            .as_ref()
            .and_then(|s| s.extension_if_defined(self.name_scopes_list.as_deref()))
            .unwrap_or_default();

        let positions = *self.positions.lock();
        StateStackFrame {
            rule_id: self.rule_id,
            enter_pos: Some(positions.enter_pos),
            anchor_pos: Some(positions.anchor_pos),
            begin_rule_captured_eol: self.begin_rule_captured_eol,
            end_rule: self.end_rule.clone(),
            name_scopes_list: name_frames,
            content_name_scopes_list: content_frames,
        }
    }

    /// Inverse of [`Self::to_frame`]. Port of `StateStackImpl.pushFrame`.
    pub fn push_frame(
        parent: Option<Arc<StateStackImpl>>,
        frame: &StateStackFrame,
    ) -> Arc<StateStackImpl> {
        let parent_names = parent.as_ref().and_then(|p| p.name_scopes_list.clone());
        let names_scope_list =
            AttributedScopeStack::from_extension(parent_names, &frame.name_scopes_list);
        let content_scope_list = AttributedScopeStack::from_extension(
            names_scope_list.clone(),
            &frame.content_name_scopes_list,
        );

        Arc::new(Self::new(
            parent,
            frame.rule_id,
            frame.enter_pos.unwrap_or(-1),
            frame.anchor_pos.unwrap_or(-1),
            frame.begin_rule_captured_eol,
            frame.end_rule.clone(),
            names_scope_list,
            content_scope_list,
        ))
    }
}

/// Serializable form of [`StateStackImpl`] frames. Matches upstream's
/// `StateStackFrame`; lives on the IPC boundary.
#[derive(Debug, Clone)]
pub struct StateStackFrame {
    pub rule_id: RuleId,
    pub enter_pos: Option<i64>,
    pub anchor_pos: Option<i64>,
    pub begin_rule_captured_eol: bool,
    pub end_rule: Option<String>,
    pub name_scopes_list: Vec<AttributedScopeStackFrame>,
    pub content_name_scopes_list: Vec<AttributedScopeStackFrame>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_counts_parent_chain() {
        let root = StateStackImpl::null();
        let child = root.push(RuleId(1), 0, 0, false, None, None, None);
        let grandchild = child.push(RuleId(2), 0, 0, false, None, None, None);
        assert_eq!(root.depth(), 1);
        assert_eq!(child.depth(), 2);
        assert_eq!(grandchild.depth(), 3);
    }

    #[test]
    fn pop_returns_parent_or_none_at_root() {
        let root = StateStackImpl::null();
        let child = root.push(RuleId(1), 0, 0, false, None, None, None);
        assert!(child.pop().is_some());
        assert!(root.pop().is_none());
        assert!(Arc::ptr_eq(&root.safe_pop(), &root));
    }

    #[test]
    fn reset_zeros_line_positions_across_chain() {
        let root = Arc::new(StateStackImpl::new(
            None,
            RuleId(0),
            3,
            5,
            false,
            None,
            None,
            None,
        ));
        let child = root.push(RuleId(1), 7, 9, false, None, None, None);
        child.reset();
        assert_eq!(root.enter_pos(), -1);
        assert_eq!(root.anchor_pos(), -1);
        assert_eq!(child.enter_pos(), -1);
    }

    #[test]
    fn with_end_rule_clones_when_value_changes() {
        let root = StateStackImpl::null();
        let child = root.push(RuleId(1), 0, 0, false, Some("a".to_string()), None, None);
        let same = child.with_end_rule("a".to_string());
        assert!(Arc::ptr_eq(&child, &same));
        let changed = child.with_end_rule("b".to_string());
        assert!(!Arc::ptr_eq(&child, &changed));
        assert_eq!(changed.end_rule(), Some("b"));
    }

    #[test]
    fn has_same_rule_as_walks_back_by_enter_pos() {
        let root = Arc::new(StateStackImpl::new(
            None,
            RuleId(0),
            5,
            -1,
            false,
            None,
            None,
            None,
        ));
        let child = root.push(RuleId(1), 5, -1, false, None, None, None);
        let other = Arc::new(StateStackImpl::new(
            None,
            RuleId(1),
            5,
            -1,
            false,
            None,
            None,
            None,
        ));
        assert!(child.has_same_rule_as(&other));

        let different_pos = Arc::new(StateStackImpl::new(
            None,
            RuleId(1),
            7,
            -1,
            false,
            None,
            None,
            None,
        ));
        assert!(!child.has_same_rule_as(&different_pos));
    }
}
