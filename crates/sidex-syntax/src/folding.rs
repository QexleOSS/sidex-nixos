//! Code folding derived from the tree-sitter parse tree.
//!
//! [`compute_folding_ranges`] walks the AST to find foldable regions such as
//! function bodies, block expressions, comment groups, and import blocks.

use serde::{Deserialize, Serialize};
use tree_sitter::Tree;

/// What kind of code region a folding range represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FoldingKind {
    /// A generic block (function body, if/match arms, etc.).
    Block,
    /// A contiguous block of comments.
    Comment,
    /// A group of import/use statements.
    Imports,
    /// A user-defined `#region` / `#endregion` region.
    Region,
}

/// A foldable range in the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldingRange {
    /// First line of the fold (zero-based, inclusive).
    pub start_line: usize,
    /// Last line of the fold (zero-based, inclusive).
    pub end_line: usize,
    /// The kind of foldable region.
    pub kind: FoldingKind,
}

/// Node types that represent foldable blocks.
const BLOCK_NODE_TYPES: &[&str] = &[
    "block",
    "declaration_list",
    "field_declaration_list",
    "enum_variant_list",
    "match_block",
    "function_item",
    "impl_item",
    "struct_item",
    "enum_item",
    "trait_item",
    "mod_item",
];

/// Node types that represent comment lines.
const COMMENT_NODE_TYPES: &[&str] = &["line_comment", "block_comment"];

/// Node types that represent imports.
const IMPORT_NODE_TYPES: &[&str] = &["use_declaration"];

/// Computes folding ranges from a tree-sitter parse tree.
///
/// The function identifies three categories of folds:
/// - **Blocks**: function bodies, struct/enum/impl bodies, match blocks.
/// - **Comments**: consecutive comment lines grouped into one fold.
/// - **Imports**: consecutive `use` declarations grouped into one fold.
#[must_use]
pub fn compute_folding_ranges(tree: &Tree, _source: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let root = tree.root_node();

    // Pass 1: collect block folds.
    collect_block_folds(root, &mut ranges);

    // Pass 2: group consecutive top-level comments.
    collect_grouped_folds(root, COMMENT_NODE_TYPES, FoldingKind::Comment, &mut ranges);

    // Pass 3: group consecutive imports.
    collect_grouped_folds(root, IMPORT_NODE_TYPES, FoldingKind::Imports, &mut ranges);

    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges
}

/// Recursively walk the tree collecting multi-line block nodes as folds.
fn collect_block_folds(node: tree_sitter::Node<'_>, out: &mut Vec<FoldingRange>) {
    let kind = node.kind();
    if BLOCK_NODE_TYPES.contains(&kind) {
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;
        if end_line > start_line {
            out.push(FoldingRange {
                start_line,
                end_line,
                kind: FoldingKind::Block,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_block_folds(child, out);
    }
}

/// Group consecutive top-level sibling nodes of the given types into a single
/// folding range.
fn collect_grouped_folds(
    root: tree_sitter::Node<'_>,
    node_types: &[&str],
    fold_kind: FoldingKind,
    out: &mut Vec<FoldingRange>,
) {
    let mut cursor = root.walk();
    let children: Vec<_> = root.children(&mut cursor).collect();

    let mut group_start: Option<usize> = None;
    let mut group_end: Option<usize> = None;

    for child in &children {
        if node_types.contains(&child.kind()) {
            let start = child.start_position().row;
            let end = child.end_position().row;
            if group_start.is_none() {
                group_start = Some(start);
            }
            group_end = Some(end);
        } else {
            flush_group(group_start, group_end, fold_kind, out);
            group_start = None;
            group_end = None;
        }
    }
    flush_group(group_start, group_end, fold_kind, out);
}

fn flush_group(
    start: Option<usize>,
    end: Option<usize>,
    kind: FoldingKind,
    out: &mut Vec<FoldingRange>,
) {
    if let (Some(s), Some(e)) = (start, end) {
        if e > s {
            out.push(FoldingRange {
                start_line: s,
                end_line: e,
                kind,
            });
        }
    }
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
    fn fold_function_body() {
        let source = "fn main() {\n    let x = 1;\n    let y = 2;\n}\n";
        let tree = parse_rust(source);
        let ranges = compute_folding_ranges(&tree, source);

        let block_folds: Vec<_> = ranges
            .iter()
            .filter(|r| r.kind == FoldingKind::Block)
            .collect();
        assert!(
            !block_folds.is_empty(),
            "should have at least one block fold"
        );

        let fn_fold = block_folds
            .iter()
            .find(|r| r.start_line == 0)
            .expect("function fold starting at line 0");
        assert_eq!(fn_fold.end_line, 3);
    }

    #[test]
    fn fold_consecutive_comments() {
        let source = "// line 1\n// line 2\n// line 3\nfn main() {}\n";
        let tree = parse_rust(source);
        let ranges = compute_folding_ranges(&tree, source);

        let comment_folds: Vec<_> = ranges
            .iter()
            .filter(|r| r.kind == FoldingKind::Comment)
            .collect();
        assert!(
            !comment_folds.is_empty(),
            "should have a grouped comment fold"
        );
        assert_eq!(comment_folds[0].start_line, 0);
        assert_eq!(comment_folds[0].end_line, 2);
    }

    #[test]
    fn fold_consecutive_imports() {
        let source = "use std::io;\nuse std::fs;\nuse std::path;\n\nfn main() {}\n";
        let tree = parse_rust(source);
        let ranges = compute_folding_ranges(&tree, source);

        let import_folds: Vec<_> = ranges
            .iter()
            .filter(|r| r.kind == FoldingKind::Imports)
            .collect();
        assert!(
            !import_folds.is_empty(),
            "should have a grouped import fold"
        );
        assert_eq!(import_folds[0].start_line, 0);
        assert_eq!(import_folds[0].end_line, 2);
    }

    #[test]
    fn single_line_function_not_folded_as_block() {
        let source = "fn noop() {}\n";
        let tree = parse_rust(source);
        let ranges = compute_folding_ranges(&tree, source);

        let block_folds: Vec<_> = ranges
            .iter()
            .filter(|r| r.kind == FoldingKind::Block)
            .collect();
        assert!(
            block_folds.is_empty(),
            "single-line function should not be foldable"
        );
    }

    #[test]
    fn empty_source_no_folds() {
        let source = "";
        let tree = parse_rust(source);
        let ranges = compute_folding_ranges(&tree, source);
        assert!(ranges.is_empty());
    }
}
