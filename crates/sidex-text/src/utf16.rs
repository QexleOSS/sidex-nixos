/// UTF-16 position types and conversion utilities for LSP compatibility.
///
/// The Language Server Protocol uses UTF-16 offsets for column positions,
/// while Rust strings are UTF-8. This module bridges the two encodings.
use serde::{Deserialize, Serialize};

use crate::Position;

/// An LSP-compatible position using UTF-16 character offsets.
///
/// This mirrors the LSP `Position` type where `character` is a UTF-16 offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Utf16Position {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based UTF-16 code unit offset within the line.
    pub character: u32,
}

impl Utf16Position {
    /// Creates a new UTF-16 position.
    #[must_use]
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// Converts a UTF-16 column offset to a character (Unicode scalar) offset
/// within the given line text.
///
/// Characters outside the Basic Multilingual Plane (above U+FFFF) are encoded
/// as two UTF-16 code units (a surrogate pair) but count as one Rust `char`.
#[must_use]
pub fn utf16_col_to_char_col(line_text: &str, utf16_col: usize) -> usize {
    let mut utf16_offset = 0;
    for (char_idx, ch) in line_text.chars().enumerate() {
        if utf16_offset >= utf16_col {
            return char_idx;
        }
        utf16_offset += ch.len_utf16();
    }
    line_text.chars().count()
}

/// Converts a character column offset to a UTF-16 column offset within the
/// given line text.
#[must_use]
pub fn char_col_to_utf16_col(line_text: &str, char_col: usize) -> usize {
    line_text.chars().take(char_col).map(char::len_utf16).sum()
}

/// Converts an [`Utf16Position`] to a [`Position`] using character offsets,
/// given access to a line's text content.
#[must_use]
pub fn lsp_position_to_position(line_text: &str, lsp_pos: Utf16Position) -> Position {
    let char_col = utf16_col_to_char_col(line_text, lsp_pos.character as usize);
    #[allow(clippy::cast_possible_truncation)]
    Position::new(lsp_pos.line, char_col as u32)
}

/// Converts a [`Position`] to an [`Utf16Position`], given access to a line's
/// text content.
#[must_use]
pub fn position_to_lsp_position(line_text: &str, pos: Position) -> Utf16Position {
    let utf16_col = char_col_to_utf16_col(line_text, pos.column as usize);
    #[allow(clippy::cast_possible_truncation)]
    Utf16Position::new(pos.line, utf16_col as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_roundtrip() {
        let text = "hello world";
        assert_eq!(utf16_col_to_char_col(text, 5), 5);
        assert_eq!(char_col_to_utf16_col(text, 5), 5);
    }

    #[test]
    fn emoji_utf16_offset() {
        // U+1F600 is encoded as a surrogate pair in UTF-16 (2 code units)
        let text = "a😀b";
        assert_eq!(char_col_to_utf16_col(text, 0), 0);
        assert_eq!(char_col_to_utf16_col(text, 1), 1);
        assert_eq!(char_col_to_utf16_col(text, 2), 3);

        assert_eq!(utf16_col_to_char_col(text, 0), 0);
        assert_eq!(utf16_col_to_char_col(text, 1), 1);
        assert_eq!(utf16_col_to_char_col(text, 3), 2);
    }

    #[test]
    fn cjk_characters() {
        let text = "你好世界";
        assert_eq!(char_col_to_utf16_col(text, 2), 2);
        assert_eq!(utf16_col_to_char_col(text, 2), 2);
    }

    #[test]
    fn mixed_bmp_and_supplementary() {
        // U+10348 needs a surrogate pair in UTF-16
        let text = "a𐍈b";
        assert_eq!(char_col_to_utf16_col(text, 0), 0);
        assert_eq!(char_col_to_utf16_col(text, 1), 1);
        assert_eq!(char_col_to_utf16_col(text, 2), 3);
        assert_eq!(char_col_to_utf16_col(text, 3), 4);

        assert_eq!(utf16_col_to_char_col(text, 0), 0);
        assert_eq!(utf16_col_to_char_col(text, 1), 1);
        assert_eq!(utf16_col_to_char_col(text, 3), 2);
        assert_eq!(utf16_col_to_char_col(text, 4), 3);
    }

    #[test]
    fn empty_string() {
        assert_eq!(utf16_col_to_char_col("", 0), 0);
        assert_eq!(char_col_to_utf16_col("", 0), 0);
    }

    #[test]
    fn lsp_position_conversion_roundtrip() {
        let text = "a😀b";
        let pos = Position::new(3, 2);
        let lsp = position_to_lsp_position(text, pos);
        assert_eq!(lsp.line, 3);
        assert_eq!(lsp.character, 3);

        let back = lsp_position_to_position(text, lsp);
        assert_eq!(back, pos);
    }

    #[test]
    fn utf16_col_past_end_clamps() {
        let text = "abc";
        assert_eq!(utf16_col_to_char_col(text, 100), 3);
    }

    #[test]
    fn combining_characters() {
        // e + combining acute accent — two chars, both BMP
        let text = "e\u{0301}x";
        assert_eq!(char_col_to_utf16_col(text, 0), 0);
        assert_eq!(char_col_to_utf16_col(text, 1), 1);
        assert_eq!(char_col_to_utf16_col(text, 2), 2);
    }

    #[test]
    fn serde_utf16_position() {
        let pos = Utf16Position::new(10, 20);
        let json = serde_json::to_string(&pos).unwrap();
        let deserialized: Utf16Position = serde_json::from_str(&json).unwrap();
        assert_eq!(pos, deserialized);
    }
}
