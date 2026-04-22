//! Type conversions between `sidex_text` types and `lsp_types`.
//!
//! Both `sidex_text::Position` and `lsp_types::Position` are zero-based
//! line/character pairs, so the mapping is straightforward.

use sidex_text::{Position, Range};

/// Converts a `sidex_text::Position` to an `lsp_types::Position`.
pub fn position_to_lsp(pos: Position) -> lsp_types::Position {
    lsp_types::Position {
        line: pos.line,
        character: pos.column,
    }
}

/// Converts an `lsp_types::Position` to a `sidex_text::Position`.
pub fn lsp_to_position(pos: lsp_types::Position) -> Position {
    Position::new(pos.line, pos.character)
}

/// Converts a `sidex_text::Range` to an `lsp_types::Range`.
pub fn range_to_lsp(range: Range) -> lsp_types::Range {
    lsp_types::Range {
        start: position_to_lsp(range.start),
        end: position_to_lsp(range.end),
    }
}

/// Converts an `lsp_types::Range` to a `sidex_text::Range`.
pub fn lsp_to_range(range: lsp_types::Range) -> Range {
    Range::new(lsp_to_position(range.start), lsp_to_position(range.end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_roundtrip() {
        let original = Position::new(10, 25);
        let lsp = position_to_lsp(original);
        let back = lsp_to_position(lsp);
        assert_eq!(original, back);
    }

    #[test]
    fn position_zero() {
        let zero = Position::ZERO;
        let lsp = position_to_lsp(zero);
        assert_eq!(lsp.line, 0);
        assert_eq!(lsp.character, 0);
        assert_eq!(lsp_to_position(lsp), zero);
    }

    #[test]
    fn range_roundtrip() {
        let original = Range::new(Position::new(1, 5), Position::new(3, 10));
        let lsp = range_to_lsp(original);
        let back = lsp_to_range(lsp);
        assert_eq!(original, back);
    }

    #[test]
    fn range_empty_roundtrip() {
        let original = Range::new(Position::new(7, 3), Position::new(7, 3));
        let lsp = range_to_lsp(original);
        let back = lsp_to_range(lsp);
        assert_eq!(original, back);
        assert!(back.is_empty());
    }

    #[test]
    fn lsp_position_fields_match() {
        let pos = Position::new(42, 99);
        let lsp = position_to_lsp(pos);
        assert_eq!(lsp.line, 42);
        assert_eq!(lsp.character, 99);
    }

    #[test]
    fn lsp_range_fields_match() {
        let range = Range::new(Position::new(1, 2), Position::new(3, 4));
        let lsp = range_to_lsp(range);
        assert_eq!(lsp.start.line, 1);
        assert_eq!(lsp.start.character, 2);
        assert_eq!(lsp.end.line, 3);
        assert_eq!(lsp.end.character, 4);
    }
}
