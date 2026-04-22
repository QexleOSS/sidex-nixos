//! Rule runtime — compiled form of `TextMate` grammar patterns.
//!
//! Faithful port of `src/rule.ts` from `vscode-textmate` (MIT,
//! Microsoft). The Rust translation splits the file into focused
//! submodules while preserving every type shape and algorithm:
//!
//! * [`raw`] — untyped `.tmLanguage.json` / `.tmLanguage.plist`
//!   types (`IRawGrammar`, `IRawRule`, `IRawCaptures`).
//! * [`include`] — `parseInclude` + the five reference variants
//!   (port of the `grammarDependencies.ts` half we need for the rule
//!   factory).
//! * [`regex_source`] — `RegExpSource` with `\A` / `\G` / `\z` rewriting.
//! * [`source_list`] — `RegExpSourceList` + `CompiledRule` scanner.
//! * [`rules`] — compiled `Rule` enum, `RuleHeader`, `RuleRegistry`.
//!
//! The factory (pattern → compiled `Rule`) lives in [`factory`] and is
//! driven by the grammar compiler in turn 4.

use serde::{Deserialize, Serialize};

pub mod factory;
pub mod include;
pub mod raw;
pub mod regex_source;
pub mod rules;
pub mod source_list;

pub use factory::RuleFactory;
pub use include::{parse_include, IncludeReference};
pub use raw::{Location, RawCaptures, RawGrammar, RawRepository, RawRule};
pub use regex_source::RegExpSource;
pub use rules::{
    BeginEndRule, BeginWhileRule, CaptureRule, IncludeOnlyRule, MatchRule, Rule, RuleHeader,
    RuleRegistry, END_RULE_ID, WHILE_RULE_ID,
};
pub use source_list::{CompiledRule, FindNextMatch, RegExpSourceList};

/// Stable identifier for a compiled rule inside a grammar.
///
/// `vscode-textmate` uses a branded `number` type; Rust has no branding
/// so we wrap `u32` in a newtype. The `i32` sentinels `END_RULE_ID` and
/// `WHILE_RULE_ID` never show up as [`RuleId`] values — they're only
/// ever read off the compiled scanner's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuleId(pub u32);

impl RuleId {
    /// Upstream's `endRuleId = -1`, re-encoded for the rule-id channel.
    pub const END: i32 = END_RULE_ID;
    /// Upstream's `whileRuleId = -2`, re-encoded for the rule-id channel.
    pub const WHILE: i32 = WHILE_RULE_ID;

    /// Encodes `RuleId` into the signed rule-id channel used by compiled
    /// scanners, preserving the `END`/`WHILE` sentinels.
    #[must_use]
    pub fn to_signed(self) -> i32 {
        i32::try_from(self.0).unwrap_or(i32::MAX)
    }
}
