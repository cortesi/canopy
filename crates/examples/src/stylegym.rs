//! Stylegym: a viewer for Canopy themes and styles.
//!
//! A sidebar titled Styles picks the theme and the effects. The untitled main
//! pane shows the theme through tabbed pages: resolved palette swatches, every
//! rule in the theme, stock widgets, syntax highlighting, and text samples. A
//! modal overlay shows how the pane dims.

use canopy::{
    Canopy, CanopyBuilder, ChildSlot, Context, ContextExt, FocusDirection, Loader, NodeId,
    NodeName, Render, TypedId, ViewContext, Widget, derive_commands,
    error::Result,
    geom::{Line, Point, Size},
    layout::{CanvasContext, Direction, Edges, Layout},
    style::{
        AttrSet, Color, PartialStyle, ResolvedStyle, StyleMap, canopy as canopy_theme, dracula,
        effects::{self, Effect},
        gruvbox, solarized,
    },
};
use canopy_widgets::{
    Button, Center, Dropdown, Frame, Input, Label, Root, Selector, Tabs,
    editor::{Editor, EditorConfig, LineNumbers, WrapMode, highlight::SyntectHighlighter},
};

/// Default bindings for the style gym demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.keymap({
    path = "stylegym/",
    phase = "before_widget",
    { key = "Tab", description = "Next focus", action = command.root.focus("Next") },
    { key = "BackTab", description = "Previous focus", action = command.root.focus("Prev") },
})

canopy.keymap({
    path = "stylegym/",
    { key = "q", description = "Quit", action = command.root.quit() },
    { key = "m", description = "Show modal", action = command.stylegym.show_modal() },
    { key = "Esc", description = "Hide modal", action = command.stylegym.hide_modal() },
    { key = { "l", "Right", "]" }, description = "Next tab", action = command.stylegym.next_tab(1) },
    {
        key = { "h", "Left", "[" },
        description = "Previous tab",
        action = command.stylegym.next_tab(-1),
    },
    { key = "1", description = "Palette tab", action = command.stylegym.show_tab(0) },
    { key = "2", description = "Rules tab", action = command.stylegym.show_tab(1) },
    { key = "3", description = "Widgets tab", action = command.stylegym.show_tab(2) },
    { key = "4", description = "Syntax tab", action = command.stylegym.show_tab(3) },
    { key = "5", description = "Text tab", action = command.stylegym.show_tab(4) },
})

canopy.keymap({
    path = "style_sheet",
    {
        key = { "j", "Down" },
        mouse = "ScrollDown",
        description = "Scroll down",
        action = command.style_sheet.scroll("Down"),
    },
    {
        key = { "k", "Up" },
        mouse = "ScrollUp",
        description = "Scroll up",
        action = command.style_sheet.scroll("Up"),
    },
    {
        key = { "H", "shift-Left" },
        description = "Scroll left",
        action = command.style_sheet.scroll("Left"),
    },
    {
        key = { "L", "shift-Right" },
        description = "Scroll right",
        action = command.style_sheet.scroll("Right"),
    },
    { key = { "PageDown", "Space" }, description = "Page down", action = command.style_sheet.page(1) },
    { key = "PageUp", description = "Page up", action = command.style_sheet.page(-1) },
})

canopy.keymap({
    path = "dropdown",
    phase = "before_widget",
    {
        key = "Enter",
        description = "Apply theme",
        action = function()
            dropdown.confirm()
            stylegym.apply_theme()
        end,
    },
    { key = "Space", description = "Toggle dropdown", action = command.dropdown.toggle() },
    { key = { "j", "Down" }, description = "Next option", action = command.dropdown.select_by(1) },
    { key = { "k", "Up" }, description = "Previous option", action = command.dropdown.select_by(-1) },
})
canopy.bind_mouse("LeftDown", { path = "dropdown", description = "Apply theme" }, function()
    stylegym.apply_theme()
end)

canopy.keymap({
    path = "selector",
    phase = "before_widget",
    {
        key = "Space",
        description = "Toggle effect",
        action = function()
            selector.toggle()
            stylegym.apply_effects()
        end,
    },
    {
        key = "Enter",
        description = "Toggle effect",
        action = function()
            selector.toggle()
            stylegym.apply_effects()
        end,
    },
    { key = { "j", "Down" }, description = "Next effect", action = command.selector.select_by(1) },
    { key = { "k", "Up" }, description = "Previous effect", action = command.selector.select_by(-1) },
})
canopy.bind_mouse("LeftDown", { path = "selector", description = "Apply effects" }, function()
    stylegym.apply_effects()
end)
"#;

/// Style paths the palette page shows, grouped under headings.
const PALETTE: &[(&str, &[&str])] = &[
    (
        "Surfaces",
        &["", "help/panel", "editor/prompt", "editor/selection"],
    ),
    (
        "Chrome",
        &[
            "frame",
            "frame/focused",
            "frame/active",
            "frame/title",
            "tabs/bar",
            "tabs/tab",
            "tabs/tab/active",
            "tabs/tab/active/focused",
        ],
    ),
    (
        "Selection",
        &[
            "selector/selected",
            "selector/focus",
            "selector/focus/selected",
            "dropdown/selected",
            "dropdown/highlight",
        ],
    ),
    (
        "Editor",
        &[
            "editor/text",
            "editor/line-number",
            "editor/line-number/current",
            "editor/search/match",
            "editor/search/current",
        ],
    ),
    ("Help", &["help/key", "help/label", "help/indicator"]),
    (
        "Named colors",
        &[
            "red", "orange", "yellow", "green", "cyan", "blue", "violet", "magenta",
        ],
    ),
];

/// Columns taken by the foreground and background swatches.
const SWATCH_WIDTH: u32 = 8;
/// Columns between the path and the values that follow it.
const PATH_GAP: u32 = 2;
/// Columns taken by a hex value and its trailing gap.
const HEX_WIDTH: u32 = 9;
/// Text painted with each style.
const SAMPLE: &str = "Aa 123";
/// Columns taken by the sample and its trailing gap.
const SAMPLE_WIDTH: u32 = 8;
/// Columns taken by the attribute names.
const ATTR_WIDTH: u32 = 20;

/// Rust source shown on the syntax page.
const RUST_SAMPLE: &str = r#"/// A point on the canvas.
#[derive(Debug, Clone, Copy)]
pub struct Point {
    x: u32,
    y: u32,
}

impl Point {
    /// Offset the point, saturating at the edges.
    pub fn offset(self, dx: i32, dy: i32) -> Self {
        let x = self.x.saturating_add_signed(dx);
        let y = self.y.saturating_add_signed(dy);
        println!("moved to ({x}, {y})\n");
        Self { x, y }
    }
}
"#;

/// Luau source shown on the syntax page.
const LUAU_SAMPLE: &str = r#"-- Bind a key for each tab.
local tabs = { "Palette", "Rules", "Widgets" }

function setup()
    for index, name in ipairs(tabs) do
        canopy.bind(tostring(index), { description = name }, function()
            stylegym.show_tab(index - 1)
        end)
    end
    return #tabs > 0
end
"#;

/// Theme option for the dropdown.
#[derive(Clone)]
pub(crate) struct ThemeOption {
    /// Theme display name.
    pub name: &'static str,
    /// Function to build the theme's StyleMap.
    pub builder: fn() -> StyleMap,
}

impl Label for ThemeOption {
    fn label(&self) -> &str {
        self.name
    }
}

/// Effect option for the selector.
#[derive(Clone)]
pub(crate) struct EffectOption {
    /// Effect display name.
    pub name: &'static str,
    /// Style effect applied when this option is selected.
    pub effect: Effect,
}

impl Label for EffectOption {
    fn label(&self) -> &str {
        self.name
    }
}

/// Available themes.
fn available_themes() -> Vec<ThemeOption> {
    vec![
        ThemeOption {
            name: "Canopy Dark",
            builder: canopy_theme::canopy_dark,
        },
        ThemeOption {
            name: "Solarized Dark",
            builder: solarized::solarized_dark,
        },
        ThemeOption {
            name: "Solarized Light",
            builder: solarized::solarized_light,
        },
        ThemeOption {
            name: "Gruvbox Dark",
            builder: gruvbox::gruvbox_dark,
        },
        ThemeOption {
            name: "Dracula",
            builder: dracula::dracula,
        },
    ]
}

/// Available effects.
fn available_effects() -> Vec<EffectOption> {
    vec![
        EffectOption {
            name: "Dim",
            effect: effects::brightness(0.5),
        },
        EffectOption {
            name: "Brighten",
            effect: effects::brightness(1.5),
        },
        EffectOption {
            name: "Grayscale",
            effect: effects::saturation(0.0),
        },
        EffectOption {
            name: "Invert",
            effect: effects::invert_rgb(),
        },
        EffectOption {
            name: "Hue Shift",
            effect: effects::hue_shift(180.0),
        },
        EffectOption {
            name: "Bold",
            effect: effects::bold(),
        },
        EffectOption {
            name: "Italic",
            effect: effects::italic(),
        },
    ]
}

// Typed keys for keyed children
canopy::slot!(ControlsSlot: Frame);
canopy::slot!(ThemeFrameSlot: Frame);
canopy::slot!(ThemeDropdownSlot: Dropdown<ThemeOption>);
canopy::slot!(EffectsFrameSlot: Frame);
canopy::slot!(EffectsSelectorSlot: Selector<EffectOption>);
canopy::slot!(RightContainerSlot: Stack);
canopy::slot!(MainFrameSlot: Frame);
canopy::slot!(TabsSlot: Tabs);
canopy::slot!(ModalSlot: Center);

/// Which style components a rule sets itself rather than inheriting.
#[derive(Clone, Copy)]
struct Components {
    /// The rule sets a foreground.
    fg: bool,
    /// The rule sets a background.
    bg: bool,
    /// The rule sets attributes.
    attrs: bool,
}

impl Components {
    /// Every component, for rows that show resolved values only.
    const ALL: Self = Self {
        fg: true,
        bg: true,
        attrs: true,
    };

    /// Return the components `style` sets.
    fn of(style: &PartialStyle) -> Self {
        Self {
            fg: style.fg.is_some(),
            bg: style.bg.is_some(),
            attrs: style.attrs.is_some(),
        }
    }
}

/// One row of a style sheet.
enum SheetRow {
    /// Column titles.
    Columns,
    /// A section heading.
    Heading(String),
    /// A style path, resolved when painted.
    Path {
        /// Canonical style path.
        path: String,
        /// Components the path's rule sets itself.
        sets: Components,
    },
    /// A blank separator.
    Blank,
}

/// A scrollable table of style paths and their resolved styles.
///
/// Each path row shows a foreground swatch, a background swatch, the path, the
/// resolved foreground and background, a sample painted with the style, and
/// the resolved attributes. Values the rule inherits are muted.
pub(crate) struct StyleSheet {
    /// Rows in display order.
    rows: Vec<SheetRow>,
}

#[derive_commands]
impl StyleSheet {
    /// Build the palette page: curated paths grouped by role.
    fn palette() -> Self {
        let mut rows = vec![SheetRow::Columns];
        for (title, paths) in PALETTE {
            rows.push(SheetRow::Blank);
            rows.push(SheetRow::Heading((*title).to_string()));
            rows.extend(paths.iter().map(|path| SheetRow::Path {
                path: (*path).to_string(),
                sets: Components::ALL,
            }));
        }
        Self { rows }
    }

    /// Build an empty rules page, filled by [`StyleSheet::set_rules`].
    fn rules() -> Self {
        Self { rows: Vec::new() }
    }

    /// List every rule in `map`, sorted by path.
    fn set_rules(&mut self, map: &StyleMap) {
        let mut rules = map.entries().collect::<Vec<_>>();
        rules.sort_by_key(|(path, _)| *path);
        self.rows = vec![
            SheetRow::Heading("Rules in the active theme. Muted values are inherited.".into()),
            SheetRow::Columns,
        ];
        self.rows
            .extend(rules.into_iter().map(|(path, style)| SheetRow::Path {
                path: path.to_string(),
                sets: Components::of(style),
            }));
    }

    /// Scroll by one line or column.
    #[command]
    pub(crate) fn scroll(&self, c: &mut dyn Context, dir: FocusDirection) {
        crate::scroll_in(c, dir);
    }

    /// Scroll by a page; negative moves up.
    #[command]
    pub(crate) fn page(&self, c: &mut dyn Context, delta: i32) {
        crate::page_by(c, delta);
    }

    /// Return the width of the path column.
    fn path_width(&self) -> u32 {
        self.rows
            .iter()
            .filter_map(|row| match row {
                SheetRow::Path { path, .. } => Some(path.chars().count() + 1),
                _ => None,
            })
            .max()
            .map_or(0, |width| width as u32)
            .max("path".len() as u32)
    }

    /// Return the column where each value starts: fg, bg, sample, attrs.
    fn value_columns(path_width: u32) -> [u32; 4] {
        let fg = SWATCH_WIDTH + path_width + PATH_GAP;
        [
            fg,
            fg + HEX_WIDTH,
            fg + 2 * HEX_WIDTH,
            fg + 2 * HEX_WIDTH + SAMPLE_WIDTH,
        ]
    }

    /// Paint one path row at canvas row `y`.
    fn paint_path(
        rndr: &mut Render,
        muted: ResolvedStyle,
        y: u32,
        path: &str,
        sets: Components,
        path_width: u32,
    ) -> Result<()> {
        let style = rndr.resolve_style(path);
        let solid = style.resolve_solid();
        if let Some(solid) = solid {
            for (x, color) in [(1, solid.fg), (4, solid.bg)] {
                let swatch = ResolvedStyle::new(color, color, AttrSet::default());
                put_text(rndr, swatch, x, y, "  ")?;
            }
        }
        rndr.text("", line(SWATCH_WIDTH, y, path_width), &format!("/{path}"))?;

        let [fg_x, bg_x, sample_x, attrs_x] = Self::value_columns(path_width);
        let (fg, bg) = solid.map_or_else(
            || ("gradient".to_string(), "gradient".to_string()),
            |solid| (hex(solid.fg), hex(solid.bg)),
        );
        put_value(rndr, muted, fg_x, y, &fg, sets.fg)?;
        put_value(rndr, muted, bg_x, y, &bg, sets.bg)?;
        rndr.text(path, line(sample_x, y, SAMPLE.len() as u32), SAMPLE)?;
        put_value(
            rndr,
            muted,
            attrs_x,
            y,
            &attr_names(style.attrs),
            sets.attrs,
        )
    }
}

impl Widget for StyleSheet {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let rect = ctx.view().view_rect_local();
        rndr.fill("", rect, ' ')?;
        let muted = muted_style(rndr);
        let path_width = self.path_width();
        let top = rect.tl.y as usize;
        let bottom = top.saturating_add(rect.h as usize);
        for (y, row) in self.rows.iter().enumerate().take(bottom).skip(top) {
            let y = y as u32;
            match row {
                SheetRow::Blank => {}
                SheetRow::Columns => {
                    put_text(rndr, muted, SWATCH_WIDTH, y, "path")?;
                    let titles = ["fg", "bg", "sample", "attrs"];
                    for (x, title) in Self::value_columns(path_width).into_iter().zip(titles) {
                        put_text(rndr, muted, x, y, title)?;
                    }
                }
                SheetRow::Heading(title) => {
                    rndr.text("frame/title", line(1, y, title.len() as u32), title)?;
                }
                SheetRow::Path { path, sets } => {
                    Self::paint_path(rndr, muted, y, path, *sets, path_width)?;
                }
            }
        }
        Ok(())
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext<'_>) -> Size {
        let width = Self::value_columns(self.path_width())[3] + ATTR_WIDTH;
        Size::new(width.max(view.w), (self.rows.len() as u32).max(view.h))
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("style_sheet")
    }
}

/// Return a one-row line.
fn line(x: u32, y: u32, w: u32) -> Line {
    Line {
        tl: Point { x, y },
        w,
    }
}

/// Return a color as `#rrggbb`.
fn hex(color: Color) -> String {
    let (r, g, b) = color.rgb();
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Return the names of the active attributes, or `-` when none are.
fn attr_names(attrs: AttrSet) -> String {
    let names = [
        (attrs.bold, "bold"),
        (attrs.dim, "dim"),
        (attrs.italic, "italic"),
        (attrs.underline, "underline"),
        (attrs.overline, "overline"),
        (attrs.crossedout, "crossed"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect::<Vec<_>>();
    if names.is_empty() {
        "-".into()
    } else {
        names.join(" ")
    }
}

/// Return the style for inherited values and column titles: the gutter
/// foreground on the page background.
fn muted_style(rndr: &Render) -> ResolvedStyle {
    let fg = rndr
        .resolve_style("editor/line-number")
        .fg
        .solid_color()
        .unwrap_or(Color::DarkGrey);
    let bg = rndr
        .resolve_style("")
        .bg
        .solid_color()
        .unwrap_or(Color::Black);
    ResolvedStyle::new(fg, bg, AttrSet::default())
}

/// Paint ASCII `text` with an explicit style, starting at `x`.
fn put_text(rndr: &mut Render, style: ResolvedStyle, x: u32, y: u32, text: &str) -> Result<()> {
    for (offset, ch) in (0u32..).zip(text.chars()) {
        rndr.put_cell(style, Point { x: x + offset, y }, ch)?;
    }
    Ok(())
}

/// Paint a value in the default style when its rule sets it, muted otherwise.
fn put_value(
    rndr: &mut Render,
    muted: ResolvedStyle,
    x: u32,
    y: u32,
    value: &str,
    set: bool,
) -> Result<()> {
    if set {
        rndr.text("", line(x, y, value.len() as u32), value)
    } else {
        put_text(rndr, muted, x, y, value)
    }
}

/// The text samples page: named colors and text attributes.
pub(crate) struct TextSamples;

impl Widget for TextSamples {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();

        // Fill background with root style so effects apply to empty space
        rndr.fill("", rect, ' ')?;

        let mut row = 0;

        // Color palette section
        rndr.text("frame/title", rect.line(row)?, "Color Palette")?;
        row += 1;

        if rect.h > row + 8 {
            rndr.text("red", rect.line(row)?, "████ Red")?;
            rndr.text("orange", rect.line(row + 1)?, "████ Orange")?;
            rndr.text("yellow", rect.line(row + 2)?, "████ Yellow")?;
            rndr.text("green", rect.line(row + 3)?, "████ Green")?;
            rndr.text("cyan", rect.line(row + 4)?, "████ Cyan")?;
            rndr.text("blue", rect.line(row + 5)?, "████ Blue")?;
            rndr.text("violet", rect.line(row + 6)?, "████ Violet")?;
            rndr.text("magenta", rect.line(row + 7)?, "████ Magenta")?;
            row += 9;
        }

        // Text styles section
        if rect.h > row + 5 {
            rndr.text("frame/title", rect.line(row)?, "Text Styles")?;
            row += 1;
            rndr.text("", rect.line(row)?, "Normal text sample")?;
            row += 1;
            rndr.text("text/bold", rect.line(row)?, "Bold text sample")?;
            row += 1;
            rndr.text("text/italic", rect.line(row)?, "Italic text sample")?;
            row += 1;
            rndr.text("text/underline", rect.line(row)?, "Underlined text sample")?;
        }

        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn name(&self) -> NodeName {
        NodeName::convert("text_samples")
    }
}

/// Modal content widget.
struct ModalContent;

impl Widget for ModalContent {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();

        // Fill background so dimmed content doesn't show through
        rndr.fill("", rect, ' ')?;

        rndr.text("", rect.line(0)?, "This is a modal overlay.")?;
        rndr.text("", rect.line(1)?, "Press Esc to dismiss.")?;

        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }
}

/// A container that lays its children out along one direction.
pub(crate) struct Stack(Direction);

impl Widget for Stack {
    fn layout(&self) -> Layout {
        Layout::fill().direction(self.0)
    }
}

/// Add stock widgets under `parent`, one titled frame each.
fn add_widget_samples(c: &mut dyn Context, parent: NodeId) -> Result<()> {
    let buttons = c.add_child_to(parent, Frame::new().with_title("Buttons"))?;
    c.set_layout_of(
        buttons,
        Layout::column()
            .fixed_height(5)
            .flex_horizontal(1)
            .padding(Edges::all(1)),
    )?;
    let row = c.add_child_to(buttons, Stack(Direction::Row))?;
    for (label, active) in [("Normal", false), ("Pressed", true)] {
        let mut button = Button::new(label);
        button.set_active(active);
        let button = c.add_child_to(row, button)?;
        c.set_layout_of(button, Layout::fill().fixed_width(14).fixed_height(3))?;
    }

    for (title, value) in [
        ("Input", "Focus me and type"),
        ("Another input", "Unfocused"),
    ] {
        let frame = c.add_child_to(parent, Frame::new().with_title(title))?;
        c.add_child_to(frame, Input::new(value))?;
        c.set_layout_of(
            frame,
            Layout::column()
                .fixed_height(3)
                .flex_horizontal(1)
                .padding(Edges::all(1)),
        )?;
    }

    let frame = c.add_child_to(parent, Frame::new().with_title("Selector"))?;
    let items = ["Checked", "Also checked", "Unchecked"].map(String::from);
    let selector = c.add_child_to(frame, Selector::new(items.to_vec()))?;
    c.with_widget_mut(selector, |selector: &mut Selector<String>, ctx| {
        selector.toggle(ctx)?;
        selector.select_by(ctx, 1)?;
        selector.toggle(ctx)
    })?;
    c.set_layout_of(
        frame,
        Layout::column()
            .fixed_height(5)
            .flex_horizontal(1)
            .padding(Edges::all(1)),
    )?;
    Ok(())
}

/// Add highlighted, searched editors under `parent`.
fn add_syntax_samples(c: &mut dyn Context, parent: NodeId) -> Result<()> {
    let samples = [
        ("Rust", "rs", RUST_SAMPLE, "self"),
        ("Luau", "luau", LUAU_SAMPLE, "tabs"),
    ];
    for (title, extension, source, query) in samples {
        let config = EditorConfig::new()
            .with_read_only(true)
            .with_line_numbers(LineNumbers::Absolute)
            .with_wrap(WrapMode::None);
        let mut editor = Editor::with_config(source, config);
        editor.set_highlighter(Some(Box::new(SyntectHighlighter::new(extension))));
        let frame = c.add_child_to(parent, Frame::new().with_title(title))?;
        let editor = c.add_child_to(frame, editor)?;
        c.set_layout_of(editor, Layout::fill())?;
        c.with_widget_mut(editor, |editor: &mut Editor, ctx| {
            editor.search(ctx, query.to_string());
            Ok(())
        })?;
    }
    Ok(())
}

/// Root widget for the stylegym demo.
pub struct Stylegym {
    /// Whether the modal is currently shown.
    modal_visible: bool,
    /// Current theme index.
    current_theme: usize,
    /// The rules page, once mounted.
    rules: Option<TypedId<StyleSheet>>,
}

impl Default for Stylegym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Stylegym {
    /// Create a new stylegym instance.
    pub fn new() -> Self {
        Self {
            modal_visible: false,
            current_theme: 0,
            rules: None,
        }
    }

    /// Execute a closure with the right container widget.
    fn with_right_container<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut Stack, &mut dyn Context) -> Result<R>,
    {
        c.with_typed_slot::<RightContainerSlot, _>(f)
    }

    /// Execute a closure with the tabs that hold the style pages.
    fn with_tabs<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut Tabs, &mut dyn Context) -> Result<R>,
    {
        self.with_right_container(c, |_, ctx| {
            ctx.with_typed_slot::<MainFrameSlot, _>(|_, ctx| ctx.with_typed_slot::<TabsSlot, _>(f))
        })
    }

    /// Install a theme and list its rules.
    fn install_theme(&self, c: &mut dyn Context, builder: fn() -> StyleMap) -> Result<()> {
        let map = builder();
        if let Some(rules) = self.rules {
            c.with_widget_mut(rules, |sheet: &mut StyleSheet, _ctx| {
                sheet.set_rules(&map);
                Ok(())
            })?;
        }
        c.set_style(map);
        Ok(())
    }

    /// Show the tab at `index`.
    #[command]
    pub(crate) fn show_tab(&self, c: &mut dyn Context, index: usize) -> Result<()> {
        self.with_tabs(c, |tabs, ctx| tabs.select(ctx, index))
    }

    /// Move to another tab by a signed offset, wrapping around.
    #[command]
    pub(crate) fn next_tab(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.with_tabs(c, |tabs, ctx| tabs.select_by(ctx, delta))
    }

    /// Show the modal overlay.
    #[command]
    pub(crate) fn show_modal(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.modal_visible {
            return Ok(());
        }
        self.modal_visible = true;

        self.with_right_container(c, |_, ctx| {
            if ctx.has_slot::<ModalSlot>()? {
                return Ok(());
            }
            let modal_id = ctx.add_slot::<ModalSlot>(Center::new())?;
            let frame_id = ctx.add_child_to(modal_id, Frame::new().with_title("Demo Modal"))?;
            ctx.add_child_to(frame_id, ModalContent)?;

            let mut layout = Layout::fill().padding(Edges::all(1));
            layout.min_width = Some(35);
            layout.max_width = Some(40);
            layout.min_height = Some(5);
            layout.max_height = Some(7);
            ctx.set_layout_of(frame_id, layout)?;
            Ok(())
        })?;

        // Dim the style pages
        self.with_tabs(c, |_tabs, ctx| {
            ctx.push_effect(ctx.node_id(), effects::brightness(0.5))
        })?;

        Ok(())
    }

    /// Hide the modal overlay.
    #[command]
    pub(crate) fn hide_modal(&mut self, c: &mut dyn Context) -> Result<()> {
        if !self.modal_visible {
            return Ok(());
        }
        self.modal_visible = false;

        self.with_right_container(c, |_, ctx| {
            if let Some(modal_id) = ctx.child_slot(ModalSlot::KEY) {
                ctx.remove_subtree(modal_id)?;
            }
            Ok(())
        })?;

        // Re-apply user effects (clears dim, applies selected effects)
        self.apply_effects(c)?;

        Ok(())
    }

    /// Apply the selected theme from the dropdown.
    #[command]
    pub(crate) fn apply_theme(&mut self, c: &mut dyn Context) -> Result<()> {
        let Some((index, builder)) =
            c.try_with_unique_descendant::<Dropdown<ThemeOption>, _>(|dropdown, _ctx| {
                Ok((dropdown.selected_index(), dropdown.selected().builder))
            })?
        else {
            return Ok(());
        };

        if index != self.current_theme {
            self.current_theme = index;
            self.install_theme(c, builder)?;
        }
        Ok(())
    }

    /// Apply the selected effects from the selector to the style pages.
    #[command]
    pub(crate) fn apply_effects(&self, c: &mut dyn Context) -> Result<()> {
        let selected = c
            .try_with_unique_descendant::<Selector<EffectOption>, _>(|selector, _ctx| {
                Ok(selector
                    .selected_items()
                    .into_iter()
                    .map(|option| option.effect.clone())
                    .collect::<Vec<_>>())
            })?
            .unwrap_or_default();

        self.with_tabs(c, |_tabs, ctx| {
            ctx.clear_effects(ctx.node_id())?;
            for effect in selected {
                ctx.push_effect(ctx.node_id(), effect)?;
            }
            if self.modal_visible {
                ctx.push_effect(ctx.node_id(), effects::brightness(0.5))?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

impl Widget for Stylegym {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Row)
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        // Create left frame (controls) - preserve Frame's padding for border
        let left_frame_id = c.add_slot::<ControlsSlot>(Frame::new().with_title("Styles"))?;
        c.set_layout_of(
            left_frame_id,
            Layout::column()
                .fixed_width(32)
                .flex_vertical(1)
                .padding(Edges::all(1)),
        )?;

        // Create theme dropdown with its own frame - no fixed height so it can
        // expand
        let theme_frame_id = c.add_slot_to(
            left_frame_id,
            ThemeFrameSlot::KEY,
            Frame::new().with_title("Theme"),
        )?;
        c.add_slot_to(
            theme_frame_id,
            ThemeDropdownSlot::KEY,
            Dropdown::new(available_themes())?,
        )?;
        c.set_layout_of(
            theme_frame_id,
            Layout::column().flex_horizontal(1).padding(Edges::all(1)),
        )?;

        // Create effects selector with its own frame
        let effects_frame_id = c.add_slot_to(
            left_frame_id,
            EffectsFrameSlot::KEY,
            Frame::new().with_title("Effects"),
        )?;
        c.add_slot_to(
            effects_frame_id,
            EffectsSelectorSlot::KEY,
            Selector::new(available_effects()),
        )?;
        c.set_layout_of(effects_frame_id, Layout::fill().padding(Edges::all(1)))?;

        // Create right container with Stack layout for modal overlay
        let right_container_id = c.add_slot::<RightContainerSlot>(Stack(Direction::Stack))?;

        // Create the styles frame and its tabbed pages
        let styles_frame_id =
            c.add_slot_to(right_container_id, MainFrameSlot::KEY, Frame::new())?;
        let tabs_id = c.add_slot_to(styles_frame_id, TabsSlot::KEY, Tabs::new())?;
        let rules = c.with_widget_mut(tabs_id, |tabs: &mut Tabs, ctx| {
            tabs.add_tab(ctx, "Palette", StyleSheet::palette())?;
            let rules = tabs.add_tab(ctx, "Rules", StyleSheet::rules())?;
            let widgets = tabs.add_tab(ctx, "Widgets", Stack(Direction::Column))?;
            add_widget_samples(ctx, widgets.into())?;
            let syntax = tabs.add_tab(ctx, "Syntax", Stack(Direction::Row))?;
            add_syntax_samples(ctx, syntax.into())?;
            tabs.add_tab(ctx, "Text", TextSamples)?;
            Ok(rules)
        })?;
        self.rules = Some(rules);
        self.install_theme(c, available_themes()[self.current_theme].builder)
    }
}

impl Loader for Stylegym {
    fn load(c: &mut Canopy) -> Result<()> {
        Root::load(c)?;
        c.add_commands::<Self>()?;
        c.add_commands::<Dropdown<ThemeOption>>()?;
        c.add_commands::<Selector<EffectOption>>()?;
        c.add_commands::<Tabs>()?;
        c.add_commands::<StyleSheet>()?;
        c.add_commands::<Input>()?;
        c.add_commands::<Editor>()?;
        Ok(())
    }
}

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("stylegym", DEFAULT_BINDINGS)
}
