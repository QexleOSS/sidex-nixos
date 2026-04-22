//! Per-line font emitter.
//!
//! Faithful port of the `LineFonts` + `FontInfo` classes from
//! `grammar.ts` (MIT, Microsoft). Emits one [`FontInfo`] entry per
//! contiguous span that shares the same font-family / font-size /
//! line-height triple; adjacent spans with identical options are
//! merged in-place (the last-emitted entry has its `end_index`
//! extended) to keep the result compact.
//!
//! Font resolution itself lives on the theme's `StyleAttributes` —
//! today those fields aren't wired through `AttributedScopeStack`
//! (upstream exposes a `FontAttribute` cache we haven't ported yet),
//! so the emitter treats every span as "no custom font" for now. The
//! shape of the emitter matches upstream exactly so wiring font
//! attributes through later doesn't require a rewrite.

use std::sync::Arc;

use crate::grammar::attr_stack::AttributedScopeStack;
use crate::tokenizer::TokenSink;

/// A contiguous span with a shared font triple.
#[derive(Debug, Clone, PartialEq)]
pub struct FontInfo {
    pub start_index: usize,
    pub end_index: usize,
    pub font_family: Option<String>,
    pub font_size_multiplier: Option<f64>,
    pub line_height_multiplier: Option<f64>,
}

impl FontInfo {
    #[must_use]
    pub fn options_equal(&self, other: &FontInfo) -> bool {
        self.font_family == other.font_family
            && self.font_size_multiplier == other.font_size_multiplier
            && self.line_height_multiplier == other.line_height_multiplier
    }
}

/// Collects [`FontInfo`] spans as the tokenizer produces tokens.
#[derive(Default)]
pub struct LineFonts {
    fonts: Vec<FontInfo>,
    last_index: usize,
}

impl LineFonts {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Port of upstream's `produceFromScopes`. Extends the final font
    /// span when adjacent options match; pushes a new span otherwise.
    fn emit(&mut self, scopes_list: Option<&AttributedScopeStack>, end_index: usize) {
        let Some(style) = scopes_list.and_then(AttributedScopeStack::style_attributes) else {
            self.last_index = end_index;
            return;
        };
        let font_family = if style.font_family.is_empty() {
            None
        } else {
            Some(style.font_family.clone())
        };
        let font_size_multiplier = (style.font_size != 0.0).then_some(style.font_size);
        let line_height_multiplier = (style.line_height != 0.0).then_some(style.line_height);

        if font_family.is_none()
            && font_size_multiplier.is_none()
            && line_height_multiplier.is_none()
        {
            self.last_index = end_index;
            return;
        }

        let font = FontInfo {
            start_index: self.last_index,
            end_index,
            font_family,
            font_size_multiplier,
            line_height_multiplier,
        };

        if let Some(last) = self.fonts.last_mut() {
            if last.end_index == self.last_index && last.options_equal(&font) {
                last.end_index = font.end_index;
                self.last_index = end_index;
                return;
            }
        }

        self.fonts.push(font);
        self.last_index = end_index;
    }

    /// Returns the emitted spans. Matches upstream's `getResult`.
    #[must_use]
    pub fn into_result(self) -> Vec<FontInfo> {
        self.fonts
    }

    /// Borrow-only accessor for callers that still need to use the
    /// emitter afterwards (e.g. mid-tokenization diagnostics).
    #[must_use]
    pub fn result(&self) -> &[FontInfo] {
        &self.fonts
    }
}

impl TokenSink<Arc<AttributedScopeStack>> for LineFonts {
    fn produce(&mut self, stack_scopes: Option<&Arc<AttributedScopeStack>>, end_index: usize) {
        self.emit(stack_scopes.map(std::convert::AsRef::as_ref), end_index);
    }

    fn produce_from_scopes(
        &mut self,
        scopes: Option<&Arc<AttributedScopeStack>>,
        end_index: usize,
    ) {
        self.emit(scopes.map(std::convert::AsRef::as_ref), end_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_emitter_returns_no_fonts() {
        let fonts = LineFonts::new();
        assert!(fonts.into_result().is_empty());
    }

    #[test]
    fn produce_with_no_style_advances_cursor_silently() {
        let mut fonts = LineFonts::new();
        fonts.produce(None, 10);
        fonts.produce(None, 20);
        assert_eq!(fonts.last_index, 20);
        assert!(fonts.result().is_empty());
    }
}
