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
//! in that language would. `Grammar` resolves a fence's language to a syntect
//! grammar for hosts that parse code themselves, and `syntect` is re-exported
//! so they parse with the same version.

use std::{borrow::Cow, ops::Range};

use canopy::{
    render::Render,
    style::{AttrSet, PartialStyle, Style, StyleRules, themes::Palette},
};

/// How a highlight span paints.
#[derive(Debug, Clone, PartialEq)]
pub enum SpanStyle {
    /// A style the highlighter chose, as a syntax theme gives it.
    Fixed(Style),
    /// A style path, resolved in the theme where the span paints. A span
    /// with a path follows a theme switch, and takes the rules its host sets
    /// for the path, such as `syntax/keyword`.
    Path(Cow<'static, str>),
}

/// Generate borrowed role paths for all syntect font-attribute combinations.
macro_rules! palette_paths {
    ($role:literal) => {
        [
            concat!("syntax/", $role, "/plain"),
            concat!("syntax/", $role, "/bold"),
            concat!("syntax/", $role, "/italic"),
            concat!("syntax/", $role, "/bold-italic"),
            concat!("syntax/", $role, "/underline"),
            concat!("syntax/", $role, "/bold-underline"),
            concat!("syntax/", $role, "/italic-underline"),
            concat!("syntax/", $role, "/bold-italic-underline"),
        ]
    };
}

/// Shared role order used by palette registration and built-in classification.
const PALETTE_PATHS: [[&str; 8]; 11] = [
    palette_paths!("text"),
    palette_paths!("keyword"),
    palette_paths!("operator"),
    palette_paths!("string"),
    palette_paths!("escape"),
    palette_paths!("constant"),
    palette_paths!("function"),
    palette_paths!("type"),
    palette_paths!("variable"),
    palette_paths!("macro"),
    palette_paths!("comment"),
];

/// Install palette-aware syntax roles. Register with `Setup::widget_styles` so
/// already prepared spans follow theme switches without syntax work.
pub fn palette_style_rules(palette: &Palette, mut rules: StyleRules<'_>) {
    let colors = [
        palette.fg,
        palette.magenta,
        palette.cyan,
        palette.green,
        palette.cyan,
        palette.orange,
        palette.blue,
        palette.yellow,
        palette.red,
        palette.violet,
        palette.muted_fg,
    ];
    let grounds = [palette.bg, palette.panel_bg, palette.focus_bg()];
    for (paths, color) in PALETTE_PATHS.iter().zip(colors) {
        let color = color.with_contrast(&grounds, 4.5);
        for (bits, path) in paths.iter().enumerate() {
            let attrs = AttrSet {
                bold: bits & 1 != 0,
                italic: bits & 2 != 0,
                underline: bits & 4 != 0,
                ..AttrSet::default()
            };
            rules = rules.style(path, PartialStyle::new().fg(color).attrs(attrs));
        }
    }
    rules.apply();
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
    /// Adopt `text` as the source being highlighted, replacing the previous
    /// source.
    ///
    /// Highlighters that carry parser state between lines need the whole
    /// source. They can keep the state of the lines that did not change, so
    /// an edit costs the lines after it. The default implementation ignores
    /// the source.
    /// The editor calls this during rendering, before requesting spans, when
    /// its source or highlighter has changed.
    fn prepare(&self, text: &str) {
        let _ = text;
    }

    /// Return highlight spans for a line of text.
    fn highlight_line(&self, line: usize, text: &str) -> Vec<HighlightSpan>;
}

#[cfg(feature = "syntax")]
pub use syntect;
#[cfg(feature = "syntax")]
pub use syntect_highlighter::{DEFAULT_THEME, Grammar, SyntectHighlighter};

/// The syntect-backed [`Highlighter`], and everything it needs.
#[cfg(feature = "syntax")]
mod syntect_highlighter {
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
        fmt, mem,
        ops::Range,
        path::Path,
        ptr,
        sync::OnceLock,
    };

    use canopy::style::{Attr, AttrSet, Color, Paint, Style, themes};
    use syntect::{
        highlighting,
        highlighting::{
            FontStyle, HighlightIterator, HighlightState, Style as SyntectStyle, Theme, ThemeSet,
        },
        parsing::{
            ParseState, ScopeStack, SyntaxDefinition, SyntaxReference, SyntaxSet, SyntaxSetBuilder,
        },
    };
    use two_face::syntax::extra_newlines;

    use super::{HighlightSpan, Highlighter, PALETTE_PATHS, SpanStyle};

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

        /// Role colors in `PALETTE_PATHS` order, after the plain text role.
        pub(super) const ROLES: [Color; 10] = [
            PINK, CYAN, GREEN, TEAL, ORANGE, BLUE, YELLOW, CORAL, AQUA, SLATE,
        ];
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

    /// Bytes a prepared source will walk before it stops carrying parser
    /// state, for sources whose lines are long.
    const MAX_STATEFUL_BYTES: usize = 512 * 1024;

    /// The longest line that is highlighted.
    ///
    /// Grammars match each line with regular expressions whose cost grows
    /// with the line, so one minified line of hundreds of kilobytes takes
    /// seconds. A source with a longer line shows as plain text, since
    /// skipping that line alone would leave the parser state wrong below it.
    const MAX_LINE_BYTES: usize = 4_096;

    /// Lines between the walk states a prepared source keeps, so a walk
    /// over an edited source resumes near the first changed line.
    const CHECKPOINT_LINES: usize = 8;

    /// A grammar: a syntax, and the set that holds it.
    ///
    /// A syntax parses only against its own set, so the two travel together.
    /// Most syntaxes come from the extended set of about 220 languages, and
    /// Luau from a set of its own.
    #[derive(Clone, Copy)]
    pub struct Grammar {
        /// The syntax.
        syntax: &'static SyntaxReference,
        /// The set that holds the syntax.
        set: &'static SyntaxSet,
    }

    impl PartialEq for Grammar {
        fn eq(&self, other: &Self) -> bool {
            ptr::eq(self.syntax, other.syntax) && ptr::eq(self.set, other.set)
        }
    }

    impl Grammar {
        /// Return the grammar a Markdown fence's info string names, if one
        /// covers it.
        ///
        /// The language is the first word of the info string, and a comma also
        /// ends it, so `rust,ignore` names Rust. A word resolves as an
        /// extension, then as a syntax name, and a few common names such as
        /// `shell` and `golang` stand for others. Plain text names no grammar.
        pub fn for_fence(info: &str) -> Option<Self> {
            let token = canopy_xmark::fence_language(info)?;
            let token = FENCE_ALIASES
                .iter()
                .find(|(alias, _)| alias.eq_ignore_ascii_case(token))
                .map_or(token, |(_, extension)| extension);
            let grammar = Self::for_extension(token)
                .or_else(|| Self::extended(|set| set.find_syntax_by_token(token)))?;
            (!grammar.is_plain()).then_some(grammar)
        }

        /// Return the syntax.
        pub fn syntax(self) -> &'static SyntaxReference {
            self.syntax
        }

        /// Return the set that holds the syntax, which parsing needs.
        pub fn syntax_set(self) -> &'static SyntaxSet {
            self.set
        }

        /// Return the grammar for a file extension, consulting the alias table
        /// when no grammar claims the extension itself.
        fn for_extension(extension: &str) -> Option<Self> {
            if extension.eq_ignore_ascii_case("luau") {
                return Some(Self::luau());
            }
            Self::extended(|set| {
                set.find_syntax_by_extension(extension).or_else(|| {
                    SYNTAX_ALIASES
                        .iter()
                        .find(|(alias, _)| alias.eq_ignore_ascii_case(extension))
                        .and_then(|(_, name)| set.find_syntax_by_name(name))
                })
            })
        }

        /// Return the extended grammar that `find` picks, if any.
        fn extended(
            find: impl FnOnce(&'static SyntaxSet) -> Option<&'static SyntaxReference>,
        ) -> Option<Self> {
            let set = syntax_set();
            find(set).map(|syntax| Self { syntax, set })
        }

        /// Return the Luau grammar, loaded on first use.
        ///
        /// Luau adds types, interpolated strings, and more to Lua, which the
        /// Lua grammar misreads. It stays out of the extended set because
        /// adding a syntax there relinks every other one, which costs far more
        /// than loading a set of one.
        fn luau() -> Self {
            static LUAU: OnceLock<SyntaxSet> = OnceLock::new();
            let set = LUAU.get_or_init(|| {
                let definition = SyntaxDefinition::load_from_str(LUAU_SYNTAX, true, None)
                    .expect("the bundled Luau grammar loads");
                let mut builder = SyntaxSetBuilder::new();
                builder.add(definition);
                builder.build()
            });
            Self {
                syntax: &set.syntaxes()[0],
                set,
            }
        }

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

    impl fmt::Debug for Grammar {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("Grammar").field(&self.syntax.name).finish()
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

    /// The parser and highlight state after a line.
    #[derive(Clone, PartialEq)]
    struct EngineState {
        /// The parser's state.
        parse: ParseState,
        /// The highlighter's scope and style state.
        highlight: HighlightState,
    }

    /// A highlighter partway through a source, with the set its grammar
    /// highlights against.
    struct Engine {
        /// The theme's highlighter.
        highlighter: highlighting::Highlighter<'static>,
        /// The state after the last line the engine saw.
        state: EngineState,
        /// The set that holds the highlighter's syntax.
        set: &'static SyntaxSet,
    }

    impl Engine {
        /// Start highlighting a source in `grammar` and `theme`.
        fn new(grammar: Grammar, theme: &'static Theme) -> Self {
            let highlighter = highlighting::Highlighter::new(theme);
            let highlight = HighlightState::new(&highlighter, ScopeStack::new());
            Self {
                highlighter,
                state: EngineState {
                    parse: ParseState::new(grammar.syntax),
                    highlight,
                },
                set: grammar.set,
            }
        }

        /// Highlight the next line.
        fn spans(&mut self, text: &str) -> Vec<HighlightSpan> {
            let Ok(ops) = self.state.parse.parse_line(text, self.set) else {
                return Vec::new();
            };
            let ranges: Vec<_> =
                HighlightIterator::new(&mut self.state.highlight, &ops, text, &self.highlighter)
                    .collect();
            spans_from(&ranges)
        }
    }

    /// The walk state before a line, from which a walk can resume.
    struct Checkpoint {
        /// The line the walk resumes at.
        line: usize,
        /// The source highlighter's state.
        engine: EngineState,
        /// The first fence the walk had not left.
        next_fence: usize,
        /// The state of the highlighter of the fence the walk was inside.
        fence: Option<EngineState>,
    }

    /// The body of a fenced code block in a Markdown source.
    #[derive(PartialEq)]
    struct Fence {
        /// The indices of the body's lines.
        lines: Range<usize>,
        /// The grammar the fence's language names, if one covers it.
        grammar: Option<Grammar>,
    }

    /// The fences of a Markdown source, in order, as the Markdown parser
    /// reads them, given the byte offset where each line starts.
    fn fences(text: &str, starts: &[usize]) -> Vec<Fence> {
        // The line that holds byte `at`.
        let line_of = |at: usize| {
            starts
                .partition_point(|&start| start <= at)
                .saturating_sub(1)
        };
        canopy_xmark::fenced_code(text)
            .into_iter()
            .map(|code| Fence {
                lines: line_of(code.body.start)..line_of(code.body.end.saturating_sub(1)) + 1,
                grammar: Grammar::for_fence(&code.info),
            })
            .collect()
    }

    /// Return the length of the longest common prefix of `a` and `b`.
    fn common_prefix(a: &[u8], b: &[u8]) -> usize {
        let whole = a
            .as_chunks::<64>()
            .0
            .iter()
            .zip(b.as_chunks::<64>().0)
            .take_while(|(a, b)| a == b)
            .count()
            * 64;
        whole
            + a[whole..]
                .iter()
                .zip(&b[whole..])
                .take_while(|(a, b)| a == b)
                .count()
    }

    /// Return the length of the longest common suffix of `a` and `b`.
    fn common_suffix(a: &[u8], b: &[u8]) -> usize {
        let whole = a
            .rchunks_exact(64)
            .zip(b.rchunks_exact(64))
            .take_while(|(a, b)| a == b)
            .count()
            * 64;
        whole
            + a[..a.len() - whole]
                .iter()
                .rev()
                .zip(b[..b.len() - whole].iter().rev())
                .take_while(|(a, b)| a == b)
                .count()
    }

    /// One source text and the highlighting walked over it so far.
    struct Source {
        /// Grammar resolved for this source, including an inferred first-line
        /// hint.
        grammar: Grammar,
        /// Text of the source, whose lines keep the newlines syntect expects.
        text: String,
        /// Byte offset where each line starts, and then the end of the text.
        starts: Vec<usize>,
        /// Theme the spans take their styles from.
        theme: &'static Theme,
        /// Highlighter positioned after the last cached line.
        engine: Engine,
        /// The fences of a Markdown source, in order.
        fences: Vec<Fence>,
        /// The first fence the walk has not left.
        next_fence: usize,
        /// Highlighter for the body of the fence the walk is inside.
        fence_engine: Option<Engine>,
        /// Spans for every line walked so far.
        spans: Vec<Vec<HighlightSpan>>,
        /// Walk states every `CHECKPOINT_LINES` lines, in order.
        checkpoints: Vec<Checkpoint>,
        /// The walk from before the last edit over the lines below it, which
        /// this walk rejoins where their states agree.
        rejoin: Option<Rejoin>,
    }

    /// The walk over the lines below an edit, from before the edit.
    struct Rejoin {
        /// Lines the edit added above the walk, or removed when negative.
        shift: isize,
        /// The walk's checkpoints, at their lines before the edit, in order.
        checkpoints: VecDeque<Checkpoint>,
        /// Line before the edit of the first of `spans`.
        first: usize,
        /// The walk's spans from line `first` on.
        spans: Vec<Vec<HighlightSpan>>,
        /// The state after the walk's last line.
        engine: EngineState,
    }

    impl Source {
        /// Hold `text` to highlight in `grammar` and `theme`.
        fn new(grammar: Grammar, text: String, theme: &'static Theme) -> Self {
            let mut starts: Vec<usize> = text
                .split_inclusive('\n')
                .scan(0, |at, line| {
                    let start = *at;
                    *at += line.len();
                    Some(start)
                })
                .collect();
            starts.push(text.len());
            let markdown = grammar
                .syntax
                .scope
                .build_string()
                .starts_with("text.html.markdown");
            Self {
                grammar,
                fences: if markdown {
                    fences(&text, &starts)
                } else {
                    Vec::new()
                },
                text,
                starts,
                theme,
                engine: Engine::new(grammar, theme),
                next_fence: 0,
                fence_engine: None,
                spans: Vec::new(),
                checkpoints: Vec::new(),
                rejoin: None,
            }
        }

        /// Return the number of lines.
        fn line_count(&self) -> usize {
            self.starts.len() - 1
        }

        /// Return the line that holds byte `at`, or the line count at the end.
        fn line_at(&self, at: usize) -> usize {
            self.starts.partition_point(|&start| start <= at) - 1
        }

        /// Adopt `text` in `grammar` as the source. Keep the walk over the
        /// lines above the first that changed, back to its checkpoint, and
        /// the walk over the lines below the last that changed, to rejoin.
        fn edit(&mut self, grammar: Grammar, text: String) {
            if grammar == self.grammar && text == self.text {
                return;
            }
            let mut next = Self::new(grammar, text, self.theme);
            if grammar != self.grammar {
                *self = next;
                return;
            }
            let (old, new) = (self.text.as_bytes(), next.text.as_bytes());
            let prefix = common_prefix(old, new);
            let suffix = common_suffix(&old[prefix..], &new[prefix..]);
            let mut keep = self.line_at(prefix).min(next.line_at(prefix));
            // A changed fence changes the walk from where either version
            // of it starts.
            let fences = self.fences.len().max(next.fences.len());
            if let Some(changed) =
                (0..fences).find(|&index| self.fences.get(index) != next.fences.get(index))
            {
                for fences in [&self.fences, &next.fences] {
                    if let Some(fence) = fences.get(changed) {
                        keep = keep.min(fence.lines.start);
                    }
                }
            }
            if self.fences.is_empty() && next.fences.is_empty() {
                next.rejoin = self.rejoin_after(old.len() - suffix, &next);
            }
            self.checkpoints
                .retain(|checkpoint| checkpoint.line <= keep);
            if let Some(checkpoint) = self.checkpoints.last() {
                next.engine.state = checkpoint.engine.clone();
                next.next_fence = checkpoint.next_fence;
                let grammar = next
                    .fences
                    .get(checkpoint.next_fence)
                    .and_then(|fence| fence.grammar);
                next.fence_engine =
                    checkpoint
                        .fence
                        .clone()
                        .zip(grammar)
                        .map(|(state, grammar)| Engine {
                            state,
                            ..Engine::new(grammar, self.theme)
                        });
                self.spans.truncate(checkpoint.line);
                next.spans = mem::take(&mut self.spans);
                next.checkpoints = mem::take(&mut self.checkpoints);
            }
            *self = next;
        }

        /// Split off the walk over the lines after byte `tail`, from which
        /// the text is unchanged in `next`, for `next` to rejoin.
        fn rejoin_after(&mut self, tail: usize, next: &Self) -> Option<Rejoin> {
            // A line after `tail` starts after an unchanged newline, so it
            // starts a line of `next` too.
            let line = self.starts.partition_point(|&start| start <= tail);
            if line >= self.line_count() {
                return None;
            }
            let moved = (self.starts[line] + next.text.len()).checked_sub(self.text.len())?;
            let new_line = next.starts.binary_search(&moved).ok()?;
            let split = self
                .checkpoints
                .partition_point(|checkpoint| checkpoint.line < line);
            let checkpoints = VecDeque::from(self.checkpoints.split_off(split));
            let first = checkpoints.front()?.line;
            Some(Rejoin {
                shift: new_line as isize - line as isize,
                spans: self.spans.split_off(first),
                first,
                checkpoints,
                engine: self.engine.state.clone(),
            })
        }

        /// Highlight in `theme`, walking again from the first line.
        fn restart(&mut self, theme: &'static Theme) {
            *self = Self::new(self.grammar, mem::take(&mut self.text), theme);
        }

        /// Walk forward until `line` is cached, and return its spans.
        ///
        /// Returns `None` when the line lies outside the source or beyond the
        /// stateful walking limit.
        fn spans_for(&mut self, line: usize) -> Option<Vec<HighlightSpan>> {
            if line >= self.line_count() {
                return None;
            }
            while self.spans.len() <= line {
                self.walk_line();
            }
            self.spans.get(line).cloned()
        }

        /// Take the rest of the walk from before the last edit, if the walk
        /// has reached one of its checkpoints in the same state, and return
        /// whether it did.
        fn rejoin(&mut self) -> bool {
            let line = self.spans.len() as isize;
            let Some(rejoin) = &mut self.rejoin else {
                return false;
            };
            while rejoin
                .checkpoints
                .front()
                .is_some_and(|checkpoint| checkpoint.line as isize + rejoin.shift < line)
            {
                rejoin.checkpoints.pop_front();
            }
            let Some(checkpoint) = rejoin.checkpoints.front() else {
                self.rejoin = None;
                return false;
            };
            if checkpoint.line as isize + rejoin.shift != line
                || checkpoint.engine != self.engine.state
            {
                return false;
            }
            let skip = checkpoint.line - rejoin.first;
            let Some(rejoin) = self.rejoin.take() else {
                return false;
            };
            self.spans.extend(rejoin.spans.into_iter().skip(skip));
            self.spans.truncate(self.line_count());
            self.checkpoints
                .extend(rejoin.checkpoints.into_iter().map(|checkpoint| Checkpoint {
                    line: (checkpoint.line as isize + rejoin.shift) as usize,
                    ..checkpoint
                }));
            self.engine.state = rejoin.engine;
            true
        }

        /// Highlight the next line, and cache its spans.
        ///
        /// The body of a fence goes to the fence's own highlighter, when a
        /// syntax covers its language, and the source's highlighter never sees
        /// it. The fence lines themselves, and a body in no known language,
        /// stay with the source's highlighter.
        fn walk_line(&mut self) {
            if self.rejoin() {
                return;
            }
            let line = self.spans.len();
            if line.is_multiple_of(CHECKPOINT_LINES) {
                self.checkpoints.push(Checkpoint {
                    line,
                    engine: self.engine.state.clone(),
                    next_fence: self.next_fence,
                    fence: self
                        .fence_engine
                        .as_ref()
                        .map(|engine| engine.state.clone()),
                });
            }
            while self
                .fences
                .get(self.next_fence)
                .is_some_and(|fence| fence.lines.end <= line)
            {
                self.next_fence += 1;
                self.fence_engine = None;
            }
            let grammar = self
                .fences
                .get(self.next_fence)
                .filter(|fence| fence.lines.contains(&line))
                .and_then(|fence| fence.grammar);
            let text = &self.text[self.starts[line]..self.starts[line + 1]];
            let engine = match grammar {
                Some(grammar) => self
                    .fence_engine
                    .get_or_insert_with(|| Engine::new(grammar, self.theme)),
                None => &mut self.engine,
            };
            let spans = engine.spans(text);
            self.spans.push(spans);
        }
    }

    /// A syntect-backed highlighter.
    ///
    /// Language detection uses the file name or extension, then the first
    /// source line. Parser state is retained for at most 4,000 lines or
    /// 512 KiB; later lines highlight independently. A source with a line
    /// longer than 4 KiB, such as a minified bundle, shows as plain text.
    pub struct SyntectHighlighter {
        /// Theme used for highlighting.
        theme: &'static Theme,
        /// Resolve built-in semantic roles through the live widget palette.
        palette_roles: bool,
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
                palette_roles: false,
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
            self.palette_roles = false;
            self.theme = theme(name.as_ref());
            if let Some(source) = self.source.get_mut().as_mut() {
                source.restart(self.theme);
            }
            self
        }

        /// Prepare borrowed semantic style paths using the built-in Canopy
        /// classifier. Colors resolve through `palette_style_rules` when
        /// painted; bold, italic, and underline survive without
        /// reparsing on theme changes. A later `with_theme` call
        /// restores fixed named-theme colors.
        #[must_use]
        pub fn with_palette_roles(self) -> Self {
            let mut result = self.with_theme(DEFAULT_THEME);
            result.palette_roles = true;
            result
        }

        /// Convert built-in fixed token identities to borrowed palette paths.
        fn paint_spans(&self, mut spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
            if self.palette_roles {
                for span in &mut spans {
                    if let SpanStyle::Fixed(style) = &span.style {
                        let role = code::ROLES
                            .iter()
                            .position(|color| Some(*color) == style.fg.solid_color())
                            .map_or(0, |index| index + 1);
                        let attrs = usize::from(style.attrs.bold)
                            | usize::from(style.attrs.italic) << 1
                            | usize::from(style.attrs.underline) << 2;
                        span.style = SpanStyle::Path(PALETTE_PATHS[role][attrs].into());
                    }
                }
            }
            spans
        }

        /// Select the syntax for `extension`, discarding any prepared source.
        pub fn set_extension(&self, extension: &str) {
            self.set_grammar(Grammar::for_extension(extension).unwrap_or_else(Grammar::plain));
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
                    Grammar::extended(|set| set.find_syntax_by_extension(&name.to_string_lossy()))
                })
                .or_else(|| {
                    path.extension()
                        .and_then(|ext| Grammar::for_extension(&ext.to_string_lossy()))
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
        pub fn set_grammar(&self, grammar: Grammar) {
            self.grammar.set(grammar);
            self.source.replace(None);
        }
    }

    impl Highlighter for SyntectHighlighter {
        fn prepare(&self, text: &str) {
            // A plain-text syntax may still be a recognizable script, so
            // consult the first line before committing to it.
            let hint = self.grammar.get();
            let grammar = if text.split('\n').any(|line| line.len() > MAX_LINE_BYTES) {
                Grammar::plain()
            } else if hint.is_plain() {
                Grammar::extended(|set| {
                    set.find_syntax_by_first_line(text.lines().next().unwrap_or_default())
                })
                .unwrap_or(hint)
            } else {
                hint
            };
            let mut bytes = 0;
            let end: usize = text
                .split_inclusive('\n')
                .take(MAX_STATEFUL_LINES)
                .take_while(|line| {
                    bytes += line.len();
                    bytes <= MAX_STATEFUL_BYTES
                })
                .map(str::len)
                .sum();
            let text = text[..end].to_string();
            let mut source = self.source.borrow_mut();
            match source.as_mut() {
                Some(source) => source.edit(grammar, text),
                None => *source = Some(Source::new(grammar, text, self.theme)),
            }
        }

        fn highlight_line(&self, line: usize, text: &str) -> Vec<HighlightSpan> {
            if let Some(source) = self.source.borrow_mut().as_mut()
                && let Some(spans) = source.spans_for(line)
            {
                return self.paint_spans(spans);
            }
            // No prepared source, or a line beyond it: highlight on its own,
            // unless it is too long to.
            if text.len() > MAX_LINE_BYTES {
                return Vec::new();
            }
            self.paint_spans(Engine::new(self.current_grammar(), self.theme).spans(text))
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
        use std::borrow::Cow;

        use super::*;
        use crate::highlight::{SpanStyle, palette_style_rules};

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
        fn palette_roles_preserve_font_attributes_and_leave_named_themes_fixed() {
            let text = "# Heading\n\n**bold** and *italic*\n";
            let fixed = SyntectHighlighter::new("md");
            let roles = SyntectHighlighter::new("md").with_palette_roles();
            fixed.prepare(text);
            roles.prepare(text);
            let mut map = themes::solarized_light().style_map();
            palette_style_rules(&themes::solarized_light(), map.rules());
            for (line, text) in text.lines().enumerate() {
                let fixed = fixed.highlight_line(line, text);
                let roles = roles.highlight_line(line, text);
                assert_eq!(fixed.len(), roles.len());
                for (fixed, role) in fixed.iter().zip(&roles) {
                    let SpanStyle::Fixed(style) = &fixed.style else {
                        panic!("fixed theme")
                    };
                    let SpanStyle::Path(path) = &role.style else {
                        panic!("palette path")
                    };
                    assert!(matches!(path, Cow::Borrowed(_)));
                    assert_eq!(fixed.range, role.range);
                    assert_eq!(style.attrs, map.resolve(path).attrs);
                }
            }
            let named = SyntectHighlighter::new("rs")
                .with_palette_roles()
                .with_theme("Solarized (light)");
            assert!(
                named
                    .highlight_line(0, "let value = 1;")
                    .iter()
                    .all(|span| matches!(span.style, SpanStyle::Fixed(_)))
            );
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

        /// Return every line's spans after preparing `source`.
        fn walk(highlighter: &SyntectHighlighter, source: &str) -> Vec<Vec<(Range<usize>, Paint)>> {
            highlighter.prepare(source);
            source
                .lines()
                .enumerate()
                .map(|(line, text)| {
                    highlighter
                        .highlight_line(line, text)
                        .into_iter()
                        .map(|span| match span.style {
                            SpanStyle::Fixed(style) => (span.range, style.fg),
                            SpanStyle::Path(path) => panic!("syntect spans are fixed, not {path}"),
                        })
                        .collect()
                })
                .collect()
        }

        #[test]
        fn an_edit_highlights_as_a_fresh_walk_does() {
            let code: String = (0..200)
                .map(|i| format!("let v{i} = \"{i}\"; // c\n"))
                .collect();
            let fenced: String = (0..40)
                .map(|i| format!("Text {i}\n\n```rust\nlet a = {i};\n/* x */\n```\n\n"))
                .collect();
            let edits = [
                ("rs", &code, code.replacen("let v100", "/* let v100", 1)),
                ("rs", &code, code.replacen("let v5 ", "let w5 ", 1)),
                (
                    "rs",
                    &code,
                    code.replacen("let v70 = \"70\"; // c\n", "", 1),
                ),
                ("rs", &code, format!("{code}let tail = 1;\n")),
                (
                    "md",
                    &fenced,
                    fenced.replacen("```rust\nlet a = 20;", "```\nlet a = 20;", 1),
                ),
                (
                    "md",
                    &fenced,
                    fenced.replacen("Text 30\n", "Text 30\n```py\n", 1),
                ),
            ];
            for (extension, source, edited) in edits {
                assert_ne!(&edited, source);
                let highlighter = SyntectHighlighter::new(extension);
                walk(&highlighter, source);
                let fresh = SyntectHighlighter::new(extension);
                assert_eq!(walk(&highlighter, &edited), walk(&fresh, &edited));
            }
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
                    Grammar::for_fence(info).map(|grammar| grammar.syntax.name.as_str()),
                    Some(expected),
                    "a `{info}` fence should highlight as {expected}"
                );
            }
            for info in ["", "nosuchlang", "text"] {
                assert!(
                    Grammar::for_fence(info).is_none(),
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
                assert_eq!(prepared.line_count(), MAX_STATEFUL_LINES);
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
        fn a_source_with_a_long_line_shows_as_plain_text() {
            let highlighter = SyntectHighlighter::new("js");
            let long = "var a=1;".repeat(MAX_LINE_BYTES / 8 + 1);
            highlighter.prepare(&format!("var x = 1;\n{long}\n"));
            assert_eq!(highlighter.syntax_name(), "Plain Text");
            assert_eq!(
                colors(&highlighter, 0, "var x = 1;"),
                colors(&SyntectHighlighter::new("txt"), 0, "var x = 1;"),
                "every line is plain, not only the long one"
            );
            let unprepared = SyntectHighlighter::new("js");
            assert!(
                unprepared.highlight_line(0, &long).is_empty(),
                "a long line on its own stays plain"
            );
        }

        #[test]
        fn preparing_long_lines_retains_a_bounded_prefix() {
            let highlighter = SyntectHighlighter::new("rs");
            let line = format!("// {}\n", "x".repeat(4_000));
            highlighter.prepare(&line.repeat(MAX_STATEFUL_BYTES / line.len() * 2));
            let prepared = highlighter.source.borrow();
            let prepared = prepared.as_ref().expect("source was prepared");
            assert_eq!(prepared.line_count(), MAX_STATEFUL_BYTES / line.len());
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

#[cfg(test)]
mod palette_tests {
    use canopy::style::themes;

    use super::*;

    #[test]
    fn every_syntax_role_has_readable_contrast_on_dark_and_light_code_grounds() {
        for palette in [themes::default_dark(), themes::solarized_light()] {
            let mut map = palette.style_map();
            palette_style_rules(&palette, map.rules());
            for path in PALETTE_PATHS.iter().flatten() {
                let foreground = map.resolve(path).fg.solid_color().unwrap();
                for ground in [palette.bg, palette.panel_bg, palette.focus_bg()] {
                    assert!(
                        foreground.contrast_ratio(ground) >= 4.49,
                        "{path}: {foreground:?} on {ground:?}"
                    );
                }
            }
        }
    }
}
