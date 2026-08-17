// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=crates/canopy, visibility=public, auto_impls=false, blanket_impls=false

pub mod canopy {
    //! Canopy: A terminal UI library.
    //!
    //! Canopy is a terminal UI library for building interactive applications.
    //! It provides an arena-based widget system with focus management, styling,
    //! and event handling.
    //!
    //! # Quick Start
    //!
    //! The main entry points are:
    //! - [`Canopy`] - The core application state
    //! - [`Widget`] - The trait implemented by all widgets
    //! - [`Context`] - The mutation API available to widgets
    //!
    //! # Module Organization
    //!
    //! - [`geom`] - Geometry primitives (Rect, Point, Size, etc.)

    pub mod layout {
        //! Layout types for configuring node positioning and sizing.

        /// Stack direction for children.
        #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
        pub enum Direction {
            /// Stack children vertically (column).
            Column,
            /// Stack children horizontally (row).
            Row,
            /// Children overlap in the same space (painter's algorithm - last child on top).
            Stack,
        }

        impl Direction {
            /// Size along the main axis.
            pub fn main_size(&self, size: Size<u32>) -> u32 {}

            /// Size along the cross axis.
            pub fn cross_size(&self, size: Size<u32>) -> u32 {}

            /// Construct a size from main and cross axis values.
            pub fn size_from_main_cross(&self, main: u32, cross: u32) -> Size<u32> {}
        }

        /// Alignment along an axis.
        #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
        pub enum Align {
            /// Align to the start of the axis.
            Start,
            /// Align to the center of the axis.
            Center,
            /// Align to the end of the axis.
            End,
        }

        /// Display mode for layout participation.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum Display {
            /// Node participates in layout and rendering.
            Block,
            /// Node is removed from layout and not rendered.
            None,
        }

        /// Sizing strategy for a single axis.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum Sizing {
            /// Size derives from `measure()` or wrapping children.
            Measure,
            /// Weighted share of remaining space along the axis.
            Flex(u32),
        }

        /// Invalid layout configuration.
        #[derive(Clone, Debug, StructuralPartialEq, PartialEq, Eq, Error, Display)]
        pub enum LayoutValidationError {
            /// A minimum bound exceeds the corresponding maximum bound.
            MinExceedsMax {
                /// Layout axis name.
                axis: &'static str,
                /// Minimum bound.
                min: u32,
                /// Maximum bound.
                max: u32,
            },
            /// A flex sizing strategy has a zero weight.
            ZeroFlexWeight {
                /// Layout axis name.
                axis: &'static str,
            },
            /// Padding on an axis overflows `u32`.
            PaddingOverflow {
                /// Padding axis name.
                axis: &'static str,
            },
        }

        impl From<LayoutValidationError> for Error {
            fn from(source: LayoutValidationError) -> Self {}
        }

        /// Edge insets for padding.
        #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
        pub struct Edges<T> {
            /// Top edge.
            pub top: T,
            /// Right edge.
            pub right: T,
            /// Bottom edge.
            pub bottom: T,
            /// Left edge.
            pub left: T,
        }

        impl<T: Copy> Edges<T> {
            /// Create edges with uniform length on all sides.
            pub fn all(v: T) -> Self {}

            /// Create edges with symmetric vertical and horizontal lengths.
            pub fn symmetric(vertical: T, horizontal: T) -> Self {}

            /// Create edges from individual values.
            pub fn new(top: T, right: T, bottom: T, left: T) -> Self {}
        }

        impl Edges<u32> {
            /// Total horizontal padding.
            pub fn horizontal(&self) -> u32 {}

            /// Total vertical padding.
            pub fn vertical(&self) -> u32 {}
        }

        /// Size with width and height.
        pub type Size<T = u32> = crate::geom::Size<T>;

        /// Layout configuration for a node.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Default)]
        pub struct Layout {
            /// Whether this node participates in layout/render.
            pub display: Display,
            /// Stack direction for children.
            pub direction: Direction,
            /// Width sizing strategy (outer size).
            pub width: Sizing,
            /// Height sizing strategy (outer size).
            pub height: Sizing,
            /// Minimum outer width constraint (cells).
            pub min_width: Option<u32>,
            /// Maximum outer width constraint (cells).
            pub max_width: Option<u32>,
            /// Minimum outer height constraint (cells).
            pub min_height: Option<u32>,
            /// Maximum outer height constraint (cells).
            pub max_height: Option<u32>,
            /// Allow horizontal overflow during measurement.
            pub overflow_x: bool,
            /// Allow vertical overflow during measurement.
            pub overflow_y: bool,
            /// Structural padding inside the widget (cells).
            pub padding: Edges<u32>,
            /// Gap between children along the main axis (cells).
            pub gap: u32,
            /// Horizontal alignment of children within the content area.
            ///
            /// For rows this aligns the complete child group on the main axis. For
            /// columns it aligns each child on the cross axis. Stacks align each child.
            pub align_horizontal: Align,
            /// Vertical alignment of children within the content area.
            ///
            /// For columns this aligns the complete child group on the main axis. For
            /// rows it aligns each child on the cross axis. Stacks align each child.
            pub align_vertical: Align,
        }

        impl Layout {
            /// Column layout with measured sizing on both axes.
            pub fn column() -> Self {}

            /// Row layout with measured sizing on both axes.
            pub fn row() -> Self {}

            /// Stack layout where children overlap in the same space.
            pub fn stack() -> Self {}

            /// Fill available space with flex sizing on both axes.
            pub fn fill() -> Self {}

            /// Remove this node from layout and rendering.
            pub fn none(self) -> Self {}

            /// Set width to flex with the provided weight.
            ///
            /// A zero weight is rejected when the layout is applied.
            pub fn flex_horizontal(self, weight: u32) -> Self {}

            /// Set height to flex with the provided weight.
            ///
            /// A zero weight is rejected when the layout is applied.
            pub fn flex_vertical(self, weight: u32) -> Self {}

            /// Set the minimum outer width.
            pub fn min_width(self, n: u32) -> Self {}

            /// Set the maximum outer width.
            pub fn max_width(self, n: u32) -> Self {}

            /// Set the minimum outer height.
            pub fn min_height(self, n: u32) -> Self {}

            /// Set the maximum outer height.
            pub fn max_height(self, n: u32) -> Self {}

            /// Allow horizontal overflow during measurement.
            pub fn overflow_x(self) -> Self {}

            /// Allow vertical overflow during measurement.
            pub fn overflow_y(self) -> Self {}

            /// Inherit overflow permission from an enclosing layout.
            ///
            /// Overflow only widens: a layout that already allows overflow on an axis keeps it.
            pub fn inherit_overflow(&mut self, x: bool, y: bool) {}

            /// Convenience: fixed outer width without a `Fixed` enum.
            pub fn fixed_width(self, n: u32) -> Self {}

            /// Convenience: fixed outer height without a `Fixed` enum.
            pub fn fixed_height(self, n: u32) -> Self {}

            /// Set padding edges.
            pub fn padding(self, edges: Edges<u32>) -> Self {}

            /// Set the main-axis gap between children.
            pub fn gap(self, n: u32) -> Self {}

            /// Set horizontal alignment of children within content area.
            pub fn align_horizontal(self, align: Align) -> Self {}

            /// Set vertical alignment of children within content area.
            pub fn align_vertical(self, align: Align) -> Self {}

            /// Center children both horizontally and vertically.
            pub fn align_center(self) -> Self {}

            /// Set the layout direction.
            pub fn direction(self, direction: Direction) -> Self {}

            /// Set width sizing strategy directly.
            pub fn width(self, sizing: Sizing) -> Self {}

            /// Set height sizing strategy directly.
            pub fn height(self, sizing: Sizing) -> Self {}

            /// Set both axes to Measure sizing.
            pub fn measured(self) -> Self {}

            /// Validate this layout configuration.
            pub fn validate(&self) -> Result<(), LayoutValidationError> {}
        }

        /// Content-box measurement constraints.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Hash)]
        pub enum Constraint {
            /// No constraint on this axis.
            Unbounded,
            /// The engine guarantees at most n cells on this axis.
            AtMost(u32),
            /// The engine guarantees exactly n cells on this axis.
            Exact(u32),
        }

        impl Constraint {
            /// Return true if this constraint is exact.
            pub fn is_exact(self) -> bool {}

            /// Return the maximum bound implied by the constraint.
            pub fn max_bound(self) -> u32 {}
        }

        /// Constraints for measuring a widget's content box.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Hash)]
        pub struct MeasureConstraints {
            /// Width constraint.
            pub width: Constraint,
            /// Height constraint.
            pub height: Constraint,
        }

        impl MeasureConstraints {
            /// Leaf widgets: clamp a content size to these constraints and return Fixed.
            pub fn clamp(&self, content: Size<u32>) -> Measurement {}

            /// Containers: request wrapping.
            pub fn wrap(&self) -> Measurement {}

            /// Clamp a size to these constraints.
            pub fn clamp_size(&self, content: Size<u32>) -> Size<u32> {}

            /// True if the main axis is exact.
            pub fn main_is_exact(&self, direction: Direction) -> bool {}

            /// True if the cross axis is exact.
            pub fn cross_is_exact(&self, direction: Direction) -> bool {}

            /// Return the main axis constraint.
            pub fn main(&self, direction: Direction) -> Constraint {}

            /// Return the cross axis constraint.
            pub fn cross(&self, direction: Direction) -> Constraint {}
        }

        /// Result of measuring a widget's content box.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum Measurement {
            /// Fixed content size for leaf widgets.
            Fixed(Size<u32>),
            /// Wrap children: engine computes content size from children.
            Wrap,
        }

        /// Canvas context for computing scrollable extents.
        pub struct CanvasContext<'a> {}

        impl<'a> CanvasContext<'a> {
            /// Construct a canvas context from a child slice.
            pub fn new(children: &'a [CanvasChild]) -> Self {}

            /// Child layout results in this node's content coordinate space.
            pub fn children(&self) -> &[CanvasChild] {}

            /// Extent of children outer rects.
            pub fn children_extent(&self) -> Size<u32> {}
        }

        /// Child layout results for canvas computations.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub struct CanvasChild {
            /// Child outer rect relative to this node's content origin.
            pub rect: crate::geom::Rect,
            /// Child canvas size in the child's content coordinates.
            pub canvas: Size<u32>,
        }

        impl CanvasChild {
            /// Construct a new canvas child.
            pub fn new(rect: Rect, canvas: Size<u32>) -> Self {}
        }
    }

    pub mod prelude {
        //! Convenience re-exports for common Canopy types.

        /// Application runtime state and renderer coordination.
        pub struct Canopy {}

        impl super::Canopy {
            /// Render the widget tree. All visible nodes are rendered.
            pub fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {}

            /// Service a bounded batch of callbacks marshalled onto the UI thread.
            ///
            /// Custom run loops should call this after receiving [`Event::Wake`]. The return value is the
            /// number of callbacks executed during this turn.
            pub fn service_automation(&mut self) -> usize {}

            /// Set the size on the root node.
            pub fn set_root_size(&mut self, size: Size) -> Result<()> {}

            /// Construct a new Canopy instance.
            pub fn new() -> Self {}

            /// Return a handle for submitting automation work to this app's UI thread.
            pub fn automation_handle(&self) -> AutomationHandle {}

            /// Mark the visible application state for redraw.
            pub fn request_redraw(&mut self) {}

            /// Return the root node ID.
            pub fn root_id(&self) -> NodeId {}

            /// Replace the visible render-target limits.
            pub fn set_render_limits(&mut self, limits: RenderLimits) -> Result<()> {}

            /// Create a detached widget node.
            pub fn create_detached<W>(&mut self, widget: W) -> Result<TypedId<W>>
            where
                W: Widget + 'static, {
            }

            /// Replace the root's children with a single node.
            pub fn set_root_child(&mut self, child: impl Into<NodeId>) -> Result<()> {}

            /// Replace the root widget while preserving its stable node ID.
            pub fn replace_root<W>(&mut self, widget: W) -> Result<TypedId<W>>
            where
                W: Widget + 'static, {
            }

            /// Return the active style map.
            pub fn style(&self) -> &StyleMap {}

            /// Mutate the active style map before the next render.
            pub fn style_mut(&mut self) -> &mut StyleMap {}

            /// Replace the active style map before the next render.
            pub fn set_style(&mut self, style: StyleMap) {}

            /// Get a reference to the current render buffer, if any.
            pub fn buf(&self) -> Option<&TermBuf> {}

            /// Run a compiled script by id on the target node.
            pub fn run_script(
                &mut self,
                node_id: impl Into<NodeId>,
                sid: script::ScriptId,
            ) -> Result<()> {
            }

            /// Compile a script and return its identifier.
            pub fn compile_script(&mut self, source: &str) -> Result<script::ScriptId> {}

            /// Evaluate a Luau source string in the current app context.
            pub fn eval_script(&mut self, source: &str) -> Result<()> {}

            /// Evaluate a Luau source string and return its value.
            pub fn eval_script_value(&mut self, source: &str) -> Result<commands::ArgValue> {}

            /// Evaluate a Luau source string with a cooperative timeout.
            pub fn eval_script_value_with_timeout(
                &mut self,
                source: &str,
                timeout: Duration,
            ) -> Result<commands::ArgValue> {
            }

            /// Configure the `@user` persistent script root.
            pub fn set_user_script_root(&mut self, root: impl Into<PathBuf>) -> Result<()> {}

            /// Configure the `@project` persistent script root.
            pub fn set_project_script_root(&mut self, root: impl Into<PathBuf>) -> Result<()> {}

            /// Invalidate cached exports from persistent script modules.
            ///
            /// Pass a root such as `@user` or `@project` to invalidate one root, or `None` to
            /// invalidate every root. Returns the new source epoch, or `None` when no module source
            /// is configured or the named root is unknown.
            pub fn invalidate_script_modules(&mut self, root: Option<&str>) -> Option<u64> {}

            /// Register an audited Ruau native module on the same surface as Canopy commands.
            pub fn register_script_module(&mut self, module: Arc<dyn NativeModule>) -> Result<()> {}

            /// Register an app-level startup script.
            pub fn register_startup_script(&mut self, name: &str, source: &str) -> Result<()> {}

            /// Require every startup script root to define a typed global.
            pub fn require_startup_global(&mut self, name: &str, type_text: &str) -> Result<()> {}

            /// Run app, user, and project startup scripts once.
            pub fn run_startup_scripts(&mut self) -> Result<usize> {}

            /// Register a Luau script as the default bindings for a widget namespace.
            pub fn register_default_bindings(&mut self, name: &str, script: &str) -> Result<()> {}

            /// Register a named fixture available to headless and live automation.
            pub fn register_fixture(&mut self, fixture: Fixture) -> Result<()> {}

            /// Return registered fixture metadata in stable name order.
            pub fn fixture_infos(&self) -> Vec<FixtureInfo> {}

            /// Apply a named fixture to the current app instance.
            pub fn apply_fixture(&mut self, name: &str) -> Result<()> {}

            /// Run a closure against the root context.
            pub fn with_root_context<R>(
                &mut self,
                f: impl FnOnce(&mut dyn crate::Context) -> Result<R>,
            ) -> Result<R> {
            }

            /// Run a closure against a mutable context bound to a node.
            pub fn with_context<R>(
                &mut self,
                node: impl Into<NodeId>,
                f: impl FnOnce(&mut dyn crate::Context) -> Result<R>,
            ) -> Result<R> {
            }

            /// Run a closure against an immutable view of the root context.
            pub fn with_root_view<R>(&self, f: impl FnOnce(&dyn crate::ViewContext) -> R) -> R {}

            /// Run a closure against an immutable view context bound to a node.
            pub fn with_view<R>(
                &self,
                node: impl Into<NodeId>,
                f: impl FnOnce(&dyn crate::ViewContext) -> R,
            ) -> Result<R> {
            }

            /// Type-check a named Luau source against the finalized app API.
            pub fn check_script(
                &mut self,
                source_name: &str,
                source: &str,
            ) -> Result<script::ScriptCheckResult> {
            }

            /// Drain and return log lines recorded by the most recent script evaluation.
            pub fn take_script_logs(&self) -> Vec<String> {}

            /// Drain and return assertion outcomes from the most recent script evaluation.
            pub fn take_script_assertions(&self) -> Vec<script::ScriptAssertion> {}

            /// Return the in-memory script evaluation journal.
            ///
            /// The journal retains the most recent entries up to the configured limit.
            /// Entry ids are monotonic and never reused, so a first id greater than
            /// one indicates that older entries were evicted or cleared.
            pub fn script_journal(&self) -> &[ScriptJournalEntry] {}

            /// Set the maximum number of retained script journal entries.
            ///
            /// When the journal exceeds the limit the oldest entries are evicted. A
            /// limit of zero disables retention entirely.
            pub fn set_script_journal_limit(&mut self, limit: usize) {}

            /// Evaluate a Luau config file from disk.
            pub fn run_config(&mut self, path: &FsPath) -> Result<()> {}

            /// Remove a binding by ID. Returns true if a binding was removed.
            pub fn unbind(&mut self, id: inputmap::BindingId) -> bool {}

            /// Remove bindings for an input, optionally filtered by mode and path.
            pub fn unbind_input(
                &mut self,
                input: inputmap::InputSpec,
                mode: Option<&str>,
                path_filter: Option<&str>,
            ) -> usize {
            }

            /// Remove all bindings from all modes.
            pub fn clear_bindings(&mut self) -> usize {}

            /// Return bindings in a mode that match a specific path.
            pub fn bindings_matching_path(
                &self,
                mode: &str,
                path: &Path,
            ) -> Vec<inputmap::MatchedBindingInfo<'_>> {
            }

            /// Return the active input mode.
            pub fn input_mode(&self) -> &str {}

            /// Set the active input mode.
            pub fn set_input_mode(&mut self, mode: &str) -> Result<()> {}

            /// Push an input mode above the current mode.
            pub fn push_input_mode(&mut self, mode: &str) -> Result<()> {}

            /// Pop the top input mode and return the new active mode.
            pub fn pop_input_mode(&mut self) -> &str {}

            /// Bind a key or mouse input to switch the active input mode.
            pub fn bind_input_mode(
                &mut self,
                mode: &str,
                input: inputmap::InputSpec,
                path_filter: &str,
                next_mode: &str,
            ) -> Result<inputmap::BindingId> {
            }

            /// Return the most recent key or mouse route trace.
            pub fn route_trace(&self) -> &[RouteTraceEntry] {}

            /// Load the commands from a command node using the default node name.
            /// Returns an error if any command id is already registered.
            pub fn add_commands<T: commands::CommandNode>(&mut self) -> Result<()> {}

            /// Finalize the script API surface for this app.
            pub fn finalize_api(&mut self) -> Result<()> {}

            /// Return the current script API finalization state.
            pub fn script_api_state(&self) -> ScriptApiState {}

            /// Return the rendered Luau definition file for a ready app.
            pub fn script_api(&self) -> Result<&str> {}

            /// Return command availability from the current focus position.
            ///
            /// This computes which commands would resolve to a target if dispatched from the current
            /// focus. For each command:
            /// - Free commands always have `resolution = Some(Free)`
            /// - Node-routed commands have `resolution = Some(Subtree{..})` or `Some(Ancestor{..})`
            ///   if a matching node exists, `None` otherwise
            pub fn command_availability_from_focus(
                &self,
            ) -> Vec<commands::CommandAvailability<'_>> {
            }

            /// Return command availability from a specific node.
            ///
            /// Computes which commands would dispatch to a target, using the same resolution logic
            /// as `commands::dispatch`:
            /// 1. First search the subtree rooted at `start` in pre-order
            /// 2. Then walk ancestors
            pub fn command_availability_from_node(
                &self,
                start: NodeId,
            ) -> Vec<commands::CommandAvailability<'_>> {
            }

            /// Generate a contextual help snapshot for the current focus.
            ///
            /// The snapshot includes:
            /// - Bindings that would match from the focus path
            /// - Commands with their availability status
            pub fn help_snapshot(&self) -> super::help::HelpSnapshot<'_> {}

            /// Build a diagnostic dump with tree, focus, and binding details.
            pub fn diagnostic_dump(&self, target: NodeId) -> String {}
        }

        /// Outcome of an accepted state mutation.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum ChangeOutcome {
            /// The requested state was already active.
            Unchanged,
            /// The request changed state.
            Changed,
        }

        impl ChangeOutcome {
            /// Return whether the request changed state.
            pub fn changed(self) -> bool {}
        }

        /// A typed key for keyed children.
        ///
        /// This trait associates a string key with a specific widget type, providing
        /// compile-time type safety for keyed child access.
        ///
        /// Use the [`crate::key!`] macro to define keys:
        ///
        /// ```
        /// use canopy::{ChildKey, Widget, key};
        ///
        /// pub struct Modal;
        /// impl Widget for Modal {}
        ///
        /// key!(ModalSlot: Modal);
        /// assert_eq!(ModalSlot::KEY, "ModalSlot");
        /// ```
        pub trait ChildKey {
            type Widget: Widget + 'static;
            const KEY: &'static str;
        }

        pub use crate::CommandArg;
        pub use crate::CommandEnum;
        /// Mutable context available to widgets during event handling.
        pub trait Context: ViewContext {
            /// Focus an attached node.
            fn set_focus(&mut self, node: NodeId) -> Result<ChangeOutcome>;

            /// Move focus in a direction within an explicit scope.
            fn focus_dir(&mut self, scope: FocusScope, dir: Direction) -> Result<ChangeOutcome>;

            /// Focus the first focusable node within an explicit scope.
            fn focus_first(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

            /// Focus the next focusable node within an explicit scope.
            fn focus_next(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

            /// Focus the previous focusable node within an explicit scope.
            fn focus_prev(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

            /// Capture mouse events for the current node.
            fn capture_mouse(&mut self) -> Result<ChangeOutcome>;

            /// Release mouse capture if held by the current node.
            fn release_mouse(&mut self) -> Result<ChangeOutcome>;

            /// Scroll the view to the specified position. Returns `true` if movement occurred.
            fn scroll_to(&mut self, x: u32, y: u32) -> bool;

            /// Scroll the view by the given offsets. Returns `true` if movement occurred.
            fn scroll_by(&mut self, x: i32, y: i32) -> bool;

            /// Scroll the view up by one page. Returns `true` if movement occurred.
            fn page_up(&mut self) -> bool {}

            /// Scroll the view down by one page. Returns `true` if movement occurred.
            fn page_down(&mut self) -> bool {}

            /// Scroll the view up by one line. Returns `true` if movement occurred.
            fn scroll_up(&mut self) -> bool {}

            /// Scroll the view down by one line. Returns `true` if movement occurred.
            fn scroll_down(&mut self) -> bool {}

            /// Scroll the view left by one line. Returns `true` if movement occurred.
            fn scroll_left(&mut self) -> bool {}

            /// Scroll the view right by one line. Returns `true` if movement occurred.
            fn scroll_right(&mut self) -> bool {}

            /// Mark this node dirty so the next frame re-runs layout.
            fn invalidate_layout(&mut self);

            /// Update the layout for the current node.
            fn with_layout(&mut self, f: &mut dyn FnMut(&mut Layout)) -> Result<()> {}

            /// Update the layout for a specific node.
            fn with_layout_of(
                &mut self,
                node: NodeId,
                f: &mut dyn FnMut(&mut Layout),
            ) -> Result<()>;

            /// Create a new widget node detached from the tree.
            fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> Result<NodeId>;

            /// Apply a related set of tree mutations atomically.
            fn apply_tree_edit(
                &mut self,
                edit: &mut dyn FnMut(&mut dyn Context) -> Result<()>,
            ) -> Result<()>;

            /// Execute a closure with mutable access to a widget and its node-bound context.
            fn with_widget_mut(
                &mut self,
                node: NodeId,
                f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
            ) -> Result<()>;

            /// Dispatch a command relative to this node.
            fn dispatch_command(
                &mut self,
                cmd: &CommandInvocation,
            ) -> StdResult<ArgValue, CommandError>;

            /// Dispatch a command with an explicit command-scope frame.
            fn dispatch_command_scoped(
                &mut self,
                frame: CommandScopeFrame,
                cmd: &CommandInvocation,
            ) -> StdResult<ArgValue, CommandError>;

            /// Return the current event snapshot for injection.
            fn current_event(&self) -> Option<&Event>;

            /// Return the current mouse event for injection.
            fn current_mouse_event(&self) -> Option<MouseEvent>;

            /// Return the current list-row context for injection.
            fn current_list_row(&self) -> Option<ListRowContext>;

            /// Add a boxed widget as a child of a specific parent and return the new node ID.
            fn add_child_to_boxed(
                &mut self,
                parent: NodeId,
                widget: Box<dyn Widget>,
            ) -> Result<NodeId>;

            /// Add a boxed widget as a keyed child of a specific parent and return the new node ID.
            fn add_child_to_keyed_boxed(
                &mut self,
                parent: NodeId,
                key: &str,
                widget: Box<dyn Widget>,
            ) -> Result<NodeId>;

            /// Attach a detached child to a parent.
            fn attach(&mut self, parent: NodeId, child: NodeId) -> Result<()>;

            /// Attach a detached child to a parent using a unique key.
            fn attach_keyed(&mut self, parent: NodeId, key: &str, child: NodeId) -> Result<()>;

            /// Detach a child from its parent.
            fn detach(&mut self, child: NodeId) -> Result<()>;

            /// Remove a node and all descendants from the arena.
            fn remove_subtree(&mut self, node: NodeId) -> Result<()>;

            /// Replace the children list for the current node.
            fn set_children(&mut self, children: Vec<NodeId>) -> Result<()> {}

            /// Replace the children list for a specific parent node.
            fn set_children_of(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()>;

            /// Set the current node's visibility.
            fn set_hidden(&mut self, hidden: bool) -> Result<ChangeOutcome> {}

            /// Set a specific node's visibility.
            fn set_hidden_of(&mut self, node: NodeId, hidden: bool) -> Result<ChangeOutcome>;

            /// Request a cooperative shutdown with the provided status code.
            fn exit(&mut self, code: i32);

            /// Add an effect to a node that will be applied during rendering.
            /// Effects stack and inherit through the tree.
            fn push_effect(&mut self, node: NodeId, effect: Effect) -> Result<()>;

            /// Clear all effects on a node.
            fn clear_effects(&mut self, node: NodeId) -> Result<()>;

            /// Set the style map to be used for rendering.
            /// The style change will be applied before the next render.
            fn set_style(&mut self, style: StyleMap);

            /// Request a help snapshot to be injected into the specified target node.
            ///
            /// This should be called before changing focus or layout, so the snapshot
            /// captures the pre-help context. After the current command returns, Canopy
            /// will capture the snapshot and inject it into the target widget.
            fn request_help_snapshot(&mut self, target: NodeId);

            /// Take the pending help snapshot, if any.
            ///
            /// This is called by help widgets to retrieve the snapshot that was
            /// captured when `request_help_snapshot` was called. Returns `None` if
            /// no snapshot is pending.
            fn take_help_snapshot(&mut self) -> Option<OwnedHelpSnapshot>;

            /// Request a diagnostic dump for a target node.
            fn request_diagnostic_dump(&mut self, target: NodeId);
        }

        /// The result of an event handler.
        #[derive(Debug, StructuralPartialEq, PartialEq, Eq, Clone)]
        pub enum EventOutcome {
            /// The event was processed and propagation stops.
            Handle,
            /// The event was processed without a state change and propagation stops.
            Consume,
            /// The event was not handled and will bubble up the tree.
            Ignore,
        }

        /// Subtree used by a focus traversal operation.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum FocusScope {
            /// The current widget's subtree.
            Current,
            /// The complete widget tree.
            Root,
            /// A subtree rooted at an explicit node.
            Node(super::id::NodeId),
        }

        /// A trait that allows widgets to perform recursive initialization of themselves and their
        /// children.
        pub trait Loader {
            /// Load commands or resources into the canopy instance.
            /// Returns an error if loading fails.
            fn load(_: &mut Canopy) -> Result<()> {}
        }

        /// Opaque identifier for a node stored in the Core arena.
        #[derive(
            Copy, Clone, Default, Eq, StructuralPartialEq, PartialEq, Ord, PartialOrd, Hash, Debug,
        )]
        pub struct NodeId(_);

        impl ToArgValue for crate::core::NodeId {
            fn to_arg_value(self) -> ArgValue {}
        }

        impl FromArgValue for crate::core::NodeId {
            fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {}
        }

        impl CommandType for crate::core::NodeId {
            fn luau_ty() -> declaration::Type {}

            fn luau_decls(registry: &mut DeclRegistry<'_>) {}
        }

        impl From<KeyData> for NodeId {
            fn from(k: KeyData) -> Self {}
        }

        impl Key for NodeId {
            fn data(&self) -> KeyData {}
        }

        impl<T> From<TypedId<T>> for NodeId {
            fn from(value: TypedId<T>) -> Self {}
        }

        /// A path of node name components.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, FromStr, Display)]
        pub struct Path {}

        impl Path {
            /// Construct an empty path.
            pub fn empty() -> Self {}

            /// Parse and validate a path from a slash-separated string.
            pub fn parse(path: &str) -> Result<Self> {}

            /// Pop an item off the end of the path, modifying it in place. Return None
            /// if the path is empty.
            pub fn pop(&mut self) -> Option<String> {}

            /// Construct a path from a slice of components.
            pub fn new<I>(v: I) -> Self
            where
                I: IntoIterator,
                I::Item: AsRef<str>, {
            }
        }

        impl From<Vec<String>> for Path {
            fn from(path: Vec<String>) -> Self {}
        }

        impl From<&[&str]> for Path {
            fn from(v: &[&str]) -> Self {}
        }

        impl From<&str> for Path {
            fn from(v: &str) -> Self {}
        }

        /// A validated path filter used to search node paths.
        ///
        /// Filters support `*` for one component and `**` for zero or more components.
        /// Literal components must be valid [`NodeName`] values.
        #[derive(Debug, Clone, FromStr)]
        pub struct PathFilter {}

        impl PathFilter {
            /// Compile a validated path filter.
            pub fn new(filter: &str) -> Result<Self> {}

            /// Compile a filter after normalizing it to a full-path match.
            pub fn normalized(filter: &str) -> Result<Self> {}

            /// Return the original filter string.
            pub fn as_str(&self) -> &str {}
        }

        /// Limits for a materialized visible render target.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Default)]
        pub struct RenderLimits {
            /// Maximum visible render-target width.
            pub max_width: u32,
            /// Maximum visible render-target height.
            pub max_height: u32,
            /// Maximum total number of materialized terminal cells.
            pub max_cells: usize,
        }

        impl RenderLimits {
            /// Construct explicit visible render-target limits.
            pub const fn new(max_width: u32, max_height: u32, max_cells: usize) -> Self {}
        }

        /// Script API finalization state.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum ScriptApiState {
            /// Registrations remain open and no surface is staged.
            Open,
            /// The surface is staged but the runtime has not been published.
            Preparing,
            /// The runtime, definitions, and module source are ready.
            Ready,
        }

        /// Slot helper for keyed children that caches the resolved typed ID.
        #[derive(Debug, Default)]
        pub struct Slot<K: ChildKey> {}

        impl<K: ChildKey> Slot<K> {
            /// Construct an empty slot.
            pub fn new() -> Self {}

            /// Clear any cached typed ID.
            pub fn clear(&mut self) {}

            /// Get or create the keyed child under the current node.
            pub fn get_or_create(
                &mut self,
                ctx: &mut dyn Context,
                make: impl FnOnce() -> K::Widget,
            ) -> Result<TypedId<K::Widget>> {
            }

            /// Get or create the keyed child under a specific parent node.
            pub fn get_or_create_in(
                &mut self,
                ctx: &mut dyn Context,
                parent: impl Into<NodeId>,
                make: impl FnOnce() -> K::Widget,
            ) -> Result<TypedId<K::Widget>> {
            }

            /// Execute a closure with a keyed child under the current node.
            pub fn with<R>(
                &mut self,
                ctx: &mut dyn Context,
                f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
            ) -> Result<R> {
            }

            /// Execute a closure with a keyed child under a specific parent node.
            pub fn with_in<R>(
                &mut self,
                ctx: &mut dyn Context,
                parent: impl Into<NodeId>,
                f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
            ) -> Result<R> {
            }
        }

        /// Type-safe wrapper around a node identifier tied to a widget type.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct TypedId<T> {}

        impl<T> From<TypedId<T>> for NodeId {
            fn from(value: TypedId<T>) -> Self {}
        }

        /// Read-only context available to widgets during render and measure.
        pub trait ViewContext {
            /// The node currently being rendered.
            fn node_id(&self) -> NodeId;

            /// The root node of the tree.
            fn root_id(&self) -> NodeId;

            /// View information for the current node.
            fn view(&self) -> View {}

            /// Cached layout configuration for the current node.
            fn layout(&self) -> Layout {}

            /// View information for a specific node.
            fn node_view(&self, node: NodeId) -> Option<View>;

            /// Layout configuration for a specific node.
            fn node_layout(&self, node: NodeId) -> Option<Layout>;

            /// Widget type identifier for a specific node.
            fn node_type_id(&self, node: NodeId) -> Option<TypeId>;

            /// Visible view rectangle in content coordinates.
            fn view_rect(&self) -> Rect {}

            /// Visible view rectangle in local outer coordinates.
            fn view_rect_local(&self) -> Rect {}

            /// Local outer rectangle for this node.
            fn outer_rect_local(&self) -> Rect {}

            /// Children of the current node in tree order.
            fn children(&self) -> Vec<NodeId> {}

            /// Children of a specific node in tree order.
            fn children_of(&self, node: NodeId) -> Vec<NodeId>;

            /// Does the current node have focus?
            fn is_focused(&self) -> bool {}

            /// Does the specified node have focus?
            fn node_is_focused(&self, node: NodeId) -> bool;

            /// Return the currently focused node, including one not yet laid out.
            fn focused_node(&self) -> Option<NodeId>;

            /// Is the current node on the focus path?
            fn is_on_focus_path(&self) -> bool {}

            /// Is the specified node on the focus path?
            fn node_is_on_focus_path(&self, node: NodeId) -> bool;

            /// Return the focused leaf under the subtree rooted at `root`.
            fn focused_leaf(&self, root: NodeId) -> Option<NodeId>;

            /// Return focusable leaves in pre-order under the subtree rooted at `root`.
            fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId>;

            /// Return the parent of a node, or `None` if it is the root or not found.
            fn parent_of(&self, node: NodeId) -> Option<NodeId>;

            /// Return whether a node exists and is attached to the root tree.
            fn node_is_attached(&self, node: NodeId) -> bool;

            /// Return the path for a node relative to a root.
            fn node_path(&self, root: NodeId, node: NodeId) -> Path;

            /// Locate the deepest visible node at a point within a subtree.
            fn locate(&self, root: NodeId, point: Point) -> Result<Option<NodeId>>;

            /// Return a keyed child relative to the current node.
            fn child_keyed(&self, key: &str) -> Option<NodeId> {}

            /// Return a keyed child relative to a specific parent node.
            fn child_keyed_in(&self, parent: NodeId, key: &str) -> Option<NodeId>;

            /// Find the first node whose path matches the filter, relative to the current node.
            ///
            /// The filter is normalized to match full paths.
            fn find_node(&self, path_filter: &str) -> Option<NodeId> {}

            /// Find the first node whose path matches the validated filter.
            fn find_node_matching(&self, path_filter: &PathFilter) -> Option<NodeId> {}

            /// Find all nodes whose paths match the filter, relative to the current node.
            ///
            /// The filter is normalized to match full paths.
            fn find_nodes(&self, path_filter: &str) -> Vec<NodeId> {}

            /// Find all nodes whose paths match the validated filter.
            fn find_nodes_matching(&self, path_filter: &PathFilter) -> Vec<NodeId> {}

            /// Peek at the pending help snapshot, if any.
            ///
            /// This is used by help widgets to check if a snapshot is available
            /// during render, without consuming it.
            fn pending_help_snapshot(&self) -> Option<&OwnedHelpSnapshot>;
        }

        /// Widgets are the behavior attached to nodes in the Core arena.
        pub trait Widget: Any + Send {
            /// Layout configuration for this widget.
            fn layout(&self) -> Layout {}

            /// Measure intrinsic content size (content box, excludes Layout padding).
            fn measure(&self, c: MeasureConstraints) -> Measurement {}

            /// Canvas size in content coordinates (for scrolling).
            ///
            /// `view` is this node's content size (outer minus padding).
            fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext<'_>) -> Size<u32> {}

            /// Render this widget's own content. Does not render children.
            fn render(&mut self, _frame: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            /// Handle events.
            fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
            }

            /// Attempt to focus this widget.
            ///
            /// Widgets can use the provided context to query their tree state (e.g., whether they have
            /// children) when deciding whether to accept focus.
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            /// Cursor specification for focused widgets.
            fn cursor(&self) -> Option<cursor::Cursor> {}

            /// Scheduled poll endpoint.
            fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {}

            /// Called when the widget is mounted in the tree, before its first render.
            ///
            /// A failed hook rolls back core-owned state. External effects and widget-owned state must be
            /// repeatable or compensating because a later mount attempt may call this hook again.
            fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {}

            /// Validation hook before a widget is removed or replaced.
            ///
            /// This hook must be side-effect free or safely repeatable.
            fn pre_remove(&mut self, _ctx: &mut dyn Context) -> Result<()> {}

            /// Called before a successfully mounted widget is removed or replaced.
            ///
            /// This hook cannot veto removal. During failure rollback, structural context operations are
            /// rejected and external cleanup must be safe to repeat.
            fn on_unmount(&mut self, _ctx: &mut dyn Context) {}

            /// Name used for commands and paths.
            fn name(&self) -> NodeName {}
        }

        pub use crate::command;
        pub use crate::derive_commands;
        pub mod error {
            //! Core error types.

            /// Result type for canopy-core operations.
            pub type Result<T> = std::result::Result<T, Error>;

            /// Parse error marker type.
            #[derive(StructuralPartialEq, PartialEq, Eq, Debug, Clone, Display, Error)]
            pub struct ParseError {
                /// Parse error message.
                pub message: String,
                /// One-based source line, when known.
                pub line: Option<usize>,
                /// Source byte offset, when known.
                pub offset: Option<usize>,
            }

            impl ParseError {
                /// Construct a parse error from a message.
                pub fn new(message: impl Into<String>) -> Self {}

                /// Construct a parse error with optional line/offset information.
                pub fn with_position(
                    message: impl Into<String>,
                    line: Option<usize>,
                    offset: Option<usize>,
                ) -> Self {
                }
            }

            /// Phase in which a node-bound widget operation failed.
            #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Display)]
            pub enum NodeOperationKind {
                /// Widget access or lifecycle callback.
                Access,
                /// Widget measurement or layout.
                Layout,
                /// Widget rendering.
                Render,
            }

            /// Stable category for a structured script or command failure.
            #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Display)]
            pub enum ScriptErrorKind {
                /// Cooperative execution timeout.
                Timeout,
                /// Node lookup failed.
                NodeNotFound,
                /// A node exists but is detached.
                NodeDetached,
                /// A value or widget type did not match.
                TypeMismatch,
                /// A requested value was not found.
                NotFound,
                /// Invalid input or operation.
                Invalid,
                /// Unclassified Canopy failure.
                Canopy,
                /// Unknown command identifier.
                UnknownCommand,
                /// Duplicate command identifier.
                DuplicateCommand,
                /// Conflicting command definition.
                ConflictingCommand,
                /// Invalid command definition.
                InvalidCommand,
                /// No command target was found.
                NoTarget,
                /// A command node handle is stale.
                InvalidNode,
                /// Positional argument count mismatch.
                ArityMismatch,
                /// Required named argument is missing.
                MissingNamedArgument,
                /// An unknown named argument was supplied.
                UnknownNamedArgument,
                /// Argument conversion failed.
                Conversion,
                /// An injected value is missing.
                MissingInjected,
                /// The routed target has the wrong widget type.
                TargetTypeMismatch,
                /// Command implementation returned an error.
                CommandExecution,
                /// Another top-level script evaluation is active.
                ScriptBusy,
            }

            impl ScriptErrorKind {
                /// Return the stable protocol label for this category.
                pub const fn as_str(self) -> &'static str {}
            }

            /// Core error type.
            #[derive(Error, Display, Debug)]
            pub enum Error {
                /// A render target exceeds its configured width limit.
                RenderWidthLimit {
                    /// Requested target width.
                    requested: u32,
                    /// Configured maximum width.
                    limit: u32,
                },
                /// A render target exceeds its configured height limit.
                RenderHeightLimit {
                    /// Requested target height.
                    requested: u32,
                    /// Configured maximum height.
                    limit: u32,
                },
                /// Render-target dimensions cannot be represented as a cell count.
                RenderCellCountOverflow {
                    /// Requested target width.
                    width: u32,
                    /// Requested target height.
                    height: u32,
                },
                /// A render target exceeds its configured total-cell limit.
                RenderCellLimit {
                    /// Requested target cell count.
                    requested: usize,
                    /// Configured maximum cell count.
                    limit: usize,
                },
                /// Render-target backing storage could not be reserved.
                RenderAllocation {
                    /// Requested target cell count.
                    cells: usize,
                },
                /// A single-cell drawing API received a character with an invalid width.
                InvalidCellCharacter {
                    /// Rejected character.
                    ch: char,
                    /// Computed terminal width.
                    width: usize,
                },
                /// Geometry failure.
                Geometry(geom::Error),
                /// Invalid layout configuration.
                InvalidLayout(crate::layout::LayoutValidationError),
                /// Terminal I/O failure.
                TerminalIo(io::Error),
                /// Run loop failure.
                RunLoop(String),
                /// Internal error.
                Internal(String),
                /// Core invariant violation.
                Invariant(String),
                /// Re-entrant widget borrow attempt.
                ReentrantWidgetBorrow(crate::core::id::NodeId),
                /// Node-bound widget operation failure with its original source.
                NodeOperation {
                    /// Operation phase.
                    kind: NodeOperationKind,
                    /// Stable operation name.
                    operation: &'static str,
                    /// Node being operated on.
                    node: crate::core::id::NodeId,
                    /// Node path at the time of failure.
                    path: String,
                    /// Original typed failure.
                    source: Box<Self>,
                },
                /// Invalid input error.
                Invalid(String),
                /// Requested item was not found.
                NotFound(String),
                /// Widget type mismatch.
                TypeMismatch {
                    /// Expected widget type name.
                    expected: String,
                    /// Actual widget type name.
                    actual: String,
                },
                /// A live node stores a different widget type than requested.
                NodeTypeMismatch {
                    /// Node whose widget type was checked.
                    node: crate::core::id::NodeId,
                    /// Requested widget type.
                    expected: &'static str,
                },
                /// A query matched multiple nodes.
                MultipleMatches,
                /// Duplicate child key under the same parent.
                DuplicateChildKey(String),
                /// Duplicate child under the same parent.
                DuplicateChild {
                    /// Parent node.
                    parent: crate::core::id::NodeId,
                    /// Child node.
                    child: crate::core::id::NodeId,
                },
                /// Child is already attached to a parent.
                AlreadyAttached(crate::core::id::NodeId),
                /// Attaching would create a parent/child cycle.
                WouldCreateCycle {
                    /// Parent node involved in the cycle.
                    parent: crate::core::id::NodeId,
                    /// Child node involved in the cycle.
                    child: crate::core::id::NodeId,
                },
                /// Invalid structural operation.
                InvalidOperation(String),
                /// Structural mutation attempted while a failed edit is unwinding.
                TreeEditDuringRollback {
                    /// Requested tree operation.
                    operation: &'static str,
                },
                /// Command dispatch failure.
                Command(crate::commands::CommandError),
                /// Parsing failure.
                Parse(ParseError),
                /// Script execution failure.
                Script(String),
                /// Script execution failure with stable host category fields.
                ScriptStructured {
                    /// Stable script-visible category.
                    kind: ScriptErrorKind,
                    /// Command id when the error came from command dispatch.
                    command: Option<String>,
                    /// Owner name when the error came from node-target resolution.
                    owner: Option<String>,
                    /// Human-readable error message.
                    message: String,
                },
                /// Script execution exceeded its cooperative timeout.
                ScriptTimeout {
                    /// Requested timeout in milliseconds.
                    timeout_ms: u64,
                },
                /// Node not found in the arena.
                NodeNotFound(crate::core::id::NodeId),
                /// Node exists but is not attached to the root tree.
                NodeDetached(crate::core::id::NodeId),
            }

            impl From<Error> for Error {
                fn from(source: geom::Error) -> Self {}
            }

            impl From<LayoutValidationError> for Error {
                fn from(source: LayoutValidationError) -> Self {}
            }

            impl From<CommandError> for Error {
                fn from(source: CommandError) -> Self {}
            }

            impl From<RecvError> for Error {
                fn from(e: mpsc::RecvError) -> Self {}
            }

            impl From<&Error> for CanopyErrorPayload {
                fn from(err: &error::Error) -> Self {}
            }
        }

        /// This enum represents all the event types that drive the application.
        #[derive(Debug, Clone)]
        pub enum Event {
            /// A keystroke
            Key(key::Key),
            /// A mouse action
            Mouse(mouse::MouseEvent),
            /// Terminal resize
            Resize(crate::geom::Size),
            /// A poll event
            Poll(Vec<crate::NodeId>),
            /// Terminal has gained focus
            FocusGained,
            /// Terminal has lost focus
            FocusLost,
            /// Cut and paste
            Paste(String),
            /// Internal wake event used to service queued automation work.
            Wake,
        }

        impl Inject for crate::event::Event {
            fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {}
        }

        /// A keystroke along with modifiers.
        /// A keystroke along with modifiers.
        #[derive(
            Debug,
            StructuralPartialEq,
            PartialEq,
            Eq,
            Clone,
            Copy,
            Hash,
            PartialEq,
            PartialEq,
            PartialEq,
            Display,
        )]
        pub struct Key {
            /// Modifier state.
            pub mods: Mods,
            /// Key code.
            pub key: KeyCode,
        }

        impl Key {
            /// Normalize key inputs for binding and matching.
            ///
            /// Normalization handles two common sources of divergence across terminals:
            ///
            /// - **Ctrl-modified ASCII control codes** (0x00–0x1F and 0x7F) are mapped to
            ///   canonical printable equivalents (e.g. 0x01 → `A`, 0x1B → `[`, 0x7F → `?`).
            ///   Some terminals emit control codes without setting the Ctrl modifier, so
            ///   these codes are treated as Ctrl-combinations even if Ctrl isn't reported.
            ///   We also map Ctrl+`_`, Ctrl+`?`, and Ctrl+`7` to `/` to align with common
            ///   `Ctrl+/` help bindings across keyboard layouts and terminal encodings.
            /// - **Shift handling** is applied after Ctrl canonicalization.
            ///
            /// Handling of the shift key is the most intricate part of this module.
            /// When we receive an event, it includes the shift modifier and also the
            /// modified character - e.g. "shift + A" or "shift + (". However, when
            /// users bind keys, it's more intuitive to bind just "A" or "(". We don't
            /// know what the keyboard mapping or input method is for the user - so it's
            /// not possible in a general way for us to map between, say, an input like
            /// "shift + 0" to the shifted key "(". Conversely, if we see an input of
            /// "shift + (", we don't know if the user pressed "shift + 0" or if they
            /// have a weird keyboard layout that actually permits "shift + (" without a
            /// shift conversion.
            ///
            /// To handle this, we have to make a lossy compromise. We define a
            /// normalisation applied to input for the purpose of key binding matching
            /// as follows:
            ///
            /// - If shift is present:
            ///     - If the key is ascii lowercase, convert it to uppercase and remove
            ///       shift
            ///     - If the key is one of a special class of characters that commonly
            ///       don't have a shift conversion (space, enter), leave shift intact
            ///     - in all other cases, just remove shift
            ///
            /// | input             | normalization    |
            /// |-------------------|------------------|
            /// | shift + A         | A                |
            /// | shift + a         | A                |
            /// | shift + )         | )                |
            /// | shift + enter     | shift + enter    |
            /// | shift + ctrl + A  | ctrl + A         |
            ///
            /// `normalize` must be called explicitly when needed - all comparison and
            /// conversion methods are literal and stright-forward, and don't perform
            /// normalization automatically.
            pub fn normalize(&self) -> Self {}

            /// Parse a key specification such as `ctrl-s`, `PageDown`, or `A`.
            pub fn parse_spec(spec: &str) -> Result<Self, String> {}
        }

        impl From<char> for Key {
            fn from(c: char) -> Self {}
        }

        impl From<KeyCode> for Key {
            fn from(c: KeyCode) -> Self {}
        }

        pub mod mouse {
            //! Mouse event types.

            /// An abstract specification for a mouse action.
            #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq)]
            pub struct Mouse {
                /// Mouse action type.
                pub action: Action,
                /// Mouse button.
                pub button: Button,
                /// Keyboard modifiers.
                pub modifiers: key::Mods,
            }

            impl Mouse {
                /// Parse a mouse specification such as `ScrollUp` or `ctrl-LeftDown`.
                pub fn parse_spec(spec: &str) -> Result<Self, String> {}
            }

            impl From<MouseEvent> for Mouse {
                fn from(o: MouseEvent) -> Self {}
            }

            /// Mouse button codes.
            #[derive(Debug, PartialOrd, StructuralPartialEq, PartialEq, Eq, Clone, Copy, Hash)]
            pub enum Button {
                /// Left mouse button.
                Left,
                /// Right mouse button.
                Right,
                /// Middle mouse button.
                Middle,
                /// No button (for move/scroll).
                None,
            }

            /// Mouse action kinds.
            #[derive(Debug, PartialOrd, StructuralPartialEq, PartialEq, Eq, Clone, Copy, Hash)]
            pub enum Action {
                /// Button press.
                Down,
                /// Button release.
                Up,
                /// Mouse drag with button held.
                Drag,
                /// Mouse moved without button.
                Moved,
                /// Scroll wheel down.
                ScrollDown,
                /// Scroll wheel up.
                ScrollUp,
                /// Horizontal scroll left.
                ScrollLeft,
                /// Horizontal scroll right.
                ScrollRight,
            }

            impl Action {
                /// Is this a button-driven action?
                pub fn is_button(&self) -> bool {}
            }

            /// A mouse input event. This has the same fields as the `Mouse` event
            /// specification, but also includes a location.
            #[derive(Debug, Clone, Copy)]
            pub struct MouseEvent {
                /// Mouse action type.
                pub action: Action,
                /// Mouse button.
                pub button: Button,
                /// Keyboard modifiers.
                pub modifiers: key::Mods,
                /// Cursor location in local coordinates relative to the node view. To map
                /// back to screen coordinates, add the node view's outer top-left.
                pub location: crate::geom::Point,
            }

            impl Inject for crate::event::mouse::MouseEvent {
                fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {}
            }

            impl From<MouseEvent> for Mouse {
                fn from(o: MouseEvent) -> Self {}
            }
        }

        pub use crate::geom::Point;
        pub use crate::geom::Rect;
        pub use crate::geom::Size;
        /// Define a typed key for keyed children.
        ///
        /// # Examples
        ///
        /// ```
        /// use canopy::{ChildKey, Widget, key};
        ///
        /// key!(Editor);
        /// impl Widget for Editor {}
        ///
        /// pub struct Modal;
        /// impl Widget for Modal {}
        /// key!(pub ModalSlot: Modal);
        ///
        /// assert_eq!(Editor::KEY, "Editor");
        /// assert_eq!(ModalSlot::KEY, "ModalSlot");
        /// ```
        #[macro_export]
        macro_rules! key {
    ($vis:vis $name:ident) => { ... };
    ($vis:vis $name:ident : $widget:ty) => { ... };
}
        /// Alignment along an axis.
        #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
        pub enum Align {
            /// Align to the start of the axis.
            Start,
            /// Align to the center of the axis.
            Center,
            /// Align to the end of the axis.
            End,
        }

        /// Content-box measurement constraints.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Hash)]
        pub enum Constraint {
            /// No constraint on this axis.
            Unbounded,
            /// The engine guarantees at most n cells on this axis.
            AtMost(u32),
            /// The engine guarantees exactly n cells on this axis.
            Exact(u32),
        }

        impl Constraint {
            /// Return true if this constraint is exact.
            pub fn is_exact(self) -> bool {}

            /// Return the maximum bound implied by the constraint.
            pub fn max_bound(self) -> u32 {}
        }

        /// Stack direction for children.
        #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
        pub enum Direction {
            /// Stack children vertically (column).
            Column,
            /// Stack children horizontally (row).
            Row,
            /// Children overlap in the same space (painter's algorithm - last child on top).
            Stack,
        }

        impl Direction {
            /// Size along the main axis.
            pub fn main_size(&self, size: Size<u32>) -> u32 {}

            /// Size along the cross axis.
            pub fn cross_size(&self, size: Size<u32>) -> u32 {}

            /// Construct a size from main and cross axis values.
            pub fn size_from_main_cross(&self, main: u32, cross: u32) -> Size<u32> {}
        }

        /// Display mode for layout participation.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum Display {
            /// Node participates in layout and rendering.
            Block,
            /// Node is removed from layout and not rendered.
            None,
        }

        /// Layout configuration for a node.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Default)]
        pub struct Layout {
            /// Whether this node participates in layout/render.
            pub display: Display,
            /// Stack direction for children.
            pub direction: Direction,
            /// Width sizing strategy (outer size).
            pub width: Sizing,
            /// Height sizing strategy (outer size).
            pub height: Sizing,
            /// Minimum outer width constraint (cells).
            pub min_width: Option<u32>,
            /// Maximum outer width constraint (cells).
            pub max_width: Option<u32>,
            /// Minimum outer height constraint (cells).
            pub min_height: Option<u32>,
            /// Maximum outer height constraint (cells).
            pub max_height: Option<u32>,
            /// Allow horizontal overflow during measurement.
            pub overflow_x: bool,
            /// Allow vertical overflow during measurement.
            pub overflow_y: bool,
            /// Structural padding inside the widget (cells).
            pub padding: Edges<u32>,
            /// Gap between children along the main axis (cells).
            pub gap: u32,
            /// Horizontal alignment of children within the content area.
            ///
            /// For rows this aligns the complete child group on the main axis. For
            /// columns it aligns each child on the cross axis. Stacks align each child.
            pub align_horizontal: Align,
            /// Vertical alignment of children within the content area.
            ///
            /// For columns this aligns the complete child group on the main axis. For
            /// rows it aligns each child on the cross axis. Stacks align each child.
            pub align_vertical: Align,
        }

        impl Layout {
            /// Column layout with measured sizing on both axes.
            pub fn column() -> Self {}

            /// Row layout with measured sizing on both axes.
            pub fn row() -> Self {}

            /// Stack layout where children overlap in the same space.
            pub fn stack() -> Self {}

            /// Fill available space with flex sizing on both axes.
            pub fn fill() -> Self {}

            /// Remove this node from layout and rendering.
            pub fn none(self) -> Self {}

            /// Set width to flex with the provided weight.
            ///
            /// A zero weight is rejected when the layout is applied.
            pub fn flex_horizontal(self, weight: u32) -> Self {}

            /// Set height to flex with the provided weight.
            ///
            /// A zero weight is rejected when the layout is applied.
            pub fn flex_vertical(self, weight: u32) -> Self {}

            /// Set the minimum outer width.
            pub fn min_width(self, n: u32) -> Self {}

            /// Set the maximum outer width.
            pub fn max_width(self, n: u32) -> Self {}

            /// Set the minimum outer height.
            pub fn min_height(self, n: u32) -> Self {}

            /// Set the maximum outer height.
            pub fn max_height(self, n: u32) -> Self {}

            /// Allow horizontal overflow during measurement.
            pub fn overflow_x(self) -> Self {}

            /// Allow vertical overflow during measurement.
            pub fn overflow_y(self) -> Self {}

            /// Inherit overflow permission from an enclosing layout.
            ///
            /// Overflow only widens: a layout that already allows overflow on an axis keeps it.
            pub fn inherit_overflow(&mut self, x: bool, y: bool) {}

            /// Convenience: fixed outer width without a `Fixed` enum.
            pub fn fixed_width(self, n: u32) -> Self {}

            /// Convenience: fixed outer height without a `Fixed` enum.
            pub fn fixed_height(self, n: u32) -> Self {}

            /// Set padding edges.
            pub fn padding(self, edges: Edges<u32>) -> Self {}

            /// Set the main-axis gap between children.
            pub fn gap(self, n: u32) -> Self {}

            /// Set horizontal alignment of children within content area.
            pub fn align_horizontal(self, align: Align) -> Self {}

            /// Set vertical alignment of children within content area.
            pub fn align_vertical(self, align: Align) -> Self {}

            /// Center children both horizontally and vertically.
            pub fn align_center(self) -> Self {}

            /// Set the layout direction.
            pub fn direction(self, direction: Direction) -> Self {}

            /// Set width sizing strategy directly.
            pub fn width(self, sizing: Sizing) -> Self {}

            /// Set height sizing strategy directly.
            pub fn height(self, sizing: Sizing) -> Self {}

            /// Set both axes to Measure sizing.
            pub fn measured(self) -> Self {}

            /// Validate this layout configuration.
            pub fn validate(&self) -> Result<(), LayoutValidationError> {}
        }

        /// Constraints for measuring a widget's content box.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Hash)]
        pub struct MeasureConstraints {
            /// Width constraint.
            pub width: Constraint,
            /// Height constraint.
            pub height: Constraint,
        }

        impl MeasureConstraints {
            /// Leaf widgets: clamp a content size to these constraints and return Fixed.
            pub fn clamp(&self, content: Size<u32>) -> Measurement {}

            /// Containers: request wrapping.
            pub fn wrap(&self) -> Measurement {}

            /// Clamp a size to these constraints.
            pub fn clamp_size(&self, content: Size<u32>) -> Size<u32> {}

            /// True if the main axis is exact.
            pub fn main_is_exact(&self, direction: Direction) -> bool {}

            /// True if the cross axis is exact.
            pub fn cross_is_exact(&self, direction: Direction) -> bool {}

            /// Return the main axis constraint.
            pub fn main(&self, direction: Direction) -> Constraint {}

            /// Return the cross axis constraint.
            pub fn cross(&self, direction: Direction) -> Constraint {}
        }

        /// Result of measuring a widget's content box.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum Measurement {
            /// Fixed content size for leaf widgets.
            Fixed(Size<u32>),
            /// Wrap children: engine computes content size from children.
            Wrap,
        }

        /// Sizing strategy for a single axis.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum Sizing {
            /// Size derives from `measure()` or wrapping children.
            Measure,
            /// Weighted share of remaining space along the axis.
            Flex(u32),
        }

        /// A renderer that only renders to a specific rectangle within the target terminal buffer.
        pub struct Render<'a> {}

        impl<'a> Render<'a> {
            /// Construct a renderer that writes into `buf`.
            ///
            /// `clip` is the visible rectangle in canvas coordinates, and `screen_origin` is where the
            /// clip's top-left lands in the buffer.
            pub fn new(
                stylemap: &'a StyleMap,
                style: &'a mut StyleManager,
                buf: &'a mut TermBuf,
                clip: geom::Rect,
                screen_origin: geom::Point,
            ) -> Self {
            }

            /// Set the effect stack for this renderer.
            pub fn with_effects(self, effects: &'a [Effect]) -> Self {}

            /// Apply the current effect stack to a style.
            /// Use this when you have a Style from a source other than the style manager.
            pub fn apply_effects(&self, style: Style) -> Style {}

            /// Resolve a style by name without applying effects.
            pub fn resolve_style_name_raw(&self, name: &str) -> Style {}

            /// Resolve a custom style at a point, applying the current effect stack.
            pub fn resolve_style_at(
                &self,
                style: Style,
                bounds: geom::Rect,
                point: geom::Point,
            ) -> ResolvedStyle {
            }

            /// Resolve a style by name at a point within bounds.
            pub fn resolve_style_name_at(
                &self,
                name: &str,
                bounds: geom::Rect,
                point: geom::Point,
            ) -> ResolvedStyle {
            }

            /// Push a style layer.
            pub fn push_layer(&mut self, name: &str) {}

            /// Fill a rectangle with a specified character. Writes out of bounds will be clipped.
            pub fn fill(&mut self, style: &str, r: geom::Rect, c: char) -> Result<()> {}

            /// Print text in the specified line. If the text is wider than the
            /// rectangle, it will be truncated; if it is shorter, it will be padded.
            pub fn text(&mut self, style: &str, l: geom::Line, txt: &str) -> Result<()> {}

            /// Write a single cell with a resolved style.
            pub fn put_cell(
                &mut self,
                style: ResolvedStyle,
                p: geom::Point,
                ch: char,
            ) -> Result<()> {
            }

            /// Write a grapheme with a resolved style, including continuation cells.
            pub fn put_grapheme(
                &mut self,
                style: ResolvedStyle,
                p: geom::Point,
                grapheme: &str,
            ) -> Result<()> {
            }
        }

        /// A node name, which consists of lowercase ASCII alphanumeric characters, plus
        /// underscores.
        #[derive(
            Debug,
            Clone,
            StructuralPartialEq,
            PartialEq,
            Eq,
            Hash,
            FromStr,
            Display,
            PartialEq,
            PartialEq,
        )]
        pub struct NodeName {}

        impl NodeName {
            /// Create a new NodeName, returning an error if the string contains invalid
            /// characters.
            pub fn new(name: &str) -> Result<Self> {}

            /// Takes a string and munges it into a valid node name. It does this by
            /// first converting the string to snake case, then removing all invalid
            /// characters.
            pub fn convert(name: &str) -> Self {}
        }

        /// Converts a string into the standard node name format, and errors if it
        /// doesn't comply to the node name standard.
        impl TryFrom<&str> for NodeName {
            type Error = Error;
            fn try_from(name: &str) -> Result<Self> {}
        }

        /// A builder for creating reusable style specifications.
        ///
        /// Use this to define styles that can be applied to multiple paths.
        ///
        /// # Example
        ///
        /// ```
        /// use canopy::style::{Attr, StyleBuilder, StyleMap, solarized};
        ///
        /// let selected = StyleBuilder::new()
        ///     .fg(solarized::BASE3)
        ///     .bg(solarized::BLUE)
        ///     .attr(Attr::Bold);
        ///
        /// let mut style_map = StyleMap::new();
        /// style_map
        ///     .rules()
        ///     .style("item/selected", selected)
        ///     .apply();
        /// ```
        #[derive(Clone, Default, Debug, StructuralPartialEq, PartialEq)]
        pub struct StyleBuilder {}

        impl StyleBuilder {
            /// Create a new empty style builder.
            pub fn new() -> Self {}

            /// Set the foreground paint.
            pub fn fg(self, paint: impl Into<Paint>) -> Self {}

            /// Set the background paint.
            pub fn bg(self, paint: impl Into<Paint>) -> Self {}

            /// Add a single attribute.
            pub fn attr(self, attr: Attr) -> Self {}

            /// Set all attributes.
            pub fn attrs(self, attrs: AttrSet) -> Self {}
        }

        impl From<StyleBuilder> for PartialStyle {
            fn from(s: StyleBuilder) -> Self {}
        }

        /// Map of style paths to partial styles.
        #[derive(Clone, Debug, Default)]
        pub struct StyleMap {}

        impl StyleMap {
            /// Construct a style map with defaults.
            pub fn new() -> Self {}

            /// Begin a fluent rule-building chain.
            ///
            /// # Example
            ///
            /// ```
            /// use canopy::style::{StyleMap, solarized};
            ///
            /// let mut style_map = StyleMap::new();
            /// style_map
            ///     .rules()
            ///     .fg("red/text", solarized::RED)
            ///     .fg("blue/text", solarized::BLUE)
            ///     .apply();
            /// ```
            pub fn rules(&mut self) -> StyleRules<'_> {}
        }

        /// Common result alias for Canopy operations.
        pub type Result<T> = error::Result<T>;
    }

    pub mod terminal {
        //! Crossterm terminal run-loop integration.

        /// Run the main render/event loop using the crossterm backend.
        ///
        /// Ctrl+C dumps the node tree and stops the loop with status 130. Keyboard enhancement flags
        /// are enabled so escape codes are unambiguous.
        pub fn runloop(cnpy: crate::Canopy) -> crate::error::Result<i32> {}
    }

    pub use canopy_geom as geom;
    /// Limits for a materialized visible render target.
    #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct RenderLimits {
        /// Maximum visible render-target width.
        pub max_width: u32,
        /// Maximum visible render-target height.
        pub max_height: u32,
        /// Maximum total number of materialized terminal cells.
        pub max_cells: usize,
    }

    impl RenderLimits {
        /// Construct explicit visible render-target limits.
        pub const fn new(max_width: u32, max_height: u32, max_cells: usize) -> Self {}
    }

    /// A 2D terminal buffer of styled cells.
    #[derive(Clone, Debug)]
    pub struct TermBuf {}

    impl TermBuf {
        /// Construct a buffer filled with the given character and style.
        pub fn new(size: impl Into<Size>, ch: char, style: ResolvedStyle) -> Result<Self> {}

        /// Construct a buffer with explicit visible render-target limits.
        pub fn new_with_limits(
            size: impl Into<Size>,
            ch: char,
            style: ResolvedStyle,
            limits: RenderLimits,
        ) -> Result<Self> {
        }

        /// Return the buffer size.
        pub fn size(&self) -> Size {}

        /// Return the buffer bounds as a rectangle.
        pub fn rect(&self) -> Rect {}

        /// Fill a rectangle with a glyph and style.
        pub fn fill(&mut self, style: &ResolvedStyle, r: Rect, ch: char) -> Result<()> {}

        /// Fill a rectangle, resolving the style separately for each cell.
        pub fn fill_with(
            &mut self,
            r: Rect,
            ch: char,
            style_at: impl Fn(Point) -> ResolvedStyle,
        ) -> Result<()> {
        }

        /// Overlay a cursor on a cell by adjusting its style.
        pub fn overlay_cursor(&mut self, location: Point, shape: cursor::CursorShape) {}

        /// Draw text clipped to the given line.
        pub fn text(&mut self, style: &ResolvedStyle, l: Line, txt: &str) -> Result<()> {}

        /// Write text along a line, resolving the style separately for each cell.
        ///
        /// The text is clipped to the line and padded with spaces to the line's width.
        pub fn text_with(
            &mut self,
            l: Line,
            txt: &str,
            style_at: impl Fn(Point) -> ResolvedStyle,
        ) -> Result<()> {
        }

        /// Get a cell by position.
        pub fn get(&self, p: Point) -> Option<&Cell> {}

        /// Return the rendered screen as rows of cell strings.
        pub fn rows(&self) -> Vec<Vec<String>> {}

        /// Return the rendered screen as newline-joined plain text.
        pub fn screen_text(&self) -> String {}

        /// Diff this terminal buffer against a previous state, emitting changes
        /// to the provided render backend.
        pub fn diff<R: RenderBackend>(&self, prev: &Self, backend: &mut R) -> Result<()> {}

        /// Render this terminal buffer in full using the provided backend,
        /// batching runs of text with the same style.
        pub fn render<R: RenderBackend>(&self, backend: &mut R) -> Result<()> {}
    }

    /// Callback marshalled onto the UI thread for live automation.
    pub type AutomationCallback = Box<dyn FnOnce(&mut Canopy) + Send + 'static>;

    /// Handle for submitting automation work to a live canopy runloop.
    #[derive(Clone)]
    pub struct AutomationHandle {}

    impl AutomationHandle {
        /// Queue a callback to run on the UI thread.
        pub fn submit(&self, callback: AutomationCallback) -> Result<()> {}

        /// Execute a closure on the UI thread and wait for its result.
        pub fn request<R, F>(&self, callback: F) -> Result<R>
        where
            R: Send + 'static,
            F: FnOnce(&mut Canopy) -> Result<R> + Send + 'static, {
        }
    }

    /// Monotonic identifier for a binding.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Hash)]
    pub struct BindingId(_);

    impl BindingId {
        /// Return the numeric binding identifier.
        pub fn as_u64(self) -> u64 {}

        /// Reconstruct a binding identifier from its numeric form.
        pub fn from_u64(id: u64) -> Self {}
    }

    /// Application runtime state and renderer coordination.
    pub struct Canopy {}

    impl super::Canopy {
        /// Render the widget tree. All visible nodes are rendered.
        pub fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {}

        /// Service a bounded batch of callbacks marshalled onto the UI thread.
        ///
        /// Custom run loops should call this after receiving [`Event::Wake`]. The return value is the
        /// number of callbacks executed during this turn.
        pub fn service_automation(&mut self) -> usize {}

        /// Set the size on the root node.
        pub fn set_root_size(&mut self, size: Size) -> Result<()> {}

        /// Construct a new Canopy instance.
        pub fn new() -> Self {}

        /// Return a handle for submitting automation work to this app's UI thread.
        pub fn automation_handle(&self) -> AutomationHandle {}

        /// Mark the visible application state for redraw.
        pub fn request_redraw(&mut self) {}

        /// Return the root node ID.
        pub fn root_id(&self) -> NodeId {}

        /// Replace the visible render-target limits.
        pub fn set_render_limits(&mut self, limits: RenderLimits) -> Result<()> {}

        /// Create a detached widget node.
        pub fn create_detached<W>(&mut self, widget: W) -> Result<TypedId<W>>
        where
            W: Widget + 'static, {
        }

        /// Replace the root's children with a single node.
        pub fn set_root_child(&mut self, child: impl Into<NodeId>) -> Result<()> {}

        /// Replace the root widget while preserving its stable node ID.
        pub fn replace_root<W>(&mut self, widget: W) -> Result<TypedId<W>>
        where
            W: Widget + 'static, {
        }

        /// Return the active style map.
        pub fn style(&self) -> &StyleMap {}

        /// Mutate the active style map before the next render.
        pub fn style_mut(&mut self) -> &mut StyleMap {}

        /// Replace the active style map before the next render.
        pub fn set_style(&mut self, style: StyleMap) {}

        /// Get a reference to the current render buffer, if any.
        pub fn buf(&self) -> Option<&TermBuf> {}

        /// Run a compiled script by id on the target node.
        pub fn run_script(
            &mut self,
            node_id: impl Into<NodeId>,
            sid: script::ScriptId,
        ) -> Result<()> {
        }

        /// Compile a script and return its identifier.
        pub fn compile_script(&mut self, source: &str) -> Result<script::ScriptId> {}

        /// Evaluate a Luau source string in the current app context.
        pub fn eval_script(&mut self, source: &str) -> Result<()> {}

        /// Evaluate a Luau source string and return its value.
        pub fn eval_script_value(&mut self, source: &str) -> Result<commands::ArgValue> {}

        /// Evaluate a Luau source string with a cooperative timeout.
        pub fn eval_script_value_with_timeout(
            &mut self,
            source: &str,
            timeout: Duration,
        ) -> Result<commands::ArgValue> {
        }

        /// Configure the `@user` persistent script root.
        pub fn set_user_script_root(&mut self, root: impl Into<PathBuf>) -> Result<()> {}

        /// Configure the `@project` persistent script root.
        pub fn set_project_script_root(&mut self, root: impl Into<PathBuf>) -> Result<()> {}

        /// Invalidate cached exports from persistent script modules.
        ///
        /// Pass a root such as `@user` or `@project` to invalidate one root, or `None` to
        /// invalidate every root. Returns the new source epoch, or `None` when no module source
        /// is configured or the named root is unknown.
        pub fn invalidate_script_modules(&mut self, root: Option<&str>) -> Option<u64> {}

        /// Register an audited Ruau native module on the same surface as Canopy commands.
        pub fn register_script_module(&mut self, module: Arc<dyn NativeModule>) -> Result<()> {}

        /// Register an app-level startup script.
        pub fn register_startup_script(&mut self, name: &str, source: &str) -> Result<()> {}

        /// Require every startup script root to define a typed global.
        pub fn require_startup_global(&mut self, name: &str, type_text: &str) -> Result<()> {}

        /// Run app, user, and project startup scripts once.
        pub fn run_startup_scripts(&mut self) -> Result<usize> {}

        /// Register a Luau script as the default bindings for a widget namespace.
        pub fn register_default_bindings(&mut self, name: &str, script: &str) -> Result<()> {}

        /// Register a named fixture available to headless and live automation.
        pub fn register_fixture(&mut self, fixture: Fixture) -> Result<()> {}

        /// Return registered fixture metadata in stable name order.
        pub fn fixture_infos(&self) -> Vec<FixtureInfo> {}

        /// Apply a named fixture to the current app instance.
        pub fn apply_fixture(&mut self, name: &str) -> Result<()> {}

        /// Run a closure against the root context.
        pub fn with_root_context<R>(
            &mut self,
            f: impl FnOnce(&mut dyn crate::Context) -> Result<R>,
        ) -> Result<R> {
        }

        /// Run a closure against a mutable context bound to a node.
        pub fn with_context<R>(
            &mut self,
            node: impl Into<NodeId>,
            f: impl FnOnce(&mut dyn crate::Context) -> Result<R>,
        ) -> Result<R> {
        }

        /// Run a closure against an immutable view of the root context.
        pub fn with_root_view<R>(&self, f: impl FnOnce(&dyn crate::ViewContext) -> R) -> R {}

        /// Run a closure against an immutable view context bound to a node.
        pub fn with_view<R>(
            &self,
            node: impl Into<NodeId>,
            f: impl FnOnce(&dyn crate::ViewContext) -> R,
        ) -> Result<R> {
        }

        /// Type-check a named Luau source against the finalized app API.
        pub fn check_script(
            &mut self,
            source_name: &str,
            source: &str,
        ) -> Result<script::ScriptCheckResult> {
        }

        /// Drain and return log lines recorded by the most recent script evaluation.
        pub fn take_script_logs(&self) -> Vec<String> {}

        /// Drain and return assertion outcomes from the most recent script evaluation.
        pub fn take_script_assertions(&self) -> Vec<script::ScriptAssertion> {}

        /// Return the in-memory script evaluation journal.
        ///
        /// The journal retains the most recent entries up to the configured limit.
        /// Entry ids are monotonic and never reused, so a first id greater than
        /// one indicates that older entries were evicted or cleared.
        pub fn script_journal(&self) -> &[ScriptJournalEntry] {}

        /// Set the maximum number of retained script journal entries.
        ///
        /// When the journal exceeds the limit the oldest entries are evicted. A
        /// limit of zero disables retention entirely.
        pub fn set_script_journal_limit(&mut self, limit: usize) {}

        /// Evaluate a Luau config file from disk.
        pub fn run_config(&mut self, path: &FsPath) -> Result<()> {}

        /// Remove a binding by ID. Returns true if a binding was removed.
        pub fn unbind(&mut self, id: inputmap::BindingId) -> bool {}

        /// Remove bindings for an input, optionally filtered by mode and path.
        pub fn unbind_input(
            &mut self,
            input: inputmap::InputSpec,
            mode: Option<&str>,
            path_filter: Option<&str>,
        ) -> usize {
        }

        /// Remove all bindings from all modes.
        pub fn clear_bindings(&mut self) -> usize {}

        /// Return bindings in a mode that match a specific path.
        pub fn bindings_matching_path(
            &self,
            mode: &str,
            path: &Path,
        ) -> Vec<inputmap::MatchedBindingInfo<'_>> {
        }

        /// Return the active input mode.
        pub fn input_mode(&self) -> &str {}

        /// Set the active input mode.
        pub fn set_input_mode(&mut self, mode: &str) -> Result<()> {}

        /// Push an input mode above the current mode.
        pub fn push_input_mode(&mut self, mode: &str) -> Result<()> {}

        /// Pop the top input mode and return the new active mode.
        pub fn pop_input_mode(&mut self) -> &str {}

        /// Bind a key or mouse input to switch the active input mode.
        pub fn bind_input_mode(
            &mut self,
            mode: &str,
            input: inputmap::InputSpec,
            path_filter: &str,
            next_mode: &str,
        ) -> Result<inputmap::BindingId> {
        }

        /// Return the most recent key or mouse route trace.
        pub fn route_trace(&self) -> &[RouteTraceEntry] {}

        /// Load the commands from a command node using the default node name.
        /// Returns an error if any command id is already registered.
        pub fn add_commands<T: commands::CommandNode>(&mut self) -> Result<()> {}

        /// Finalize the script API surface for this app.
        pub fn finalize_api(&mut self) -> Result<()> {}

        /// Return the current script API finalization state.
        pub fn script_api_state(&self) -> ScriptApiState {}

        /// Return the rendered Luau definition file for a ready app.
        pub fn script_api(&self) -> Result<&str> {}

        /// Return command availability from the current focus position.
        ///
        /// This computes which commands would resolve to a target if dispatched from the current
        /// focus. For each command:
        /// - Free commands always have `resolution = Some(Free)`
        /// - Node-routed commands have `resolution = Some(Subtree{..})` or `Some(Ancestor{..})`
        ///   if a matching node exists, `None` otherwise
        pub fn command_availability_from_focus(&self) -> Vec<commands::CommandAvailability<'_>> {}

        /// Return command availability from a specific node.
        ///
        /// Computes which commands would dispatch to a target, using the same resolution logic
        /// as `commands::dispatch`:
        /// 1. First search the subtree rooted at `start` in pre-order
        /// 2. Then walk ancestors
        pub fn command_availability_from_node(
            &self,
            start: NodeId,
        ) -> Vec<commands::CommandAvailability<'_>> {
        }

        /// Generate a contextual help snapshot for the current focus.
        ///
        /// The snapshot includes:
        /// - Bindings that would match from the focus path
        /// - Commands with their availability status
        pub fn help_snapshot(&self) -> super::help::HelpSnapshot<'_> {}

        /// Build a diagnostic dump with tree, focus, and binding details.
        pub fn diagnostic_dump(&self, target: NodeId) -> String {}
    }

    /// Outcome of an accepted state mutation.
    #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
    pub enum ChangeOutcome {
        /// The requested state was already active.
        Unchanged,
        /// The request changed state.
        Changed,
    }

    impl ChangeOutcome {
        /// Return whether the request changed state.
        pub fn changed(self) -> bool {}
    }

    /// A typed key for keyed children.
    ///
    /// This trait associates a string key with a specific widget type, providing
    /// compile-time type safety for keyed child access.
    ///
    /// Use the [`crate::key!`] macro to define keys:
    ///
    /// ```
    /// use canopy::{ChildKey, Widget, key};
    ///
    /// pub struct Modal;
    /// impl Widget for Modal {}
    ///
    /// key!(ModalSlot: Modal);
    /// assert_eq!(ModalSlot::KEY, "ModalSlot");
    /// ```
    pub trait ChildKey {
        type Widget: Widget + 'static;
        const KEY: &'static str;
    }

    /// Mutable context available to widgets during event handling.
    pub trait Context: ViewContext {
        /// Focus an attached node.
        fn set_focus(&mut self, node: NodeId) -> Result<ChangeOutcome>;

        /// Move focus in a direction within an explicit scope.
        fn focus_dir(&mut self, scope: FocusScope, dir: Direction) -> Result<ChangeOutcome>;

        /// Focus the first focusable node within an explicit scope.
        fn focus_first(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

        /// Focus the next focusable node within an explicit scope.
        fn focus_next(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

        /// Focus the previous focusable node within an explicit scope.
        fn focus_prev(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

        /// Capture mouse events for the current node.
        fn capture_mouse(&mut self) -> Result<ChangeOutcome>;

        /// Release mouse capture if held by the current node.
        fn release_mouse(&mut self) -> Result<ChangeOutcome>;

        /// Scroll the view to the specified position. Returns `true` if movement occurred.
        fn scroll_to(&mut self, x: u32, y: u32) -> bool;

        /// Scroll the view by the given offsets. Returns `true` if movement occurred.
        fn scroll_by(&mut self, x: i32, y: i32) -> bool;

        /// Scroll the view up by one page. Returns `true` if movement occurred.
        fn page_up(&mut self) -> bool {}

        /// Scroll the view down by one page. Returns `true` if movement occurred.
        fn page_down(&mut self) -> bool {}

        /// Scroll the view up by one line. Returns `true` if movement occurred.
        fn scroll_up(&mut self) -> bool {}

        /// Scroll the view down by one line. Returns `true` if movement occurred.
        fn scroll_down(&mut self) -> bool {}

        /// Scroll the view left by one line. Returns `true` if movement occurred.
        fn scroll_left(&mut self) -> bool {}

        /// Scroll the view right by one line. Returns `true` if movement occurred.
        fn scroll_right(&mut self) -> bool {}

        /// Mark this node dirty so the next frame re-runs layout.
        fn invalidate_layout(&mut self);

        /// Update the layout for the current node.
        fn with_layout(&mut self, f: &mut dyn FnMut(&mut Layout)) -> Result<()> {}

        /// Update the layout for a specific node.
        fn with_layout_of(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Layout)) -> Result<()>;

        /// Create a new widget node detached from the tree.
        fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> Result<NodeId>;

        /// Apply a related set of tree mutations atomically.
        fn apply_tree_edit(
            &mut self,
            edit: &mut dyn FnMut(&mut dyn Context) -> Result<()>,
        ) -> Result<()>;

        /// Execute a closure with mutable access to a widget and its node-bound context.
        fn with_widget_mut(
            &mut self,
            node: NodeId,
            f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
        ) -> Result<()>;

        /// Dispatch a command relative to this node.
        fn dispatch_command(
            &mut self,
            cmd: &CommandInvocation,
        ) -> StdResult<ArgValue, CommandError>;

        /// Dispatch a command with an explicit command-scope frame.
        fn dispatch_command_scoped(
            &mut self,
            frame: CommandScopeFrame,
            cmd: &CommandInvocation,
        ) -> StdResult<ArgValue, CommandError>;

        /// Return the current event snapshot for injection.
        fn current_event(&self) -> Option<&Event>;

        /// Return the current mouse event for injection.
        fn current_mouse_event(&self) -> Option<MouseEvent>;

        /// Return the current list-row context for injection.
        fn current_list_row(&self) -> Option<ListRowContext>;

        /// Add a boxed widget as a child of a specific parent and return the new node ID.
        fn add_child_to_boxed(&mut self, parent: NodeId, widget: Box<dyn Widget>)
            -> Result<NodeId>;

        /// Add a boxed widget as a keyed child of a specific parent and return the new node ID.
        fn add_child_to_keyed_boxed(
            &mut self,
            parent: NodeId,
            key: &str,
            widget: Box<dyn Widget>,
        ) -> Result<NodeId>;

        /// Attach a detached child to a parent.
        fn attach(&mut self, parent: NodeId, child: NodeId) -> Result<()>;

        /// Attach a detached child to a parent using a unique key.
        fn attach_keyed(&mut self, parent: NodeId, key: &str, child: NodeId) -> Result<()>;

        /// Detach a child from its parent.
        fn detach(&mut self, child: NodeId) -> Result<()>;

        /// Remove a node and all descendants from the arena.
        fn remove_subtree(&mut self, node: NodeId) -> Result<()>;

        /// Replace the children list for the current node.
        fn set_children(&mut self, children: Vec<NodeId>) -> Result<()> {}

        /// Replace the children list for a specific parent node.
        fn set_children_of(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()>;

        /// Set the current node's visibility.
        fn set_hidden(&mut self, hidden: bool) -> Result<ChangeOutcome> {}

        /// Set a specific node's visibility.
        fn set_hidden_of(&mut self, node: NodeId, hidden: bool) -> Result<ChangeOutcome>;

        /// Request a cooperative shutdown with the provided status code.
        fn exit(&mut self, code: i32);

        /// Add an effect to a node that will be applied during rendering.
        /// Effects stack and inherit through the tree.
        fn push_effect(&mut self, node: NodeId, effect: Effect) -> Result<()>;

        /// Clear all effects on a node.
        fn clear_effects(&mut self, node: NodeId) -> Result<()>;

        /// Set the style map to be used for rendering.
        /// The style change will be applied before the next render.
        fn set_style(&mut self, style: StyleMap);

        /// Request a help snapshot to be injected into the specified target node.
        ///
        /// This should be called before changing focus or layout, so the snapshot
        /// captures the pre-help context. After the current command returns, Canopy
        /// will capture the snapshot and inject it into the target widget.
        fn request_help_snapshot(&mut self, target: NodeId);

        /// Take the pending help snapshot, if any.
        ///
        /// This is called by help widgets to retrieve the snapshot that was
        /// captured when `request_help_snapshot` was called. Returns `None` if
        /// no snapshot is pending.
        fn take_help_snapshot(&mut self) -> Option<OwnedHelpSnapshot>;

        /// Request a diagnostic dump for a target node.
        fn request_diagnostic_dump(&mut self, target: NodeId);
    }

    /// A named, reproducible application state.
    #[derive(Clone)]
    pub struct Fixture {
        /// Fixture name.
        pub name: String,
        /// Human-readable fixture description.
        pub description: String,
        /// Setup closure applied to the current canopy instance.
        pub setup: std::sync::Arc<
            dyn Fn(&mut super::canopy::Canopy) -> crate::error::Result<()> + Send + Sync,
        >,
    }

    impl Fixture {
        /// Construct a fixture from owned name/description values.
        pub fn new(
            name: impl Into<String>,
            description: impl Into<String>,
            setup: impl Fn(&mut Canopy) -> Result<()> + Send + Sync + 'static,
        ) -> Self {
        }

        /// Return fixture metadata without the setup closure.
        pub fn info(&self) -> FixtureInfo {}
    }

    /// Serializable metadata about a registered fixture.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
    pub struct FixtureInfo {
        /// Fixture name.
        pub name: String,
        /// Human-readable fixture description.
        pub description: String,
    }

    impl JsonSchema for FixtureInfo {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Subtree used by a focus traversal operation.
    #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
    pub enum FocusScope {
        /// The current widget's subtree.
        Current,
        /// The complete widget tree.
        Root,
        /// A subtree rooted at an explicit node.
        Node(super::id::NodeId),
    }

    /// Input event used for bindings.
    ///
    /// Key inputs are normalized when stored or matched so bindings are resilient
    /// to terminal differences in Ctrl/Shift representations.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq, Display)]
    pub enum InputSpec {
        /// Mouse input.
        Mouse(crate::event::mouse::Mouse),
        /// Keyboard input.
        Key(crate::event::key::Key),
    }

    /// Ordered keyed child collection helper.
    ///
    /// Stores a stable mapping from keys to node IDs plus a current order. Use
    /// [`KeyedChildren::reconcile`] to create, update, and reorder children based on a desired key list.
    #[derive(Debug, Default)]
    pub struct KeyedChildren<K, W> {}

    impl<K, W> KeyedChildren<K, W>
    where
        K: Eq + Hash + Clone,
        W: Widget + 'static,
    {
        /// Construct an empty keyed collection.
        pub fn new() -> Self {}

        /// Return true if there are no ordered keys.
        pub fn is_empty(&self) -> bool {}

        /// Return the number of ordered keys.
        pub fn len(&self) -> usize {}

        /// Return the ordered key slice.
        pub fn keys(&self) -> &[K] {}

        /// Return the node ID for a key, if present.
        pub fn id_for(&self, key: &K) -> Option<TypedId<W>> {}

        /// Return the node ID at a given index, if present.
        pub fn id_at(&self, index: usize) -> Option<TypedId<W>> {}

        /// Iterate node IDs in the current order.
        pub fn iter_ids(&self) -> impl Iterator<Item = TypedId<W>> + '_ {}

        /// Reconcile this collection against the desired key order.
        pub fn reconcile<I, C, U>(
            &mut self,
            ctx: &mut dyn Context,
            desired: I,
            create: C,
            update: U,
            remove: RemovePolicy,
        ) -> Result<Vec<TypedId<W>>>
        where
            I: IntoIterator<Item = K>,
            C: FnMut(&K) -> Result<W>,
            U: FnMut(&K, TypedId<W>, &mut dyn Context) -> Result<()>, {
        }
    }

    /// A trait that allows widgets to perform recursive initialization of themselves and their
    /// children.
    pub trait Loader {
        /// Load commands or resources into the canopy instance.
        /// Returns an error if loading fails.
        fn load(_: &mut Canopy) -> Result<()> {}
    }

    /// Opaque identifier for a node stored in the Core arena.
    #[derive(
        Copy, Clone, Default, Eq, StructuralPartialEq, PartialEq, Ord, PartialOrd, Hash, Debug,
    )]
    pub struct NodeId(_);

    impl ToArgValue for crate::core::NodeId {
        fn to_arg_value(self) -> ArgValue {}
    }

    impl FromArgValue for crate::core::NodeId {
        fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {}
    }

    impl CommandType for crate::core::NodeId {
        fn luau_ty() -> declaration::Type {}

        fn luau_decls(registry: &mut DeclRegistry<'_>) {}
    }

    impl From<KeyData> for NodeId {
        fn from(k: KeyData) -> Self {}
    }

    impl Key for NodeId {
        fn data(&self) -> KeyData {}
    }

    impl<T> From<TypedId<T>> for NodeId {
        fn from(value: TypedId<T>) -> Self {}
    }

    /// A path of node name components.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, FromStr, Display)]
    pub struct Path {}

    impl Path {
        /// Construct an empty path.
        pub fn empty() -> Self {}

        /// Parse and validate a path from a slash-separated string.
        pub fn parse(path: &str) -> Result<Self> {}

        /// Pop an item off the end of the path, modifying it in place. Return None
        /// if the path is empty.
        pub fn pop(&mut self) -> Option<String> {}

        /// Construct a path from a slice of components.
        pub fn new<I>(v: I) -> Self
        where
            I: IntoIterator,
            I::Item: AsRef<str>, {
        }
    }

    impl From<Vec<String>> for Path {
        fn from(path: Vec<String>) -> Self {}
    }

    impl From<&[&str]> for Path {
        fn from(v: &[&str]) -> Self {}
    }

    impl From<&str> for Path {
        fn from(v: &str) -> Self {}
    }

    /// A validated path filter used to search node paths.
    ///
    /// Filters support `*` for one component and `**` for zero or more components.
    /// Literal components must be valid [`NodeName`] values.
    #[derive(Debug, Clone, FromStr)]
    pub struct PathFilter {}

    impl PathFilter {
        /// Compile a validated path filter.
        pub fn new(filter: &str) -> Result<Self> {}

        /// Compile a filter after normalizing it to a full-path match.
        pub fn normalized(filter: &str) -> Result<Self> {}

        /// Return the original filter string.
        pub fn as_str(&self) -> &str {}
    }

    /// Policy for removing children that are no longer desired.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
    pub enum RemovePolicy {
        /// Detach nodes from the tree but keep them alive.
        Detach,
        /// Remove nodes and their descendants from the arena.
        RemoveSubtree,
        /// Hide nodes and keep them available for reuse.
        Hide,
    }

    /// A phase in key or mouse event routing.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
    pub enum RoutePhase {
        /// The initial routing target was selected.
        Target,
        /// A binding matched before the widget received the event.
        PreEventBinding,
        /// The event was offered to a widget.
        WidgetEvent,
        /// A binding matched after the widget ignored the event.
        PostEventBinding,
        /// Routing moved from a node to its parent.
        Bubble,
        /// A resolved binding is being executed.
        BindingExecution,
        /// A widget or binding handled the event.
        Handled,
        /// Routing ended without a handler.
        Unhandled,
    }

    impl RoutePhase {
        /// Return a stable diagnostic label for this phase.
        pub fn as_str(self) -> &'static str {}
    }

    /// One entry in the most recent input route trace.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq)]
    pub struct RouteTraceEntry {
        /// Routing phase.
        pub phase: RoutePhase,
        /// Node associated with this route step.
        pub node: Option<crate::core::NodeId>,
        /// Path visible to binding resolution at this route step.
        pub path: String,
        /// Human-readable route detail.
        pub detail: String,
    }

    /// Script API finalization state.
    #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
    pub enum ScriptApiState {
        /// Registrations remain open and no surface is staged.
        Open,
        /// The surface is staged but the runtime has not been published.
        Preparing,
        /// The runtime, definitions, and module source are ready.
        Ready,
    }

    /// Replayable record of one script evaluation.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ScriptJournalEntry {
        /// Monotonic journal id.
        pub id: u64,
        /// Script origin such as `eval`, `config:<path>`, or `startup:app`.
        pub origin: String,
        /// Evaluated source text.
        pub source: String,
        /// Whether the evaluation completed successfully.
        pub ok: bool,
        /// Error message when `ok` is false.
        pub error: Option<String>,
        /// Logs emitted by the script.
        pub logs: Vec<String>,
        /// Assertions emitted by the script.
        pub assertions: Vec<script::ScriptAssertion>,
        /// Wall-clock duration in milliseconds.
        pub duration_ms: u64,
    }

    /// Filesystem roots used by Canopy's persistent Luau module source.
    #[derive(Clone, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
    pub struct ScriptModuleRoots {}

    impl ScriptModuleRoots {
        /// Construct an empty root set.
        pub fn new() -> Self {}

        /// Return the configured `@user` root.
        pub fn user_root(&self) -> Option<&Path> {}

        /// Return the configured `@project` root.
        pub fn project_root(&self) -> Option<&Path> {}

        /// Mount `@user` at `root`.
        pub fn set_user_root(&mut self, root: impl Into<PathBuf>) {}

        /// Mount `@project` at `root`.
        pub fn set_project_root(&mut self, root: impl Into<PathBuf>) {}

        /// Locate the nearest `.canopy` directory at or above `start`.
        pub fn discover_project_root(start: impl AsRef<Path>) -> Option<PathBuf> {}
    }

    /// Slot helper for keyed children that caches the resolved typed ID.
    #[derive(Debug, Default)]
    pub struct Slot<K: ChildKey> {}

    impl<K: ChildKey> Slot<K> {
        /// Construct an empty slot.
        pub fn new() -> Self {}

        /// Clear any cached typed ID.
        pub fn clear(&mut self) {}

        /// Get or create the keyed child under the current node.
        pub fn get_or_create(
            &mut self,
            ctx: &mut dyn Context,
            make: impl FnOnce() -> K::Widget,
        ) -> Result<TypedId<K::Widget>> {
        }

        /// Get or create the keyed child under a specific parent node.
        pub fn get_or_create_in(
            &mut self,
            ctx: &mut dyn Context,
            parent: impl Into<NodeId>,
            make: impl FnOnce() -> K::Widget,
        ) -> Result<TypedId<K::Widget>> {
        }

        /// Execute a closure with a keyed child under the current node.
        pub fn with<R>(
            &mut self,
            ctx: &mut dyn Context,
            f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
        ) -> Result<R> {
        }

        /// Execute a closure with a keyed child under a specific parent node.
        pub fn with_in<R>(
            &mut self,
            ctx: &mut dyn Context,
            parent: impl Into<NodeId>,
            f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
        ) -> Result<R> {
        }
    }

    /// Type-safe wrapper around a node identifier tied to a widget type.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct TypedId<T> {}

    impl<T> From<TypedId<T>> for NodeId {
        fn from(value: TypedId<T>) -> Self {}
    }

    /// Read-only context available to widgets during render and measure.
    pub trait ViewContext {
        /// The node currently being rendered.
        fn node_id(&self) -> NodeId;

        /// The root node of the tree.
        fn root_id(&self) -> NodeId;

        /// View information for the current node.
        fn view(&self) -> View {}

        /// Cached layout configuration for the current node.
        fn layout(&self) -> Layout {}

        /// View information for a specific node.
        fn node_view(&self, node: NodeId) -> Option<View>;

        /// Layout configuration for a specific node.
        fn node_layout(&self, node: NodeId) -> Option<Layout>;

        /// Widget type identifier for a specific node.
        fn node_type_id(&self, node: NodeId) -> Option<TypeId>;

        /// Visible view rectangle in content coordinates.
        fn view_rect(&self) -> Rect {}

        /// Visible view rectangle in local outer coordinates.
        fn view_rect_local(&self) -> Rect {}

        /// Local outer rectangle for this node.
        fn outer_rect_local(&self) -> Rect {}

        /// Children of the current node in tree order.
        fn children(&self) -> Vec<NodeId> {}

        /// Children of a specific node in tree order.
        fn children_of(&self, node: NodeId) -> Vec<NodeId>;

        /// Does the current node have focus?
        fn is_focused(&self) -> bool {}

        /// Does the specified node have focus?
        fn node_is_focused(&self, node: NodeId) -> bool;

        /// Return the currently focused node, including one not yet laid out.
        fn focused_node(&self) -> Option<NodeId>;

        /// Is the current node on the focus path?
        fn is_on_focus_path(&self) -> bool {}

        /// Is the specified node on the focus path?
        fn node_is_on_focus_path(&self, node: NodeId) -> bool;

        /// Return the focused leaf under the subtree rooted at `root`.
        fn focused_leaf(&self, root: NodeId) -> Option<NodeId>;

        /// Return focusable leaves in pre-order under the subtree rooted at `root`.
        fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId>;

        /// Return the parent of a node, or `None` if it is the root or not found.
        fn parent_of(&self, node: NodeId) -> Option<NodeId>;

        /// Return whether a node exists and is attached to the root tree.
        fn node_is_attached(&self, node: NodeId) -> bool;

        /// Return the path for a node relative to a root.
        fn node_path(&self, root: NodeId, node: NodeId) -> Path;

        /// Locate the deepest visible node at a point within a subtree.
        fn locate(&self, root: NodeId, point: Point) -> Result<Option<NodeId>>;

        /// Return a keyed child relative to the current node.
        fn child_keyed(&self, key: &str) -> Option<NodeId> {}

        /// Return a keyed child relative to a specific parent node.
        fn child_keyed_in(&self, parent: NodeId, key: &str) -> Option<NodeId>;

        /// Find the first node whose path matches the filter, relative to the current node.
        ///
        /// The filter is normalized to match full paths.
        fn find_node(&self, path_filter: &str) -> Option<NodeId> {}

        /// Find the first node whose path matches the validated filter.
        fn find_node_matching(&self, path_filter: &PathFilter) -> Option<NodeId> {}

        /// Find all nodes whose paths match the filter, relative to the current node.
        ///
        /// The filter is normalized to match full paths.
        fn find_nodes(&self, path_filter: &str) -> Vec<NodeId> {}

        /// Find all nodes whose paths match the validated filter.
        fn find_nodes_matching(&self, path_filter: &PathFilter) -> Vec<NodeId> {}

        /// Peek at the pending help snapshot, if any.
        ///
        /// This is used by help widgets to check if a snapshot is available
        /// during render, without consuming it.
        fn pending_help_snapshot(&self) -> Option<&OwnedHelpSnapshot>;
    }

    pub mod commands {
        //! Command definition and dispatch.

        pub use ruau::declaration;
        /// Canonical dynamic representation for command arguments and return values.
        #[derive(Clone, Debug, StructuralPartialEq, PartialEq)]
        pub enum ArgValue {
            /// Null value.
            Null,
            /// Boolean value.
            Bool(bool),
            /// Integer value.
            Int(i64),
            /// Unsigned integer value.
            UInt(u64),
            /// Float value.
            Float(f64),
            /// String value.
            String(String),
            /// Opaque node handle.
            Node(crate::core::NodeId),
            /// Array value.
            Array(Vec<Self>),
            /// Map value.
            Map(std::collections::BTreeMap<String, Self>),
        }

        impl ArgValue {
            /// Convert this dynamic value into JSON for external automation APIs.
            pub fn to_json_value(&self) -> Result<JsonValue, CommandError> {}

            /// Convert this dynamic value into external automation JSON.
            ///
            /// Opaque `NodeId` values become descriptive tokens for reporting, but those
            /// tokens are not accepted by [`ArgValue::from_json_value`].
            pub fn to_external_json_value(&self) -> Result<JsonValue, CommandError> {}

            /// Convert JSON into an `ArgValue` for external automation APIs.
            pub fn from_json_value(value: JsonValue) -> Result<Self, CommandError> {}
        }

        impl ToArgValue for ArgValue {
            fn to_arg_value(self) -> ArgValue {}
        }

        impl FromArgValue for ArgValue {
            fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {}
        }

        impl CommandType for ArgValue {
            fn luau_ty() -> declaration::Type {}
        }

        /// Direction for focus movement commands.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub enum FocusDirection {
            /// Move to the next focusable node.
            Next,
            /// Move to the previous focusable node.
            Prev,
            /// Move focus up.
            Up,
            /// Move focus down.
            Down,
            /// Move focus left.
            Left,
            /// Move focus right.
            Right,
        }

        impl ToArgValue for FocusDirection {
            fn to_arg_value(self) -> canopy::commands::ArgValue {}
        }

        impl FromArgValue for FocusDirection {
            fn from_arg_value(
                v: &canopy::commands::ArgValue,
            ) -> ::std::result::Result<Self, canopy::commands::CommandError> {
            }
        }

        impl CommandType for FocusDirection {
            fn luau_ty() -> canopy::commands::declaration::Type {}

            fn luau_decls(registry: &mut canopy::commands::DeclRegistry<'_>) {}
        }

        /// Direction for zoom commands.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub enum ZoomDirection {
            /// Zoom in.
            In,
            /// Zoom out.
            Out,
        }

        impl ToArgValue for ZoomDirection {
            fn to_arg_value(self) -> canopy::commands::ArgValue {}
        }

        impl FromArgValue for ZoomDirection {
            fn from_arg_value(
                v: &canopy::commands::ArgValue,
            ) -> ::std::result::Result<Self, canopy::commands::CommandError> {
            }
        }

        impl CommandType for ZoomDirection {
            fn luau_ty() -> canopy::commands::declaration::Type {}

            fn luau_decls(registry: &mut canopy::commands::DeclRegistry<'_>) {}
        }

        /// Convert a typed value into an ArgValue.
        pub trait ToArgValue {
            /// Encode the value as an ArgValue.
            fn to_arg_value(self) -> ArgValue;
        }

        /// Convert an ArgValue into a typed value.
        pub trait FromArgValue: Sized {
            /// Decode the value from an ArgValue.
            fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError>;
        }

        /// Static Luau type metadata for values in command signatures.
        pub trait CommandType {
            /// Luau type expression for this Rust value.
            fn luau_ty() -> declaration::Type;

            /// Registers declaration items needed by this type.
            fn luau_decls(_registry: &mut DeclRegistry<'_>) {}
        }

        /// Marker trait for serde-backed command arguments.
        pub trait CommandArg: Serialize + DeserializeOwned + 'static {}

        /// Registry for declaration items required by command argument and return types.
        ///
        /// Tracks in-flight named registrations so recursive and shared types
        /// terminate: a type's `luau_decls` claims its name with [`DeclRegistry::begin`]
        /// before recursing into field types.
        pub struct DeclRegistry<'a> {}

        impl<'a> DeclRegistry<'a> {
            /// Wrap a declaration builder.
            pub fn new(builder: &'a mut declaration::Builder) -> Self {}

            /// Claim a type name for registration.
            ///
            /// Returns false when the name is already present or in progress, in which
            /// case the caller must skip both recursion and registration.
            pub fn begin(&mut self, name: &str) -> bool {}

            /// Registers an alias declaration.
            pub fn alias(&mut self, alias: declaration::Alias) {}

            /// Registers a class declaration.
            pub fn class(&mut self, class: declaration::Class) {}

            /// Registers an external type name.
            pub fn extern_ty(&mut self, name: impl Into<declaration::Text>) {}
        }

        /// Wrapper for fallible serde argument conversion.
        pub struct SerdeArg<T>(pub T);

        impl<T> SerdeArg<T>
        where
            T: Serialize,
        {
            /// Encode a serde argument into ArgValue, returning conversion errors.
            pub fn try_to_arg_value(self) -> Result<ArgValue, CommandError> {}
        }

        impl<T> TryToArgValue for SerdeArg<T>
        where
            T: Serialize,
        {
            fn try_to_arg_value(self) -> Result<ArgValue, CommandError> {}
        }

        /// Convert a typed value into an ArgValue with fallible encoding.
        pub trait TryToArgValue {
            /// Encode the value as an ArgValue.
            fn try_to_arg_value(self) -> Result<ArgValue, CommandError>;
        }

        /// Identifier for a command.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Hash, Display)]
        pub struct CommandId(pub &'static str);

        /// Canonical argument container for command invocation.
        #[derive(Clone, Debug, StructuralPartialEq, PartialEq, Default)]
        pub enum CommandArgs {
            /// Positional arguments.
            Positional(Vec<ArgValue>),
            /// Named arguments.
            Named(std::collections::BTreeMap<String, ArgValue>),
        }

        impl CommandArgs {
            /// Build command arguments with fallible conversions.
            pub fn try_from_args(args: impl TryIntoCommandArgs) -> Result<Self, CommandError> {}
        }

        impl From<()> for CommandArgs {
            fn from(_: ()) -> Self {}
        }

        impl<T, const N: usize> From<[T; N]> for CommandArgs
        where
            T: ToArgValue,
        {
            fn from(values: [T; N]) -> Self {}
        }

        impl<T> From<Vec<T>> for CommandArgs
        where
            T: ToArgValue,
        {
            fn from(values: Vec<T>) -> Self {}
        }

        impl<T> From<BTreeMap<String, T>> for CommandArgs
        where
            T: ToArgValue,
        {
            fn from(values: BTreeMap<String, T>) -> Self {}
        }

        impl TryIntoCommandArgs for CommandArgs {
            fn try_into_command_args(self) -> Result<CommandArgs, CommandError> {}
        }

        /// Fallible conversion into command arguments.
        pub trait TryIntoCommandArgs {
            /// Convert into command arguments.
            fn try_into_command_args(self) -> Result<CommandArgs, CommandError>;
        }

        /// A command invocation with encoded arguments.
        #[derive(Clone, Debug, StructuralPartialEq, PartialEq)]
        pub struct CommandInvocation {
            /// Command identifier.
            pub id: CommandId,
            /// Invocation arguments.
            pub args: CommandArgs,
        }

        impl From<CommandCall> for CommandInvocation {
            fn from(call: CommandCall) -> Self {}
        }

        impl From<&'static CommandSpec> for CommandInvocation {
            fn from(spec: &'static CommandSpec) -> Self {}
        }

        /// Identifies how a command parameter is provided.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum CommandParamKind {
            /// Provided by injection.
            Injected,
            /// Provided by user arguments.
            User,
        }

        /// Static metadata for a type in command signatures.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct CommandTypeSpec {
            /// Rust type name for introspection.
            pub rust: &'static str,
            /// Luau type expression factory.
            pub ty: fn() -> declaration::Type,
            /// Declaration dependency registration function.
            pub decls: fn(_: &mut DeclRegistry<'a>),
            /// Optional documentation string.
            pub doc: Option<&'static str>,
        }

        impl CommandTypeSpec {
            /// Returns the Luau type expression.
            pub fn luau_ty(self) -> declaration::Type {}

            /// Registers declaration dependencies for this type.
            pub fn luau_decls(self, registry: &mut DeclRegistry<'_>) {}
        }

        /// Static metadata for a command parameter.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub struct CommandParamSpec {
            /// Parameter name for named argument binding.
            pub name: &'static str,
            /// Parameter kind.
            pub kind: CommandParamKind,
            /// Optional parameter documentation.
            pub doc: Option<&'static str>,
            /// Type metadata.
            pub ty: CommandTypeSpec,
            /// Whether the parameter is optional.
            pub optional: bool,
            /// Optional default expression string.
            pub default: Option<&'static str>,
        }

        /// Static metadata for a command return type.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum CommandReturnSpec {
            /// Unit return.
            Unit,
            /// Non-unit return.
            Value(CommandTypeSpec),
        }

        /// Erased invoke function signature.
        pub type InvokeFn = fn(
            target: Option<&mut dyn Any>,
            ctx: &mut dyn Context,
            inv: &CommandInvocation,
        ) -> Result<ArgValue, CommandError>;

        /// Command dispatch routing.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum CommandDispatchKind {
            /// Invoke with `target = None`.
            Free,
            /// Route to a node by owner name.
            Node {
                /// Owner node name.
                owner: &'static str,
            },
        }

        /// Documentation metadata for a command.
        #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
        pub struct CommandDocSpec {
            /// Short, single-line description for tables/tooltips.
            pub short: Option<&'static str>,
            /// Full description (future: rich help/palette).
            pub long: Option<&'static str>,
            /// Hide from interactive help unless explicitly requested.
            pub hidden: bool,
        }

        /// Static metadata for a command.
        #[derive(Clone, Copy, Debug)]
        pub struct CommandSpec {
            /// Command identifier.
            pub id: CommandId,
            /// Command name.
            pub name: &'static str,
            /// Dispatch routing.
            pub dispatch: CommandDispatchKind,
            /// Parameter specs.
            pub params: &'static [CommandParamSpec],
            /// Return spec.
            pub ret: CommandReturnSpec,
            /// Documentation metadata.
            pub doc: CommandDocSpec,
            /// Erased invoke entrypoint.
            pub invoke: InvokeFn,
        }

        impl CommandSpec {
            /// Build a call to this command with no arguments.
            pub fn call(&self) -> CommandCall {}

            /// Build a call to this command.
            pub fn call_with(&self, args: impl Into<CommandArgs>) -> CommandCall {}

            /// Build a call to this command with fallible argument conversion.
            pub fn try_call_with(
                &self,
                args: impl TryIntoCommandArgs,
            ) -> Result<CommandCall, CommandError> {
            }
        }

        impl From<&'static CommandSpec> for CommandInvocation {
            fn from(spec: &'static CommandSpec) -> Self {}
        }

        /// Resolution of a command dispatch target.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum CommandResolution {
            /// Command is free (no target).
            Free,
            /// Command would dispatch to a node in the focus subtree.
            Subtree {
                /// Target node ID.
                target: crate::core::NodeId,
            },
            /// Command would dispatch to an ancestor of focus.
            Ancestor {
                /// Target node ID.
                target: crate::core::NodeId,
            },
        }

        impl CommandResolution {
            /// Return the resolved node target, if this command dispatches to a node.
            pub fn target(self) -> Option<NodeId> {}
        }

        /// Command availability from a given focus context.
        #[derive(Clone, Copy, Debug)]
        pub struct CommandAvailability<'a> {
            /// Command specification.
            pub spec: &'a CommandSpec,
            /// Resolution if the command has a target, or `None` if no target exists.
            pub resolution: Option<CommandResolution>,
        }

        /// The CommandNode trait is implemented by widgets to expose commands.
        pub trait CommandNode {
            /// Return a list of commands for this node.
            fn commands() -> &'static [&'static CommandSpec]
            where
                Self: Sized;
        }

        /// Builder for a command invocation.
        #[derive(Clone, Debug)]
        pub struct CommandCall {}

        impl CommandCall {
            /// Convert into an invocation.
            pub fn invocation(self) -> CommandInvocation {}
        }

        impl From<CommandCall> for CommandInvocation {
            fn from(call: CommandCall) -> Self {}
        }

        /// Collection of available commands keyed by id.
        #[derive(Clone, Debug, Default)]
        pub struct CommandSet {}

        impl CommandSet {
            /// Construct an empty command set.
            pub fn new() -> Self {}

            /// Add a command batch atomically.
            ///
            /// Repeating an equivalent definition is idempotent. A conflicting definition or invalid
            /// batch leaves the set unchanged.
            pub fn add(
                &mut self,
                specs: &'static [&'static CommandSpec],
            ) -> Result<(), CommandError> {
            }

            /// Get a command by id.
            pub fn get(&self, id: &str) -> Option<&'static CommandSpec> {}

            /// Iterate over all command specs.
            pub fn iter(&self) -> impl Iterator<Item = (&'static str, &'static CommandSpec)> + '_ {}
        }

        /// Error type for command dispatch and conversion.
        #[derive(Debug, Error, Display)]
        pub enum CommandError {
            /// Unknown command identifier.
            UnknownCommand {
                /// Requested command id.
                id: String,
            },
            /// Duplicate command identifier.
            DuplicateCommand {
                /// Duplicate command id.
                id: String,
            },
            /// A command ID was registered with a different specification.
            ConflictingCommand {
                /// Conflicting command id.
                id: String,
            },
            /// Static command metadata is invalid.
            InvalidCommand {
                /// Invalid command id.
                id: String,
                /// Validation failure.
                message: String,
            },
            /// No matching target found for a node-routed command.
            NoTarget {
                /// Requested command id.
                id: String,
                /// Expected owner node name.
                owner: String,
            },
            /// A node handle no longer points at a live node.
            InvalidNode {
                /// Stale node id.
                id: crate::core::NodeId,
            },
            /// Incorrect number of arguments.
            ArityMismatch {
                /// Expected positional argument count.
                expected: usize,
                /// Actual positional argument count.
                got: usize,
            },
            /// Missing named argument.
            MissingNamedArg {
                /// Parameter name.
                name: String,
            },
            /// Unknown named argument.
            UnknownNamedArg {
                /// Provided name.
                name: String,
                /// Allowed names.
                allowed: Vec<&'static str>,
            },
            /// Type mismatch error.
            TypeMismatch {
                /// Parameter name.
                param: String,
                /// Expected type.
                expected: &'static str,
                /// Provided type.
                got: String,
            },
            /// Missing injected value.
            MissingInjected {
                /// Parameter name.
                param: String,
                /// Expected injected type.
                expected: &'static str,
            },
            /// Conversion error.
            Conversion {
                /// Parameter name.
                param: String,
                /// Error message.
                message: String,
            },
            /// The command target did not have the registered owner type.
            TargetTypeMismatch,
            /// Command execution failure.
            Exec(Box<dyn StdError + Send + Sync>),
        }

        impl From<CommandError> for Error {
            fn from(source: CommandError) -> Self {}
        }

        impl From<&CommandError> for CanopyErrorPayload {
            fn from(err: &commands::CommandError) -> Self {}
        }

        /// Errors raised during injection.
        #[derive(Debug)]
        pub enum InjectError {
            /// Required injected value missing.
            Missing {
                /// Expected type.
                expected: &'static str,
            },
            /// Injected value failed.
            Failed {
                /// Expected type.
                expected: &'static str,
                /// Error message.
                message: String,
            },
        }

        /// Trait for injectable parameters.
        pub trait Inject: Sized {
            /// Inject a value from the context.
            fn inject(ctx: &dyn Context) -> Result<Self, InjectError>;
        }

        /// Explicit injection wrapper.
        #[derive(Debug, Clone, Copy)]
        pub struct Injected<T>(pub T);

        impl<T> Inject for Injected<T>
        where
            T: Inject,
        {
            fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {}
        }

        /// Explicit user argument wrapper.
        #[derive(Debug)]
        pub struct Arg<T>(pub T);

        impl<T> FromArgValue for Arg<T>
        where
            T: FromArgValue,
        {
            fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {}
        }

        /// Context passed to list row injections.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub struct ListRowContext {
            /// Owning list node id.
            pub list: crate::core::NodeId,
            /// Row index.
            pub index: usize,
        }

        impl Inject for ListRowContext {
            fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {}
        }

        /// Command scope frame for injection.
        #[derive(Debug, Clone, Default)]
        pub struct CommandScopeFrame {
            /// Event snapshot.
            pub event: Option<crate::event::Event>,
            /// Mouse event snapshot.
            pub mouse: Option<crate::event::mouse::MouseEvent>,
            /// List row context.
            pub list_row: Option<ListRowContext>,
        }
    }

    pub mod cursor {
        //! Cursor and position helpers.

        /// Cursor glyph shape variants.
        #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq)]
        pub enum CursorShape {
            /// Underscore cursor.
            Underscore,
            /// Vertical bar cursor.
            Line,
            /// Block cursor.
            Block,
        }

        /// Cursor position, shape, and blink behavior.
        #[derive(Debug, Clone, Hash, StructuralPartialEq, PartialEq, Eq)]
        pub struct Cursor {
            /// Location of the cursor, relative to (0, 0) in the node view rect.
            pub location: geom::Point,
            /// Shape of the cursor.
            pub shape: CursorShape,
            /// Should the cursor blink?
            pub blink: bool,
        }

        impl Add<Point> for Cursor {
            type Output = Cursor;
            fn add(self, other: geom::Point) -> Self {}
        }
    }

    pub mod error {
        //! Core error types.

        /// Result type for canopy-core operations.
        pub type Result<T> = std::result::Result<T, Error>;

        /// Parse error marker type.
        #[derive(StructuralPartialEq, PartialEq, Eq, Debug, Clone, Display, Error)]
        pub struct ParseError {
            /// Parse error message.
            pub message: String,
            /// One-based source line, when known.
            pub line: Option<usize>,
            /// Source byte offset, when known.
            pub offset: Option<usize>,
        }

        impl ParseError {
            /// Construct a parse error from a message.
            pub fn new(message: impl Into<String>) -> Self {}

            /// Construct a parse error with optional line/offset information.
            pub fn with_position(
                message: impl Into<String>,
                line: Option<usize>,
                offset: Option<usize>,
            ) -> Self {
            }
        }

        /// Phase in which a node-bound widget operation failed.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Display)]
        pub enum NodeOperationKind {
            /// Widget access or lifecycle callback.
            Access,
            /// Widget measurement or layout.
            Layout,
            /// Widget rendering.
            Render,
        }

        /// Stable category for a structured script or command failure.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Display)]
        pub enum ScriptErrorKind {
            /// Cooperative execution timeout.
            Timeout,
            /// Node lookup failed.
            NodeNotFound,
            /// A node exists but is detached.
            NodeDetached,
            /// A value or widget type did not match.
            TypeMismatch,
            /// A requested value was not found.
            NotFound,
            /// Invalid input or operation.
            Invalid,
            /// Unclassified Canopy failure.
            Canopy,
            /// Unknown command identifier.
            UnknownCommand,
            /// Duplicate command identifier.
            DuplicateCommand,
            /// Conflicting command definition.
            ConflictingCommand,
            /// Invalid command definition.
            InvalidCommand,
            /// No command target was found.
            NoTarget,
            /// A command node handle is stale.
            InvalidNode,
            /// Positional argument count mismatch.
            ArityMismatch,
            /// Required named argument is missing.
            MissingNamedArgument,
            /// An unknown named argument was supplied.
            UnknownNamedArgument,
            /// Argument conversion failed.
            Conversion,
            /// An injected value is missing.
            MissingInjected,
            /// The routed target has the wrong widget type.
            TargetTypeMismatch,
            /// Command implementation returned an error.
            CommandExecution,
            /// Another top-level script evaluation is active.
            ScriptBusy,
        }

        impl ScriptErrorKind {
            /// Return the stable protocol label for this category.
            pub const fn as_str(self) -> &'static str {}
        }

        /// Core error type.
        #[derive(Error, Display, Debug)]
        pub enum Error {
            /// A render target exceeds its configured width limit.
            RenderWidthLimit {
                /// Requested target width.
                requested: u32,
                /// Configured maximum width.
                limit: u32,
            },
            /// A render target exceeds its configured height limit.
            RenderHeightLimit {
                /// Requested target height.
                requested: u32,
                /// Configured maximum height.
                limit: u32,
            },
            /// Render-target dimensions cannot be represented as a cell count.
            RenderCellCountOverflow {
                /// Requested target width.
                width: u32,
                /// Requested target height.
                height: u32,
            },
            /// A render target exceeds its configured total-cell limit.
            RenderCellLimit {
                /// Requested target cell count.
                requested: usize,
                /// Configured maximum cell count.
                limit: usize,
            },
            /// Render-target backing storage could not be reserved.
            RenderAllocation {
                /// Requested target cell count.
                cells: usize,
            },
            /// A single-cell drawing API received a character with an invalid width.
            InvalidCellCharacter {
                /// Rejected character.
                ch: char,
                /// Computed terminal width.
                width: usize,
            },
            /// Geometry failure.
            Geometry(geom::Error),
            /// Invalid layout configuration.
            InvalidLayout(crate::layout::LayoutValidationError),
            /// Terminal I/O failure.
            TerminalIo(io::Error),
            /// Run loop failure.
            RunLoop(String),
            /// Internal error.
            Internal(String),
            /// Core invariant violation.
            Invariant(String),
            /// Re-entrant widget borrow attempt.
            ReentrantWidgetBorrow(crate::core::id::NodeId),
            /// Node-bound widget operation failure with its original source.
            NodeOperation {
                /// Operation phase.
                kind: NodeOperationKind,
                /// Stable operation name.
                operation: &'static str,
                /// Node being operated on.
                node: crate::core::id::NodeId,
                /// Node path at the time of failure.
                path: String,
                /// Original typed failure.
                source: Box<Self>,
            },
            /// Invalid input error.
            Invalid(String),
            /// Requested item was not found.
            NotFound(String),
            /// Widget type mismatch.
            TypeMismatch {
                /// Expected widget type name.
                expected: String,
                /// Actual widget type name.
                actual: String,
            },
            /// A live node stores a different widget type than requested.
            NodeTypeMismatch {
                /// Node whose widget type was checked.
                node: crate::core::id::NodeId,
                /// Requested widget type.
                expected: &'static str,
            },
            /// A query matched multiple nodes.
            MultipleMatches,
            /// Duplicate child key under the same parent.
            DuplicateChildKey(String),
            /// Duplicate child under the same parent.
            DuplicateChild {
                /// Parent node.
                parent: crate::core::id::NodeId,
                /// Child node.
                child: crate::core::id::NodeId,
            },
            /// Child is already attached to a parent.
            AlreadyAttached(crate::core::id::NodeId),
            /// Attaching would create a parent/child cycle.
            WouldCreateCycle {
                /// Parent node involved in the cycle.
                parent: crate::core::id::NodeId,
                /// Child node involved in the cycle.
                child: crate::core::id::NodeId,
            },
            /// Invalid structural operation.
            InvalidOperation(String),
            /// Structural mutation attempted while a failed edit is unwinding.
            TreeEditDuringRollback {
                /// Requested tree operation.
                operation: &'static str,
            },
            /// Command dispatch failure.
            Command(crate::commands::CommandError),
            /// Parsing failure.
            Parse(ParseError),
            /// Script execution failure.
            Script(String),
            /// Script execution failure with stable host category fields.
            ScriptStructured {
                /// Stable script-visible category.
                kind: ScriptErrorKind,
                /// Command id when the error came from command dispatch.
                command: Option<String>,
                /// Owner name when the error came from node-target resolution.
                owner: Option<String>,
                /// Human-readable error message.
                message: String,
            },
            /// Script execution exceeded its cooperative timeout.
            ScriptTimeout {
                /// Requested timeout in milliseconds.
                timeout_ms: u64,
            },
            /// Node not found in the arena.
            NodeNotFound(crate::core::id::NodeId),
            /// Node exists but is not attached to the root tree.
            NodeDetached(crate::core::id::NodeId),
        }

        impl From<Error> for Error {
            fn from(source: geom::Error) -> Self {}
        }

        impl From<LayoutValidationError> for Error {
            fn from(source: LayoutValidationError) -> Self {}
        }

        impl From<CommandError> for Error {
            fn from(source: CommandError) -> Self {}
        }

        impl From<RecvError> for Error {
            fn from(e: mpsc::RecvError) -> Self {}
        }

        impl From<&Error> for CanopyErrorPayload {
            fn from(err: &error::Error) -> Self {}
        }
    }

    pub mod event {
        //! Input event types.

        pub mod key {
            //! Keyboard event types.
            //! This module contains the core primitives to represent keyboard input.

            /// Modifier key state.
            #[derive(Default, Debug, StructuralPartialEq, PartialEq, Eq, Clone, Copy, Hash)]
            pub struct Mods {
                /// Shift is active.
                pub shift: bool,
                /// Control is active.
                pub ctrl: bool,
                /// Alt is active.
                pub alt: bool,
            }

            impl Add<KeyCode> for Mods {
                type Output = Key;
                fn add(self, key: KeyCode) -> Self::Output {}
            }

            impl Add<char> for Mods {
                type Output = Key;
                fn add(self, other: char) -> Self::Output {}
            }

            impl Add for Mods {
                type Output = Mods;
                fn add(self, other: Self) -> Self::Output {}
            }

            /// No modifiers pressed.
            pub const Empty: Mods = _;

            /// Shift-only modifier state.
            pub const Shift: Mods = _;

            /// Control-only modifier state.
            pub const Ctrl: Mods = _;

            /// Alt-only modifier state.
            pub const Alt: Mods = _;

            /// Physical modifier key codes.
            #[derive(Debug, PartialOrd, StructuralPartialEq, PartialEq, Hash, Eq, Clone, Copy)]
            pub enum ModifierKeyCode {
                /// Left Shift key.
                LeftShift,
                /// Left Control key.
                LeftControl,
                /// Left Alt key.
                LeftAlt,
                /// Left Super key.
                LeftSuper,
                /// Left Hyper key.
                LeftHyper,
                /// Left Meta key.
                LeftMeta,
                /// Right Shift key.
                RightShift,
                /// Right Control key.
                RightControl,
                /// Right Alt key.
                RightAlt,
                /// Right Super key.
                RightSuper,
                /// Right Hyper key.
                RightHyper,
                /// Right Meta key.
                RightMeta,
                /// Iso Level3 Shift key.
                IsoLevel3Shift,
                /// Iso Level5 Shift key.
                IsoLevel5Shift,
            }

            /// Media key codes.
            #[derive(Debug, PartialOrd, StructuralPartialEq, PartialEq, Hash, Eq, Clone, Copy)]
            pub enum MediaKeyCode {
                /// Play media key.
                Play,
                /// Pause media key.
                Pause,
                /// Play/Pause media key.
                PlayPause,
                /// Reverse media key.
                Reverse,
                /// Stop media key.
                Stop,
                /// Fast-forward media key.
                FastForward,
                /// Rewind media key.
                Rewind,
                /// Next-track media key.
                TrackNext,
                /// Previous-track media key.
                TrackPrevious,
                /// Record media key.
                Record,
                /// Lower-volume media key.
                LowerVolume,
                /// Raise-volume media key.
                RaiseVolume,
                /// Mute media key.
                MuteVolume,
            }

            /// Logical key codes.
            #[derive(
                Debug,
                PartialOrd,
                StructuralPartialEq,
                PartialEq,
                Hash,
                Eq,
                Clone,
                Copy,
                PartialEq,
                Display,
            )]
            pub enum KeyCode {
                /// Backspace key.
                Backspace,
                /// Enter/return key.
                Enter,
                /// Left arrow key.
                Left,
                /// Right arrow key.
                Right,
                /// Up arrow key.
                Up,
                /// Down arrow key.
                Down,
                /// Home key.
                Home,
                /// End key.
                End,
                /// Page up key.
                PageUp,
                /// Page down key.
                PageDown,
                /// Tab key.
                Tab,
                /// Shift + Tab key.
                BackTab,
                /// Delete key.
                Delete,
                /// Insert key.
                Insert,
                /// Null key code.
                Null,
                /// Escape key.
                Esc,
                /// Caps lock key.
                CapsLock,
                /// Scroll lock key.
                ScrollLock,
                /// Num lock key.
                NumLock,
                /// Print screen key.
                PrintScreen,
                /// Pause key.
                Pause,
                /// Menu key.
                Menu,
                /// Keypad "begin" key.
                KeypadBegin,
                /// F key.
                ///
                /// `KeyEvent::F(1)` represents F1 key, etc.
                F(u8),
                /// A character.
                ///
                /// `KeyEvent::Char('c')` represents `c` character, etc.
                Char(char),
                /// Media key code.
                Media(MediaKeyCode),
                /// Modifier key code.
                Modifier(ModifierKeyCode),
            }

            impl Add<KeyCode> for Mods {
                type Output = Key;
                fn add(self, key: KeyCode) -> Self::Output {}
            }

            impl From<char> for KeyCode {
                fn from(c: char) -> Self {}
            }

            impl From<KeyCode> for Key {
                fn from(c: KeyCode) -> Self {}
            }

            /// A keystroke along with modifiers.
            /// A keystroke along with modifiers.
            #[derive(
                Debug,
                StructuralPartialEq,
                PartialEq,
                Eq,
                Clone,
                Copy,
                Hash,
                PartialEq,
                PartialEq,
                PartialEq,
                Display,
            )]
            pub struct Key {
                /// Modifier state.
                pub mods: Mods,
                /// Key code.
                pub key: KeyCode,
            }

            impl Key {
                /// Normalize key inputs for binding and matching.
                ///
                /// Normalization handles two common sources of divergence across terminals:
                ///
                /// - **Ctrl-modified ASCII control codes** (0x00–0x1F and 0x7F) are mapped to
                ///   canonical printable equivalents (e.g. 0x01 → `A`, 0x1B → `[`, 0x7F → `?`).
                ///   Some terminals emit control codes without setting the Ctrl modifier, so
                ///   these codes are treated as Ctrl-combinations even if Ctrl isn't reported.
                ///   We also map Ctrl+`_`, Ctrl+`?`, and Ctrl+`7` to `/` to align with common
                ///   `Ctrl+/` help bindings across keyboard layouts and terminal encodings.
                /// - **Shift handling** is applied after Ctrl canonicalization.
                ///
                /// Handling of the shift key is the most intricate part of this module.
                /// When we receive an event, it includes the shift modifier and also the
                /// modified character - e.g. "shift + A" or "shift + (". However, when
                /// users bind keys, it's more intuitive to bind just "A" or "(". We don't
                /// know what the keyboard mapping or input method is for the user - so it's
                /// not possible in a general way for us to map between, say, an input like
                /// "shift + 0" to the shifted key "(". Conversely, if we see an input of
                /// "shift + (", we don't know if the user pressed "shift + 0" or if they
                /// have a weird keyboard layout that actually permits "shift + (" without a
                /// shift conversion.
                ///
                /// To handle this, we have to make a lossy compromise. We define a
                /// normalisation applied to input for the purpose of key binding matching
                /// as follows:
                ///
                /// - If shift is present:
                ///     - If the key is ascii lowercase, convert it to uppercase and remove
                ///       shift
                ///     - If the key is one of a special class of characters that commonly
                ///       don't have a shift conversion (space, enter), leave shift intact
                ///     - in all other cases, just remove shift
                ///
                /// | input             | normalization    |
                /// |-------------------|------------------|
                /// | shift + A         | A                |
                /// | shift + a         | A                |
                /// | shift + )         | )                |
                /// | shift + enter     | shift + enter    |
                /// | shift + ctrl + A  | ctrl + A         |
                ///
                /// `normalize` must be called explicitly when needed - all comparison and
                /// conversion methods are literal and stright-forward, and don't perform
                /// normalization automatically.
                pub fn normalize(&self) -> Self {}

                /// Parse a key specification such as `ctrl-s`, `PageDown`, or `A`.
                pub fn parse_spec(spec: &str) -> Result<Self, String> {}
            }

            impl From<char> for Key {
                fn from(c: char) -> Self {}
            }

            impl From<KeyCode> for Key {
                fn from(c: KeyCode) -> Self {}
            }
        }

        pub mod mouse {
            //! Mouse event types.

            /// An abstract specification for a mouse action.
            #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq)]
            pub struct Mouse {
                /// Mouse action type.
                pub action: Action,
                /// Mouse button.
                pub button: Button,
                /// Keyboard modifiers.
                pub modifiers: key::Mods,
            }

            impl Mouse {
                /// Parse a mouse specification such as `ScrollUp` or `ctrl-LeftDown`.
                pub fn parse_spec(spec: &str) -> Result<Self, String> {}
            }

            impl From<MouseEvent> for Mouse {
                fn from(o: MouseEvent) -> Self {}
            }

            /// Mouse button codes.
            #[derive(Debug, PartialOrd, StructuralPartialEq, PartialEq, Eq, Clone, Copy, Hash)]
            pub enum Button {
                /// Left mouse button.
                Left,
                /// Right mouse button.
                Right,
                /// Middle mouse button.
                Middle,
                /// No button (for move/scroll).
                None,
            }

            /// Mouse action kinds.
            #[derive(Debug, PartialOrd, StructuralPartialEq, PartialEq, Eq, Clone, Copy, Hash)]
            pub enum Action {
                /// Button press.
                Down,
                /// Button release.
                Up,
                /// Mouse drag with button held.
                Drag,
                /// Mouse moved without button.
                Moved,
                /// Scroll wheel down.
                ScrollDown,
                /// Scroll wheel up.
                ScrollUp,
                /// Horizontal scroll left.
                ScrollLeft,
                /// Horizontal scroll right.
                ScrollRight,
            }

            impl Action {
                /// Is this a button-driven action?
                pub fn is_button(&self) -> bool {}
            }

            /// A mouse input event. This has the same fields as the `Mouse` event
            /// specification, but also includes a location.
            #[derive(Debug, Clone, Copy)]
            pub struct MouseEvent {
                /// Mouse action type.
                pub action: Action,
                /// Mouse button.
                pub button: Button,
                /// Keyboard modifiers.
                pub modifiers: key::Mods,
                /// Cursor location in local coordinates relative to the node view. To map
                /// back to screen coordinates, add the node view's outer top-left.
                pub location: crate::geom::Point,
            }

            impl Inject for crate::event::mouse::MouseEvent {
                fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {}
            }

            impl From<MouseEvent> for Mouse {
                fn from(o: MouseEvent) -> Self {}
            }
        }

        /// This enum represents all the event types that drive the application.
        #[derive(Debug, Clone)]
        pub enum Event {
            /// A keystroke
            Key(key::Key),
            /// A mouse action
            Mouse(mouse::MouseEvent),
            /// Terminal resize
            Resize(crate::geom::Size),
            /// A poll event
            Poll(Vec<crate::NodeId>),
            /// Terminal has gained focus
            FocusGained,
            /// Terminal has lost focus
            FocusLost,
            /// Cut and paste
            Paste(String),
            /// Internal wake event used to service queued automation work.
            Wake,
        }

        impl Inject for crate::event::Event {
            fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {}
        }
    }

    pub mod help {
        //! Help snapshot API.
        //! Help snapshot API for context-aware help.
        //!
        //! This module provides types and functions to generate a snapshot of available bindings and
        //! commands from a given focus context. The snapshot can be used to build help overlays,
        //! command palettes, or discoverable keybinding references.

        /// Classification of how a binding matched the focus path.
        #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
        pub enum BindingKind {
            /// Binding matched exactly at the focus path (pre-event override).
            PreEventOverride,
            /// Binding matched as a fallback after event bubbling (post-event fallback).
            PostEventFallback,
        }

        /// A binding in the help snapshot.
        #[derive(Debug, Clone)]
        pub struct HelpBinding<'a> {
            /// Identifier of the matched binding.
            pub id: crate::core::inputmap::BindingId,
            /// The input (key or mouse) that triggers this binding.
            pub input: crate::core::inputmap::InputSpec,
            /// The mode this binding belongs to.
            pub mode: &'a str,
            /// The original path filter string.
            pub path_filter: &'a str,
            /// The binding target (script or command).
            pub target: &'a crate::core::inputmap::BindingTarget,
            /// Classification of how this binding matched.
            pub kind: BindingKind,
            /// Human-readable label derived from command docs or script source.
            pub label: String,
        }

        /// A command in the help snapshot.
        #[derive(Debug, Clone)]
        pub struct HelpCommand<'a> {
            /// Owner type name (`None` for Free commands).
            pub owner: Option<&'static str>,
            /// Command specification.
            pub spec: &'a crate::commands::CommandSpec,
            /// Resolution if the command has a target, or `None` if no target exists.
            pub resolution: Option<crate::commands::CommandResolution>,
        }

        impl<'a> HelpCommand<'a> {
            /// Returns true if this command can be dispatched from the current context.
            pub fn is_available(&self) -> bool {}
        }

        /// A contextual help snapshot combining bindings and commands.
        #[derive(Debug)]
        pub struct HelpSnapshot<'a> {
            /// Current focus node ID.
            pub focus: crate::core::NodeId,
            /// Path from root to focus.
            pub focus_path: crate::path::Path,
            /// Current input mode name.
            pub input_mode: &'a str,
            /// Bindings that match the current context.
            pub bindings: Vec<HelpBinding<'a>>,
            /// Commands with their availability status.
            pub commands: Vec<HelpCommand<'a>>,
        }

        impl<'a> HelpSnapshot<'a> {
            /// Return only bindings that would fire as pre-event overrides.
            pub fn pre_event_bindings(&self) -> Vec<&HelpBinding<'a>> {}

            /// Return only bindings that would fire as post-event fallbacks.
            pub fn fallback_bindings(&self) -> Vec<&HelpBinding<'a>> {}

            /// Return only commands that are currently available (have a target).
            pub fn available_commands(&self) -> Vec<&HelpCommand<'a>> {}

            /// Return only commands that are currently unavailable (no target).
            pub fn unavailable_commands(&self) -> Vec<&HelpCommand<'a>> {}

            /// Convert to an owned version for storage.
            pub fn to_owned(&self) -> OwnedHelpSnapshot {}
        }

        /// Derive a human-readable label for a binding target.
        ///
        /// For scripts that are simple command calls (e.g., `root::focus_next()`), looks up
        /// the command's documentation. For compound scripts, falls back to the source.
        pub fn binding_label<F, G>(
            target: &crate::core::inputmap::BindingTarget,
            commands: &crate::commands::CommandSet,
            script_source: F,
            luau_label: G,
        ) -> String
        where
            F: Fn(crate::script::ScriptId) -> Option<String>,
            G: Fn(crate::script::LuauFunctionId) -> Option<String>, {
        }

        /// Owned version of [`HelpBinding`] for storage without lifetimes.
        #[derive(Debug, Clone)]
        pub struct OwnedHelpBinding {
            /// The input (key or mouse) that triggers this binding.
            pub input: crate::core::inputmap::InputSpec,
            /// The mode this binding belongs to.
            pub mode: String,
            /// The original path filter string.
            pub path_filter: String,
            /// Classification of how this binding matched.
            pub kind: BindingKind,
            /// Human-readable label derived from command docs or script source.
            pub label: String,
            /// Match metadata for sorting.
            pub path_match: crate::path::PathMatch,
        }

        /// Owned version of [`HelpCommand`] for storage without lifetimes.
        #[derive(Debug, Clone)]
        pub struct OwnedHelpCommand {
            /// Command identifier.
            pub id: String,
            /// Owner type name (None for Free commands).
            pub owner: Option<String>,
            /// Short description.
            pub short: Option<String>,
            /// Resolution if the command has a target.
            pub resolution: Option<crate::commands::CommandResolution>,
            /// Whether this command is hidden from help.
            pub hidden: bool,
        }

        impl OwnedHelpCommand {
            /// Returns true if this command can be dispatched from the current context.
            pub fn is_available(&self) -> bool {}
        }

        /// Owned version of [`HelpSnapshot`] for storage without lifetimes.
        #[derive(Debug, Clone)]
        pub struct OwnedHelpSnapshot {
            /// Path from root to focus.
            pub focus_path: crate::path::Path,
            /// Current input mode name.
            pub input_mode: String,
            /// Bindings that match the current context.
            pub bindings: Vec<OwnedHelpBinding>,
            /// Commands with their availability status.
            pub commands: Vec<OwnedHelpCommand>,
        }
    }

    pub mod path {
        //! Path and traversal helpers.

        /// A path of node name components.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, FromStr, Display)]
        pub struct Path {}

        impl Path {
            /// Construct an empty path.
            pub fn empty() -> Self {}

            /// Parse and validate a path from a slash-separated string.
            pub fn parse(path: &str) -> Result<Self> {}

            /// Pop an item off the end of the path, modifying it in place. Return None
            /// if the path is empty.
            pub fn pop(&mut self) -> Option<String> {}

            /// Construct a path from a slice of components.
            pub fn new<I>(v: I) -> Self
            where
                I: IntoIterator,
                I::Item: AsRef<str>, {
            }
        }

        impl From<Vec<String>> for Path {
            fn from(path: Vec<String>) -> Self {}
        }

        impl From<&[&str]> for Path {
            fn from(v: &[&str]) -> Self {}
        }

        impl From<&str> for Path {
            fn from(v: &str) -> Self {}
        }

        /// A validated path filter used to search node paths.
        ///
        /// Filters support `*` for one component and `**` for zero or more components.
        /// Literal components must be valid [`NodeName`] values.
        #[derive(Debug, Clone, FromStr)]
        pub struct PathFilter {}

        impl PathFilter {
            /// Compile a validated path filter.
            pub fn new(filter: &str) -> Result<Self> {}

            /// Compile a filter after normalizing it to a full-path match.
            pub fn normalized(filter: &str) -> Result<Self> {}

            /// Return the original filter string.
            pub fn as_str(&self) -> &str {}
        }

        /// A match expression that can be applied to paths.
        /// The matcher supports `*` (one component), `**` (zero or more), and optional anchors.
        #[derive(Debug, Clone)]
        pub struct PathMatcher {}

        impl PathMatcher {
            /// Compile a path matcher from a filter string.
            pub fn new(path: &str) -> Result<Self> {}

            /// Return the original filter string used to construct this matcher.
            pub fn filter(&self) -> &str {}

            /// Check whether the path filter matches a given path.
            /// Returns the matched depth for use in quick checks.
            pub fn check(&self, path: &Path) -> Option<usize> {}

            /// Check whether the path filter matches a given path, returning match metadata.
            pub fn check_match(&self, path: &Path) -> Option<PathMatch> {}
        }

        /// Path match metadata used for input precedence.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub struct PathMatch {
            /// Count of literal segments in the pattern.
            pub literals: usize,
            /// Number of path components matched.
            pub depth: usize,
            /// Whether the match ends at the end of the path.
            pub anchored_end: bool,
        }
    }

    pub mod render {
        //! Rendering interfaces.

        /// The trait implemented by renderers.
        pub trait RenderBackend {
            /// Apply a style to the following text output
            fn style(&mut self, style: &ResolvedStyle) -> Result<()>;

            /// Output text to screen. This method is used for all text output.
            fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()>;

            /// Return true if the backend can shift characters within a line.
            fn supports_char_shift(&self) -> bool {}

            /// Shift characters within a line starting at the location.
            /// Positive counts insert blanks and shift right, negative counts delete and shift left.
            fn shift_chars(&mut self, _loc: geom::Point, _count: i32) -> Result<()> {}

            /// Return true if the backend can shift lines within a region.
            fn supports_line_shift(&self) -> bool {}

            /// Shift lines within the inclusive (top..=bottom) region.
            /// Positive counts shift content down, negative counts shift content up.
            fn shift_lines(&mut self, _top: u32, _bottom: u32, _count: i32) -> Result<()> {}

            /// Flush output to the terminal.
            fn flush(&mut self) -> Result<()>;

            /// Reset the backend to a clean state.
            fn reset(&mut self) -> Result<()> {}
        }

        /// A render backend that discards all output.
        ///
        /// Rendering through this backend refreshes the terminal buffer without
        /// producing user-visible output, so callers can inspect the buffer directly.
        #[derive(Default)]
        pub struct NopBackend;

        impl NopBackend {
            /// Construct a no-op backend.
            pub fn new() -> Self {}
        }

        impl RenderBackend for NopBackend {
            fn style(&mut self, _style: &ResolvedStyle) -> Result<()> {}

            fn text(&mut self, _loc: geom::Point, _txt: &str) -> Result<()> {}

            fn flush(&mut self) -> Result<()> {}
        }

        /// A renderer that only renders to a specific rectangle within the target terminal buffer.
        pub struct Render<'a> {}

        impl<'a> Render<'a> {
            /// Construct a renderer that writes into `buf`.
            ///
            /// `clip` is the visible rectangle in canvas coordinates, and `screen_origin` is where the
            /// clip's top-left lands in the buffer.
            pub fn new(
                stylemap: &'a StyleMap,
                style: &'a mut StyleManager,
                buf: &'a mut TermBuf,
                clip: geom::Rect,
                screen_origin: geom::Point,
            ) -> Self {
            }

            /// Set the effect stack for this renderer.
            pub fn with_effects(self, effects: &'a [Effect]) -> Self {}

            /// Apply the current effect stack to a style.
            /// Use this when you have a Style from a source other than the style manager.
            pub fn apply_effects(&self, style: Style) -> Style {}

            /// Resolve a style by name without applying effects.
            pub fn resolve_style_name_raw(&self, name: &str) -> Style {}

            /// Resolve a custom style at a point, applying the current effect stack.
            pub fn resolve_style_at(
                &self,
                style: Style,
                bounds: geom::Rect,
                point: geom::Point,
            ) -> ResolvedStyle {
            }

            /// Resolve a style by name at a point within bounds.
            pub fn resolve_style_name_at(
                &self,
                name: &str,
                bounds: geom::Rect,
                point: geom::Point,
            ) -> ResolvedStyle {
            }

            /// Push a style layer.
            pub fn push_layer(&mut self, name: &str) {}

            /// Fill a rectangle with a specified character. Writes out of bounds will be clipped.
            pub fn fill(&mut self, style: &str, r: geom::Rect, c: char) -> Result<()> {}

            /// Print text in the specified line. If the text is wider than the
            /// rectangle, it will be truncated; if it is shorter, it will be padded.
            pub fn text(&mut self, style: &str, l: geom::Line, txt: &str) -> Result<()> {}

            /// Write a single cell with a resolved style.
            pub fn put_cell(
                &mut self,
                style: ResolvedStyle,
                p: geom::Point,
                ch: char,
            ) -> Result<()> {
            }

            /// Write a grapheme with a resolved style, including continuation cells.
            pub fn put_grapheme(
                &mut self,
                style: ResolvedStyle,
                p: geom::Point,
                grapheme: &str,
            ) -> Result<()> {
            }
        }
    }

    pub mod script {
        //! Scripting support.

        pub mod defs {
            //! Render Luau definition files from the current command set.

            /// Render the complete Luau definition file for the current command set.
            pub fn render_definitions(
                commands: &crate::commands::CommandSet,
                default_binding_owners: &std::collections::BTreeSet<String>,
                fixtures: &[crate::FixtureInfo],
            ) -> String {
            }

            /// Return the Luau type recorded in command metadata.
            pub fn command_type_to_luau(spec: &crate::commands::CommandTypeSpec) -> String {}
        }

        /// Filesystem roots used by Canopy's persistent Luau module source.
        #[derive(Clone, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
        pub struct ScriptModuleRoots {}

        impl ScriptModuleRoots {
            /// Construct an empty root set.
            pub fn new() -> Self {}

            /// Return the configured `@user` root.
            pub fn user_root(&self) -> Option<&Path> {}

            /// Return the configured `@project` root.
            pub fn project_root(&self) -> Option<&Path> {}

            /// Mount `@user` at `root`.
            pub fn set_user_root(&mut self, root: impl Into<PathBuf>) {}

            /// Mount `@project` at `root`.
            pub fn set_project_root(&mut self, root: impl Into<PathBuf>) {}

            /// Locate the nearest `.canopy` directory at or above `start`.
            pub fn discover_project_root(start: impl AsRef<Path>) -> Option<PathBuf> {}
        }

        /// Script identifier.
        pub type ScriptId = u64;

        /// Stable handle for a stored Luau closure.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Hash)]
        pub struct LuauFunctionId(_);

        /// Recorded assertion outcome for a script evaluation.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
        pub struct ScriptAssertion {
            /// Whether the assertion passed.
            pub passed: bool,
            /// Assertion message or fallback description.
            pub message: String,
        }

        /// Structured Luau typecheck diagnostic.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Display)]
        pub struct ScriptCheckDiagnostic {
            /// Diagnostic source name, when the diagnostic belongs to a named source.
            pub source: Option<String>,
            /// Diagnostic severity such as `error` or `warning`.
            pub severity: String,
            /// One-based line number, or zero when the diagnostic is not source-bound.
            pub line: usize,
            /// One-based column number, or zero when the diagnostic is not source-bound.
            pub column: usize,
            /// Human-readable diagnostic message.
            pub message: String,
        }

        impl ScriptCheckDiagnostic {
            /// Construct an error diagnostic at a source location.
            pub fn error(line: usize, column: usize, message: impl Into<String>) -> Self {}

            /// Return true if this diagnostic should fail script evaluation.
            pub fn is_error(&self) -> bool {}
        }

        /// Stable result returned by Luau typechecking APIs.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq)]
        pub struct ScriptCheckResult {}

        impl ScriptCheckResult {
            /// Construct a successful typecheck result.
            pub fn ok() -> Self {}

            /// Return true if there are no failing diagnostics.
            pub fn is_ok(&self) -> bool {}

            /// Return all diagnostics.
            pub fn diagnostics(&self) -> &[ScriptCheckDiagnostic] {}

            /// Return true when the result contains failing diagnostics.
            pub fn has_errors(&self) -> bool {}

            /// Return failing diagnostics.
            pub fn errors(&self) -> impl Iterator<Item = &ScriptCheckDiagnostic> {}
        }
    }

    pub mod state {
        //! Shared node name types.

        /// Return true if the character is valid in a node name.
        pub fn valid_nodename_char(c: char) -> bool {}

        /// Return true if the full name is valid.
        pub fn valid_nodename(name: &str) -> bool {}

        /// A node name, which consists of lowercase ASCII alphanumeric characters, plus
        /// underscores.
        #[derive(
            Debug,
            Clone,
            StructuralPartialEq,
            PartialEq,
            Eq,
            Hash,
            FromStr,
            Display,
            PartialEq,
            PartialEq,
        )]
        pub struct NodeName {}

        impl NodeName {
            /// Create a new NodeName, returning an error if the string contains invalid
            /// characters.
            pub fn new(name: &str) -> Result<Self> {}

            /// Takes a string and munges it into a valid node name. It does this by
            /// first converting the string to snake case, then removing all invalid
            /// characters.
            pub fn convert(name: &str) -> Self {}
        }

        /// Converts a string into the standard node name format, and errors if it
        /// doesn't comply to the node name standard.
        impl TryFrom<&str> for NodeName {
            type Error = Error;
            fn try_from(name: &str) -> Result<Self> {}
        }
    }

    pub mod style {
        //! Styling and color helpers.

        pub mod dracula {
            //! Dracula theme.
            //! Dracula theme - a dark theme with vibrant colors.
            //!
            //! Based on the Dracula theme: <https://draculatheme.com>

            /// Background.
            pub const BACKGROUND: super::Color = _;

            /// Current line / selection background.
            pub const CURRENT_LINE: super::Color = _;

            /// Selection.
            pub const SELECTION: super::Color = _;

            /// Foreground.
            pub const FOREGROUND: super::Color = _;

            /// Comment color (also used for subtle elements).
            pub const COMMENT: super::Color = _;

            /// Red.
            pub const RED: super::Color = _;

            /// Orange.
            pub const ORANGE: super::Color = _;

            /// Yellow.
            pub const YELLOW: super::Color = _;

            /// Green.
            pub const GREEN: super::Color = _;

            /// Cyan.
            pub const CYAN: super::Color = _;

            /// Purple.
            pub const PURPLE: super::Color = _;

            /// Pink.
            pub const PINK: super::Color = _;

            /// ANSI black.
            pub const ANSI_BLACK: super::Color = _;

            /// Build a Dracula style map.
            pub fn dracula() -> super::StyleMap {}
        }

        pub mod effects {
            //! Style effects system.
            //! Style effects system for transforming styles during rendering.
            //!
            //! Effects are transformations applied to styles that inherit through the node tree.
            //! They can modify colors, attributes, or both.

            /// A style transformation that can be applied during rendering.
            ///
            /// Effects are stacked and applied in order during render traversal.
            /// They inherit through the tree unless explicitly cleared.
            pub trait StyleEffect: Send + Sync + Debug {
                /// Apply this effect to a style, returning the transformed style.
                fn apply(&self, style: Style) -> Style;
            }

            /// Shared handle for effects stored on nodes and stacked during rendering.
            pub type Effect = std::sync::Arc<dyn StyleEffect>;

            /// A built-in effect that maps colors.
            #[derive(Debug, Clone, Copy)]
            pub enum ColorEffect {
                /// Scale brightness by a factor.
                ScaleBrightness(f32),
                /// Adjust saturation.
                Saturation(f32),
                /// Invert RGB channels.
                Invert,
                /// Shift hue by degrees.
                HueShift(f32),
            }

            impl StyleEffect for ColorEffect {
                fn apply(&self, style: Style) -> Style {}
            }

            /// Create a brightness effect. Factor below 1.0 dims, above 1.0 brightens.
            pub fn brightness(factor: f32) -> Effect {}

            /// Create a saturation effect. 0.0 = grayscale, 1.0 = unchanged.
            pub fn saturation(factor: f32) -> Effect {}

            /// Create an effect that inverts RGB channels (255-value).
            pub fn invert_rgb() -> Effect {}

            /// Create a hue shift effect.
            pub fn hue_shift(degrees: f32) -> Effect {}

            /// Add a single attribute.
            #[derive(Debug, Clone, Copy)]
            pub struct AddAttr(pub super::Attr);

            impl StyleEffect for AddAttr {
                fn apply(&self, style: Style) -> Style {}
            }

            /// Create an effect that adds bold attribute.
            pub fn bold() -> Effect {}

            /// Create an effect that adds italic attribute.
            pub fn italic() -> Effect {}
        }

        pub mod gruvbox {
            //! Gruvbox theme.
            //! Gruvbox theme - a retro groove color scheme.
            //!
            //! Based on the gruvbox theme by morhetz: <https://github.com/morhetz/gruvbox>

            /// Dark background (hard contrast).
            pub const DARK0_HARD: super::Color = _;

            /// Dark background (default).
            pub const DARK0: super::Color = _;

            /// Dark background (soft contrast).
            pub const DARK0_SOFT: super::Color = _;

            /// Dark background 1.
            pub const DARK1: super::Color = _;

            /// Dark background 2.
            pub const DARK2: super::Color = _;

            /// Dark background 3.
            pub const DARK3: super::Color = _;

            /// Dark background 4.
            pub const DARK4: super::Color = _;

            /// Light foreground 0.
            pub const LIGHT0: super::Color = _;

            /// Light foreground 1.
            pub const LIGHT1: super::Color = _;

            /// Light foreground 2.
            pub const LIGHT2: super::Color = _;

            /// Light foreground 3.
            pub const LIGHT3: super::Color = _;

            /// Light foreground 4.
            pub const LIGHT4: super::Color = _;

            /// Gray.
            pub const GRAY: super::Color = _;

            /// Bright red.
            pub const RED: super::Color = _;

            /// Bright green.
            pub const GREEN: super::Color = _;

            /// Bright yellow.
            pub const YELLOW: super::Color = _;

            /// Bright blue.
            pub const BLUE: super::Color = _;

            /// Bright purple.
            pub const PURPLE: super::Color = _;

            /// Bright aqua/cyan.
            pub const AQUA: super::Color = _;

            /// Bright orange.
            pub const ORANGE: super::Color = _;

            /// Build a dark gruvbox style map.
            pub fn gruvbox_dark() -> super::StyleMap {}
        }

        pub mod solarized {
            //! Solarized theme.

            /// Solarized base03.
            pub const BASE03: super::Color = _;

            /// Solarized base02.
            pub const BASE02: super::Color = _;

            /// Solarized base01.
            pub const BASE01: super::Color = _;

            /// Solarized base00.
            pub const BASE00: super::Color = _;

            /// Solarized base0.
            pub const BASE0: super::Color = _;

            /// Solarized base1.
            pub const BASE1: super::Color = _;

            /// Solarized base2.
            pub const BASE2: super::Color = _;

            /// Solarized base3.
            pub const BASE3: super::Color = _;

            /// Solarized yellow.
            pub const YELLOW: super::Color = _;

            /// Solarized orange.
            pub const ORANGE: super::Color = _;

            /// Solarized red.
            pub const RED: super::Color = _;

            /// Solarized magenta.
            pub const MAGENTA: super::Color = _;

            /// Solarized violet.
            pub const VIOLET: super::Color = _;

            /// Solarized blue.
            pub const BLUE: super::Color = _;

            /// Solarized cyan.
            pub const CYAN: super::Color = _;

            /// Solarized green.
            pub const GREEN: super::Color = _;

            /// Black.
            pub const BLACK: super::Color = _;

            /// Build a dark solarized style map.
            pub fn solarized_dark() -> super::StyleMap {}

            /// Build a light solarized style map.
            pub fn solarized_light() -> super::StyleMap {}
        }

        /// A terminal color value.
        #[derive(Copy, Clone, Debug, StructuralPartialEq, PartialEq, Eq, Ord, PartialOrd, Hash)]
        pub enum Color {
            /// Black.
            Black,
            /// Dark grey.
            DarkGrey,
            /// Red.
            Red,
            /// Dark red.
            DarkRed,
            /// Green.
            Green,
            /// Dark green.
            DarkGreen,
            /// Yellow.
            Yellow,
            /// Dark yellow.
            DarkYellow,
            /// Blue.
            Blue,
            /// Dark blue.
            DarkBlue,
            /// Magenta.
            Magenta,
            /// Dark magenta.
            DarkMagenta,
            /// Cyan.
            Cyan,
            /// Dark cyan.
            DarkCyan,
            /// White.
            White,
            /// Grey.
            Grey,
            /// RGB color.
            Rgb {
                /// Red channel.
                r: u8,
                /// Green channel.
                g: u8,
                /// Blue channel.
                b: u8,
            },
            /// An ANSI color. See [256 colors - cheat
            /// sheet](https://jonasjacek.github.io/colors/) for more info.
            AnsiValue(u8),
        }

        impl Color {
            /// Return this color's RGB channels.
            ///
            /// Named colors and ANSI-256 values use the standard palette mappings.
            pub fn rgb(self) -> (u8, u8, u8) {}

            /// Scale brightness by a factor. 0.0 = black, 1.0 = unchanged, 2.0 = double brightness.
            pub fn scale_brightness(self, factor: f32) -> Self {}

            /// Adjust saturation. 0.0 = grayscale, 1.0 = unchanged, 2.0 = double saturation.
            pub fn saturation(self, factor: f32) -> Self {}

            /// Blend this color with another. ratio 0.0 = self, 1.0 = other.
            pub fn blend(self, other: Self, ratio: f32) -> Self {}

            /// Invert RGB channels (255 - value for each channel).
            pub fn invert_rgb(self) -> Self {}

            /// Shift hue by degrees (0-360).
            pub fn shift_hue(self, degrees: f32) -> Self {}
        }

        impl From<Color> for Paint {
            fn from(color: Color) -> Self {}
        }

        /// Shared handle for effects stored on nodes and stacked during rendering.
        pub type Effect = std::sync::Arc<dyn StyleEffect>;

        /// A style transformation that can be applied during rendering.
        ///
        /// Effects are stacked and applied in order during render traversal.
        /// They inherit through the tree unless explicitly cleared.
        pub trait StyleEffect: Send + Sync + Debug {
            /// Apply this effect to a style, returning the transformed style.
            fn apply(&self, style: Style) -> Style;
        }

        /// The role colours a theme assigns.
        ///
        /// Each field names the role a colour plays, not the colour itself, so the same rule set can
        /// render a light theme, a dark theme, or any other palette.
        #[derive(Debug, Clone, Copy)]
        pub struct Palette {
            /// Default foreground.
            pub fg: super::Color,
            /// Default background, and the foreground drawn on top of `accent`.
            pub bg: super::Color,
            /// Inactive frame and tab borders, and the tab bar itself.
            pub frame: super::Color,
            /// Border of the frame that owns the active subtree.
            pub frame_active: super::Color,
            /// Frame title text.
            pub frame_title: super::Color,
            /// Primary accent: focus, selection, and the active tab.
            pub accent: super::Color,
            /// Foreground on panel backgrounds, one step away from `fg`.
            pub muted_fg: super::Color,
            /// Background of panels such as the help overlay, prompt, and inactive tabs.
            pub panel_bg: super::Color,
            /// Foreground of the active tab, drawn on `accent`.
            pub tab_active_fg: super::Color,
            /// Editor selection background.
            pub selection_bg: super::Color,
            /// Editor line-number gutter.
            pub line_number: super::Color,
            /// Named blue.
            pub blue: super::Color,
            /// Named red.
            pub red: super::Color,
            /// Named magenta.
            pub magenta: super::Color,
            /// Named violet.
            pub violet: super::Color,
            /// Named cyan, also the help overlay's key colour.
            pub cyan: super::Color,
            /// Named green.
            pub green: super::Color,
            /// Named yellow, also the search-match background.
            pub yellow: super::Color,
            /// Named orange, also the current-search-match background.
            pub orange: super::Color,
            /// Named black.
            pub black: super::Color,
        }

        /// Build the shared rule set for one palette.
        pub fn theme(p: &Palette) -> super::StyleMap {}

        /// A text attribute.
        #[derive(Debug, StructuralPartialEq, PartialEq, Eq, Clone, Copy)]
        pub enum Attr {
            /// Bold text.
            Bold,
            /// Crossed out text.
            CrossedOut,
            /// Dim text.
            Dim,
            /// Italic text.
            Italic,
            /// Overlined text.
            Overline,
            /// Underlined text.
            Underline,
        }

        /// A set of active text attributes.
        #[derive(Debug, StructuralPartialEq, PartialEq, Eq, Clone, Copy, Default)]
        pub struct AttrSet {
            /// Bold flag.
            pub bold: bool,
            /// Crossed out flag.
            pub crossedout: bool,
            /// Dim flag.
            pub dim: bool,
            /// Italic flag.
            pub italic: bool,
            /// Overline flag.
            pub overline: bool,
            /// Underline flag.
            pub underline: bool,
        }

        impl AttrSet {
            /// Construct a set of text attributes with a single attribute turned on.
            pub fn new(attr: Attr) -> Self {}

            /// A helper for progressive construction of attribute sets.
            pub fn with(self, attr: Attr) -> Self {}
        }

        /// A gradient stop in a paint specification.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq)]
        pub struct GradientStop {
            /// Offset along the gradient (0.0-1.0).
            pub offset: f32,
            /// Color at this stop.
            pub color: Color,
        }

        impl GradientStop {
            /// Construct a gradient stop, clamping the offset to 0.0-1.0.
            pub fn new(offset: f32, color: Color) -> Self {}
        }

        /// A gradient paint specification.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq)]
        pub struct GradientSpec {
            /// Gradient angle in degrees (0 = left to right, 90 = top to bottom).
            pub angle_deg: f32,
            /// Ordered list of gradient stops.
            pub stops: Vec<GradientStop>,
        }

        impl GradientSpec {
            /// Construct a gradient from explicit stops.
            pub fn with_stops(angle_deg: f32, stops: Vec<GradientStop>) -> Self {}

            /// Map all colors in this gradient through a transform.
            pub fn map_colors(&self, f: impl Fn(Color) -> Color) -> Self {}

            /// Resolve a gradient color at a point within a rectangle.
            pub fn color_at(&self, rect: geom::Rect, point: geom::Point) -> Color {}
        }

        impl From<GradientSpec> for Paint {
            fn from(spec: GradientSpec) -> Self {}
        }

        /// A paint definition for a style channel.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq)]
        pub enum Paint {
            /// Solid color fill.
            Solid(Color),
            /// Gradient fill.
            Gradient(GradientSpec),
        }

        impl Paint {
            /// Construct a solid paint.
            pub fn solid(color: Color) -> Self {}

            /// Construct a gradient paint.
            pub fn gradient(spec: GradientSpec) -> Self {}

            /// Return the solid color if this paint is solid.
            pub fn solid_color(&self) -> Option<Color> {}

            /// Resolve the paint at a location.
            pub fn resolve(&self, rect: geom::Rect, point: geom::Point) -> Color {}

            /// Map colors within this paint.
            pub fn map_colors(&self, f: impl Fn(Color) -> Color) -> Self {}
        }

        impl From<Color> for Paint {
            fn from(color: Color) -> Self {}
        }

        impl From<GradientSpec> for Paint {
            fn from(spec: GradientSpec) -> Self {}
        }

        /// A resolved style specification stored in terminal buffers.
        #[derive(Debug, StructuralPartialEq, PartialEq, Eq, Clone, Copy)]
        pub struct ResolvedStyle {
            /// Foreground color.
            pub fg: Color,
            /// Background color.
            pub bg: Color,
            /// Text attributes.
            pub attrs: AttrSet,
        }

        impl ResolvedStyle {
            /// Construct a resolved style from components.
            pub fn new(fg: Color, bg: Color, attrs: AttrSet) -> Self {}
        }

        /// A paint-based style specification.
        #[derive(Debug, StructuralPartialEq, PartialEq, Clone)]
        pub struct Style {
            /// Foreground paint.
            pub fg: Paint,
            /// Background paint.
            pub bg: Paint,
            /// Text attributes.
            pub attrs: AttrSet,
        }

        impl Style {
            /// Resolve the style at a location within a rectangle.
            pub fn resolve_at(&self, rect: geom::Rect, point: geom::Point) -> ResolvedStyle {}

            /// Resolve the style to a solid variant if both paints are solid.
            pub fn resolve_solid(&self) -> Option<ResolvedStyle> {}
        }

        /// A possibly partial style specification, which is stored in a StyleManager.
        /// Partial styles are completely resolved during the style resolution process.
        #[derive(Default, Debug, StructuralPartialEq, PartialEq, Clone)]
        pub struct PartialStyle {
            /// Optional foreground paint.
            pub fg: Option<Paint>,
            /// Optional background paint.
            pub bg: Option<Paint>,
            /// Optional attributes.
            pub attrs: Option<AttrSet>,
        }

        impl PartialStyle {
            /// Create a new PartialStyle with only a foreground paint.
            pub fn fg(fg: impl Into<Paint>) -> Self {}

            /// Create a new PartialStyle with only a background paint.
            pub fn bg(bg: impl Into<Paint>) -> Self {}

            /// Create a new PartialStyle with only attributes.
            pub fn attrs(attrs: AttrSet) -> Self {}

            /// Resolve the partial style into a full style.
            pub fn resolve(&self) -> Style {}

            /// Merge two partial styles.
            pub fn join(&self, other: &Self) -> Self {}

            /// Return true if all components are set.
            pub fn is_complete(&self) -> bool {}
        }

        impl From<StyleBuilder> for PartialStyle {
            fn from(s: StyleBuilder) -> Self {}
        }

        /// A builder for creating reusable style specifications.
        ///
        /// Use this to define styles that can be applied to multiple paths.
        ///
        /// # Example
        ///
        /// ```
        /// use canopy::style::{Attr, StyleBuilder, StyleMap, solarized};
        ///
        /// let selected = StyleBuilder::new()
        ///     .fg(solarized::BASE3)
        ///     .bg(solarized::BLUE)
        ///     .attr(Attr::Bold);
        ///
        /// let mut style_map = StyleMap::new();
        /// style_map
        ///     .rules()
        ///     .style("item/selected", selected)
        ///     .apply();
        /// ```
        #[derive(Clone, Default, Debug, StructuralPartialEq, PartialEq)]
        pub struct StyleBuilder {}

        impl StyleBuilder {
            /// Create a new empty style builder.
            pub fn new() -> Self {}

            /// Set the foreground paint.
            pub fn fg(self, paint: impl Into<Paint>) -> Self {}

            /// Set the background paint.
            pub fn bg(self, paint: impl Into<Paint>) -> Self {}

            /// Add a single attribute.
            pub fn attr(self, attr: Attr) -> Self {}

            /// Set all attributes.
            pub fn attrs(self, attrs: AttrSet) -> Self {}
        }

        impl From<StyleBuilder> for PartialStyle {
            fn from(s: StyleBuilder) -> Self {}
        }

        /// Map of style paths to partial styles.
        #[derive(Clone, Debug, Default)]
        pub struct StyleMap {}

        impl StyleMap {
            /// Construct a style map with defaults.
            pub fn new() -> Self {}

            /// Begin a fluent rule-building chain.
            ///
            /// # Example
            ///
            /// ```
            /// use canopy::style::{StyleMap, solarized};
            ///
            /// let mut style_map = StyleMap::new();
            /// style_map
            ///     .rules()
            ///     .fg("red/text", solarized::RED)
            ///     .fg("blue/text", solarized::BLUE)
            ///     .apply();
            /// ```
            pub fn rules(&mut self) -> StyleRules<'_> {}
        }

        /// A fluent builder for adding style rules to a StyleMap.
        ///
        /// Created via [`StyleMap::rules()`]. Collects path/style pairs and commits
        /// them on [`.apply()`](StyleRules::apply).
        pub struct StyleRules<'a> {}

        impl<'a> StyleRules<'a> {
            /// Set the foreground paint for a path.
            ///
            /// If a rule already exists for this path, the foreground paint is merged
            /// with the existing style.
            pub fn fg(self, path: &str, paint: impl Into<Paint>) -> Self {}

            /// Set the background paint for a path.
            ///
            /// If a rule already exists for this path, the background paint is merged
            /// with the existing style.
            pub fn bg(self, path: &str, paint: impl Into<Paint>) -> Self {}

            /// Add a single attribute for a path.
            ///
            /// If a rule already exists for this path, the attribute is merged
            /// with the existing style.
            pub fn attr(self, path: &str, attr: Attr) -> Self {}

            /// Set all attributes for a path.
            ///
            /// If a rule already exists for this path, the attributes are merged
            /// with the existing style.
            pub fn attrs(self, path: &str, attrs: AttrSet) -> Self {}

            /// Apply a complete style to a path.
            ///
            /// If a rule already exists for this path, the style is merged
            /// with the existing style (new values take precedence).
            pub fn style(self, path: &str, style: impl Into<PartialStyle>) -> Self {}

            /// Apply a complete style to multiple paths.
            ///
            /// If a rule already exists for any path, the style is merged
            /// with the existing style (new values take precedence).
            pub fn style_all(self, paths: &[&str], style: impl Into<PartialStyle>) -> Self {}

            /// Set a path prefix for all subsequent rules.
            ///
            /// Can be called multiple times; each call replaces the previous prefix.
            pub fn prefix(self, prefix: &str) -> Self {}

            /// Clear the current prefix.
            pub fn no_prefix(self) -> Self {}

            /// Commit all pending rules to the StyleMap.
            pub fn apply(self) {}
        }

        /// A hierarchical style manager.
        ///
        /// `Style` objects are entered into the manager with '/'-separated paths. For
        /// example:
        ///
        ///   / white, black
        ///   /frame -> grey, None
        ///   /frame/selected -> blue, None
        ///
        /// The first entry with the empty path is the global default. Every
        /// `StyleManager` is guaranteed to have a default Style object with non-None
        /// foreground and background colors, so style resolution always succeeds.
        ///
        /// `Style` objects also contain text attributes.
        ///
        /// During rendering, a node may push a name onto the stack of layers tracked by
        /// the `Style` object. Layers are maintained for a node and all its
        /// descendants, and `Canopy` manages poppping layers back off the stack at the
        /// appropriate time during rendering.
        ///
        /// When a colour is resolved, we first try to find the specified path under
        /// each layer to the root; failing that we look up the default colours for each
        /// layer to the root.
        ///
        /// So given a layer stack ["foo"], and an attempt to look up "frame/selected",
        /// we try the following lookups in order: ["foo/frame/selected",
        /// "/frame/selected", "foo", ""].
        #[derive(Debug, StructuralPartialEq, PartialEq, Eq, Clone, Default)]
        pub struct StyleManager {}

        impl StyleManager {
            /// Construct a new style manager.
            pub fn new() -> Self {}

            /// Reset all layers and levels.
            pub fn reset(&mut self) {}

            /// Increment the render level.
            pub fn push(&mut self) {}

            /// Decrement the render level and pop any layers at this level.
            pub fn pop(&mut self) {}

            /// Push onto the layer stack with the current render level.
            pub fn push_layer(&mut self, name: &str) {}

            /// Resolve a style path.
            pub fn get(&self, smap: &StyleMap, path: &str) -> Style {}
        }
    }

    pub mod text {
        //! Text utilities.

        /// Slice a string by display columns, returning the substring and its width.
        pub fn slice_by_columns(s: &str, start: usize, max: usize) -> (&str, usize) {}

        /// Return the display width of a grapheme cluster, capped at terminal cell widths.
        pub fn grapheme_width(grapheme: &str) -> usize {}

        /// Expand tabs into spaces using the configured tab stop.
        pub fn expand_tabs(s: &str, tab_stop: usize) -> String {}
    }

    pub mod view {
        //! View management.

        /// Render-time view information for a node.
        #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
        pub struct View {
            /// Outer rect in screen coordinates (signed for scroll translations).
            pub outer: crate::geom::RectI32,
            /// Content rect in screen coordinates (outer inset by padding).
            pub content: crate::geom::RectI32,
            /// Viewport offset in content coordinates (scroll position).
            pub tl: crate::geom::Point,
            /// Canvas size in content coordinates.
            pub canvas: crate::geom::Size,
        }

        impl View {
            /// Size of the outer rect.
            pub fn outer_size(&self) -> Size {}

            /// Size of the content rect.
            pub fn content_size(&self) -> Size {}

            /// True if the view is zero-sized.
            pub fn is_zero(&self) -> bool {}

            /// Offset from the outer origin to the content origin, in local coordinates.
            pub fn content_origin(&self) -> Point {}

            /// Visible view rectangle in content coordinates.
            pub fn view_rect(&self) -> Rect {}

            /// Visible view rectangle in local outer coordinates.
            pub fn view_rect_local(&self) -> Rect {}

            /// Local outer rectangle with origin at (0,0).
            pub fn outer_rect_local(&self) -> Rect {}

            /// Build a view from signed outer/content rects and content/canvas sizes.
            pub fn new(outer: RectI32, content: RectI32, tl: Point, canvas: Size) -> Self {}

            /// Calculates the (pre, active, post) rectangles needed to draw a vertical
            /// scroll bar for this view in the specified margin rect.
            pub fn vactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {}

            /// Calculates the (pre, active, post) rectangles needed to draw a horizontal
            /// scroll bar for this view in the specified margin rect.
            pub fn hactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {}
        }
    }

    pub use canopy_derive::command;
    pub use canopy_derive::derive_commands;
    pub use canopy_derive::CommandArg;
    pub use canopy_derive::CommandEnum;
    /// The result of an event handler.
    #[derive(Debug, StructuralPartialEq, PartialEq, Eq, Clone)]
    pub enum EventOutcome {
        /// The event was processed and propagation stops.
        Handle,
        /// The event was processed without a state change and propagation stops.
        Consume,
        /// The event was not handled and will bubble up the tree.
        Ignore,
    }

    /// Widgets are the behavior attached to nodes in the Core arena.
    pub trait Widget: Any + Send {
        /// Layout configuration for this widget.
        fn layout(&self) -> Layout {}

        /// Measure intrinsic content size (content box, excludes Layout padding).
        fn measure(&self, c: MeasureConstraints) -> Measurement {}

        /// Canvas size in content coordinates (for scrolling).
        ///
        /// `view` is this node's content size (outer minus padding).
        fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext<'_>) -> Size<u32> {}

        /// Render this widget's own content. Does not render children.
        fn render(&mut self, _frame: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

        /// Handle events.
        fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {}

        /// Attempt to focus this widget.
        ///
        /// Widgets can use the provided context to query their tree state (e.g., whether they have
        /// children) when deciding whether to accept focus.
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

        /// Cursor specification for focused widgets.
        fn cursor(&self) -> Option<cursor::Cursor> {}

        /// Scheduled poll endpoint.
        fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {}

        /// Called when the widget is mounted in the tree, before its first render.
        ///
        /// A failed hook rolls back core-owned state. External effects and widget-owned state must be
        /// repeatable or compensating because a later mount attempt may call this hook again.
        fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {}

        /// Validation hook before a widget is removed or replaced.
        ///
        /// This hook must be side-effect free or safely repeatable.
        fn pre_remove(&mut self, _ctx: &mut dyn Context) -> Result<()> {}

        /// Called before a successfully mounted widget is removed or replaced.
        ///
        /// This hook cannot veto removal. During failure rollback, structural context operations are
        /// rejected and external cleanup must be safe to repeat.
        fn on_unmount(&mut self, _ctx: &mut dyn Context) {}

        /// Name used for commands and paths.
        fn name(&self) -> NodeName {}
    }

    /// Convenience macro for building named arguments.
    #[macro_export]
    macro_rules! named_args {
    ($($key:ident : $value:expr),* $(,)?) => { ... };
}
    /// Build a [`Color`](crate::style::Color) from a `#RRGGBB` or `RRGGBB` literal at compile time.
    #[macro_export]
    macro_rules! rgb {
    ($hex:literal) => { ... };
}
    /// Define a typed key for keyed children.
    ///
    /// # Examples
    ///
    /// ```
    /// use canopy::{ChildKey, Widget, key};
    ///
    /// key!(Editor);
    /// impl Widget for Editor {}
    ///
    /// pub struct Modal;
    /// impl Widget for Modal {}
    /// key!(pub ModalSlot: Modal);
    ///
    /// assert_eq!(Editor::KEY, "Editor");
    /// assert_eq!(ModalSlot::KEY, "ModalSlot");
    /// ```
    #[macro_export]
    macro_rules! key {
    ($vis:vis $name:ident) => { ... };
    ($vis:vis $name:ident : $widget:ty) => { ... };
}
}
