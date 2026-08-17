// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=crates/examples, visibility=public, auto_impls=false, blanket_impls=false

pub mod canopy_examples {
    //! Example widgets used by canopy demos.

    pub mod chargym {
        //! Char gym example nodes.
        //! Chargym: A Unicode width and wide character demo.

        /// Root node for the chargym demo.
        #[derive(Default)]
        pub struct CharGym {}

        impl CharGym {
            /// Construct a new chargym demo.
            pub fn new() -> Self {}
        }

        impl CommandNode for CharGym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for CharGym {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}
        }

        impl Loader for CharGym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the chargym demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod editorgym {
        //! Editor gym example nodes.

        /// Root widget for the editor gym demo.
        #[derive(Default)]
        pub struct EditorGym;

        impl EditorGym {
            /// Construct a new editor gym demo.
            pub fn new() -> Self {}

            /// Scroll the outer pane by one line in the specified direction.
            pub fn scroll(&mut self, c: &mut dyn Context, dir: geom::Direction) {}

            /// Page in the outer pane.
            pub fn page(&mut self, c: &mut dyn Context, dir: geom::Direction) {}

            /// Scroll up by one line.
            pub fn scroll_up(&mut self, c: &mut dyn Context) {}

            /// Scroll down by one line.
            pub fn scroll_down(&mut self, c: &mut dyn Context) {}

            /// Scroll left by one column.
            pub fn scroll_left(&mut self, c: &mut dyn Context) {}

            /// Scroll right by one column.
            pub fn scroll_right(&mut self, c: &mut dyn Context) {}

            /// Page up by one screen.
            pub fn page_up(&mut self, c: &mut dyn Context) {}

            /// Page down by one screen.
            pub fn page_down(&mut self, c: &mut dyn Context) {}

            /// Scroll the outer pane to an absolute content position.
            pub fn scroll_to(&mut self, c: &mut dyn Context, x: u32, y: u32) {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_up() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_down() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_left() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_right() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_page_up() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_page_down() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_to() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for EditorGym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for EditorGym {
            fn layout(&self) -> Layout {}

            fn canvas(&self, view: Size<u32>, ctx: &CanvasContext<'_>) -> Size<u32> {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}
        }

        impl Loader for EditorGym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the editor gym demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod focusgym {
        //! Focus gym example nodes.

        /// A focusable block that can split into children.
        pub struct Block {}

        impl Block {
            /// Return a typed command reference for this command.
            pub fn cmd_add() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_split() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_flex_grow_inc() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_flex_grow_dec() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_flex_shrink_inc() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_flex_shrink_dec() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_focus() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for Block {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for Block {
            fn accept_focus(&self, ctx: &dyn ViewContext) -> bool {}

            fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

            fn layout(&self) -> Layout {}
        }

        /// Root node for the focus gym demo.
        #[derive(Default)]
        pub struct FocusGym;

        impl FocusGym {
            /// Construct a new focus gym.
            pub fn new() -> Self {}

            /// Return a typed command reference for this command.
            pub fn cmd_delete_focused() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for FocusGym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for FocusGym {
            fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}
        }

        impl Loader for FocusGym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the focus gym demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod fontgym {
        //! Font gym example nodes.

        /// Demo node that renders ASCII font banners.
        #[derive(Default)]
        pub struct FontGym {}

        impl FontGym {
            /// Construct a new font gym demo.
            pub fn new() -> Self {}

            /// Trigger a redraw.
            pub fn redraw(&mut self, _ctx: &mut dyn Context) {}

            /// Return a typed command reference for this command.
            pub fn cmd_redraw() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for FontGym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for FontGym {
            fn layout(&self) -> Layout {}

            fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {}

            fn poll(&mut self, ctx: &mut dyn Context) -> Option<Duration> {}
        }

        impl Loader for FontGym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for focus navigation.
        pub fn setup_bindings(c: &mut canopy::Canopy) -> canopy::error::Result<()> {}
    }

    pub mod framegym {
        //! Frame gym example nodes.

        /// A widget that renders a test pattern.
        #[derive(Default)]
        pub struct TestPattern {}

        impl TestPattern {
            /// Construct the test pattern node.
            pub fn new() -> Self {}

            /// Scroll to an absolute content position.
            pub fn scroll_to(&mut self, c: &mut dyn Context, x: u32, y: u32) {}

            /// Scroll by one line in the specified direction.
            pub fn scroll(&mut self, c: &mut dyn Context, dir: Direction) {}

            /// Page in the specified direction.
            pub fn page(&mut self, c: &mut dyn Context, dir: Direction) {}

            /// Scroll up by one line.
            pub fn scroll_up(&mut self, c: &mut dyn Context) {}

            /// Scroll down by one line.
            pub fn scroll_down(&mut self, c: &mut dyn Context) {}

            /// Scroll left by one column.
            pub fn scroll_left(&mut self, c: &mut dyn Context) {}

            /// Scroll right by one column.
            pub fn scroll_right(&mut self, c: &mut dyn Context) {}

            /// Page up by one screen.
            pub fn page_up(&mut self, c: &mut dyn Context) {}

            /// Page down by one screen.
            pub fn page_down(&mut self, c: &mut dyn Context) {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_to() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_up() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_down() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_left() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_right() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_page_up() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_page_down() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for TestPattern {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for TestPattern {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn layout(&self) -> Layout {}

            fn measure(&self, c: MeasureConstraints) -> Measurement {}

            fn canvas(&self, _view: Size<u32>, _ctx: &CanvasContext<'_>) -> Size<u32> {}

            fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}
        }

        /// Root node for the frame gym demo.
        #[derive(Default)]
        pub struct FrameGym;

        impl FrameGym {
            /// Construct a new frame gym.
            pub fn new() -> Self {}
        }

        impl CommandNode for FrameGym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for FrameGym {
            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}
        }

        impl Loader for FrameGym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the frame gym demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod imgview {
        //! Image viewer example nodes.

        /// Configure key bindings for the image viewer.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod intervals {
        //! Intervals example nodes.

        /// Counter widget that increments on a timer.
        #[derive(Default)]
        pub struct CounterItem {}

        impl CounterItem {
            /// Construct a new counter item.
            pub fn new() -> Self {}

            /// Increment the counter.
            pub fn tick(&mut self, ctx: &mut dyn Context) -> Result<()> {}
        }

        impl Selectable for CounterItem {
            fn set_selected(&mut self, selected: bool) {}
        }

        impl CommandNode for CounterItem {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for CounterItem {
            fn layout(&self) -> Layout {}

            fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {}

            fn measure(&self, c: MeasureConstraints) -> Measurement {}

            fn render(&mut self, rndr: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn name(&self) -> NodeName {}
        }

        /// Status bar widget for the intervals demo.
        pub struct StatusBar;

        impl CommandNode for StatusBar {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for StatusBar {
            fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}
        }

        /// Root node for the intervals demo.
        #[derive(Default)]
        pub struct Intervals;

        impl Intervals {
            /// Construct a new intervals demo.
            pub fn new() -> Self {}

            /// Append a new list item.
            pub fn add_item(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Return a typed command reference for this command.
            pub fn cmd_add_item() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for Intervals {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for Intervals {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {}
        }

        impl Loader for Intervals {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the intervals demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod listgym {
        //! List gym example nodes.

        /// Focusable list entry that renders text content.
        pub struct ListEntry {}

        impl ListEntry {
            /// Construct a new list entry from a text widget.
            pub fn new(text: Text) -> Self {}
        }

        impl CommandNode for ListEntry {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Selectable for ListEntry {
            fn set_selected(&mut self, selected: bool) {}
        }

        impl Widget for ListEntry {
            fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

            fn measure(&self, c: MeasureConstraints) -> Measurement {}

            fn canvas(&self, view: Size<u32>, ctx: &CanvasContext<'_>) -> Size<u32> {}

            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn name(&self) -> NodeName {}
        }

        /// Status bar widget for the list gym demo.
        #[derive(Default)]
        pub struct StatusBar;

        impl StatusBar {
            /// Construct a status bar.
            pub fn new() -> Self {}
        }

        impl CommandNode for StatusBar {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for StatusBar {
            fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}
        }

        /// Root node for the list gym demo.
        #[derive(Default)]
        pub struct ListGym;

        impl ListGym {
            /// Construct a new list gym demo.
            pub fn new() -> Self {}

            /// Add an item after the current focus.
            pub fn add_item(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Add an item at the end of the list.
            pub fn append_item(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Clear all items from the list.
            pub fn clear(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Add a new column containing a list.
            pub fn add_column(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Delete the focused column.
            pub fn delete_column(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Return a typed command reference for this command.
            pub fn cmd_add_item() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_append_item() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_clear() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_add_column() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_delete_column() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for ListGym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for ListGym {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}
        }

        impl Loader for ListGym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the list gym demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod pager {
        //! Pager example nodes.

        /// Simple pager widget for file contents.
        pub struct Pager {}

        impl Pager {
            /// Construct a pager with initial contents.
            pub fn new(contents: &str) -> Self {}
        }

        impl CommandNode for Pager {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for Pager {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, _rndr: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}
        }

        impl Loader for Pager {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the pager demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod stylegym {
        //! Stylegym example nodes.
        //! Stylegym: A demonstration app for Canopy's styling features.
        //!
        //! This example showcases themes, effects, and modal overlays in a two-pane layout.

        /// Theme option for the dropdown.
        #[derive(Clone)]
        pub struct ThemeOption {
            /// Theme display name.
            pub name: &'static str,
            /// Function to build the theme's StyleMap.
            pub builder: fn() -> canopy::style::StyleMap,
        }

        impl Label for ThemeOption {
            fn label(&self) -> &str {}
        }

        /// Effect option for the selector.
        #[derive(Clone)]
        pub struct EffectOption {
            /// Effect display name.
            pub name: &'static str,
        }

        impl Label for EffectOption {
            fn label(&self) -> &str {}
        }

        /// The demo content pane showing styled samples.
        pub struct DemoContent;

        impl CommandNode for DemoContent {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for DemoContent {
            fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

            fn layout(&self) -> Layout {}
        }

        /// Root widget for the stylegym demo.
        #[derive(Default)]
        pub struct Stylegym {}

        impl Stylegym {
            /// Create a new stylegym instance.
            pub fn new() -> Self {}

            /// Show the modal overlay.
            pub fn show_modal(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Hide the modal overlay.
            pub fn hide_modal(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Apply the selected theme from the dropdown.
            pub fn apply_theme(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Apply the selected effects from the selector to the demo pane.
            pub fn apply_effects(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Return a typed command reference for this command.
            pub fn cmd_show_modal() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_hide_modal() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_apply_theme() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_apply_effects() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for Stylegym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for Stylegym {
            fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn layout(&self) -> Layout {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}
        }

        impl Loader for Stylegym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Set up key bindings for the stylegym demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod termgym {
        //! Terminal gym example nodes.

        /// Multi-terminal demo widget.
        #[derive(Default)]
        pub struct TermGym {}

        impl TermGym {
            /// Construct the terminal gym demo.
            pub fn new() -> Self {}

            /// Create a new terminal instance.
            pub fn new_terminal(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Create a new terminal instance while keeping focus on the sidebar.
            pub fn new_terminal_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Switch to the next terminal instance.
            pub fn next_terminal(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Switch to the previous terminal instance.
            pub fn prev_terminal(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Switch to the next terminal while keeping focus on the sidebar.
            pub fn next_terminal_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Switch to the previous terminal while keeping focus on the sidebar.
            pub fn prev_terminal_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Delete the active terminal and keep focus on the sidebar.
            pub fn delete_terminal(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Focus the terminal list sidebar.
            pub fn focus_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Focus the active terminal instance.
            pub fn focus_active_terminal(&mut self, c: &mut dyn Context) -> Result<()> {}

            /// Activate a terminal from a sidebar row selection.
            pub fn activate_terminal(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {}

            /// Return a typed command reference for this command.
            pub fn cmd_new_terminal() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_new_terminal_sidebar() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_next_terminal() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_prev_terminal() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_next_terminal_sidebar() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_prev_terminal_sidebar() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_delete_terminal() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_focus_sidebar() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_focus_active_terminal() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_activate_terminal() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for TermGym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for TermGym {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}
        }

        impl Loader for TermGym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings and styles for the terminal gym demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod textgym {
        //! Text gym example nodes.

        /// Demo node that displays multiple text variants.
        #[derive(Default)]
        pub struct TextGym;

        impl TextGym {
            /// Construct a new text gym demo.
            pub fn new() -> Self {}

            /// Trigger a redraw.
            pub fn redraw(&mut self, _ctx: &mut dyn Context) {}

            /// Return a typed command reference for this command.
            pub fn cmd_redraw() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for TextGym {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for TextGym {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}
        }

        impl Loader for TextGym {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the text gym demo.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}
    }

    pub mod widget {
        //! Widget demo nodes.
        //! Widget demo entry points.

        /// Font widget configuration.
        pub struct FontDemo {}

        impl FontDemo {
            /// Build a font demo widget.
            pub fn new(
                text: impl Into<String>,
                fonts: Vec<FontSource>,
                interval: Duration,
                exit_after_cycle: bool,
                effects: FontEffects,
            ) -> Self {
            }
        }

        impl Widget for FontDemo {
            fn layout(&self) -> Layout {}

            fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {}

            fn poll(&mut self, ctx: &mut dyn Context) -> Option<Duration> {}

            fn render(&mut self, _rndr: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn name(&self) -> NodeName {}
        }

        /// Font source data for demo cycling.
        #[derive(Debug, Clone)]
        pub struct FontSource {}

        impl FontSource {
            /// Build a font source from raw bytes.
            pub fn new(label: impl Into<String>, bytes: Vec<u8>) -> Self {}
        }

        /// Terminal demo widget with three commands.
        #[derive(Default)]
        pub struct TermDemo {}

        impl TermDemo {
            /// Construct a terminal demo.
            pub fn new() -> Self {}

            /// Cycle to the next tab.
            pub fn next_tab(&mut self, ctx: &mut dyn Context) -> Result<()> {}

            /// Return a typed command reference for this command.
            pub fn cmd_next_tab() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for TermDemo {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for TermDemo {
            fn layout(&self) -> Layout {}

            fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, rndr: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn name(&self) -> NodeName {}
        }

        /// Common sizing configuration for widget demos.
        #[derive(Debug, Clone, Copy, Default)]
        pub struct DemoSize {
            /// Optional fixed width override.
            pub width: Option<u32>,
            /// Optional fixed height override.
            pub height: Option<u32>,
        }

        impl DemoSize {
            /// Create sizing overrides.
            pub fn new(width: Option<u32>, height: Option<u32>) -> Self {}
        }

        /// Host widget that centers a padded child within optional sizing overrides.
        pub struct DemoHost {}

        impl DemoHost {
            /// Build a demo host for the provided widget.
            pub fn new(child: impl Into<Box<dyn Widget>>, size: DemoSize, frame: bool) -> Self {}

            /// Set the inner padding for the demo host.
            pub fn with_inner_padding(self, padding: u32) -> Self {}

            /// Set the outer padding for the demo host.
            pub fn with_outer_padding(self, padding: u32) -> Self {}
        }

        impl Widget for DemoHost {
            fn layout(&self) -> Layout {}

            fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {}

            fn render(&mut self, _rndr: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn name(&self) -> NodeName {}
        }

        /// List widget configuration.
        #[derive(Default)]
        pub struct ListDemo {}

        impl ListDemo {
            /// Build a list demo widget.
            pub fn new(interval: Duration) -> Self {}
        }

        impl Widget for ListDemo {
            fn layout(&self) -> Layout {}

            fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {}

            fn poll(&mut self, ctx: &mut dyn Context) -> Option<Duration> {}

            fn name(&self) -> NodeName {}
        }
    }

    pub mod widget_editor {
        //! Widget editor example nodes.

        /// Widget editor example that opens a Rust file with syntax highlighting.
        pub struct WidgetEditor {}

        impl WidgetEditor {
            /// Construct a widget editor from file contents and metadata.
            pub fn new(
                contents: impl Into<String>,
                extension: impl Into<String>,
                title: impl Into<String>,
            ) -> Self {
            }
        }

        impl CommandNode for WidgetEditor {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for WidgetEditor {
            fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}
        }

        impl Loader for WidgetEditor {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Install key bindings for the widget editor example.
        pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {}

        /// Return a lowercase file extension hint for syntax selection.
        pub fn file_extension(path: &std::path::Path) -> String {}

        /// Return a short title for the editor frame.
        pub fn file_title(path: &std::path::Path) -> String {}
    }

    /// Finalize and print the Luau API definitions for a demo app.
    pub fn print_luau_api(cnpy: &mut canopy::Canopy) -> canopy::error::Result<()> {}

    /// Install one demo app under a root and run the terminal loop.
    pub fn run_demo<T: Widget + Loader + 'static>(
        cnpy: canopy::Canopy,
        app: T,
        inspector: bool,
    ) -> canopy::error::Result<i32> {
    }
}
