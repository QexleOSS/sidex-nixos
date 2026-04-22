//! Stack diffing for incremental re-tokenization.
//!
//! Port of `src/diffStateStacks.ts` from `vscode-textmate` (MIT,
//! Copyright Microsoft Corporation). Compares two rule stacks that
//! share a common prefix and produces the minimum number of
//! `(pop, push)` operations to transform one into the other.
//!
//! The diff is consumed by [`apply_state_stack_diff`] which replays
//! the transformation on an arbitrary starting stack — useful when
//! pushing tokenization work off to a worker and re-running the
//! changes the worker made on top of the UI thread's current stack.

use std::sync::Arc;

use crate::grammar::state_stack::{StateStackFrame, StateStackImpl};

/// The minimum transformation required to turn one stack into another.
#[derive(Debug, Clone, Default)]
pub struct StackDiff {
    /// Number of frames to pop off the source stack.
    pub pops: u32,
    /// Frames to push (outer-to-inner order) after popping.
    pub new_frames: Vec<StateStackFrame>,
}

/// Computes the diff between `first` and `second`. Walks both stacks
/// parent-ward, peeling whichever side is deeper until the two nodes
/// coincide (or both reach the root). Popped frames are counted; new
/// frames are collected as [`StateStackFrame`]s so they can be
/// re-pushed via [`apply_state_stack_diff`].
///
/// Complexity is O(|depth(first)| + |depth(second)|) in the worst
/// case; typically both stacks share most of their parents so the
/// walk is short.
#[must_use]
pub fn diff_state_stacks_ref_eq(
    first: &Arc<StateStackImpl>,
    second: &Arc<StateStackImpl>,
) -> StackDiff {
    let mut pops: u32 = 0;
    let mut new_frames: Vec<StateStackFrame> = Vec::new();

    let mut cur_first: Option<Arc<StateStackImpl>> = Some(Arc::clone(first));
    let mut cur_second: Option<Arc<StateStackImpl>> = Some(Arc::clone(second));

    // Loop until the two cursors point at the same node (including
    // both-None at the root).
    loop {
        match (&cur_first, &cur_second) {
            (Some(a), Some(b)) if Arc::ptr_eq(a, b) => break,
            (None, None) => break,
            _ => {}
        }

        let first_depth = cur_first.as_ref().map_or(0, |s| s.depth());
        let second_depth = cur_second.as_ref().map_or(0, |s| s.depth());

        if cur_first.is_some() && (cur_second.is_none() || first_depth >= second_depth) {
            // Pop from the first cursor.
            pops = pops.saturating_add(1);
            cur_first = cur_first.as_ref().and_then(StateStackImpl::pop);
        } else {
            // Push the second cursor's frame onto the diff.
            let node = cur_second
                .as_ref()
                .expect("second must be Some when first is None or deeper");
            new_frames.push(node.to_frame());
            cur_second = cur_second.as_ref().and_then(StateStackImpl::pop);
        }
    }

    new_frames.reverse();
    StackDiff { pops, new_frames }
}

/// Replays `diff` on top of `stack`: pops `diff.pops` frames, then
/// pushes every frame in `diff.new_frames` in order.
///
/// Returns `None` only if the diff requests more pops than the stack
/// has frames available — which in practice means the diff wasn't
/// built against a compatible base. Callers treat this as "re-tokenize
/// from scratch" in that case, matching upstream's behavior.
#[must_use]
pub fn apply_state_stack_diff(
    stack: Option<Arc<StateStackImpl>>,
    diff: &StackDiff,
) -> Option<Arc<StateStackImpl>> {
    let mut cur = stack;
    for _ in 0..diff.pops {
        cur = cur.and_then(|s| StateStackImpl::pop(&s));
    }
    for frame in &diff.new_frames {
        cur = Some(StateStackImpl::push_frame(cur, frame));
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::RuleId;

    fn null_stack() -> Arc<StateStackImpl> {
        StateStackImpl::null()
    }

    #[test]
    fn identical_stacks_diff_to_zero() {
        let stack = null_stack().push(RuleId(1), 0, 0, false, None, None, None);
        let diff = diff_state_stacks_ref_eq(&stack, &stack);
        assert_eq!(diff.pops, 0);
        assert!(diff.new_frames.is_empty());
    }

    #[test]
    fn popping_diff_counts_extra_frames() {
        let base = null_stack();
        let with_child = base.push(RuleId(1), 0, 0, false, None, None, None);
        let diff = diff_state_stacks_ref_eq(&with_child, &base);
        assert_eq!(diff.pops, 1);
        assert!(diff.new_frames.is_empty());
    }

    #[test]
    fn pushing_diff_records_frames_in_order() {
        let base = null_stack();
        let a = base.push(RuleId(1), 0, 0, false, None, None, None);
        let b = a.push(RuleId(2), 0, 0, false, None, None, None);
        let diff = diff_state_stacks_ref_eq(&base, &b);
        assert_eq!(diff.pops, 0);
        assert_eq!(diff.new_frames.len(), 2);
    }

    #[test]
    fn apply_replays_pops_and_pushes() {
        let base = null_stack();
        let a = base.push(RuleId(1), 0, 0, false, None, None, None);
        let b = a.push(RuleId(2), 0, 0, false, None, None, None);
        let diff = diff_state_stacks_ref_eq(&base, &b);
        let applied = apply_state_stack_diff(Some(Arc::clone(&base)), &diff).unwrap();
        assert_eq!(applied.depth(), b.depth());
    }
}
