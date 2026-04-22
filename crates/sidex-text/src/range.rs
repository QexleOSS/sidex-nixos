use serde::{Deserialize, Serialize};

use crate::Position;

/// A range in a text document, defined by a start and end [`Position`].
///
/// Ranges are always normalized so that `start <= end`. If constructed with
/// a start greater than the end, the values are swapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    /// The inclusive start position of the range.
    pub start: Position,
    /// The exclusive end position of the range.
    pub end: Position,
}

impl Range {
    /// Creates a new range, normalizing so that `start <= end`.
    #[must_use]
    pub fn new(start: Position, end: Position) -> Self {
        if start <= end {
            Self { start, end }
        } else {
            Self {
                start: end,
                end: start,
            }
        }
    }

    /// Returns `true` if the range is empty (start equals end).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns `true` if the range contains the given position.
    ///
    /// A position is contained if it is >= start and < end, or if the range
    /// is empty and the position equals start.
    #[must_use]
    pub fn contains(&self, pos: Position) -> bool {
        if self.is_empty() {
            return pos == self.start;
        }
        pos >= self.start && pos < self.end
    }

    /// Returns `true` if this range fully contains another range.
    #[must_use]
    pub fn contains_range(&self, other: &Range) -> bool {
        other.start >= self.start && other.end <= self.end
    }

    /// Returns `true` if this range intersects with another range.
    #[must_use]
    pub fn intersects(&self, other: &Range) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Returns the union of this range and another, covering both.
    #[must_use]
    pub fn union(&self, other: &Range) -> Range {
        Range {
            start: std::cmp::min(self.start, other.start),
            end: std::cmp::max(self.end, other.end),
        }
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}..{}]", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_normalizes_order() {
        let r = Range::new(Position::new(2, 0), Position::new(0, 0));
        assert_eq!(r.start, Position::new(0, 0));
        assert_eq!(r.end, Position::new(2, 0));
    }

    #[test]
    fn is_empty() {
        let r = Range::new(Position::new(1, 5), Position::new(1, 5));
        assert!(r.is_empty());
    }

    #[test]
    fn contains_position() {
        let r = Range::new(Position::new(1, 0), Position::new(1, 10));
        assert!(r.contains(Position::new(1, 0)));
        assert!(r.contains(Position::new(1, 5)));
        assert!(!r.contains(Position::new(1, 10)));
        assert!(!r.contains(Position::new(0, 5)));
    }

    #[test]
    fn contains_range() {
        let outer = Range::new(Position::new(0, 0), Position::new(5, 0));
        let inner = Range::new(Position::new(1, 0), Position::new(3, 0));
        assert!(outer.contains_range(&inner));
        assert!(!inner.contains_range(&outer));
    }

    #[test]
    fn intersects() {
        let a = Range::new(Position::new(0, 0), Position::new(2, 0));
        let b = Range::new(Position::new(1, 0), Position::new(3, 0));
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn no_intersect() {
        let a = Range::new(Position::new(0, 0), Position::new(1, 0));
        let b = Range::new(Position::new(1, 0), Position::new(2, 0));
        assert!(!a.intersects(&b));
    }

    #[test]
    fn union() {
        let a = Range::new(Position::new(1, 0), Position::new(3, 0));
        let b = Range::new(Position::new(2, 0), Position::new(5, 0));
        let u = a.union(&b);
        assert_eq!(u.start, Position::new(1, 0));
        assert_eq!(u.end, Position::new(5, 0));
    }

    #[test]
    fn serde_roundtrip() {
        let r = Range::new(Position::new(1, 2), Position::new(3, 4));
        let json = serde_json::to_string(&r).unwrap();
        let deserialized: Range = serde_json::from_str(&json).unwrap();
        assert_eq!(r, deserialized);
    }
}
