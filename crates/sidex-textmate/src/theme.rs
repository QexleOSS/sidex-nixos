//! Theme model.
//!
//! Faithful Rust port of `src/theme.ts` from `vscode-textmate` (MIT,
//! Copyright Microsoft Corporation). Parses `IRawTheme` into a compiled
//! [`Theme`] that resolves a scope stack into a [`StyleAttributes`].
//!
//! Matches upstream's algorithm bit-for-bit:
//!
//! * `parseTheme` normalizes raw theme settings into [`ParsedThemeRule`]
//!   objects, expanding comma-lists into one rule per scope.
//! * `resolveParsedThemeRules` sorts the rules lexicographically,
//!   extracts leading empty-scope defaults, and builds the trie.
//! * [`ThemeTrieElement::match`] walks the trie by `.`-segmented scope
//!   head / tail, returning rules sorted by specificity.
//! * [`ThemeTrieElement::insert`] handles parent-scope merging with
//!   main-rule inheritance when adding rules with parent selectors.

mod rule;
mod scope;
mod trie;

pub use rule::{FontStyle, ParsedThemeRule, StyleAttributes, ThemeTrieElementRule};
pub use scope::{ScopePath, ScopeStack};
pub use trie::ThemeTrieElement;

use std::sync::Arc;

use crate::utils::CachedFn;

/// Color palette shared across theme rules. Colors are normalized to
/// uppercase and de-duplicated. `ColorId(0)` is reserved for "no color"
/// so rules that don't set a field leave the packed metadata at zero.
#[derive(Debug, Default, Clone)]
pub struct ColorMap {
    is_frozen: bool,
    last_id: u32,
    id_to_color: Vec<String>,
    color_to_id: std::collections::HashMap<String, u32>,
}

impl ColorMap {
    pub fn new() -> Self {
        Self {
            is_frozen: false,
            last_id: 0,
            id_to_color: vec![String::new()],
            color_to_id: std::collections::HashMap::new(),
        }
    }

    /// Seeds the palette from an existing list. The resulting map is
    /// frozen; [`Self::get_id`] for a new color panics like upstream.
    pub fn from_frozen(colors: &[String]) -> Self {
        let mut id_to_color = Vec::with_capacity(colors.len());
        let mut color_to_id = std::collections::HashMap::with_capacity(colors.len());
        for (idx, color) in colors.iter().enumerate() {
            let id = u32::try_from(idx).unwrap_or(u32::MAX);
            id_to_color.push(color.clone());
            color_to_id.insert(color.clone(), id);
        }
        let last_id = u32::try_from(colors.len().saturating_sub(1)).unwrap_or(0);
        Self {
            is_frozen: true,
            last_id,
            id_to_color,
            color_to_id,
        }
    }

    /// Returns the id for a color string, allocating a new one when the
    /// map isn't frozen. `None` → `0`, matching the upstream contract.
    ///
    /// # Panics
    ///
    /// Panics when the map is frozen and the color wasn't pre-seeded.
    pub fn get_id(&mut self, color: Option<&str>) -> u32 {
        let Some(color) = color else {
            return 0;
        };
        let upper = color.to_ascii_uppercase();
        if let Some(&id) = self.color_to_id.get(&upper) {
            return id;
        }
        assert!(!self.is_frozen, "Missing color in color map - {upper}");
        self.last_id = self.last_id.saturating_add(1);
        let id = self.last_id;
        self.color_to_id.insert(upper.clone(), id);
        if self.id_to_color.len() <= id as usize {
            self.id_to_color.resize(id as usize + 1, String::new());
        }
        self.id_to_color[id as usize] = upper;
        id
    }

    /// Flattened palette: index `i` → color string.
    #[must_use]
    pub fn color_map(&self) -> Vec<String> {
        self.id_to_color.clone()
    }
}

/// Compiled theme — ready to match scope stacks against.
pub struct Theme {
    color_map: ColorMap,
    defaults: StyleAttributes,
    root: Arc<ThemeTrieElement>,
    cache: CachedFn<String, Vec<ThemeTrieElementRule>>,
}

impl Theme {
    /// Constructs a [`Theme`] from an [`IRawTheme`]-shaped settings slice
    /// and an optional seeded color map. Equivalent to
    /// `Theme.createFromRawTheme` upstream.
    pub fn create_from_raw(settings: &[RawThemeSetting], color_map: Option<Vec<String>>) -> Self {
        Self::create_from_parsed(parse_theme(settings), color_map)
    }

    /// Internal constructor matching `Theme.createFromParsedTheme`.
    #[must_use]
    pub fn create_from_parsed(rules: Vec<ParsedThemeRule>, color_map: Option<Vec<String>>) -> Self {
        resolve_parsed_theme_rules(rules, color_map)
    }

    pub(super) fn new(
        color_map: ColorMap,
        defaults: StyleAttributes,
        root: ThemeTrieElement,
    ) -> Self {
        Self {
            color_map,
            defaults,
            root: Arc::new(root),
            cache: CachedFn::new(),
        }
    }

    pub fn color_map(&self) -> Vec<String> {
        self.color_map.color_map()
    }

    pub fn defaults(&self) -> &StyleAttributes {
        &self.defaults
    }

    /// Resolves a scope stack into a [`StyleAttributes`]. Returns `None`
    /// when the stack exists but no rule matched — matching the upstream
    /// contract (falls back to the editor-wide default).
    pub fn r#match(&self, scope_path: Option<&ScopeStack>) -> Option<StyleAttributes> {
        let Some(path) = scope_path else {
            return Some(self.defaults.clone());
        };
        let scope_name = path.scope_name().to_string();
        let root = Arc::clone(&self.root);
        let matches = self.cache.get(scope_name, move |name| root.r#match(name));

        let parent = path.parent();
        let effective = matches
            .into_iter()
            .find(|rule| scope_path_matches_parent_scopes(parent, &rule.parent_scopes))?;

        Some(StyleAttributes {
            font_style: effective.font_style,
            foreground_id: effective.foreground,
            background_id: effective.background,
            font_family: effective.font_family,
            font_size: effective.font_size,
            line_height: effective.line_height,
        })
    }
}

/// A single raw theme setting (matches `IRawThemeSetting`).
#[derive(Debug, Clone)]
pub struct RawThemeSetting {
    pub name: Option<String>,
    pub scope: ScopeField,
    pub settings: RawSettings,
}

/// `scope` is either a comma-separated string, an array of strings, or
/// missing entirely. `Missing` maps onto `[""]` during parsing.
#[derive(Debug, Clone)]
pub enum ScopeField {
    String(String),
    Array(Vec<String>),
    Missing,
}

/// Settings bag — all fields optional, matching `IRawThemeSetting.settings`.
#[derive(Debug, Clone, Default)]
pub struct RawSettings {
    pub font_style: Option<String>,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub line_height: Option<f64>,
}

/// Converts a scope stack's parent chain into a match decision against
/// the parent-scopes list stored on a [`ThemeTrieElementRule`]. Matches
/// the upstream `_scopePathMatchesParentScopes` algorithm exactly.
pub fn scope_path_matches_parent_scopes(
    mut scope_path: Option<&ScopeStack>,
    parent_scopes: &[String],
) -> bool {
    if parent_scopes.is_empty() {
        return true;
    }

    let mut index = 0;
    while index < parent_scopes.len() {
        let mut scope_pattern = parent_scopes[index].as_str();
        let mut scope_must_match = false;

        // Child combinator (`parent > child`).
        if scope_pattern == ">" {
            if index == parent_scopes.len() - 1 {
                return false;
            }
            index += 1;
            scope_pattern = parent_scopes[index].as_str();
            scope_must_match = true;
        }

        loop {
            let Some(path) = scope_path else {
                return false;
            };
            if matches_scope(path.scope_name(), scope_pattern) {
                break;
            }
            if scope_must_match {
                return false;
            }
            scope_path = path.parent();
        }

        scope_path = scope_path.and_then(ScopeStack::parent);
        index += 1;
    }

    true
}

fn matches_scope(scope_name: &str, scope_pattern: &str) -> bool {
    scope_pattern == scope_name
        || (scope_name.starts_with(scope_pattern)
            && scope_name.as_bytes().get(scope_pattern.len()) == Some(&b'.'))
}

/// Parses raw theme settings into rules — direct port of `parseTheme`.
#[must_use]
pub fn parse_theme(settings: &[RawThemeSetting]) -> Vec<ParsedThemeRule> {
    let mut out: Vec<ParsedThemeRule> = Vec::new();

    for (index, entry) in settings.iter().enumerate() {
        let scopes: Vec<String> = match &entry.scope {
            ScopeField::String(raw) => {
                let trimmed = raw.trim_start_matches(',').trim_end_matches(',');
                trimmed.split(',').map(str::to_string).collect()
            }
            ScopeField::Array(list) => list.clone(),
            ScopeField::Missing => vec![String::new()],
        };

        let font_style = parse_font_style(entry.settings.font_style.as_deref());
        let foreground = entry
            .settings
            .foreground
            .as_deref()
            .filter(|s| crate::utils::is_valid_hex_color(s))
            .map(str::to_string);
        let background = entry
            .settings
            .background
            .as_deref()
            .filter(|s| crate::utils::is_valid_hex_color(s))
            .map(str::to_string);
        let font_family = entry.settings.font_family.clone().unwrap_or_default();
        let font_size = entry.settings.font_size.unwrap_or(0.0);
        let line_height = entry.settings.line_height.unwrap_or(0.0);

        for raw_scope in &scopes {
            let scope = raw_scope.trim();
            let mut segments: Vec<&str> = scope.split_whitespace().collect();
            let leaf = segments.pop().unwrap_or("").to_string();
            let parent_scopes: Option<Vec<String>> = if segments.is_empty() {
                None
            } else {
                // Parent scopes arrive in reverse order (deepest first).
                let mut rev: Vec<String> = segments.iter().map(|s| (*s).to_string()).collect();
                rev.reverse();
                Some(rev)
            };

            out.push(ParsedThemeRule {
                scope: leaf,
                parent_scopes,
                index: u32::try_from(index).unwrap_or(u32::MAX),
                font_style,
                foreground: foreground.clone(),
                background: background.clone(),
                font_family: font_family.clone(),
                font_size,
                line_height,
            });
        }
    }

    out
}

fn parse_font_style(raw: Option<&str>) -> FontStyle {
    let Some(raw) = raw else {
        return FontStyle::NOT_SET;
    };
    let mut style = FontStyle::NONE;
    for segment in raw.split_whitespace() {
        style = match segment {
            "italic" => style | FontStyle::ITALIC,
            "bold" => style | FontStyle::BOLD,
            "underline" => style | FontStyle::UNDERLINE,
            "strikethrough" => style | FontStyle::STRIKETHROUGH,
            _ => style,
        };
    }
    style
}

/// Produces a human-readable description of a font-style bitmask —
/// equivalent to `fontStyleToString` upstream.
#[must_use]
pub fn font_style_to_string(style: FontStyle) -> String {
    if style.is_not_set() {
        return "not set".to_string();
    }
    let mut out = String::new();
    if style.contains(FontStyle::ITALIC) {
        out.push_str("italic ");
    }
    if style.contains(FontStyle::BOLD) {
        out.push_str("bold ");
    }
    if style.contains(FontStyle::UNDERLINE) {
        out.push_str("underline ");
    }
    if style.contains(FontStyle::STRIKETHROUGH) {
        out.push_str("strikethrough ");
    }
    if out.is_empty() {
        out.push_str("none");
    }
    out.trim_end().to_string()
}

fn resolve_parsed_theme_rules(
    mut rules: Vec<ParsedThemeRule>,
    color_map_input: Option<Vec<String>>,
) -> Theme {
    rules.sort_by(|a, b| match crate::utils::strcmp(&a.scope, &b.scope) {
        0 => {
            match crate::utils::str_arr_cmp(a.parent_scopes.as_deref(), b.parent_scopes.as_deref())
            {
                0 => a.index.cmp(&b.index),
                n if n < 0 => std::cmp::Ordering::Less,
                _ => std::cmp::Ordering::Greater,
            }
        }
        n if n < 0 => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Greater,
    });

    let mut default_font_style = FontStyle::NONE;
    let mut default_foreground = "#000000".to_string();
    let mut default_background = "#ffffff".to_string();
    let mut default_font_family = String::new();
    let mut default_font_size = 0.0;
    let mut default_line_height = 0.0;

    while rules.first().is_some_and(|r| r.scope.is_empty()) {
        let incoming = rules.remove(0);
        if !incoming.font_style.is_not_set() {
            default_font_style = incoming.font_style;
        }
        if let Some(fg) = incoming.foreground {
            default_foreground = fg;
        }
        if let Some(bg) = incoming.background {
            default_background = bg;
        }
        if !incoming.font_family.is_empty() {
            default_font_family = incoming.font_family;
        }
        if incoming.font_size != 0.0 {
            default_font_size = incoming.font_size;
        }
        if incoming.line_height != 0.0 {
            default_line_height = incoming.line_height;
        }
    }

    let mut color_map = match color_map_input {
        Some(list) => ColorMap::from_frozen(&list),
        None => ColorMap::new(),
    };
    let defaults = StyleAttributes {
        font_style: default_font_style,
        foreground_id: color_map.get_id(Some(&default_foreground)),
        background_id: color_map.get_id(Some(&default_background)),
        font_family: default_font_family.clone(),
        font_size: default_font_size,
        line_height: default_line_height,
    };

    let main_rule = ThemeTrieElementRule {
        scope_depth: 0,
        parent_scopes: Vec::new(),
        font_style: FontStyle::NOT_SET,
        foreground: 0,
        background: 0,
        font_family: default_font_family,
        font_size: default_font_size,
        line_height: default_line_height,
    };
    let mut root = ThemeTrieElement::new(main_rule, Vec::new());

    for rule in rules {
        let fg_id = color_map.get_id(rule.foreground.as_deref());
        let bg_id = color_map.get_id(rule.background.as_deref());
        root.insert(
            0,
            &rule.scope,
            rule.parent_scopes.as_deref(),
            rule.font_style,
            fg_id,
            bg_id,
            &rule.font_family,
            rule.font_size,
            rule.line_height,
        );
    }

    Theme::new(color_map, defaults, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(scope: &str, fg: Option<&str>, style: Option<&str>) -> RawThemeSetting {
        RawThemeSetting {
            name: None,
            scope: if scope.is_empty() {
                ScopeField::Missing
            } else {
                ScopeField::String(scope.to_string())
            },
            settings: RawSettings {
                font_style: style.map(str::to_string),
                foreground: fg.map(str::to_string),
                ..RawSettings::default()
            },
        }
    }

    #[test]
    fn parse_font_style_handles_known_keywords() {
        assert_eq!(
            parse_font_style(Some("italic bold")),
            FontStyle::ITALIC | FontStyle::BOLD
        );
        assert_eq!(parse_font_style(Some("")), FontStyle::NONE);
        assert_eq!(parse_font_style(None), FontStyle::NOT_SET);
    }

    #[test]
    fn font_style_to_string_matches_upstream() {
        assert_eq!(font_style_to_string(FontStyle::NOT_SET), "not set");
        assert_eq!(font_style_to_string(FontStyle::NONE), "none");
        assert_eq!(
            font_style_to_string(FontStyle::ITALIC | FontStyle::BOLD),
            "italic bold"
        );
    }

    #[test]
    fn parse_theme_splits_comma_scopes_and_captures_parent() {
        let settings = vec![setting("meta.function string, comment", Some("#abc"), None)];
        let rules = parse_theme(&settings);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].scope, "string");
        assert_eq!(
            rules[0].parent_scopes.as_deref(),
            Some(&["meta.function".to_string()][..])
        );
        assert_eq!(rules[1].scope, "comment");
        assert!(rules[1].parent_scopes.is_none());
    }

    #[test]
    fn color_map_normalizes_to_uppercase_and_dedupes() {
        let mut map = ColorMap::new();
        let id1 = map.get_id(Some("#abcdef"));
        let id2 = map.get_id(Some("#ABCDEF"));
        assert_eq!(id1, id2);
        assert_eq!(map.get_id(None), 0);
    }

    #[test]
    fn matches_scope_handles_dotted_prefix() {
        assert!(matches_scope("comment.line", "comment"));
        assert!(matches_scope("comment", "comment"));
        assert!(!matches_scope("commentary", "comment"));
    }

    #[test]
    fn theme_end_to_end_matches_scope_stack() {
        let settings = vec![
            RawThemeSetting {
                name: None,
                scope: ScopeField::Missing,
                settings: RawSettings {
                    foreground: Some("#c5c8c6".to_string()),
                    background: Some("#1d1f21".to_string()),
                    ..RawSettings::default()
                },
            },
            setting("comment", Some("#7c7c7c"), Some("italic")),
            setting("string", Some("#b5bd68"), None),
            setting("meta.function string.quoted", Some("#de935f"), None),
        ];
        let theme = Theme::create_from_raw(&settings, None);
        let defaults = theme.defaults().clone();
        assert_ne!(defaults.foreground_id, 0);

        let comment_stack =
            ScopeStack::from_segments(&["source.ts", "comment.line.double-slash"]).unwrap();
        let comment_style = theme.r#match(Some(&comment_stack)).unwrap();
        assert!(comment_style.font_style.contains(FontStyle::ITALIC));

        let nested_stack =
            ScopeStack::from_segments(&["source.ts", "meta.function", "string.quoted"]).unwrap();
        let nested_style = theme.r#match(Some(&nested_stack)).unwrap();
        // The parent-scope selector wins, so its foreground beats the
        // plain `string` rule's foreground.
        let plain_string_stack =
            ScopeStack::from_segments(&["source.ts", "string.quoted"]).unwrap();
        let plain_style = theme.r#match(Some(&plain_string_stack)).unwrap();
        assert_ne!(nested_style.foreground_id, plain_style.foreground_id);
    }
}
