//! Grammar file entry point — `.tmLanguage.json` vs `.tmLanguage.plist`.
//!
//! Port of `src/parseRawGrammar.ts` from `vscode-textmate` (MIT,
//! Copyright Microsoft Corporation). Dispatches to the right parser
//! based on the file extension; when the extension is ambiguous (no
//! path supplied) we fall back to the plist loader because that's
//! upstream's choice.

use crate::rule::RawGrammar;
use crate::TextMateError;

/// Parses a grammar file's contents. `file_path` is only used to
/// pick between the JSON and plist parsers; pass `None` to force the
/// plist path (matches upstream's default branch).
pub fn parse_raw_grammar(
    content: &str,
    file_path: Option<&str>,
) -> Result<RawGrammar, TextMateError> {
    if file_path.is_some_and(|p| p.to_ascii_lowercase().ends_with(".json")) {
        return RawGrammar::from_json(content);
    }
    RawGrammar::from_plist(content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_path_dispatches_to_from_json() {
        let raw = r#"{ "scopeName": "source.demo", "patterns": [] }"#;
        let grammar = parse_raw_grammar(raw, Some("demo.tmLanguage.json")).unwrap();
        assert_eq!(grammar.scope_name, "source.demo");
    }

    #[test]
    fn json_path_accepts_mixed_case_extensions() {
        let raw = r#"{ "scopeName": "source.demo", "patterns": [] }"#;
        let grammar = parse_raw_grammar(raw, Some("demo.tmLanguage.JSON")).unwrap();
        assert_eq!(grammar.scope_name, "source.demo");
    }
}
