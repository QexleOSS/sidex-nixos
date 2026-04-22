//! Incremental document parsing using tree-sitter.
//!
//! [`DocumentParser`] wraps a tree-sitter [`Parser`] and maintains the current
//! parse tree for a single document. After edits, call [`parse_incremental`]
//! with the previous tree and a list of [`InputEdit`]s for fast re-parsing.
//!
//! [`parse_incremental`]: DocumentParser::parse_incremental

use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::language::Language;

/// Wraps a tree-sitter [`Parser`] for a single document, storing the latest
/// parse tree for incremental updates.
pub struct DocumentParser {
    parser: Parser,
    tree: Option<Tree>,
}

impl std::fmt::Debug for DocumentParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentParser")
            .field("has_tree", &self.tree.is_some())
            .finish_non_exhaustive()
    }
}

impl DocumentParser {
    /// Creates a new parser configured for the given [`Language`].
    ///
    /// # Panics
    ///
    /// Panics if the tree-sitter language version is incompatible.
    #[must_use]
    pub fn new(language: &Language) -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&language.ts_language)
            .expect("language version mismatch");
        Self { parser, tree: None }
    }

    /// Parses `source` from scratch, replacing the stored tree.
    pub fn parse(&mut self, source: &str) -> Option<&Tree> {
        self.tree = self.parser.parse(source, None);
        self.tree.as_ref()
    }

    /// Re-parses `source` incrementally after applying the given edits to the
    /// old tree.
    ///
    /// Each edit in `edits` is applied to `old_tree` before parsing, allowing
    /// tree-sitter to re-use unchanged portions of the tree.
    pub fn parse_incremental(
        &mut self,
        source: &str,
        old_tree: &Tree,
        edits: &[InputEdit],
    ) -> Option<&Tree> {
        let mut tree = old_tree.clone();
        for edit in edits {
            tree.edit(edit);
        }
        self.tree = self.parser.parse(source, Some(&tree));
        self.tree.as_ref()
    }

    /// Returns a reference to the current parse tree, if any.
    #[must_use]
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }
}

/// Convert a [`sidex_text::EditOperation`] + [`sidex_text::Buffer`] into a
/// tree-sitter [`InputEdit`].
///
/// The buffer should reflect the state **before** the edit was applied.
#[must_use]
pub fn to_input_edit(edit: &sidex_text::EditOperation, buffer: &sidex_text::Buffer) -> InputEdit {
    let start_line = edit.range.start.line as usize;
    let start_col = edit.range.start.column as usize;
    let end_line = edit.range.end.line as usize;
    let end_col = edit.range.end.column as usize;

    let start_byte = buffer.char_to_byte(buffer.position_to_offset(edit.range.start));
    let old_end_byte = buffer.char_to_byte(buffer.position_to_offset(edit.range.end));

    let new_text_bytes = edit.text.as_bytes();
    let new_end_byte = start_byte + new_text_bytes.len();

    let newline_count = edit.text.bytes().filter(|&b| b == b'\n').count();
    let new_end_line = start_line + newline_count;
    let new_end_col = if newline_count == 0 {
        start_col + edit.text.len()
    } else {
        edit.text.len() - edit.text.rfind('\n').unwrap() - 1
    };

    InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position: Point {
            row: start_line,
            column: start_col,
        },
        old_end_position: Point {
            row: end_line,
            column: end_col,
        },
        new_end_position: Point {
            row: new_end_line,
            column: new_end_col,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::Language as SidexLanguage;

    fn make_rust_lang() -> SidexLanguage {
        SidexLanguage {
            name: "rust".into(),
            ts_language: tree_sitter_rust::LANGUAGE.into(),
            highlight_query: None,
            injection_query: None,
            file_extensions: vec![".rs".into()],
            line_comment: Some("//".into()),
            block_comment: Some(("/*".into(), "*/".into())),
            auto_closing_pairs: vec![],
            surrounding_pairs: vec![],
            indent_rules: vec![],
            word_pattern: None,
            on_enter_rules: vec![],
            folding_rules: None,
        }
    }

    #[test]
    fn parse_valid_rust() {
        let lang = make_rust_lang();
        let mut parser = DocumentParser::new(&lang);
        let source = "fn main() { println!(\"hello\"); }";
        let tree = parser.parse(source);
        assert!(tree.is_some());
        let root = tree.unwrap().root_node();
        assert!(!root.has_error(), "parse tree should have no errors");
    }

    #[test]
    fn parse_stores_tree() {
        let lang = make_rust_lang();
        let mut parser = DocumentParser::new(&lang);
        assert!(parser.tree().is_none());
        parser.parse("fn main() {}");
        assert!(parser.tree().is_some());
    }

    #[test]
    fn incremental_parse() {
        let lang = make_rust_lang();
        let mut parser = DocumentParser::new(&lang);

        let source_v1 = "fn main() { let x = 1; }";
        parser.parse(source_v1);
        let old_tree = parser.tree().unwrap().clone();

        let source_v2 = "fn main() { let x = 42; }";
        let edit = InputEdit {
            start_byte: 20,
            old_end_byte: 21,
            new_end_byte: 22,
            start_position: Point { row: 0, column: 20 },
            old_end_position: Point { row: 0, column: 21 },
            new_end_position: Point { row: 0, column: 22 },
        };

        let tree = parser.parse_incremental(source_v2, &old_tree, &[edit]);
        assert!(tree.is_some());
        let root = tree.unwrap().root_node();
        assert!(!root.has_error(), "incremental parse should have no errors");
    }

    #[test]
    fn to_input_edit_insert() {
        let buffer = sidex_text::Buffer::from_str("fn main() {}");
        let edit_op = sidex_text::EditOperation::insert(
            sidex_text::Position::new(0, 11),
            " let x = 1;".into(),
        );

        let ie = to_input_edit(&edit_op, &buffer);
        assert_eq!(ie.start_byte, 11);
        assert_eq!(ie.old_end_byte, 11);
        assert_eq!(ie.new_end_byte, 11 + " let x = 1;".len());
    }

    #[test]
    fn to_input_edit_delete() {
        let buffer = sidex_text::Buffer::from_str("fn main() { let x = 1; }");
        let edit_op = sidex_text::EditOperation::delete(sidex_text::Range::new(
            sidex_text::Position::new(0, 11),
            sidex_text::Position::new(0, 23),
        ));

        let ie = to_input_edit(&edit_op, &buffer);
        assert_eq!(ie.start_byte, 11);
        assert_eq!(ie.old_end_byte, 23);
        assert_eq!(ie.new_end_byte, 11);
    }
}
