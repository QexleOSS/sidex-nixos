//! Packed 32-bit token metadata.
//!
//! Faithful Rust port of `src/encodedTokenAttributes.ts` from the
//! `vscode-textmate` project (MIT, Copyright Microsoft Corporation).
//!
//! The wire layout is:
//!
//! ```text
//!     3322 2222 2222 1111 1111 1100 0000 0000
//!     1098 7654 3210 9876 5432 1098 7654 3210
//!  -------------------------------------------
//!     bbbb bbbb ffff ffff fFFF FBTT LLLL LLLL
//!  -------------------------------------------
//!  - L = LanguageId          (8 bits, offset  0)
//!  - T = StandardTokenType   (2 bits, offset  8)
//!  - B = Balanced bracket    (1 bit,  offset 10)
//!  - F = FontStyle           (4 bits, offset 11)
//!  - f = Foreground palette  (9 bits, offset 15)
//!  - b = Background palette  (8 bits, offset 24)
//! ```
//!
//! Matches `EncodedTokenDataConsts` upstream so the packed `u32` values
//! are consumable by Monaco's renderer without translation.

use serde::{Deserialize, Serialize};

pub const LANGUAGEID_MASK: u32 = 0b0000_0000_0000_0000_0000_0000_1111_1111;
pub const TOKEN_TYPE_MASK: u32 = 0b0000_0000_0000_0000_0000_0011_0000_0000;
pub const BALANCED_BRACKETS_MASK: u32 = 0b0000_0000_0000_0000_0000_0100_0000_0000;
pub const FONT_STYLE_MASK: u32 = 0b0000_0000_0000_0000_0111_1000_0000_0000;
pub const FOREGROUND_MASK: u32 = 0b0000_0000_1111_1111_1000_0000_0000_0000;
pub const BACKGROUND_MASK: u32 = 0b1111_1111_0000_0000_0000_0000_0000_0000;

pub const LANGUAGEID_OFFSET: u32 = 0;
pub const TOKEN_TYPE_OFFSET: u32 = 8;
pub const BALANCED_BRACKETS_OFFSET: u32 = 10;
pub const FONT_STYLE_OFFSET: u32 = 11;
pub const FOREGROUND_OFFSET: u32 = 15;
pub const BACKGROUND_OFFSET: u32 = 24;

/// Four standard token categories. Values must match upstream
/// `StandardTokenType` — they are serialized into the packed metadata
/// word and read directly by the editor.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StandardTokenType {
    Other = 0,
    Comment = 1,
    String = 2,
    RegEx = 3,
}

impl StandardTokenType {
    pub const fn from_bits(bits: u32) -> Self {
        match bits & 0b11 {
            1 => Self::Comment,
            2 => Self::String,
            3 => Self::RegEx,
            _ => Self::Other,
        }
    }
}

/// Extension of [`StandardTokenType`] that carries an additional
/// `NotSet` sentinel for `EncodedTokenAttributes::set` calls that want
/// to leave the token-type field untouched.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalStandardTokenType {
    Other = 0,
    Comment = 1,
    String = 2,
    RegEx = 3,
    NotSet = 8,
}

impl From<StandardTokenType> for OptionalStandardTokenType {
    fn from(value: StandardTokenType) -> Self {
        match value {
            StandardTokenType::Other => Self::Other,
            StandardTokenType::Comment => Self::Comment,
            StandardTokenType::String => Self::String,
            StandardTokenType::RegEx => Self::RegEx,
        }
    }
}

/// Bitflags for text decoration. Matches `FontStyle` in `theme.ts`.
///
/// The sentinel `NotSet` (`-1`) is used by `set` to mean "don't change
/// the font style field"; the stored bits never actually contain it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontStyle(pub i32);

impl FontStyle {
    pub const NOT_SET: Self = Self(-1);
    pub const NONE: Self = Self(0);
    pub const ITALIC: Self = Self(1);
    pub const BOLD: Self = Self(2);
    pub const UNDERLINE: Self = Self(4);
    pub const STRIKETHROUGH: Self = Self(8);

    pub const fn bits(self) -> u32 {
        #[allow(clippy::cast_sign_loss)]
        let bits = (self.0 as u32) & 0b1111;
        bits
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn is_not_set(self) -> bool {
        self.0 == Self::NOT_SET.0
    }
}

impl std::ops::BitOr for FontStyle {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Packed token metadata as stored on the wire.
///
/// All field accessors match the upstream `EncodedTokenAttributes`
/// namespace (same semantics, same bit math).
pub type EncodedTokenAttributes = u32;

#[inline]
#[must_use]
pub fn get_language_id(metadata: EncodedTokenAttributes) -> u32 {
    (metadata & LANGUAGEID_MASK) >> LANGUAGEID_OFFSET
}

#[inline]
#[must_use]
pub fn get_token_type(metadata: EncodedTokenAttributes) -> StandardTokenType {
    StandardTokenType::from_bits((metadata & TOKEN_TYPE_MASK) >> TOKEN_TYPE_OFFSET)
}

#[inline]
#[must_use]
pub fn contains_balanced_brackets(metadata: EncodedTokenAttributes) -> bool {
    (metadata & BALANCED_BRACKETS_MASK) != 0
}

#[inline]
#[must_use]
pub fn get_font_style(metadata: EncodedTokenAttributes) -> FontStyle {
    #[allow(clippy::cast_possible_wrap)]
    let bits = ((metadata & FONT_STYLE_MASK) >> FONT_STYLE_OFFSET) as i32;
    FontStyle(bits)
}

#[inline]
#[must_use]
pub fn get_foreground(metadata: EncodedTokenAttributes) -> u32 {
    (metadata & FOREGROUND_MASK) >> FOREGROUND_OFFSET
}

#[inline]
#[must_use]
pub fn get_background(metadata: EncodedTokenAttributes) -> u32 {
    (metadata & BACKGROUND_MASK) >> BACKGROUND_OFFSET
}

/// Binary dump of the packed metadata in the same format VS Code prints
/// for debugging (`toBinaryStr`).
#[must_use]
pub fn to_binary_str(metadata: EncodedTokenAttributes) -> String {
    format!("{metadata:032b}")
}

/// Update fields on a packed metadata word.
///
/// A value of `0`, [`OptionalStandardTokenType::NotSet`], or
/// [`FontStyle::NOT_SET`] means "leave this field unchanged" — matching
/// the upstream `EncodedTokenAttributes.set` behavior.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn set(
    metadata: EncodedTokenAttributes,
    language_id: u32,
    token_type: OptionalStandardTokenType,
    new_balanced_brackets: Option<bool>,
    font_style: FontStyle,
    foreground: u32,
    background: u32,
) -> EncodedTokenAttributes {
    let mut out_language = get_language_id(metadata);
    let mut out_token = match get_token_type(metadata) {
        StandardTokenType::Other => OptionalStandardTokenType::Other,
        StandardTokenType::Comment => OptionalStandardTokenType::Comment,
        StandardTokenType::String => OptionalStandardTokenType::String,
        StandardTokenType::RegEx => OptionalStandardTokenType::RegEx,
    };
    let mut out_balanced: u32 = u32::from(contains_balanced_brackets(metadata));
    let mut out_font = get_font_style(metadata);
    let mut out_foreground = get_foreground(metadata);
    let mut out_background = get_background(metadata);

    if language_id != 0 {
        out_language = language_id;
    }
    if token_type != OptionalStandardTokenType::NotSet {
        out_token = token_type;
    }
    if let Some(flag) = new_balanced_brackets {
        out_balanced = u32::from(flag);
    }
    if !font_style.is_not_set() {
        out_font = font_style;
    }
    if foreground != 0 {
        out_foreground = foreground;
    }
    if background != 0 {
        out_background = background;
    }

    let token_bits = match out_token {
        OptionalStandardTokenType::Other | OptionalStandardTokenType::NotSet => 0,
        OptionalStandardTokenType::Comment => 1,
        OptionalStandardTokenType::String => 2,
        OptionalStandardTokenType::RegEx => 3,
    };

    (out_language << LANGUAGEID_OFFSET)
        | (token_bits << TOKEN_TYPE_OFFSET)
        | (out_balanced << BALANCED_BRACKETS_OFFSET)
        | (out_font.bits() << FONT_STYLE_OFFSET)
        | (out_foreground << FOREGROUND_OFFSET)
        | (out_background << BACKGROUND_OFFSET)
}

/// Builds a packed metadata word from scratch. Convenience for callers
/// that don't need the "leave this field unchanged" semantics of [`set`].
#[must_use]
pub fn pack(
    language_id: u32,
    token_type: StandardTokenType,
    contains_balanced_brackets: bool,
    font_style: FontStyle,
    foreground: u32,
    background: u32,
) -> EncodedTokenAttributes {
    let balanced = u32::from(contains_balanced_brackets);
    ((language_id & 0xFF) << LANGUAGEID_OFFSET)
        | (((token_type as u32) & 0b11) << TOKEN_TYPE_OFFSET)
        | ((balanced & 0b1) << BALANCED_BRACKETS_OFFSET)
        | ((font_style.bits() & 0b1111) << FONT_STYLE_OFFSET)
        | ((foreground & 0x1FF) << FOREGROUND_OFFSET)
        | ((background & 0xFF) << BACKGROUND_OFFSET)
}

/// A single emitted token: byte offset within the line plus the
/// packed-metadata word. Same `(offset, metadata)` pair Monaco consumes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EncodedToken {
    pub offset: u32,
    pub metadata: EncodedTokenAttributes,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_round_trips_all_fields() {
        let meta = pack(
            42,
            StandardTokenType::String,
            true,
            FontStyle::ITALIC | FontStyle::BOLD,
            300,
            7,
        );
        assert_eq!(get_language_id(meta), 42);
        assert_eq!(get_token_type(meta), StandardTokenType::String);
        assert!(contains_balanced_brackets(meta));
        let fs = get_font_style(meta);
        assert!(fs.contains(FontStyle::ITALIC));
        assert!(fs.contains(FontStyle::BOLD));
        assert_eq!(get_foreground(meta), 300);
        assert_eq!(get_background(meta), 7);
    }

    #[test]
    fn set_only_updates_provided_fields() {
        let base = pack(
            1,
            StandardTokenType::Comment,
            false,
            FontStyle::ITALIC,
            10,
            20,
        );
        let updated = set(
            base,
            0,
            OptionalStandardTokenType::NotSet,
            None,
            FontStyle::NOT_SET,
            0,
            0,
        );
        // All fields carried "leave unchanged" sentinels, so output == base.
        assert_eq!(updated, base);
    }

    #[test]
    fn set_overrides_specific_fields() {
        let base = pack(1, StandardTokenType::Other, false, FontStyle::NONE, 0, 0);
        let updated = set(
            base,
            5,
            OptionalStandardTokenType::String,
            Some(true),
            FontStyle::BOLD,
            99,
            45,
        );
        assert_eq!(get_language_id(updated), 5);
        assert_eq!(get_token_type(updated), StandardTokenType::String);
        assert!(contains_balanced_brackets(updated));
        assert_eq!(get_font_style(updated), FontStyle::BOLD);
        assert_eq!(get_foreground(updated), 99);
        assert_eq!(get_background(updated), 45);
    }

    #[test]
    fn binary_dump_is_32_bits() {
        let dump = to_binary_str(0);
        assert_eq!(dump.len(), 32);
        assert!(dump.chars().all(|c| c == '0'));
    }
}
