//! Text diff algorithms (Myers-style) for computing minimal edit distances.
//!
//! Provides both character-level (`compute_diff`) and line-level
//! (`compute_line_diff`) diff operations, similar to the diff engine backing
//! Monaco's inline-diff and merge-editor views.

use serde::{Deserialize, Serialize};

/// A contiguous block of changes between two texts.
///
/// Describes that `original[original_start .. original_start + original_length]`
/// should be replaced by `modified[modified_start .. modified_start + modified_length]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffChange {
    /// Start index in the original text (line or char, depending on context).
    pub original_start: usize,
    /// Number of elements removed from the original.
    pub original_length: usize,
    /// Start index in the modified text.
    pub modified_start: usize,
    /// Number of elements inserted into the modified text.
    pub modified_length: usize,
}

/// A single line-level diff entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineDiff {
    /// The line is unchanged.
    Equal(String),
    /// The line was added in the modified version.
    Added(String),
    /// The line was removed from the original.
    Removed(String),
    /// The line was modified (old, new).
    Modified(String, String),
}

// ── Myers diff on generic slices ─────────────────────────────────────

/// Core Myers diff on slices of equatable items.
///
/// Returns a list of `DiffChange` blocks. Equal runs are not represented
/// explicitly — only the changed regions are returned.
#[allow(
    clippy::many_single_char_names,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn myers_diff<T: PartialEq>(old: &[T], new: &[T]) -> Vec<DiffChange> {
    let n = old.len();
    let m = new.len();
    let max = n + m;

    if max == 0 {
        return Vec::new();
    }

    // V maps k → x; stored with offset so negative k works.
    let offset = max;
    let v_size = 2 * max + 1;
    let mut v: Vec<usize> = vec![0; v_size];
    // Store the trace so we can reconstruct the path.
    let mut trace: Vec<Vec<usize>> = Vec::new();

    'outer: for d in 0..=max {
        trace.push(v.clone());
        let mut k = -(d as isize);
        while k <= d as isize {
            let idx = (k + offset as isize) as usize;

            let mut x = if k == -(d as isize) || (k != d as isize && v[idx - 1] < v[idx + 1]) {
                v[idx + 1]
            } else {
                v[idx - 1] + 1
            };
            let mut y = (x as isize - k) as usize;

            while x < n && y < m && old[x] == new[y] {
                x += 1;
                y += 1;
            }

            v[idx] = x;

            if x >= n && y >= m {
                break 'outer;
            }

            k += 2;
        }
    }

    // Backtrack to find the edit script.
    let mut changes: Vec<DiffChange> = Vec::new();
    let mut x = n;
    let mut y = m;

    for d in (0..trace.len()).rev() {
        let v_d = &trace[d];
        let k = x as isize - y as isize;

        let prev_k = if k == -(d as isize)
            || (k != d as isize
                && v_d[(k - 1 + offset as isize) as usize]
                    < v_d[(k + 1 + offset as isize) as usize])
        {
            k + 1
        } else {
            k - 1
        };

        let prev_x = v_d[(prev_k + offset as isize) as usize];
        let prev_y = (prev_x as isize - prev_k) as usize;

        // Walk back the snake (diagonal)
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
        }

        if d == 0 {
            break;
        }

        if prev_x < x {
            // Deletion from original
            changes.push(DiffChange {
                original_start: prev_x,
                original_length: x - prev_x,
                modified_start: prev_y,
                modified_length: 0,
            });
        } else if prev_y < y {
            // Insertion into modified
            changes.push(DiffChange {
                original_start: prev_x,
                original_length: 0,
                modified_start: prev_y,
                modified_length: y - prev_y,
            });
        }

        x = prev_x;
        y = prev_y;
    }

    changes.reverse();
    merge_adjacent_changes(&changes)
}

/// Merge adjacent/overlapping diff changes into combined blocks.
fn merge_adjacent_changes(changes: &[DiffChange]) -> Vec<DiffChange> {
    if changes.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::new();
    let mut current = changes[0].clone();

    for c in &changes[1..] {
        let current_orig_end = current.original_start + current.original_length;
        let current_mod_end = current.modified_start + current.modified_length;

        if c.original_start <= current_orig_end && c.modified_start <= current_mod_end {
            let new_orig_end = (c.original_start + c.original_length).max(current_orig_end);
            let new_mod_end = (c.modified_start + c.modified_length).max(current_mod_end);
            current.original_length = new_orig_end - current.original_start;
            current.modified_length = new_mod_end - current.modified_start;
        } else {
            merged.push(current);
            current = c.clone();
        }
    }
    merged.push(current);
    merged
}

// ── Public API ───────────────────────────────────────────────────────

/// Computes character-level diff changes between two strings using the
/// Myers diff algorithm.
pub fn compute_diff(original: &str, modified: &str) -> Vec<DiffChange> {
    let old_chars: Vec<char> = original.chars().collect();
    let new_chars: Vec<char> = modified.chars().collect();
    myers_diff(&old_chars, &new_chars)
}

/// Computes line-level diff changes between two sets of lines.
pub fn compute_line_diff(original: &[&str], modified: &[&str]) -> Vec<LineDiff> {
    let changes = myers_diff(original, modified);

    let mut result = Vec::new();
    let mut orig_idx: usize = 0;
    let mut mod_idx: usize = 0;

    for change in &changes {
        // Equal lines before this change
        while orig_idx < change.original_start && mod_idx < change.modified_start {
            result.push(LineDiff::Equal(original[orig_idx].to_string()));
            orig_idx += 1;
            mod_idx += 1;
        }

        let removed = change.original_length;
        let added = change.modified_length;
        let common = removed.min(added);

        for i in 0..common {
            result.push(LineDiff::Modified(
                original[change.original_start + i].to_string(),
                modified[change.modified_start + i].to_string(),
            ));
        }
        for i in common..removed {
            result.push(LineDiff::Removed(
                original[change.original_start + i].to_string(),
            ));
        }
        for i in common..added {
            result.push(LineDiff::Added(
                modified[change.modified_start + i].to_string(),
            ));
        }

        orig_idx = change.original_start + removed;
        mod_idx = change.modified_start + added;
    }

    // Trailing equal lines
    while orig_idx < original.len() && mod_idx < modified.len() {
        result.push(LineDiff::Equal(original[orig_idx].to_string()));
        orig_idx += 1;
        mod_idx += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── compute_diff (character-level) ───────────────────────────────

    #[test]
    fn diff_identical() {
        let changes = compute_diff("hello", "hello");
        assert!(changes.is_empty());
    }

    #[test]
    fn diff_empty_to_text() {
        let changes = compute_diff("", "hello");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].original_start, 0);
        assert_eq!(changes[0].original_length, 0);
        assert_eq!(changes[0].modified_start, 0);
        assert_eq!(changes[0].modified_length, 5);
    }

    #[test]
    fn diff_text_to_empty() {
        let changes = compute_diff("hello", "");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].original_start, 0);
        assert_eq!(changes[0].original_length, 5);
        assert_eq!(changes[0].modified_length, 0);
    }

    #[test]
    fn diff_insertion() {
        let changes = compute_diff("ac", "abc");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].original_start, 1);
        assert_eq!(changes[0].original_length, 0);
        assert_eq!(changes[0].modified_start, 1);
        assert_eq!(changes[0].modified_length, 1);
    }

    #[test]
    fn diff_deletion() {
        let changes = compute_diff("abc", "ac");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].original_start, 1);
        assert_eq!(changes[0].original_length, 1);
        assert_eq!(changes[0].modified_start, 1);
        assert_eq!(changes[0].modified_length, 0);
    }

    #[test]
    fn diff_replacement() {
        let changes = compute_diff("abc", "aXc");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].original_start, 1);
        assert_eq!(changes[0].original_length, 1);
        assert_eq!(changes[0].modified_start, 1);
        assert_eq!(changes[0].modified_length, 1);
    }

    #[test]
    fn diff_both_empty() {
        let changes = compute_diff("", "");
        assert!(changes.is_empty());
    }

    #[test]
    fn diff_complex() {
        let changes = compute_diff("abcdef", "abXdeYf");
        // c→X and insert Y after e
        assert!(!changes.is_empty());
        // Just verify the total edit distance is reasonable
        let total_orig: usize = changes.iter().map(|c| c.original_length).sum();
        let total_mod: usize = changes.iter().map(|c| c.modified_length).sum();
        assert!(total_orig <= 3);
        assert!(total_mod <= 3);
    }

    // ── compute_line_diff ────────────────────────────────────────────

    #[test]
    fn line_diff_identical() {
        let orig = vec!["a", "b", "c"];
        let modi = vec!["a", "b", "c"];
        let diff = compute_line_diff(&orig, &modi);
        assert_eq!(diff.len(), 3);
        assert!(diff.iter().all(|d| matches!(d, LineDiff::Equal(_))));
    }

    #[test]
    fn line_diff_added() {
        let orig: Vec<&str> = vec!["a", "c"];
        let modi: Vec<&str> = vec!["a", "b", "c"];
        let diff = compute_line_diff(&orig, &modi);
        assert!(diff.contains(&LineDiff::Equal("a".into())));
        assert!(diff.contains(&LineDiff::Equal("c".into())));
        let has_added = diff
            .iter()
            .any(|d| matches!(d, LineDiff::Added(s) if s == "b"));
        assert!(has_added);
    }

    #[test]
    fn line_diff_removed() {
        let orig: Vec<&str> = vec!["a", "b", "c"];
        let modi: Vec<&str> = vec!["a", "c"];
        let diff = compute_line_diff(&orig, &modi);
        let has_removed = diff
            .iter()
            .any(|d| matches!(d, LineDiff::Removed(s) if s == "b"));
        assert!(has_removed);
    }

    #[test]
    fn line_diff_modified() {
        let orig: Vec<&str> = vec!["a", "b", "c"];
        let modi: Vec<&str> = vec!["a", "B", "c"];
        let diff = compute_line_diff(&orig, &modi);
        let has_modified = diff
            .iter()
            .any(|d| matches!(d, LineDiff::Modified(old, new) if old == "b" && new == "B"));
        assert!(has_modified);
    }

    #[test]
    fn line_diff_empty_to_lines() {
        let orig: Vec<&str> = vec![];
        let modi: Vec<&str> = vec!["a", "b"];
        let diff = compute_line_diff(&orig, &modi);
        assert_eq!(diff.len(), 2);
        assert!(diff.iter().all(|d| matches!(d, LineDiff::Added(_))));
    }

    #[test]
    fn line_diff_lines_to_empty() {
        let orig: Vec<&str> = vec!["a", "b"];
        let modi: Vec<&str> = vec![];
        let diff = compute_line_diff(&orig, &modi);
        assert_eq!(diff.len(), 2);
        assert!(diff.iter().all(|d| matches!(d, LineDiff::Removed(_))));
    }

    #[test]
    fn line_diff_complete_replacement() {
        let orig: Vec<&str> = vec!["a", "b"];
        let modi: Vec<&str> = vec!["x", "y"];
        let diff = compute_line_diff(&orig, &modi);
        assert_eq!(diff.len(), 2);
        assert!(diff.iter().all(|d| matches!(d, LineDiff::Modified(_, _))));
    }

    // ── merge_adjacent_changes ───────────────────────────────────────

    #[test]
    fn merge_adjacent() {
        let changes = vec![
            DiffChange {
                original_start: 0,
                original_length: 1,
                modified_start: 0,
                modified_length: 0,
            },
            DiffChange {
                original_start: 1,
                original_length: 1,
                modified_start: 0,
                modified_length: 0,
            },
        ];
        let merged = merge_adjacent_changes(&changes);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].original_start, 0);
        assert_eq!(merged[0].original_length, 2);
    }

    #[test]
    fn merge_non_adjacent() {
        let changes = vec![
            DiffChange {
                original_start: 0,
                original_length: 1,
                modified_start: 0,
                modified_length: 1,
            },
            DiffChange {
                original_start: 5,
                original_length: 1,
                modified_start: 5,
                modified_length: 1,
            },
        ];
        let merged = merge_adjacent_changes(&changes);
        assert_eq!(merged.len(), 2);
    }
}
