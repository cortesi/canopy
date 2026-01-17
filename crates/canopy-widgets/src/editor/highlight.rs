use std::ops::Range;

use canopy::style::{Attr, AttrSet, Color, Paint, Style};
use syntect::{
    easy::HighlightLines,
    highlighting,
    highlighting::{FontStyle, Style as SyntectStyle, Theme, ThemeSet},
    parsing::{SyntaxReference, SyntaxSet},
};

/// A highlighted span for a single line.
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    /// Character range covered by the span.
    pub range: Range<usize>,
    /// Style to apply to the span.
    pub style: Style,
}

/// Trait for providing syntax highlighting spans.
pub trait Highlighter: Send {
    /// Return highlight spans for a line of text.
    fn highlight_line(&self, line: usize, text: &str) -> Vec<HighlightSpan>;
}

/// A basic syntect-backed highlighter.
#[derive(Debug, Clone)]
pub struct SyntectHighlighter {
    /// Loaded syntax set.
    syntax_set: SyntaxSet,
    /// Theme used for highlighting.
    theme: Theme,
    /// File extension hint for syntax selection.
    extension: String,
}

impl SyntectHighlighter {
    /// Construct a new syntect highlighter for the provided extension.
    pub fn new(extension: impl Into<String>) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let themes = ThemeSet::load_defaults();
        let theme = default_theme(&themes);
        Self {
            syntax_set,
            theme,
            extension: extension.into(),
        }
    }

    /// Construct a syntect highlighter with a named theme.
    pub fn with_theme_name(extension: impl Into<String>, theme_name: impl AsRef<str>) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .get(theme_name.as_ref())
            .cloned()
            .unwrap_or_else(|| default_theme(&themes));
        Self {
            syntax_set,
            theme,
            extension: extension.into(),
        }
    }

    /// Construct a syntect highlighter using a specific theme.
    pub fn with_theme(extension: impl Into<String>, theme: Theme) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        Self {
            syntax_set,
            theme,
            extension: extension.into(),
        }
    }

    /// Construct a highlighter using the plain text syntax.
    pub fn plain() -> Self {
        Self::new("txt")
    }

    /// Resolve the syntax definition for the configured extension.
    fn syntax(&self) -> SyntaxReference {
        self.syntax_set
            .find_syntax_by_extension(&self.extension)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
            .clone()
    }
}

impl Highlighter for SyntectHighlighter {
    fn highlight_line(&self, _line: usize, text: &str) -> Vec<HighlightSpan> {
        let syntax = self.syntax();
        let mut highlighter = HighlightLines::new(&syntax, &self.theme);
        let ranges = highlighter
            .highlight_line(text, &self.syntax_set)
            .unwrap_or_default();
        let mut spans = Vec::new();
        let mut char_offset = 0usize;
        for (style, slice) in ranges {
            let len = slice.chars().count();
            let range = char_offset..char_offset.saturating_add(len);
            char_offset = char_offset.saturating_add(len);
            spans.push(HighlightSpan {
                range,
                style: map_style(style),
            });
        }
        spans
    }
}

impl Default for SyntectHighlighter {
    fn default() -> Self {
        Self::plain()
    }
}

/// Return the default theme from the provided theme set.
fn default_theme(themes: &ThemeSet) -> Theme {
    themes
        .themes
        .get("Solarized (dark)")
        .cloned()
        .or_else(|| themes.themes.values().next().cloned())
        .unwrap_or_default()
}

/// Convert a syntect style to a canopy style.
fn map_style(style: SyntectStyle) -> Style {
    let attrs = map_attrs(style.font_style);
    Style {
        fg: Paint::solid(map_color(style.foreground)),
        bg: Paint::solid(map_color(style.background)),
        attrs,
    }
}

/// Convert a syntect color to a canopy color.
fn map_color(color: highlighting::Color) -> Color {
    Color::Rgb {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

/// Convert syntect font styles to canopy attributes.
fn map_attrs(style: FontStyle) -> AttrSet {
    let mut attrs = AttrSet::default();
    if style.contains(FontStyle::BOLD) {
        attrs = attrs.with(Attr::Bold);
    }
    if style.contains(FontStyle::ITALIC) {
        attrs = attrs.with(Attr::Italic);
    }
    if style.contains(FontStyle::UNDERLINE) {
        attrs = attrs.with(Attr::Underline);
    }
    attrs
}
