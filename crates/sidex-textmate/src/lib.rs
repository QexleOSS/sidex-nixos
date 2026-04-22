//! Native Rust port of Microsoft's `vscode-textmate` package (MIT).
//!
//! Goal: drop-in replacement with byte-for-byte tokenization parity.
//! Ported modules land incrementally; each module is a faithful
//! translation of the corresponding file upstream.
//!
//! Modules landed so far:
//!
//! * [`metadata`] — `encodedTokenAttributes.ts`
//! * [`matcher`]  — `matcher.ts`
//! * [`utils`]    — `utils.ts`
//! * [`regex`]    — Oniguruma FFI (wraps the `onig` crate, which is how
//!   `vscode-textmate` also reaches Oniguruma via WASM)
//!
//! Still to port (planned follow-up turns):
//!
//! * `theme.ts`
//! * `rule.ts`
//! * `grammar/grammarDependencies.ts`
//! * `grammar/grammar.ts`
//! * `grammar/tokenizeString.ts`
//! * `registry.ts`
//! * `diffStateStacks.ts`

pub mod diff_state_stacks;
pub mod grammar;
pub mod matcher;
pub mod metadata;
pub mod parse_raw_grammar;
pub mod regex;
pub mod registry;
pub mod rule;
pub mod theme;
pub mod tokenizer;
pub mod utils;

pub use diff_state_stacks::{apply_state_stack_diff, diff_state_stacks_ref_eq, StackDiff};
pub use grammar::{
    contains_rtl, init_grammar, AttributedScopeStack, AttributedScopeStackFrame,
    BalancedBracketSelectors, BasicScopeAttributes, BasicScopeAttributesProvider,
    EmbeddedLanguagesMap, FontInfo, Grammar, LineFonts, LineTokens, ScopeMetadataSource,
    StateStackFrame, StateStackImpl, Token, TokenEmitMode, TokenTypeMap, TokenTypeMatcher,
    TokenTypeOverride, TokenizeLineBinaryResult, TokenizeLineResult,
};
pub use matcher::{create_matchers, MatcherWithPriority, Priority};
pub use metadata::{
    contains_balanced_brackets, get_background, get_font_style, get_foreground, get_language_id,
    get_token_type, pack, set, to_binary_str, EncodedToken, EncodedTokenAttributes, FontStyle,
    OptionalStandardTokenType, StandardTokenType,
};
pub use parse_raw_grammar::parse_raw_grammar;
pub use regex::{OnigRegex, RegexBuilder};
pub use registry::Registry;
pub use rule::{
    parse_include, BeginEndRule, BeginWhileRule, CaptureRule, CompiledRule, FindNextMatch,
    IncludeOnlyRule, IncludeReference, Location, MatchRule, RawCaptures, RawGrammar, RawRepository,
    RawRule, RegExpSource, RegExpSourceList, Rule, RuleFactory, RuleHeader, RuleId, RuleRegistry,
    END_RULE_ID, WHILE_RULE_ID,
};
pub use theme::{
    font_style_to_string, parse_theme, scope_path_matches_parent_scopes, ColorMap, ParsedThemeRule,
    RawSettings, RawThemeSetting, ScopeField, ScopeStack, StyleAttributes, Theme, ThemeTrieElement,
    ThemeTrieElementRule,
};
pub use tokenizer::{
    tokenize_string, GrammarRuntime, Injection, InjectionMatcher, MatchResult, StateStack,
    TokenSink, TokenizeInput, TokenizeStringResult,
};
pub use utils::{
    basename, escape_regex_chars, is_valid_hex_color, regex_source, str_arr_cmp, strcmp, CachedFn,
    CaptureIndex, LazyRegex,
};

#[derive(Debug, thiserror::Error)]
pub enum TextMateError {
    #[error("grammar parse error: {0}")]
    GrammarParse(String),

    #[error("grammar references unknown scope `{0}`")]
    UnknownScope(String),

    #[error("regex compile failed: {0}")]
    RegexCompile(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type TextMateResult<T> = std::result::Result<T, TextMateError>;
