use serde::{Deserialize, Serialize};

use crate::{Position, Range};

/// A single edit operation that replaces a range of text with new text.
///
/// All edits are modeled as "replace range with text": insertions have an empty
/// range, deletions have empty text, and replacements have both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditOperation {
    /// The range to replace.
    pub range: Range,
    /// The replacement text.
    pub text: String,
}

impl EditOperation {
    /// Creates an insert operation at the given position.
    #[must_use]
    pub fn insert(pos: Position, text: String) -> Self {
        Self {
            range: Range::new(pos, pos),
            text,
        }
    }

    /// Creates a delete operation for the given range.
    #[must_use]
    pub fn delete(range: Range) -> Self {
        Self {
            range,
            text: String::new(),
        }
    }

    /// Creates a replace operation for the given range with new text.
    #[must_use]
    pub fn replace(range: Range, text: String) -> Self {
        Self { range, text }
    }
}

/// Describes a change that occurred in a buffer after an edit was applied.
///
/// This can be used for incremental updates to downstream consumers such as
/// LSP servers or syntax highlighters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// The range that was replaced.
    pub range: Range,
    /// The new text that replaced the range.
    pub text: String,
    /// The number of characters that were replaced.
    pub range_length: usize,
}

impl std::fmt::Display for EditOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.range.is_empty() {
            write!(f, "Insert({}, {:?})", self.range.start, self.text)
        } else if self.text.is_empty() {
            write!(f, "Delete({})", self.range)
        } else {
            write!(f, "Replace({}, {:?})", self.range, self.text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_operation() {
        let op = EditOperation::insert(Position::new(1, 5), "hello".into());
        assert!(op.range.is_empty());
        assert_eq!(op.text, "hello");
    }

    #[test]
    fn delete_operation() {
        let op = EditOperation::delete(Range::new(Position::new(0, 0), Position::new(0, 5)));
        assert!(op.text.is_empty());
    }

    #[test]
    fn replace_operation() {
        let range = Range::new(Position::new(0, 0), Position::new(0, 5));
        let op = EditOperation::replace(range, "world".into());
        assert_eq!(op.range, range);
        assert_eq!(op.text, "world");
    }

    #[test]
    fn serde_roundtrip() {
        let op = EditOperation::insert(Position::new(2, 3), "test".into());
        let json = serde_json::to_string(&op).unwrap();
        let deserialized: EditOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, deserialized);
    }
}
