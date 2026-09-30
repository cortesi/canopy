//! Syntax highlighting helpers.
//!
//! [`Highlighter`](crate::highlight::Highlighter) and
//! [`HighlightSpan`](crate::highlight::HighlightSpan) carry no dependency of
//! their own: a host can implement highlighting however it likes.
//! `SyntectHighlighter`,
//! behind the `syntax` feature, resolves a syntax from a file name or from
//! the text itself, then highlights lines incrementally. Highlighting a line
//! needs the parser state left by every line above it, so the highlighter
//! walks forward from the last line it has seen and caches the spans it
//! produces. A source set through
//! [`Highlighter::prepare`](crate::highlight::Highlighter::prepare) therefore
//! costs only the lines that are actually asked for, and multi-line constructs
//! such as block comments keep their state. In a Markdown source, the body of
//! a fenced code block highlights in the language its fence names, as a file
//! in that language would.

use std::{borrow::Cow, ops::Range};

use canopy::{render::Render, style::Style};

/// How a highlight span paints.
#[derive(Debug, Clone)]
pub enum SpanStyle {
    /// A style the highlighter chose, as a syntax theme gives it.
    Fixed(Style),
    /// A style path, resolved in the theme where the span paints. A span
    /// with a path follows a theme switch, and takes the rules its host sets
    /// for the path, such as `syntax/keyword`.
    Path(Cow<'static, str>),
}

/// A highlighted span for a single line.
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    /// Character range covered by the span.
    pub range: Range<usize>,
    /// Style to apply to the span.
    pub style: SpanStyle,
}

impl HighlightSpan {
    /// Construct a span that paints `range` in `style`.
    pub fn fixed(range: Range<usize>, style: Style) -> Self {
        Self {
            range,
            style: SpanStyle::Fixed(style),
        }
    }

    /// Construct a span that paints `range` in the style that `path` resolves
    /// to where the span paints.
    pub fn path(range: Range<usize>, path: impl Into<Cow<'static, str>>) -> Self {
        Self {
            range,
            style: SpanStyle::Path(path.into()),
        }
    }

    /// Return the style of the span over the ground of `base`, with the
    /// render effects applied once.
    pub(crate) fn paint_style(&self, render: &Render, base: &Style) -> Style {
        match &self.style {
            SpanStyle::Fixed(style) => {
                let mut style = render.apply_effects(style.clone());
                style.bg = base.bg.clone();
                style
            }
            SpanStyle::Path(path) => {
                let mut style = render.resolve_style(path);
                style.bg = base.bg.clone();
                style
            }
        }
    }
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

#[cfg(feature = "syntax")]
pub use syntect_highlighter::{DEFAULT_THEME, SyntectHighlighter};

/// The syntect-backed [`Highlighter`], and everything it needs.
#[cfg(feature = "syntax")]
mod syntect_highlighter {
    use std::{
        cell::{Cell, RefCell},
        fmt, mem,
        path::Path,
        sync::OnceLock,
    };

    use canopy::style::{Attr, AttrSet, Color, Paint, Style, themes};
    use syntect::{
        easy::HighlightLines,
        highlighting,
        highlighting::{FontStyle, Style as SyntectStyle, Theme, ThemeSet},
        parsing::{SyntaxDefinition, SyntaxReference, SyntaxSet, SyntaxSetBuilder},
    };
    use two_face::syntax::extra_newlines;

    use super::{HighlightSpan, Highlighter};

    /// Syntax colours, apart from the style palette's named colours, which are
    /// too soft to tell code apart at a glance. The colours sit near one OKLCH
    /// lightness at high chroma, with hues spread so no two roles look alike.
    mod code {
        use canopy::{rgb, style::Color};

        /// Keywords and storage.
        pub(super) const PINK: Color = rgb!("#f472dc");
        /// Operators.
        pub(super) const CYAN: Color = rgb!("#58e0f6");
        /// Strings and inline code.
        pub(super) const GREEN: Color = rgb!("#81e86a");
        /// Escapes and regular expressions.
        pub(super) const TEAL: Color = rgb!("#4ae6d6");
        /// Numbers and constants.
        pub(super) const ORANGE: Color = rgb!("#ffa666");
        /// Functions, headings, and links.
        pub(super) const BLUE: Color = rgb!("#6fb6fe");
        /// Types.
        pub(super) const YELLOW: Color = rgb!("#f9d544");
        /// Parameters, language variables such as `self`, and tags.
        pub(super) const CORAL: Color = rgb!("#fe8b7d");
        /// Macros, attributes, and decorators.
        pub(super) const AQUA: Color = rgb!("#28d6df");
        /// Comments and fence markers, which recede.
        pub(super) const SLATE: Color = rgb!("#7a879c");
    }

    /// Theme used when the caller names none, matching the Canopy style theme.
    pub const DEFAULT_THEME: &str = "Canopy (dark)";

    /// Extensions a bundled syntax covers under another name.
    ///
    /// Each entry is a dialect close enough to its host grammar to read well.
    const SYNTAX_ALIASES: &[(&str, &str)] = &[("jsonc", "JSON"), ("json5", "JSON")];

    /// The Luau grammar, converted from the one GitHub and editors use.
    const LUAU_SYNTAX: &str = include_str!("../syntaxes/Luau.sublime-syntax");

    /// Fence names that no grammar claims, and the extensions they stand for.
    const FENCE_ALIASES: &[(&str, &str)] = &[
        ("shell", "sh"),
        ("console", "sh"),
        ("shell-session", "sh"),
        ("zsh", "sh"),
        ("c++", "cpp"),
        ("golang", "go"),
        ("patch", "diff"),
    ];

    /// Lines a prepared source will walk before it stops carrying parser state.
    ///
    /// Walking is linear, so an unbounded jump into a large file would stall a
    /// render. Past this point lines highlight without the state above them.
    const MAX_STATEFUL_LINES: usize = 4_000;

    /// A syntax, and the set that holds it.
    ///
    /// A syntax highlights only against its own set. Most syntaxes come from
    /// the extended set, and Luau from a set of its own.
    #[derive(Clone, Copy)]
    struct Grammar {
        /// The syntax.
        syntax: &'static SyntaxReference,
        /// The set that holds the syntax.
        set: &'static SyntaxSet,
    }

    impl Grammar {
        /// Return the grammar for text with no known type.
        fn plain() -> Self {
            let set = syntax_set();
            Self {
                syntax: set.find_syntax_plain_text(),
                set,
            }
        }

        /// Return whether this grammar leaves text plain.
        fn is_plain(self) -> bool {
            self.syntax.name == Self::plain().syntax.name
        }
    }

    /// Return the syntax definitions shared by every highlighter.
    ///
    /// These are the extended definitions, which cover about three times as
    /// many languages as syntect's own defaults, TOML and TypeScript among
    /// them. Loading them costs about thirty milliseconds, so they load
    /// once.
    fn syntax_set() -> &'static SyntaxSet {
        static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
        SYNTAXES.get_or_init(extra_newlines)
    }

    /// Return the extended grammar named `f` finds, if any.
    fn extended(
        find: impl FnOnce(&'static SyntaxSet) -> Option<&'static SyntaxReference>,
    ) -> Option<Grammar> {
        let set = syntax_set();
        find(set).map(|syntax| Grammar { syntax, set })
    }

    /// Return the Luau grammar, loaded on first use.
    ///
    /// Luau adds types, interpolated strings, and more to Lua, which the Lua
    /// grammar misreads. It stays out of the extended set because adding a
    /// syntax there relinks every other one, which costs far more than
    /// loading a set of one.
    fn luau() -> Grammar {
        static LUAU: OnceLock<SyntaxSet> = OnceLock::new();
        let set = LUAU.get_or_init(|| {
            let definition = SyntaxDefinition::load_from_str(LUAU_SYNTAX, true, None)
                .expect("the bundled Luau grammar loads");
            let mut builder = SyntaxSetBuilder::new();
            builder.add(definition);
            builder.build()
        });
        Grammar {
            syntax: &set.syntaxes()[0],
            set,
        }
    }

    /// Return the grammar for a file extension, consulting the alias table
    /// when no grammar claims the extension itself.
    fn grammar_for_extension(extension: &str) -> Option<Grammar> {
        if extension.eq_ignore_ascii_case("luau") {
            return Some(luau());
        }
        extended(|set| {
            set.find_syntax_by_extension(extension).or_else(|| {
                SYNTAX_ALIASES
                    .iter()
                    .find(|(alias, _)| alias.eq_ignore_ascii_case(extension))
                    .and_then(|(_, name)| set.find_syntax_by_name(name))
            })
        })
    }

    /// Return the grammar a fence's info string names, if one covers it.
    ///
    /// The language is the first word of the info string, and a comma also
    /// ends it, so `rust,ignore` names Rust. A word resolves as an extension,
    /// then as a syntax name. Plain text names no grammar.
    fn grammar_for_fence(info: &str) -> Option<Grammar> {
        let token = info
            .trim_start()
            .split(|c: char| c.is_whitespace() || c == ',')
            .next()
            .filter(|token| !token.is_empty())?;
        let token = FENCE_ALIASES
            .iter()
            .find(|(alias, _)| alias.eq_ignore_ascii_case(token))
            .map_or(token, |(_, extension)| extension);
        let grammar = grammar_for_extension(token)
            .or_else(|| extended(|set| set.find_syntax_by_token(token)))?;
        (!grammar.is_plain()).then_some(grammar)
    }

    /// Return the themes shared by every highlighter: syntect's defaults and
    /// [`DEFAULT_THEME`].
    fn theme_set() -> &'static ThemeSet {
        static THEMES: OnceLock<ThemeSet> = OnceLock::new();
        THEMES.get_or_init(|| {
            let mut themes = ThemeSet::load_defaults();
            themes
                .themes
                .insert(DEFAULT_THEME.to_string(), canopy_theme());
            themes
        })
    }

    /// Build the syntax theme that matches the Canopy style theme.
    ///
    /// Keywords are pink, functions blue, strings green, numbers and constants
    /// orange, and types yellow. The ground, text, and chrome come from the
    /// style palette.
    fn canopy_theme() -> Theme {
        let palette = themes::default_dark();
        let rules: &[(&str, Option<Color>, Option<FontStyle>)] = &[
            (
                "comment, punctuation.definition.comment",
                Some(code::SLATE),
                Some(FontStyle::ITALIC),
            ),
            (
                "keyword, storage, keyword.operator.word",
                Some(code::PINK),
                None,
            ),
            ("keyword.operator", Some(code::CYAN), None),
            (
                "string, punctuation.definition.string",
                Some(code::GREEN),
                None,
            ),
            (
                "constant.character.escape, string.regexp",
                Some(code::TEAL),
                None,
            ),
            // Code interpolated into a string reads as code, set off by
            // its delimiters.
            (
                "meta.embedded, meta.template.expression, meta.interpolation",
                Some(palette.fg),
                None,
            ),
            (
                "punctuation.definition.interpolated-string-expression, \
                 punctuation.definition.template-expression, \
                 punctuation.section.interpolation",
                Some(code::PINK),
                None,
            ),
            ("constant, support.constant", Some(code::ORANGE), None),
            (
                "entity.name.function, support.function, variable.function",
                Some(code::BLUE),
                None,
            ),
            (
                "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, \
                 entity.name.trait, entity.name.union, entity.other.inherited-class, \
                 support.type, support.class",
                Some(code::YELLOW),
                None,
            ),
            (
                "variable.language, variable.parameter",
                Some(code::CORAL),
                None,
            ),
            (
                "support.macro, entity.name.macro, meta.annotation, meta.attribute",
                Some(code::AQUA),
                None,
            ),
            ("entity.name.tag", Some(code::CORAL), None),
            ("entity.other.attribute-name", Some(code::YELLOW), None),
            (
                "markup.heading, entity.name.section",
                Some(code::BLUE),
                Some(FontStyle::BOLD),
            ),
            ("markup.bold", None, Some(FontStyle::BOLD)),
            ("markup.italic", None, Some(FontStyle::ITALIC)),
            (
                "string.other.link, meta.link.inline.description, \
                 meta.image.inline.description",
                Some(code::BLUE),
                None,
            ),
            ("markup.underline.link", Some(code::SLATE), None),
            (
                "punctuation.definition.list_item, markup.list.numbered.bullet",
                Some(code::ORANGE),
                None,
            ),
            ("entity.name.table", Some(code::BLUE), None),
            ("markup.inserted", Some(code::GREEN), None),
            ("markup.deleted, invalid", Some(code::CORAL), None),
            ("markup.changed", Some(code::YELLOW), None),
            ("markup.raw", Some(code::GREEN), None),
            ("markup.quote", Some(code::YELLOW), Some(FontStyle::ITALIC)),
            (
                "punctuation.definition.raw.code-fence",
                Some(code::SLATE),
                None,
            ),
            ("meta.diff.header, meta.diff.range", Some(code::BLUE), None),
        ];
        let scopes = rules
            .iter()
            .map(|&(scope, foreground, font_style)| highlighting::ThemeItem {
                scope: scope.parse().expect("canopy theme scopes parse"),
                style: highlighting::StyleModifier {
                    foreground: foreground.map(syntect_color),
                    background: None,
                    font_style,
                },
            })
            .collect();
        Theme {
            name: Some(DEFAULT_THEME.to_string()),
            settings: highlighting::ThemeSettings {
                foreground: Some(syntect_color(palette.fg)),
                background: Some(syntect_color(palette.bg)),
                caret: Some(syntect_color(palette.accent)),
                selection: Some(syntect_color(palette.element_bg)),
                gutter_foreground: Some(syntect_color(palette.line_number)),
                ..highlighting::ThemeSettings::default()
            },
            scopes,
            ..Theme::default()
        }
    }

    /// Convert a canopy color to an opaque syntect color.
    fn syntect_color(color: Color) -> highlighting::Color {
        let (r, g, b) = color.rgb();
        highlighting::Color {
            r,
            g,
            b,
            a: u8::MAX,
        }
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

    /// A highlighter partway through a source, with the set its grammar
    /// highlights against.
    struct Engine {
        /// The highlighter, positioned after the last line it saw.
        lines: HighlightLines<'static>,
        /// The set that holds the highlighter's syntax.
        set: &'static SyntaxSet,
    }

    impl Engine {
        /// Start highlighting a source in `grammar` and `theme`.
        fn new(grammar: Grammar, theme: &'static Theme) -> Self {
            Self {
                lines: HighlightLines::new(grammar.syntax, theme),
                set: grammar.set,
            }
        }

        /// Highlight the next line.
        fn spans(&mut self, text: &str) -> Vec<HighlightSpan> {
            let ranges = self
                .lines
                .highlight_line(text, self.set)
                .unwrap_or_default();
            spans_from(&ranges)
        }
    }

    /// A fenced code block open in a Markdown source.
    struct Fence {
        /// The fence character, a backtick or a tilde.
        marker: char,
        /// The length of the opening run. A run at least this long closes the
        /// fence.
        len: usize,
        /// Highlighter for the body, when a grammar covers the fence's
        /// language.
        engine: Option<Engine>,
    }

    impl Fence {
        /// Return the fence that `line` opens, if it opens one.
        ///
        /// A run of three or more backticks or tildes opens a fence. Any indent
        /// may come before it, so a fence inside a list item opens too. The
        /// info string after a run of backticks holds no backtick.
        fn open(line: &str, theme: &'static Theme) -> Option<Self> {
            let rest = line.trim_start();
            let marker = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
            let len = rest.chars().take_while(|&c| c == marker).count();
            // The marker is one byte long, so the run ends at byte `len`.
            let info = &rest[len..];
            if len < 3 || (marker == '`' && info.contains('`')) {
                return None;
            }
            Some(Self {
                marker,
                len,
                engine: grammar_for_fence(info).map(|grammar| Engine::new(grammar, theme)),
            })
        }

        /// Return whether `line` closes the fence.
        fn closed_by(&self, line: &str) -> bool {
            let rest = line.trim_start();
            let run = rest.chars().take_while(|&c| c == self.marker).count();
            run >= self.len && rest[run..].trim().is_empty()
        }
    }

    /// One source text and the highlighting walked over it so far.
    struct Source {
        /// Grammar resolved for this source, including an inferred first-line
        /// hint.
        grammar: Grammar,
        /// Lines of the source, including the trailing newline syntect expects.
        lines: Vec<String>,
        /// Theme the spans take their styles from.
        theme: &'static Theme,
        /// Whether the source is Markdown, whose fences highlight their bodies
        /// in their own languages.
        markdown: bool,
        /// Highlighter positioned after the last cached line.
        engine: Engine,
        /// The fence the walk is inside, in a Markdown source.
        fence: Option<Fence>,
        /// Spans for every line walked so far.
        spans: Vec<Vec<HighlightSpan>>,
    }

    impl Source {
        /// Hold `lines` to highlight in `grammar` and `theme`.
        fn new(grammar: Grammar, lines: Vec<String>, theme: &'static Theme) -> Self {
            Self {
                grammar,
                lines,
                theme,
                markdown: grammar
                    .syntax
                    .scope
                    .build_string()
                    .starts_with("text.html.markdown"),
                engine: Engine::new(grammar, theme),
                fence: None,
                spans: Vec::new(),
            }
        }

        /// Highlight in `theme`, walking again from the first line.
        fn restart(&mut self, theme: &'static Theme) {
            *self = Self::new(self.grammar, mem::take(&mut self.lines), theme);
        }

        /// Walk forward until `line` is cached, and return its spans.
        ///
        /// Returns `None` when the line lies outside the source or beyond the
        /// stateful walking limit.
        fn spans_for(&mut self, line: usize) -> Option<Vec<HighlightSpan>> {
            if line >= self.lines.len() {
                return None;
            }
            while self.spans.len() <= line {
                self.walk_line();
            }
            self.spans.get(line).cloned()
        }

        /// Highlight the next line, and cache its spans.
        ///
        /// The body of a fence goes to the fence's own highlighter, when a
        /// syntax covers its language, and the source's highlighter never sees
        /// it. The fence lines themselves, and a body in no known language,
        /// stay with the source's highlighter.
        fn walk_line(&mut self) {
            let text = &self.lines[self.spans.len()];
            let in_body = match &self.fence {
                Some(fence) if fence.closed_by(text) => {
                    self.fence = None;
                    false
                }
                Some(_) => true,
                None => {
                    if self.markdown {
                        self.fence = Fence::open(text, self.theme);
                    }
                    false
                }
            };
            let engine = match &mut self.fence {
                Some(Fence {
                    engine: Some(engine),
                    ..
                }) if in_body => engine,
                _ => &mut self.engine,
            };
            let spans = engine.spans(text);
            self.spans.push(spans);
        }
    }

    /// A syntect-backed highlighter.
    ///
    /// Language detection uses the file name or extension, then the first
    /// source line. Parser state is retained for at most 4,000 lines; later
    /// lines highlight independently.
    pub struct SyntectHighlighter {
        /// Theme used for highlighting.
        theme: &'static Theme,
        /// Grammar selected by the file name or extension, before source
        /// detection.
        grammar: Cell<Grammar>,
        /// The source being highlighted, once one is prepared.
        source: RefCell<Option<Source>>,
    }

    impl SyntectHighlighter {
        /// Construct a highlighter for the provided file extension.
        #[must_use]
        pub fn new(extension: impl AsRef<str>) -> Self {
            let highlighter = Self {
                theme: theme(DEFAULT_THEME),
                grammar: Cell::new(Grammar::plain()),
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
                source.restart(self.theme);
            }
            self
        }

        /// Select the syntax for `extension`, discarding any prepared source.
        pub fn set_extension(&self, extension: &str) {
            self.set_grammar(grammar_for_extension(extension).unwrap_or_else(Grammar::plain));
        }

        /// Select the syntax for `path`, discarding any prepared source.
        ///
        /// A file name is tried first, so `Makefile` and `Dockerfile` resolve,
        /// then the extension.
        pub fn set_path(&self, path: impl AsRef<Path>) {
            let path = path.as_ref();
            let grammar = path
                .file_name()
                .and_then(|name| {
                    extended(|set| set.find_syntax_by_extension(&name.to_string_lossy()))
                })
                .or_else(|| {
                    path.extension()
                        .and_then(|ext| grammar_for_extension(&ext.to_string_lossy()))
                })
                .unwrap_or_else(Grammar::plain);
            self.set_grammar(grammar);
        }

        /// Return the name of the syntax in use.
        #[must_use]
        pub fn syntax_name(&self) -> String {
            self.current_grammar().syntax.name.clone()
        }

        /// Return the prepared source grammar, or the configured hint before
        /// preparation.
        fn current_grammar(&self) -> Grammar {
            self.source
                .borrow()
                .as_ref()
                .map_or(self.grammar.get(), |source| source.grammar)
        }

        /// Install `grammar` and drop state built with the previous one.
        fn set_grammar(&self, grammar: Grammar) {
            self.grammar.set(grammar);
            self.source.replace(None);
        }
    }

    impl Highlighter for SyntectHighlighter {
        fn prepare(&self, text: &str) {
            // A plain-text syntax may still be a recognizable script, so
            // consult the first line before committing to it.
            let hint = self.grammar.get();
            let grammar = if hint.is_plain() {
                extended(|set| {
                    set.find_syntax_by_first_line(text.lines().next().unwrap_or_default())
                })
                .unwrap_or(hint)
            } else {
                hint
            };
            let lines = text
                .split_inclusive('\n')
                .take(MAX_STATEFUL_LINES)
                .map(str::to_string)
                .collect::<Vec<_>>();
            *self.source.borrow_mut() = Some(Source::new(grammar, lines, self.theme));
        }

        fn highlight_line(&self, line: usize, text: &str) -> Vec<HighlightSpan> {
            if let Some(source) = self.source.borrow_mut().as_mut()
                && let Some(spans) = source.spans_for(line)
            {
                return spans;
            }
            // No prepared source, or a line beyond it: highlight on its own.
            Engine::new(self.current_grammar(), self.theme).spans(text)
        }
    }

    impl fmt::Debug for SyntectHighlighter {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("SyntectHighlighter")
                .field("syntax", &self.current_grammar().syntax.name)
                .finish_non_exhaustive()
        }
    }

    /// Convert syntect's styled slices into character-ranged spans.
    fn spans_from(ranges: &[(SyntectStyle, &str)]) -> Vec<HighlightSpan> {
        let mut spans = Vec::with_capacity(ranges.len());
        let mut offset = 0usize;
        for (style, slice) in ranges {
            // Trailing newlines are part of the slice but not of the rendered
            // line.
            let len = slice.trim_end_matches(['\n', '\r']).chars().count();
            if len == 0 {
                continue;
            }
            let range = offset..offset.saturating_add(len);
            offset = offset.saturating_add(len);
            spans.push(HighlightSpan::fixed(range, map_style(*style)));
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
        use crate::highlight::SpanStyle;

        /// Return the foreground colors of a line's spans.
        fn colors(highlighter: &SyntectHighlighter, line: usize, text: &str) -> Vec<Paint> {
            highlighter
                .highlight_line(line, text)
                .into_iter()
                .map(|span| match span.style {
                    SpanStyle::Fixed(style) => style.fg,
                    SpanStyle::Path(path) => panic!("syntect spans are fixed, not {path}"),
                })
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
            // A file browser previews whatever a project holds, so the set has
            // to reach past syntect's own defaults.
            for (path, expected) in [
                ("main.rs", "Rust"),
                ("Cargo.toml", "TOML"),
                ("Cargo.lock", "TOML"),
                ("init.luau", "Luau"),
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
        fn fence_names_resolve() {
            for (info, expected) in [
                ("rust", "Rust"),
                ("rust,ignore", "Rust"),
                ("Python extra words", "Python"),
                (" luau", "Luau"),
                ("shell", "Bourne Again Shell (bash)"),
                ("console", "Bourne Again Shell (bash)"),
                ("c++", "C++"),
                ("golang", "Go"),
                ("toml", "TOML"),
                ("diff", "Diff"),
            ] {
                assert_eq!(
                    grammar_for_fence(info).map(|grammar| grammar.syntax.name.as_str()),
                    Some(expected),
                    "a `{info}` fence should highlight as {expected}"
                );
            }
            for info in ["", "nosuchlang", "text"] {
                assert!(
                    grammar_for_fence(info).is_none(),
                    "a `{info}` fence has no syntax"
                );
            }
        }

        /// Return the colors of `text` highlighted as the first line of a file
        /// with `extension`.
        fn file_colors(extension: &str, text: &str) -> Vec<Paint> {
            let highlighter = SyntectHighlighter::new(extension);
            highlighter.prepare(&format!("{text}\n"));
            colors(&highlighter, 0, text)
        }

        #[test]
        fn markdown_fences_highlight_in_their_language() {
            let highlighter = SyntectHighlighter::for_path("README.md");
            highlighter.prepare(
                "# Notes\n\n```luau\nlocal x = \"s\" -- note\n```\n\n\
                 ~~~rust,ignore\nfn main() { let x = y; }\n~~~\n\n\
                 - Step:\n\n  ```toml\n  a = 1\n  ```\n",
            );
            for (line, text, extension) in [
                (3, "local x = \"s\" -- note", "luau"),
                (7, "fn main() { let x = y; }", "rs"),
                (13, "  a = 1", "toml"),
            ] {
                assert_eq!(
                    colors(&highlighter, line, text),
                    file_colors(extension, text),
                    "a {extension} fence should highlight as a {extension} file"
                );
            }
        }

        #[test]
        fn markdown_resumes_after_a_fence() {
            let highlighter = SyntectHighlighter::for_path("notes.md");
            // The body leaves a string open, which must end with the fence.
            highlighter.prepare("```rust\nlet s = \"open\n```\n# After\n");
            assert_eq!(
                colors(&highlighter, 3, "# After"),
                file_colors("md", "# After")
            );
        }

        #[test]
        fn only_a_long_enough_run_closes_a_fence() {
            let highlighter = SyntectHighlighter::for_path("notes.md");
            highlighter.prepare("````rust\n```\nlet x = 1;\n````\n# After\n");
            assert_eq!(
                colors(&highlighter, 2, "let x = 1;"),
                file_colors("rs", "let x = 1;"),
                "a shorter run leaves the fence open"
            );
            assert_eq!(
                colors(&highlighter, 4, "# After"),
                file_colors("md", "# After")
            );
        }

        #[test]
        fn a_fence_in_no_known_language_stays_markdown() {
            let highlighter = SyntectHighlighter::for_path("notes.md");
            highlighter.prepare("```nosuchlang\nlet x = 1;\n```\n");
            let raw = SyntectHighlighter::for_path("notes.md");
            raw.prepare("```\nlet x = 1;\n```\n");
            assert_eq!(
                colors(&highlighter, 1, "let x = 1;"),
                colors(&raw, 1, "let x = 1;")
            );
        }

        /// Return the color of the character at `column` in `text`, as line
        /// `line` of `highlighter`'s source.
        fn color_at(
            highlighter: &SyntectHighlighter,
            line: usize,
            text: &str,
            column: usize,
        ) -> Option<Paint> {
            highlighter
                .highlight_line(line, text)
                .into_iter()
                .find(|span| span.range.contains(&column))
                .map(|span| match span.style {
                    SpanStyle::Fixed(style) => style.fg,
                    SpanStyle::Path(path) => panic!("syntect spans are fixed, not {path}"),
                })
        }

        /// Return the column where `needle` starts in `text`.
        fn column(text: &str, needle: &str) -> usize {
            let byte = text.find(needle).expect("the needle is in the text");
            text[..byte].chars().count()
        }

        #[test]
        fn luau_highlights_its_own_syntax() {
            let lines = [
                "export type Entry = { name: string }",
                "local count: number = 0",
                "local s = `{count} items`",
                "local keys = { key = \"j\" }",
                "continue",
            ];
            let highlighter = SyntectHighlighter::for_path("init.luau");
            highlighter.prepare(&(lines.join("\n") + "\n"));
            let at = |line: usize, needle: &str| {
                let text = lines[line];
                color_at(&highlighter, line, text, column(text, needle))
            };
            let keyword = at(0, "export");
            assert_eq!(at(0, "type"), keyword, "`type` declares a type");
            assert_eq!(at(4, "continue"), keyword, "`continue` is a keyword");
            let plain = at(1, "count");
            assert_ne!(at(1, "number"), plain, "an annotation shows its type");
            let string = at(2, "items");
            assert_ne!(at(2, "count"), string, "interpolated code is not string");
            assert_ne!(at(3, "key"), at(3, "\"j\""), "a table key is not string");
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
}
