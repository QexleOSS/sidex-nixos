//! Raw grammar types — the untyped JSON/plist shape.
//!
//! Port of `src/rawGrammar.ts` from `vscode-textmate` (MIT, Microsoft).
//! These are the types `serde` deserializes into; the compilation
//! pipeline in [`super::factory`] walks them into compiled [`super::Rule`]s.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A filename/line/char triple attached to grammar nodes for error
/// messages and debug output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub filename: String,
    pub line: u32,
    pub char: u32,
}

/// A raw rule — the untyped shape that arrives from JSON. Every field is
/// optional because `TextMate` grammars are free-form; the rule factory
/// distinguishes [`super::MatchRule`] / [`super::BeginEndRule`] /
/// [`super::BeginWhileRule`] / [`super::IncludeOnlyRule`] by which keys
/// are populated.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRule {
    /// Internal id assigned during compilation. Never present on disk.
    #[serde(default, skip)]
    pub id: Option<u32>,

    #[serde(default)]
    pub include: Option<String>,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub content_name: Option<String>,

    #[serde(default, rename = "match")]
    pub match_: Option<String>,

    #[serde(default)]
    pub captures: Option<RawCaptures>,

    #[serde(default)]
    pub begin: Option<String>,

    #[serde(default)]
    pub begin_captures: Option<RawCaptures>,

    #[serde(default)]
    pub end: Option<String>,

    #[serde(default)]
    pub end_captures: Option<RawCaptures>,

    #[serde(default, rename = "while")]
    pub while_: Option<String>,

    #[serde(default)]
    pub while_captures: Option<RawCaptures>,

    #[serde(default)]
    pub patterns: Option<Vec<RawRule>>,

    #[serde(default)]
    pub repository: Option<RawRepository>,

    #[serde(default)]
    pub apply_end_pattern_last: Option<bool>,

    #[serde(default, rename = "$vscodeTextmateLocation")]
    pub location: Option<Location>,
}

/// Captures map — numeric keys as strings. Matches `IRawCapturesMap`.
pub type RawCaptures = BTreeMap<String, RawRule>;

/// Repository — named reusable subpatterns. Supports `$self` / `$base`
/// sentinels, which the grammar loader resolves during compilation.
pub type RawRepository = BTreeMap<String, RawRule>;

/// Top-level grammar record as written on disk. Matches `IRawGrammar`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawGrammar {
    pub scope_name: String,

    #[serde(default)]
    pub repository: RawRepository,

    #[serde(default)]
    pub patterns: Vec<RawRule>,

    #[serde(default)]
    pub injections: Option<BTreeMap<String, RawRule>>,

    #[serde(default)]
    pub injection_selector: Option<String>,

    #[serde(default)]
    pub file_types: Option<Vec<String>>,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub first_line_match: Option<String>,

    #[serde(default, rename = "$vscodeTextmateLocation")]
    pub location: Option<Location>,
}

impl RawGrammar {
    /// Parses a `.tmLanguage.json` document.
    pub fn from_json(contents: &str) -> Result<Self, crate::TextMateError> {
        serde_json::from_str(contents)
            .map_err(|e| crate::TextMateError::GrammarParse(e.to_string()))
    }

    /// Parses a `.tmLanguage.plist` blob (XML or binary plist).
    pub fn from_plist(bytes: &[u8]) -> Result<Self, crate::TextMateError> {
        plist::from_bytes(bytes).map_err(|e| crate::TextMateError::GrammarParse(e.to_string()))
    }
}
