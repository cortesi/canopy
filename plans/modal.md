# Modal System Design

This document specifies the design for a centralized modal system in Canopy, enabling properly
centered dialogs that overlay content with computed background dimming.

## Problem Statement

The current todo example uses an ad-hoc overlay approach with flex spacers to approximate centering.
This has fundamental limitations:

1. **No true overlapping**: Children in Column/Row layouts are positioned sequentially, not stacked
2. **Spacer hacks**: Centering requires manual spacer widgets with flex layouts
3. **No dimming mechanism**: No way to dim background content when showing modals
4. **Poor ergonomics**: Each modal requires boilerplate setup

## Design Goals

1. **Layout extension**: Add stacking capability where children overlap in the same space
2. **Alignment support**: Add alignment properties to position children within available space
3. **Style effects system**: A general, trait-based mechanism for computed style transformations
4. **Stacking effects**: Effects compose/stack through the tree, enabling layered transformations
5. **Full style control**: Effects can transform colors AND text attributes
6. **Layer integration**: Effects can be associated with style layers
7. **Reusable widgets**: Provide Center and Modal widgets for common patterns
8. **Backward compatibility**: Existing layouts continue to work unchanged

---

## Part 1: Layout System Extensions

### 1.1 Direction::Stack

Add a new layout direction where all children occupy the same space, rendered in tree order (last
child on top - painter's algorithm).

```rust
pub enum Direction {
    Column,  // Stack children vertically
    Row,     // Stack children horizontally
    Stack,   // Children overlap, all positioned at (0,0)
}
```

**Behavior for Stack direction:**
- All visible children are positioned at `(0, 0)` relative to the parent's content origin
- All children receive the full content area as their available space
- Children are rendered in tree order (first child rendered first, last child on top)
- Gap property is ignored (no sequential spacing needed)
- Flex weights on children's main axis are resolved against the full content area
- **Hit-testing**: Events should check children in reverse order (last child first) so the visually
  top-most element receives events. This matches the painter's algorithm rendering order.

**Layout engine changes** (in `world.rs`):

```rust
fn layout_children(&mut self, node_id: NodeId, layout: Layout, content: Size<u32>) {
    let children = self.visible_children(node_id);
    if children.is_empty() {
        return;
    }

    match layout.direction {
        LayoutDirection::Stack => {
            // Stack: all children get full content area, positioned according to alignment
            for child in &children {
                // First, layout the child to determine its size
                self.layout_node(*child, content.into(), Point::zero());

                // Then apply alignment to position the child within content area
                let child_size = self.node_size(*child);
                let offset_x = align_offset(child_size.w, content.w, layout.align_horizontal);
                let offset_y = align_offset(child_size.h, content.h, layout.align_vertical);
                self.set_node_position(*child, Point::new(offset_x, offset_y));
            }
        }
        LayoutDirection::Row | LayoutDirection::Column => {
            // Existing sequential layout logic (also applies cross-axis alignment)...
        }
    }
}
```

### 1.2 Alignment Properties

Add alignment properties to control how children are positioned within available space when their
measured size is smaller than the available area.

```rust
/// Alignment along an axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

pub struct Layout {
    // ... existing fields ...

    /// Horizontal alignment of children within content area.
    pub align_horizontal: Align,
    /// Vertical alignment of children within content area.
    pub align_vertical: Align,
}
```

**Builder methods:**

```rust
impl Layout {
    pub fn align_horizontal(mut self, align: Align) -> Self {
        self.align_horizontal = align;
        self
    }

    pub fn align_vertical(mut self, align: Align) -> Self {
        self.align_vertical = align;
        self
    }

    pub fn align_center(self) -> Self {
        self.align_horizontal(Align::Center).align_vertical(Align::Center)
    }
}
```

**Alignment behavior:**

Alignment applies AFTER a child's size is resolved but BEFORE its final position is set. The
alignment determines where the child is placed within the available space.

For **Stack direction**: Each child is aligned independently within the full content area.

For **Row/Column directions**:
- `align_horizontal` aligns children along the cross axis when direction is Column
- `align_vertical` aligns children along the cross axis when direction is Row
- Main axis alignment could be added later (similar to CSS `justify-content`)

**Position calculation:**

```rust
fn align_offset(child_size: u32, available: u32, align: Align) -> u32 {
    match align {
        Align::Start => 0,
        Align::Center => available.saturating_sub(child_size) / 2,
        Align::End => available.saturating_sub(child_size),
    }
}
```

---

## Part 2: New Widgets

### 2.1 Stack Widget

A simple container using Stack direction. Useful when you need children to overlap.

```rust
// widgets/stack.rs

/// Container that renders children overlapping in the same space.
/// Children are rendered in tree order (last child on top).
pub struct Stack;

impl Widget for Stack {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Stack)
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("stack")
    }
}
```

### 2.2 Center Widget

Centers its single child both horizontally and vertically.

```rust
// widgets/center.rs

/// Container that centers its child within available space.
pub struct Center;

impl Widget for Center {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Stack)
            .align_center()
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("center")
    }
}
```

**Usage:**
```rust
let center_id = ctx.add_orphan(Center);
let content_id = ctx.add_orphan(my_content);
ctx.mount_child_to(center_id, content_id)?;
```

### 2.3 Modal Widget

A convenience widget that centers content. Used in combination with the effects system -
the parent applies effects (like `effects::dim()`) to the background content, while the Modal
renders without those effects (siblings don't inherit from each other in a Stack).

```rust
// widgets/modal.rs

/// A modal container that centers its content.
///
/// For the dimming effect, the parent should push an effect on the background content
/// using `c.push_effect(background_id, effects::dim(0.5))`. The Modal itself renders
/// at full brightness since it's a sibling to the dimmed content, not a descendant.
pub struct Modal;

impl Modal {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Modal {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Modal {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Stack)
            .align_center()
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("modal")
    }
}
```

**Note**: The Modal widget is intentionally simple - it just centers content. Visual effects like
dimming are handled by the Style Effects System (Part 3), which allows any subtree to have
arbitrary style transformations applied without special widget support.

---

## Part 3: Style Effects System

A general mechanism for computed style transformations that stack through the widget tree. Effects
transform resolved styles without requiring separate style definitions, enabling dimming,
highlighting, desaturation, inversion, and custom transformations.

### 3.1 Core Concept: The StyleEffect Trait

Effects are defined by a trait that transforms a `Style`:

```rust
/// A transformation applied to styles during rendering.
/// Effects are stackable and applied in order during tree traversal.
pub trait StyleEffect: Send + Sync + Debug {
    /// Transform a style. Receives the current style and returns the modified version.
    fn apply(&self, style: Style) -> Style;

    /// Clone this effect into a boxed trait object.
    fn box_clone(&self) -> Box<dyn StyleEffect>;
}
```

The trait requires:
- `Debug` for diagnostics (printing active effects)
- `Send + Sync` for flexibility in threading models
- `box_clone()` so nodes with effects can be cloned with their effects intact

These bounds add some complexity to custom effect implementations but provide maximum flexibility.

### 3.2 Color Primitive Transformations

The effects system is built on fundamental color operations defined on `Color`.

**Color conversion strategy**: All color variants (named colors, ANSI-256, Default) are converted
to RGB for transformation, then the result is kept as RGB. This assumes modern terminal support for
RGB colors (which most terminals provide). The tradeoff is that we lose deferred terminal theme
colors, but we gain consistent, predictable color transformations across all color types.

```rust
impl Color {
    /// Convert any color variant to RGB for transformation.
    /// Named colors and ANSI-256 use standard palette mappings.
    /// Default colors map to White (fg) or Black (bg) - caller should handle this case.
    pub fn to_rgb(self) -> Color;

    /// Scale brightness by multiplying RGB components.
    /// factor < 1.0 dims, factor > 1.0 brightens.
    pub fn scale_brightness(self, factor: f32) -> Self;

    /// Scale saturation toward/away from grayscale.
    /// factor = 0.0 is grayscale, 1.0 is unchanged, > 1.0 increases saturation.
    pub fn scale_saturation(self, factor: f32) -> Self;

    /// Blend this color toward another by the given amount (0.0 = self, 1.0 = other).
    pub fn blend(self, other: Color, amount: f32) -> Self;

    /// Invert this color's RGB channels (255 - component for each channel).
    /// Note: This is channel inversion, not fg/bg swap. See effects::swap_fg_bg() for that.
    pub fn invert_rgb(self) -> Self;

    /// Shift hue by degrees (0-360).
    pub fn shift_hue(self, degrees: f32) -> Self;
}
```

These operations cover the main use cases for UI effects. Additional operations (contrast, gamma,
temperature) can be added later if needed.

### 3.3 Attribute Transformations

Effects can also modify text attributes:

```rust
impl AttrSet {
    /// Force an attribute on.
    pub fn with(self, attr: Attr) -> Self;

    /// Force an attribute off.
    pub fn without(self, attr: Attr) -> Self;
}
```

Note: A `toggle()` method was considered but omitted - toggling is confusing for effects because
the result depends on the incoming state, making effects non-deterministic when composed.

Effects can either modify individual attributes or replace the entire `AttrSet` - both are naturally
supported since effects receive the full `Style` and return a new one. No special API is needed.

### 3.4 Built-in Effects Module

The `effects` module provides constructors for common transformations:

```rust
pub mod effects {
    /// Reduce brightness. factor < 1.0 dims (0.5 = half brightness).
    pub fn dim(factor: f32) -> Box<dyn StyleEffect>;

    /// Increase brightness. factor > 1.0 brightens (1.5 = 50% brighter).
    pub fn brighten(factor: f32) -> Box<dyn StyleEffect>;

    /// Adjust saturation. 0.0 = grayscale, 1.0 = unchanged, > 1.0 = oversaturated.
    /// Example: saturation(0.0) removes all color, saturation(0.5) is half-saturated.
    pub fn saturation(factor: f32) -> Box<dyn StyleEffect>;

    /// Swap foreground and background colors.
    pub fn swap_fg_bg() -> Box<dyn StyleEffect>;

    /// Invert RGB channels of both fg and bg (255 - component).
    pub fn invert_rgb() -> Box<dyn StyleEffect>;

    /// Blend all colors toward a target color.
    pub fn tint(color: Color, amount: f32) -> Box<dyn StyleEffect>;

    /// Force the dim text attribute on (terminal-level dimming).
    pub fn attr_dim() -> Box<dyn StyleEffect>;

    /// Force bold attribute on.
    pub fn bold() -> Box<dyn StyleEffect>;

    /// Force italic attribute on.
    pub fn italic() -> Box<dyn StyleEffect>;

    /// Remove all text attributes.
    pub fn plain() -> Box<dyn StyleEffect>;
}
```

The built-in effects focus on fundamental transformations. Semantic effects (focus, error, warning,
selection) can be composed from these primitives by users as needed.

### 3.5 Effect Stacking

Effects stack (compose) as they propagate through the tree. When a node adds effects, they are
applied AFTER any inherited effects:

```
App (no effects)
├── Content (effects: [dim(0.5)])
│   ├── List (no effects → inherits [dim(0.5)])
│   │   └── SelectedItem (effects: [swap_fg_bg()] → effective [dim(0.5), swap_fg_bg()])
│   └── DisabledPanel (effects: [saturation(0.3)] → effective [dim(0.5), saturation(0.3)])
└── Modal (no effects → inherits [] from App)
```

The effect stack is applied in order: first inherited effects, then local effects.

A reset mechanism is provided via `c.set_clear_inherited_effects(node_id, true)` rather than a
special effect, since resetting isn't a transformation. This is essential for the modal pattern
where the modal content should not inherit the background's dimming effect.

The reset is all-or-nothing by design. If finer control is needed, nodes can clear inherited effects
and re-apply only the wanted ones. This keeps the initial implementation simple while covering the
primary use case (modals that render without parent effects).

### 3.6 Node Effect Storage

Nodes store their local effects. Most nodes won't have effects, so we use `Option<Box<...>>` to
avoid allocating an empty Vec for every node:

```rust
pub struct Node {
    // ... existing fields ...

    /// Effects to apply to this node and descendants.
    /// These stack on top of any inherited effects.
    /// None for the common case of no effects (avoids per-node Vec allocation).
    effects: Option<Box<Vec<Box<dyn StyleEffect>>>>,

    /// If true, clear inherited effects before applying local effects.
    clear_inherited_effects: bool,
}
```

This trades slight complexity in the effect manipulation code for significant memory savings when
most nodes have no effects (the common case).

### 3.7 Context API

```rust
trait Context {
    /// Add an effect to a node. Effects stack with any existing effects.
    fn push_effect(&mut self, node_id: NodeId, effect: Box<dyn StyleEffect>) -> Result<()>;

    /// Remove all effects from a node.
    fn clear_effects(&mut self, node_id: NodeId) -> Result<()>;

    /// Set whether this node clears inherited effects before applying its own.
    fn set_clear_inherited_effects(&mut self, node_id: NodeId, clear: bool) -> Result<()>;
}
```

The API is explicit - users call `c.push_effect(id, effects::dim(0.5))` rather than convenience
methods like `c.dim(id, 0.5)`. This keeps the API surface minimal and consistent.

Effects are append-only. To update effects (e.g., when showing/hiding a modal), use the pattern:
```rust
c.clear_effects(node_id)?;
c.push_effect(node_id, effects::dim(0.5))?;
```

This prevents accidental effect stacking when toggling UI states.

### 3.8 Render Integration

The Render struct maintains the current effect stack:

```rust
pub struct Render<'a> {
    // ... existing fields ...

    /// Current effect stack, applied in order.
    effects: Vec<&'a dyn StyleEffect>,
}

impl<'a> Render<'a> {
    /// Resolve a style name and apply the current effect stack.
    fn resolve_style(&self, name: &str) -> Style {
        let base = self.style.get(self.stylemap, name);
        self.apply_effects(base)
    }

    /// Apply the current effect stack to a style.
    /// Use this when you have a Style from a source other than resolve_style().
    pub fn apply_effects(&self, style: Style) -> Style {
        let mut result = style;
        for effect in &self.effects {
            result = effect.apply(result);
        }
        result
    }
}
```

### 3.9 Render Traversal

During traversal, effects are accumulated:

```rust
fn render_traversal(
    &mut self,
    node_id: NodeId,
    inherited_effects: &[&dyn StyleEffect],
) -> Result<()> {
    let node = &self.core.nodes[node_id];

    // Build effective effects list
    let local_effects = node.effects.as_ref().map(|v| v.as_slice()).unwrap_or(&[]);
    let effective_effects: Vec<&dyn StyleEffect> = if node.clear_inherited_effects {
        // Start fresh with only local effects
        local_effects.iter().map(|e| e.as_ref()).collect()
    } else {
        // Inherit and extend
        inherited_effects
            .iter()
            .copied()
            .chain(local_effects.iter().map(|e| e.as_ref()))
            .collect()
    };

    // Create render context with effects
    let rndr = Render::new(...).with_effects(&effective_effects);

    // Render this node
    widget.render(&mut rndr, &ctx)?;

    // Render children with accumulated effects
    for child in children {
        self.render_traversal(child, &effective_effects)?;
    }

    Ok(())
}
```

### 3.10 Layer Integration (Deferred)

Layer-effect integration is deferred to a future enhancement. The node-level effects API
(`push_effect`, `clear_effects`, `set_clear_inherited_effects`) is complete and covers all
immediate use cases.

A future enhancement could allow style layers to have associated effects that apply automatically
when the layer is pushed. This would enable consistent theming where semantic states like
"disabled" could automatically apply both style overrides AND effects.

### 3.11 Custom Effects

Users can define custom effects by implementing the trait:

```rust
struct HighContrastEffect {
    boost: f32,
}

impl StyleEffect for HighContrastEffect {
    fn apply(&self, style: Style) -> Style {
        // Increase contrast between fg and bg
        let fg_luma = luminance(style.fg);
        let bg_luma = luminance(style.bg);

        if fg_luma > bg_luma {
            // Light on dark: brighten fg, dim bg
            Style {
                fg: style.fg.scale_brightness(1.0 + self.boost),
                bg: style.bg.scale_brightness(1.0 - self.boost * 0.5),
                attrs: style.attrs,
            }
        } else {
            // Dark on light: dim fg, brighten bg
            Style {
                fg: style.fg.scale_brightness(1.0 - self.boost * 0.5),
                bg: style.bg.scale_brightness(1.0 + self.boost),
                attrs: style.attrs,
            }
        }
    }

    fn box_clone(&self) -> Box<dyn StyleEffect> {
        Box::new(self.clone())
    }
}
```

### 3.12 Why This Design?

1. **General**: Any transformation expressible as `Style -> Style` can be an effect
2. **Composable**: Effects stack naturally, enabling complex combinations
3. **Computed**: Works with any theme - transformations are applied to resolved colors
4. **Extensible**: Users can define custom effects for domain-specific needs
5. **Ergonomic**: Built-in effects cover common cases with simple constructors
6. **Attribute-aware**: Effects can modify text attributes, not just colors
7. **Layer-integrated**: Effects can work with the existing style layer system

---

## Part 4: Usage Patterns

### 4.1 Simple Centered Modal with Dimming

```rust
use canopy::effects;

// In your app widget
fn show_modal(&mut self, c: &mut dyn Context) -> Result<()> {
    let modal_id = c.add_orphan(Modal::new());
    let frame_id = c.add_orphan(frame::Frame::new().with_title("Enter Item"));
    let input_id = c.add_orphan(Input::new(""));

    c.mount_child_to(frame_id, input_id)?;
    c.mount_child_to(modal_id, frame_id)?;

    // Dim the background content
    c.push_effect(self.content_id, effects::dim(0.5))?;

    // Update children to include modal (Stack layout makes it overlay)
    self.modal_id = Some(modal_id);
    self.sync_children(c)?;

    c.set_focus(input_id);
    Ok(())
}

fn hide_modal(&mut self, c: &mut dyn Context) -> Result<()> {
    // Remove effects from background
    c.clear_effects(self.content_id)?;

    // Remove modal from children
    self.modal_id = None;
    self.sync_children(c)?;
    Ok(())
}
```

### 4.2 App Structure with Modal Support

```rust
// Your app should use Stack at the top level to support modals
pub struct App {
    content_id: Option<NodeId>,
    modal_id: Option<NodeId>,
}

impl App {
    fn sync_children(&mut self, c: &mut dyn Context) -> Result<()> {
        let content_id = self.content_id.expect("not initialized");
        let mut children = vec![content_id];

        if let Some(modal_id) = self.modal_id {
            children.push(modal_id);
        }

        c.set_children(children)
    }
}

impl Widget for App {
    fn layout(&self) -> Layout {
        // Stack direction allows modal to overlay content
        Layout::fill().direction(Direction::Stack)
    }
}
```

### 4.3 Stacking Multiple Effects

Effects compose naturally. Apply multiple effects to a node for combined transformations:

```rust
use canopy::effects;

// Disabled panel: dimmed AND desaturated
c.push_effect(panel_id, effects::dim(0.7))?;
c.push_effect(panel_id, effects::saturation(0.5))?;  // 50% saturation

// Selected item: swapped colors AND bold
c.push_effect(selected_id, effects::swap_fg_bg())?;
c.push_effect(selected_id, effects::bold())?;

// Error state: tinted red
c.push_effect(error_id, effects::tint(Color::Red, 0.3))?;
```

### 4.4 Effect Inheritance Through Tree

Effects propagate to descendants, enabling subtree-wide transformations:

```rust
use canopy::effects;

// Dim the entire content area - all descendants inherit the dim
c.push_effect(content_id, effects::dim(0.5))?;

// A child can add more effects (stacks with inherited dim)
c.push_effect(disabled_section, effects::saturation(0.3))?;
// Effective: dim(0.5) then saturation(0.3)

// A special child can clear inherited effects entirely
c.set_clear_inherited_effects(alert_id, true)?;
// alert_id renders at full brightness, ignoring parent's dim
```

### 4.5 Practical Examples

**Inactive tabs:**
```rust
for tab_id in inactive_tabs {
    c.push_effect(tab_id, effects::dim(0.6))?;
}
c.clear_effects(active_tab_id)?;  // Active tab at full brightness
```

**Focus indication:**
```rust
c.push_effect(focused_id, effects::brighten(1.2))?;
```

**Warning/Error states:**
```rust
c.push_effect(warning_id, effects::tint(Color::Yellow, 0.2))?;
c.push_effect(error_id, effects::tint(Color::Red, 0.3))?;
```

**Disabled controls:**
```rust
c.push_effect(disabled_id, effects::saturation(0.0))?;  // Full grayscale
c.push_effect(disabled_id, effects::dim(0.7))?;
```

**Selection highlight:**
```rust
c.push_effect(selected_id, effects::swap_fg_bg())?;
```

### 4.6 Custom Effects

Define domain-specific effects by implementing `StyleEffect`:

```rust
use canopy::{Style, StyleEffect};

/// Applies a "ghost" effect for elements being dragged
struct GhostEffect;

impl StyleEffect for GhostEffect {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg.scale_brightness(0.5).scale_saturation(0.3),
            bg: style.bg.blend(Color::Rgb { r: 128, g: 128, b: 255 }, 0.2),
            attrs: style.attrs.with(Attr::Italic),
        }
    }

    fn box_clone(&self) -> Box<dyn StyleEffect> {
        Box::new(GhostEffect)
    }
}

// Usage
c.push_effect(dragged_id, Box::new(GhostEffect))?;
```

---

## Staged Execution Plan

### Stage 1: Layout Extension - Direction::Stack

Add the Stack direction to the layout system.

1. [x] Add `Stack` variant to `Direction` enum in `layout.rs`
2. [x] Add `direction()` builder method to `Layout` if not present
3. [x] Modify `layout_children` in `world.rs` to handle Stack direction
4. [x] Modify `measure_wrap_content` in `world.rs` to handle Stack direction (max of children sizes)
5. [x] Add unit tests for Stack layout behavior
6. [x] Run all tests, lint, and format

### Stage 2: Layout Extension - Alignment

Add alignment properties to the layout system.

1. [x] Add `Align` enum to `layout.rs`
2. [x] Add `align_horizontal` and `align_vertical` fields to `Layout`
3. [x] Add builder methods: `align_horizontal()`, `align_vertical()`, `align_center()`
4. [x] Implement `align_offset()` helper function in `world.rs`
5. [x] Modify `layout_children` to apply alignment when positioning children
6. [x] Ensure alignment works for Stack direction (primary use case)
7. [x] Add unit tests for alignment behavior
8. [x] Run all tests, lint, and format

### Stage 3: Color Primitive Transformations

Add transformation methods to Color for the effects system.

1. [x] Add `to_rgb()` method to convert any Color variant to RGB
2. [x] Add helper to convert ANSI 256-color indices to RGB
3. [x] Add helper to convert named colors to RGB
4. [x] Add `scale_brightness(factor: f32)` method
5. [x] Add `scale_saturation(factor: f32)` method
6. [x] Add `blend(other: Color, amount: f32)` method
7. [x] Add `invert_rgb()` method
8. [x] Add `shift_hue(degrees: f32)` method (if included)
9. [x] Add unit tests for all color transformations
10. [x] Run all tests, lint, and format

### Stage 4: StyleEffect Trait and Built-in Effects

Implement the core effects system.

1. [x] Define `StyleEffect` trait in `style/effects.rs`
2. [x] Add `effects` module to `style/mod.rs`
3. [x] Implement `effects::dim(factor)`
4. [x] Implement `effects::brighten(factor)`
5. [x] Implement `effects::saturation(factor)`
6. [x] Implement `effects::swap_fg_bg()`
7. [x] Implement `effects::invert_rgb()`
8. [x] Implement `effects::tint(color, amount)`
9. [x] Implement attribute effects: `effects::bold()`, `effects::italic()`, `effects::plain()`
10. [x] Add unit tests for all built-in effects
11. [x] Run all tests, lint, and format

### Stage 5: Node Effect Storage and Context API

Integrate effects into the node/context system.

1. [x] Add `effects: Vec<Box<dyn StyleEffect>>` field to `Node`
2. [x] Add `clear_inherited_effects: bool` field to `Node`
3. [x] Add `push_effect()` method to `Context` trait
4. [x] Add `clear_effects()` method to `Context` trait
5. [x] Add `set_clear_inherited_effects()` method to `Context` trait
6. [x] Implement all Context methods in `CoreContext`
7. [x] Add unit tests for effect storage and retrieval
8. [x] Run all tests, lint, and format

### Stage 6: Render Integration

Wire effects into the rendering pipeline.

1. [x] Add `effects: Vec<&dyn StyleEffect>` field to `Render`
2. [x] Add `with_effects()` builder method to `Render`
3. [x] Modify `resolve_style()` to apply effect stack
4. [x] Modify `render_traversal` in `canopy.rs` to accumulate and pass effects
5. [x] Handle `clear_inherited_effects` flag in traversal
6. [x] Add integration tests for effect rendering
7. [x] Run all tests, lint, and format

### Stage 7: Stack and Center Widgets

Implement the container widgets.

1. [x] Create `widgets/stack.rs` with `Stack` widget
2. [x] Create `widgets/center.rs` with `Center` widget
3. [x] Add modules to `widgets/mod.rs` and export widgets
4. [x] Add derive_commands to both widgets
5. [x] Add unit tests for Stack and Center widgets
6. [x] Run all tests, lint, and format

### Stage 8: Modal Widget

Implement the Modal convenience widget.

1. [x] Create `widgets/modal.rs` with `Modal` widget
2. [x] Add module to `widgets/mod.rs` and export
3. [x] Add derive_commands
4. [x] Add unit tests
5. [x] Run all tests, lint, and format

### Stage 9: Todo Example Refactor

Update the todo example to use the new modal system.

1. [x] Change `Todo` widget layout to use `Direction::Stack`
2. [x] Remove ad-hoc `Overlay` and `Spacer` widgets
3. [x] Create modal using new `Modal` widget
4. [x] Add `c.push_effect(content_id, effects::dim(0.5))` for background dimming
5. [x] Simplify `ensure_overlay` and `sync_children` methods
6. [x] Update layout configuration for the input frame
7. [x] Test visually to ensure proper centering and dimming
8. [x] Run all tests, lint, and format

### Stage 10: Documentation and Polish

1. [x] Add doc comments to all new public types and methods
2. [x] Update any relevant documentation
3. [x] Consider adding a modal example demonstrating the new widgets
4. [x] Final review of API surface for ergonomics

---

## Future Enhancements

1. **Layer-effect integration**: Allow style layers to have associated effects that apply
   automatically when the layer is pushed. This would enable consistent theming for semantic states
   like "disabled" or "selected".

2. **Main axis alignment for Row/Column**: Add `justify-content`-like main axis alignment. Current
   design focuses on Stack + center alignment.

3. **Focus trapping**: Modal could automatically trap focus within its content.

4. **Animation**: Modal show/hide animations (fade, slide, etc.)

---

## Open Questions

1. **Multiple modals**: The Stack approach naturally supports multiple overlapping modals. Should we
   add z-index or explicit ordering controls?

2. **Effect optimization**: For performance, should we cache transformed styles? Or is the
   transformation cheap enough to apply on every render?

Note: ANSI/Default color handling is addressed in the ASK block in Section 3.2.
