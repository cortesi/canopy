//! Syntax highlighting helpers.
//!
//! [`SyntectHighlighter`] resolves a syntax from a file name or from the text
//! itself, then highlights lines incrementally. Highlighting a line needs the
//! parser state left by every line above it, so the highlighter walks forward
//! from the last line it has seen and caches the spans it produces. A source
//! set through [`Highlighter::prepare`] therefore costs only the lines that are
//! actually asked for, and multi-line constructs such as block comments keep
//! their state.

use std::{
    cell::{Cell, RefCell},
    fmt,
    ops::Range,
    path::Path,
    sync::OnceLock,
};

use canopy::style::{Attr, AttrSet, Color, Paint, Style};
use syntect::{
    easy::HighlightLines,
    highlighting,
    highlighting::{FontStyle, Style as SyntectStyle, Theme, ThemeSet},
    parsing::{SyntaxReference, SyntaxSet},
};
use two_face::syntax::extra_newlines;

/// Theme used when the caller names none.
pub const DEFAULT_THEME: &str = "Solarized (dark)";

/// Extensions a bundled syntax covers under another name.
///
/// Each entry is a dialect close enough to its host grammar to read well.
const SYNTAX_ALIASES: &[(&str, &str)] = &[
    // Luau is a Lua dialect. The Lua grammar covers everything but its type
    // annotations.
    ("luau", "Lua"),
    ("jsonc", "JSON"),
    ("json5", "JSON"),
];

/// Lines a prepared source will walk before it stops carrying parser state.
///
/// Walking is linear, so an unbounded jump into a large file would stall a
/// render. Past this point lines highlight without the state above them.
const MAX_STATEFUL_LINES: usize = 4_000;

/// A highlighted span for a single line.
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    /// Character range covered by the span.
    pub range: Range<usize>,
    /// Style to apply to the span.
    pub style: Style,
}

/// Trait for providing syntax highlighting spans.
pub trait Highlighter {
    /// Adopt `text` as the source being highlighted, discarding earlier state.
    ///
    /// Highlighters that carry parser state between lines need the whole
    /// source. The default implementation ignores it.
    /// The editor calls this during rendering, before requesting spans, when
    /// its source or highlighter has changed.
    fn prepare(&self, text: &str) {
        let _ = text;
    }

    /// Return highlight spans for a line of text.
    fn highlight_line(&self, line: usize, text: &str) -> Vec<HighlightSpan>;
}

/// Return the syntax definitions shared by every highlighter.
///
/// These are the extended definitions, which cover about three times as many
/// languages as syntect's own defaults, TOML and TypeScript among them. Loading
/// them costs about thirty milliseconds, so they load once.
fn syntax_set() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(extra_newlines)
}

/// Return the syntax for a file extension, consulting the alias table when no
/// grammar claims the extension itself.
fn syntax_for_extension(extension: &str) -> Option<&'static SyntaxReference> {
    let syntaxes = syntax_set();
    syntaxes.find_syntax_by_extension(extension).or_else(|| {
        SYNTAX_ALIASES
            .iter()
            .find(|(alias, _)| alias.eq_ignore_ascii_case(extension))
            .and_then(|(_, name)| syntaxes.find_syntax_by_name(name))
    })
}

/// Return the themes shared by every highlighter.
fn theme_set() -> &'static ThemeSet {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    THEMES.get_or_init(ThemeSet::load_defaults)
}

/// Return a theme by name, falling back to any available theme.
fn theme(name: &str) -> &'static Theme {
    let themes = theme_set();
    themes
        .themes
        .get(name)
        .or_else(|| themes.themes.get(DEFAULT_THEME))
        .or_else(|| themes.themes.values().next())
        .expect("syntect ships default themes")
}

/// One source text and the highlighting walked over it so far.
struct Source {
    /// Syntax resolved for this source, including an inferred first-line hint.
    syntax: &'static SyntaxReference,
    /// Lines of the source, including the trailing newline syntect expects.
    lines: Vec<String>,
    /// Highlighter positioned after the last cached line.
    engine: HighlightLines<'static>,
    /// Spans for every line walked so far.
    spans: Vec<Vec<HighlightSpan>>,
}

impl Source {
    /// Walk forward until `line` is cached, and return its spans.
    ///
    /// Returns `None` when the line lies outside the source or beyond the
    /// stateful walking limit.
    fn spans_for(&mut self, line: usize) -> Option<Vec<HighlightSpan>> {
        if line >= self.lines.len() {
            return None;
        }
        while self.spans.len() <= line {
            let next = self.spans.len();
            let text = &self.lines[next];
            let ranges = self
                .engine
                .highlight_line(text, syntax_set())
                .unwrap_or_default();
            self.spans.push(spans_from(&ranges));
        }
        self.spans.get(line).cloned()
    }
}

/// A syntect-backed highlighter.
///
/// Language detection uses the file name or extension, then the first source
/// line. Parser state is retained for at most 4,000 lines; later lines
/// highlight independently.
pub struct SyntectHighlighter {
    /// Theme used for highlighting.
    theme: &'static Theme,
    /// Syntax selected by the file name or extension, before source detection.
    syntax: Cell<&'static SyntaxReference>,
    /// The source being highlighted, once one is prepared.
    source: RefCell<Option<Source>>,
}

impl SyntectHighlighter {
    /// Construct a highlighter for the provided file extension.
    #[must_use]
    pub fn new(extension: impl AsRef<str>) -> Self {
        let highlighter = Self {
            theme: theme(DEFAULT_THEME),
            syntax: Cell::new(plain_text()),
            source: RefCell::new(None),
        };
        highlighter.set_extension(extension.as_ref());
        highlighter
    }

    /// Construct a highlighter for the file at `path`.
    #[must_use]
    pub fn for_path(path: impl AsRef<Path>) -> Self {
        let highlighter = Self::new("");
        highlighter.set_path(path);
        highlighter
    }

    /// Use the named theme, falling back to [`DEFAULT_THEME`].
    #[must_use]
    pub fn with_theme(mut self, name: impl AsRef<str>) -> Self {
        self.theme = theme(name.as_ref());
        if let Some(source) = self.source.get_mut().as_mut() {
            source.engine = HighlightLines::new(source.syntax, self.theme);
            source.spans.clear();
        }
        self
    }

    /// Select the syntax for `extension`, discarding any prepared source.
    pub fn set_extension(&self, extension: &str) {
        self.set_syntax(syntax_for_extension(extension).unwrap_or_else(plain_text));
    }

    /// Select the syntax for `path`, discarding any prepared source.
    ///
    /// A file name is tried first, so `Makefile` and `Dockerfile` resolve, then
    /// the extension.
    pub fn set_path(&self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        let syntax = path
            .file_name()
            .and_then(|name| syntax_set().find_syntax_by_extension(&name.to_string_lossy()))
            .or_else(|| {
                path.extension()
                    .and_then(|ext| syntax_for_extension(&ext.to_string_lossy()))
            })
            .unwrap_or_else(plain_text);
        self.set_syntax(syntax);
    }

    /// Return the name of the syntax in use.
    #[must_use]
    pub fn syntax_name(&self) -> String {
        self.current_syntax().name.clone()
    }

    /// Return the prepared source syntax, or the configured hint before
    /// preparation.
    fn current_syntax(&self) -> &'static SyntaxReference {
        self.source
            .borrow()
            .as_ref()
            .map_or(self.syntax.get(), |source| source.syntax)
    }

    /// Install `syntax` and drop state built with the previous one.
    fn set_syntax(&self, syntax: &'static SyntaxReference) {
        self.syntax.set(syntax);
        self.source.replace(None);
    }
}

impl Highlighter for SyntectHighlighter {
    fn prepare(&self, text: &str) {
        // A plain-text syntax may still be a recognizable script, so consult
        // the first line before committing to it.
        let hint = self.syntax.get();
        let syntax = if hint.name == plain_text().name {
            syntax_set()
                .find_syntax_by_first_line(text.lines().next().unwrap_or_default())
                .unwrap_or(hint)
        } else {
            hint
        };
        let lines = text
            .split_inclusive('\n')
            .take(MAX_STATEFUL_LINES)
            .map(str::to_string)
            .collect::<Vec<_>>();
        *self.source.borrow_mut() = Some(Source {
            syntax,
            lines,
            engine: HighlightLines::new(syntax, self.theme),
            spans: Vec::new(),
        });
    }

    fn highlight_line(&self, line: usize, text: &str) -> Vec<HighlightSpan> {
        if let Some(source) = self.source.borrow_mut().as_mut()
            && let Some(spans) = source.spans_for(line)
        {
            return spans;
        }
        // No prepared source, or a line beyond it: highlight on its own.
        let mut engine = HighlightLines::new(self.current_syntax(), self.theme);
        let ranges = engine
            .highlight_line(text, syntax_set())
            .unwrap_or_default();
        spans_from(&ranges)
    }
}

impl fmt::Debug for SyntectHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntectHighlighter")
            .field("syntax", &self.current_syntax().name)
            .finish_non_exhaustive()
    }
}

/// Return the syntax used for text with no known type.
fn plain_text() -> &'static SyntaxReference {
    syntax_set().find_syntax_plain_text()
}

/// Convert syntect's styled slices into character-ranged spans.
fn spans_from(ranges: &[(SyntectStyle, &str)]) -> Vec<HighlightSpan> {
    let mut spans = Vec::with_capacity(ranges.len());
    let mut offset = 0usize;
    for (style, slice) in ranges {
        // Trailing newlines are part of the slice but not of the rendered line.
        let len = slice.trim_end_matches(['\n', '\r']).chars().count();
        if len == 0 {
            continue;
        }
        let range = offset..offset.saturating_add(len);
        offset = offset.saturating_add(len);
        spans.push(HighlightSpan {
            range,
            style: map_style(*style),
        });
    }
    spans
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Return the foreground colors of a line's spans.
    fn colors(highlighter: &SyntectHighlighter, line: usize, text: &str) -> Vec<Paint> {
        highlighter
            .highlight_line(line, text)
            .into_iter()
            .map(|span| span.style.fg)
            .collect()
    }

    #[test]
    fn block_comment_state_carries_between_lines() {
        let source = "/* one\ntwo */\nlet x = 1;\n";
        let highlighter = SyntectHighlighter::new("rs");
        highlighter.prepare(source);

        let opener = colors(&highlighter, 0, "/* one");
        let inside = colors(&highlighter, 1, "two */");
        assert_eq!(
            opener.first(),
            inside.first(),
            "a continued block comment keeps the comment color"
        );

        let code = colors(&highlighter, 2, "let x = 1;");
        assert_ne!(
            code.first(),
            inside.first(),
            "code after the comment is styled as code"
        );
    }

    #[test]
    fn the_common_languages_have_a_syntax() {
        // A file browser previews whatever a project holds, so the set has to
        // reach past syntect's own defaults.
        for (path, expected) in [
            ("main.rs", "Rust"),
            ("Cargo.toml", "TOML"),
            ("Cargo.lock", "TOML"),
            ("init.luau", "Lua"),
            ("conf.lua", "Lua"),
            ("app.ts", "TypeScript"),
            ("view.tsx", "TypeScriptReact"),
            ("main.zig", "Zig"),
            ("shell.nix", "Nix"),
            ("main.go", "Go"),
            ("api.py", "Python"),
            ("run.sh", "Bourne Again Shell (bash)"),
            ("data.json", "JSON"),
            ("tsconfig.jsonc", "JSON"),
            ("config.yaml", "YAML"),
            ("README.md", "Markdown"),
            ("Dockerfile", "Dockerfile"),
            ("Makefile", "Makefile"),
            (".gitignore", "Git Ignore"),
            ("query.sql", "SQL"),
            ("main.c", "C"),
            ("lib.cpp", "C++"),
            ("App.swift", "Swift"),
            ("main.rb", "Ruby"),
            ("infra.tf", "Terraform"),
            ("schema.graphql", "GraphQL"),
        ] {
            assert_eq!(
                SyntectHighlighter::for_path(path).syntax_name(),
                expected,
                "{path} should highlight as {expected}"
            );
        }
    }

    #[test]
    fn an_unknown_extension_falls_back_to_plain_text() {
        assert_eq!(
            SyntectHighlighter::for_path("mystery.zzzz").syntax_name(),
            "Plain Text"
        );
    }

    #[test]
    fn syntax_resolves_by_name_extension_and_first_line() {
        let by_extension = SyntectHighlighter::for_path("src/main.rs");
        assert_eq!(by_extension.syntax_name(), "Rust");

        let by_name = SyntectHighlighter::for_path("project/Makefile");
        assert_eq!(by_name.syntax_name(), "Makefile");

        let unknown = SyntectHighlighter::for_path("script");
        assert_eq!(unknown.syntax_name(), "Plain Text");
        unknown.prepare("#!/bin/bash\necho hi\n");
        assert_eq!(
            unknown.syntax_name(),
            "Bourne Again Shell (bash)",
            "a shebang names the syntax when the file name does not"
        );
    }

    #[test]
    fn syntax_detection_only_uses_the_first_line() {
        let highlighter = SyntectHighlighter::new("");
        highlighter.prepare("Ordinary notes\n#!/usr/bin/env python3\nprint('hello')\n");
        assert_eq!(highlighter.syntax_name(), "Plain Text");
    }

    #[test]
    fn preparing_a_new_source_rechecks_inferred_syntax() {
        let highlighter = SyntectHighlighter::new("");
        highlighter.prepare("#!/bin/bash\necho hello\n");
        assert_eq!(highlighter.syntax_name(), "Bourne Again Shell (bash)");
        highlighter.prepare("#!/usr/bin/env python3\nprint('hello')\n");
        assert_eq!(highlighter.syntax_name(), "Python");
        highlighter.prepare("Ordinary notes\n");
        assert_eq!(highlighter.syntax_name(), "Plain Text");

        highlighter.set_extension("rs");
        highlighter.prepare("#!/usr/bin/env python3\n");
        assert_eq!(highlighter.syntax_name(), "Rust", "the explicit hint wins");
    }

    #[test]
    fn changing_theme_preserves_the_prepared_source() {
        let source = "/* a comment\nstill inside */\n";
        let highlighter = SyntectHighlighter::new("rs");
        highlighter.prepare(source);
        let _before = colors(&highlighter, 1, "still inside */");
        let highlighter = highlighter.with_theme("Solarized (light)");

        let expected = SyntectHighlighter::new("rs").with_theme("Solarized (light)");
        expected.prepare(source);
        assert_eq!(
            colors(&highlighter, 1, "still inside */"),
            colors(&expected, 1, "still inside */")
        );
    }

    #[test]
    fn preparing_large_sources_only_retains_stateful_lines() {
        let highlighter = SyntectHighlighter::new("rs");
        let source = "// a source line\n".repeat(MAX_STATEFUL_LINES * 10);
        highlighter.prepare(&source);
        {
            let prepared = highlighter.source.borrow();
            let prepared = prepared.as_ref().expect("source was prepared");
            assert_eq!(prepared.lines.len(), MAX_STATEFUL_LINES);
            assert!(
                prepared.spans.is_empty(),
                "preparation must not parse lines"
            );
        }
        assert_eq!(
            colors(&highlighter, MAX_STATEFUL_LINES + 1, "let x = 1;"),
            colors(&SyntectHighlighter::new("rs"), 0, "let x = 1;"),
            "uncached lines retain stateless highlighting"
        );
    }

    #[test]
    fn preparing_a_new_source_discards_earlier_state() {
        let highlighter = SyntectHighlighter::new("rs");
        highlighter.prepare("/* open\nstill inside\n");
        let inside = colors(&highlighter, 1, "still inside");

        highlighter.prepare("let x = 1;\nstill inside\n");
        let plain = colors(&highlighter, 1, "still inside");
        assert_ne!(inside.first(), plain.first());
    }

    #[test]
    fn lines_outside_a_prepared_source_still_highlight() {
        let highlighter = SyntectHighlighter::new("rs");
        highlighter.prepare("let x = 1;\n");
        assert!(
            !highlighter.highlight_line(9, "let y = 2;").is_empty(),
            "a line past the source falls back to stateless highlighting"
        );
    }

    #[test]
    fn spans_cover_the_line_without_its_newline() {
        let highlighter = SyntectHighlighter::new("rs");
        let text = "let x = 1;";
        highlighter.prepare(&format!("{text}\n"));
        let spans = highlighter.highlight_line(0, text);
        let end = spans
            .last()
            .expect("a highlighted line has spans")
            .range
            .end;
        assert_eq!(end, text.chars().count());
    }
}
