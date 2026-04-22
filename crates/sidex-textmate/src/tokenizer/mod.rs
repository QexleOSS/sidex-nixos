//! Line tokenization — port of `tokenizeString.ts` from `vscode-textmate`
//! (MIT, Microsoft).
//!
//! The file has two entry points upstream — the public
//! `_tokenizeString` and the internal `_checkWhileConditions` — plus
//! `handleCaptures`, `matchRule`, `matchInjections`, and
//! `prepareRuleSearch`. We preserve all of them, split across:
//!
//! * [`contracts`] — trait interfaces for `Grammar`, `StateStack`,
//!   `AttributedScopeStack`, `TokenSink`, `Injection`. Turn 5 will
//!   provide concrete implementations.
//! * [`hot_path`] — [`hot_path::tokenize_string`] plus the helper
//!   functions that drive a single line's tokenization loop.
//!
//! The tokenizer is intentionally generic: it takes the grammar,
//! stack, and sink via traits so the unit tests in turn 5 can drive
//! it with in-memory fixtures before the Grammar compiler lands.

pub mod contracts;
pub mod hot_path;

#[cfg(test)]
mod tests;

pub use contracts::{
    AttributedScopeStack, GrammarRuntime, Injection, InjectionMatcher, MatchResult, ScanContext,
    StateStack, TokenSink, TokenizeStringResult,
};
pub use hot_path::{tokenize_string, TokenizeInput};
