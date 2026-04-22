/// A zero-based line/column position within a text buffer.
///
/// Both `line` and `column` are zero-indexed. The column represents a
/// character (Unicode scalar value) offset within the line.
use serde::{Deserialize, Serialize};

/// A zero-based position in a text document, specified by line and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based column (character offset within the line).
    pub column: u32,
}

impl Position {
    /// Creates a new position at the given line and column.
    #[must_use]
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }

    /// The origin position (line 0, column 0).
    pub const ZERO: Self = Self { line: 0, column: 0 };
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.line
            .cmp(&other.line)
            .then(self.column.cmp(&other.column))
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_position() {
        let pos = Position::new(3, 7);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 7);
    }

    #[test]
    fn zero_constant() {
        assert_eq!(Position::ZERO, Position::new(0, 0));
    }

    #[test]
    fn ordering() {
        let a = Position::new(0, 0);
        let b = Position::new(0, 5);
        let c = Position::new(1, 0);
        let d = Position::new(1, 3);

        assert!(a < b);
        assert!(b < c);
        assert!(c < d);
        assert!(a < d);
    }

    #[test]
    fn equality() {
        let a = Position::new(2, 4);
        let b = Position::new(2, 4);
        assert_eq!(a, b);
    }

    #[test]
    fn display() {
        let pos = Position::new(10, 25);
        assert_eq!(format!("{pos}"), "10:25");
    }

    #[test]
    fn copy_semantics() {
        let a = Position::new(1, 2);
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn serde_roundtrip() {
        let pos = Position::new(5, 10);
        let json = serde_json::to_string(&pos).unwrap();
        let deserialized: Position = serde_json::from_str(&json).unwrap();
        assert_eq!(pos, deserialized);
    }
}
