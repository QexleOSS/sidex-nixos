//! Bracket matching using the tree-sitter parse tree.
//!
//! Instead of scanning raw text, bracket matching walks the AST to find
//! matching bracket pairs, which correctly handles brackets inside strings
//! and comments.

use sidex_text::Position;
use tree_sitter::Tree;

/// Bracket pairs to match.
const BRACKET_PAIRS: &[(&str, &str)] = &[("(", ")"), ("[", "]"), ("{", "}"), ("<", ">")];

/// Finds the matching bracket for the bracket at `pos` in the source.
///
/// Returns `None` if `pos` is not on a bracket character, or if no match
/// is found within the parse tree.
#[must_use]
pub fn find_matching_bracket(tree: &Tree, source: &str, pos: Position) -> Option<Position> {
    let point = tree_sitter::Point {
        row: pos.line as usize,
        column: pos.column as usize,
    };

    let root = tree.root_node();
    let node = root.descendant_for_point_range(point, point)?;

    let node_text = node.utf8_text(source.as_bytes()).ok()?;

    // Determine whether we're on an opening or closing bracket.
    let (is_open, _open, _close) = bracket_role(node_text)?;

    let parent = node.parent()?;

    if is_open {
        // Find the last child of the parent that is the matching close bracket.
        let child_count = parent.child_count();
        for i in (0..child_count).rev() {
            let sibling = parent.child(i)?;
            let sib_text = sibling.utf8_text(source.as_bytes()).ok()?;
            if let Some((sib_open, _, _)) = bracket_role(sib_text) {
                if !sib_open && sibling.start_byte() > node.start_byte() {
                    return Some(node_position(sibling));
                }
            }
        }
    } else {
        // Find the first child of the parent that is the matching open bracket.
        let child_count = parent.child_count();
        for i in 0..child_count {
            let sibling = parent.child(i)?;
            let sib_text = sibling.utf8_text(source.as_bytes()).ok()?;
            if let Some((sib_open, _, _)) = bracket_role(sib_text) {
                if sib_open && sibling.start_byte() < node.start_byte() {
                    return Some(node_position(sibling));
                }
            }
        }
    }

    None
}

/// Returns `(is_open, open_str, close_str)` if `text` is a recognized bracket.
fn bracket_role(text: &str) -> Option<(bool, &'static str, &'static str)> {
    for &(open, close) in BRACKET_PAIRS {
        if text == open {
            return Some((true, open, close));
        }
        if text == close {
            return Some((false, open, close));
        }
    }
    None
}

/// Convert a tree-sitter node's start position to a [`Position`].
#[allow(clippy::cast_possible_truncation)]
fn node_position(node: tree_sitter::Node<'_>) -> Position {
    let p = node.start_position();
    Position::new(p.row as u32, p.column as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_rust(source: &str) -> Tree {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang).unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn match_open_paren() {
        let source = "fn main() {}";
        let tree = parse_rust(source);
        // `(` is at (0, 7)
        let result = find_matching_bracket(&tree, source, Position::new(0, 7));
        assert_eq!(result, Some(Position::new(0, 8)));
    }

    #[test]
    fn match_close_paren() {
        let source = "fn main() {}";
        let tree = parse_rust(source);
        // `)` is at (0, 8)
        let result = find_matching_bracket(&tree, source, Position::new(0, 8));
        assert_eq!(result, Some(Position::new(0, 7)));
    }

    #[test]
    fn match_curly_braces() {
        let source = "fn main() { let x = 1; }";
        let tree = parse_rust(source);
        // `{` is at (0, 10)
        let result = find_matching_bracket(&tree, source, Position::new(0, 10));
        assert_eq!(result, Some(Position::new(0, 23)));
    }

    #[test]
    fn no_match_on_non_bracket() {
        let source = "fn main() {}";
        let tree = parse_rust(source);
        let result = find_matching_bracket(&tree, source, Position::new(0, 0));
        assert!(result.is_none());
    }
}
