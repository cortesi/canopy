# Stylegym: Style System Demonstration App

An example application that exercises and demonstrates Canopy's styling features, including themes,
effects, and modal overlays.

---

## Overview

Stylegym is a two-pane application:
- **Left pane (control panel)**: Theme selector, effect toggles, and modal trigger
- **Right pane (demo content)**: Mixed widgets showcasing how styles render

The app serves both as a visual demonstration and a testing ground for the style system.

---

## Part 1: Layout Structure

### 1.1 Two-Pane Layout

The root widget uses a Row direction with two children:
- Left pane: Fixed width (~30 characters) containing the control panel
- Right pane: Flex-grow content area with demo widgets

### 1.2 Control Panel Structure

The left pane contains vertically stacked sections:
1. Theme selector (dropdown)
2. Effects toggles (multi-select)
3. Modal trigger (key binding hint)

Each section has a header label and the widget below it.

---

## Part 2: Theme System

### 2.1 Available Themes

The app includes four themes for switching:

1. **Solarized Dark** - existing theme (dark blue-gray background)
2. **Solarized Light** - inverted solarized (cream background)
3. **Gruvbox Dark** - warm, retro colors with dark background
4. **Dracula** - purple-accented dark theme

### 2.2 Theme Switching Mechanism

Theme switching replaces the entire `StyleMap` in Canopy. Each theme provides a function that
returns a complete `StyleMap` with all necessary style definitions. Switching triggers a full
re-render automatically since the next render pass will use the new styles.

Implementation:
- Each theme is a function: `fn theme_name() -> StyleMap`
- `Canopy.style` field holds the active StyleMap
- Dropdown change calls a method that swaps the StyleMap

---

## Part 3: Effects Panel

### 3.1 Effect Toggles

The effects panel allows toggling multiple effects on/off. When enabled, effects apply to the demo
content pane only. Effects are applied in selection order (the order they were enabled).

Available effects:
- **Dim** - `dim(0.5)` - reduce brightness by half
- **Brighten** - `brighten(1.3)` - increase brightness
- **Grayscale** - `saturation(0.0)` - remove all color
- **Invert** - `invert_rgb()` - invert RGB channels
- **Hue Shift** - `hue_shift(180.0)` - rotate colors 180 degrees
- **Bold** - `bold()` - add bold attribute
- **Italic** - `italic()` - add italic attribute

### 3.2 Effect Application

Effects are tracked as an ordered list. When the user enables an effect, it's appended to the list.
When disabled, it's removed. The demo content pane has the full effect stack applied via
`push_effect()`.

---

## Part 4: Demo Content Pane

### 4.1 Content Structure

The right pane displays varied styled content demonstrating how themes and effects render:

1. **Color Palette** - Shows theme accent colors (red, green, blue, yellow, etc.) as labeled
   swatches
2. **Text Samples** - Paragraphs showing normal text, plus examples with bold, italic, underline
3. **Framed Section** - A frame widget containing nested content
4. **Small List** - A simple list demonstrating list styling

### 4.2 Color Palette Display

The palette shows all accent colors from the current theme:
- Red, Orange, Yellow, Green, Cyan, Blue, Violet, Magenta
- Each as a short colored bar with label

This makes it easy to see how effects (especially hue shift, saturation, invert) transform the
theme colors.

---

## Part 5: Modal Demonstration

### 5.1 Modal Trigger

Press 'm' to show a modal overlay.

### 5.2 Modal Content

The modal contains a simple framed message:
```
┌─ Demo Modal ─────────────────┐
│                              │
│  This is a modal overlay.    │
│  Press Esc to dismiss.       │
│                              │
└──────────────────────────────┘
```

### 5.3 Background Dimming

When the modal is shown, only the demo content pane (right side) is dimmed using `effects::dim(0.5)`.
The control panel remains at full brightness and interactive, allowing the user to continue changing
themes/effects while the modal is visible.

---

## Part 6: New Widgets

Two new widgets are needed for the control panel.

### 6.1 Dropdown Widget

A dropdown/select box for single-value selection (theme picker). Shows the current selection with
an indicator, and expands to show options when activated.

```rust
pub struct Dropdown<T> {
    items: Vec<T>,
    selected: usize,
    expanded: bool,
}

impl<T> Dropdown<T> {
    pub fn new(items: Vec<T>) -> Self;
    pub fn selected(&self) -> &T;
    pub fn selected_index(&self) -> usize;
    pub fn set_selected(&mut self, index: usize);
}
```

**Visual states:**
- Collapsed: Shows `[Current Value ▼]`
- Expanded: Shows list of options with current highlighted

**Commands:**
- `toggle` - expand/collapse the dropdown
- `select_next` / `select_prev` - navigate when expanded
- `confirm` - select highlighted item and collapse

### 6.2 Selector Widget (Multi-select)

A checkbox-style list for multi-value selection (effects panel).

```rust
pub struct Selector<T> {
    items: Vec<T>,
    focused: usize,
    selected: Vec<usize>,  // Indices of selected items, in selection order
}

impl<T> Selector<T> {
    pub fn new(items: Vec<T>) -> Self;
    pub fn selected_indices(&self) -> &[usize];
    pub fn selected_items(&self) -> Vec<&T>;
    pub fn toggle_focused(&mut self);
}
```

**Visual rendering:**
- Selected items show `[x]` prefix
- Unselected items show `[ ]` prefix
- Focused item has highlight/inverse background

**Commands:**
- `select_next` / `select_prev` - move focus
- `toggle` - toggle selection of focused item
- `select_first` / `select_last` - jump to ends

---

## Part 7: Key Bindings

Global bindings for stylegym:
- `q` - Quit
- `Tab` - Cycle focus between control panel sections
- `m` - Show modal
- `Esc` - Dismiss modal (or collapse dropdown if expanded)

Within dropdown (when focused):
- `Enter` / `Space` - Toggle expand/collapse
- `j` / `Down` - Next option (when expanded)
- `k` / `Up` - Previous option (when expanded)
- `Enter` - Confirm selection (when expanded)
- `Esc` - Collapse without changing

Within selector:
- `j` / `Down` - Move focus down
- `k` / `Up` - Move focus up
- `Space` / `Enter` - Toggle selection

---

## Staged Execution Plan

### Stage 1: Theme Research and Implementation

Research popular terminal themes and implement them.

1. [x] Research Gruvbox color palette and implement `gruvbox_dark()` in a new `style/gruvbox.rs`
2. [x] Research Dracula color palette and implement `dracula()` in a new `style/dracula.rs`
3. [x] Implement `solarized_light()` in `style/solarized.rs`
4. [x] Ensure all themes define the same style paths (/, /frame, /tab, accent colors, etc.)
5. [x] Export theme modules from `style/mod.rs`
6. [x] Run tests, lint, format

### Stage 2: Dropdown Widget

Create the Dropdown widget for single-select with expand/collapse.

1. [x] Create `widgets/dropdown.rs` with `Dropdown<T>` struct
2. [x] Implement `DropdownItem` trait for rendering items
3. [x] Implement collapsed state rendering (shows current value with ▼)
4. [x] Implement expanded state rendering (list of options)
5. [x] Add selection state management
6. [x] Add commands: `toggle`, `select_next`, `select_prev`, `confirm`
7. [x] Add to `widgets/mod.rs` exports
8. [x] Add unit tests
9. [x] Run tests, lint, format

### Stage 3: Selector Widget

Create the Selector widget for multi-select.

1. [x] Create `widgets/selector.rs` with `Selector<T>` struct
2. [x] Implement `SelectorItem` trait for rendering items
3. [x] Implement selection state management (focused index, selected indices in order)
4. [x] Implement `toggle_focused()` logic
5. [x] Add visual rendering with `[x]/[ ]` indicators
6. [x] Add commands: `select_next`, `select_prev`, `toggle`, `select_first`, `select_last`
7. [x] Add to `widgets/mod.rs` exports
8. [x] Add unit tests
9. [x] Run tests, lint, format

### Stage 4: Stylegym App Structure

Create the basic app layout.

1. [x] Create `crates/examples/src/stylegym.rs`
2. [x] Implement `Stylegym` root widget with Row layout (left: fixed 30, right: flex)
3. [x] Create `ControlPanel` widget (left pane) with Column layout
4. [x] Create `DemoContent` widget (right pane) placeholder
5. [x] Add basic key bindings (q to quit, Tab to cycle focus)
6. [x] Add to `crates/examples/src/lib.rs` exports
7. [x] Run tests, lint, format

### Stage 5: Theme Switching

Implement the theme dropdown and switching.

1. [x] Add theme names as items in Dropdown
2. [x] Track current theme in app state
3. [x] Implement theme switching by replacing `Canopy.style` (via Context::set_style and Core::pending_style)
4. [x] Wire dropdown changes to trigger theme switch (Enter to confirm + apply_theme)
5. [x] Test theme switching visually
6. [x] Run tests, lint, format

### Stage 6: Effects Panel

Implement the effects Selector.

1. [x] Add effect names as items in Selector
2. [x] Track active effects as ordered Vec in app state
3. [x] On selection change, rebuild effect list and apply to demo pane via `push_effect`
4. [x] Clear effects when all deselected
5. [x] Test effect stacking (multiple effects at once)
6. [x] Run tests, lint, format

### Stage 7: Demo Content

Build the demo content pane.

1. [x] Create color palette widget showing accent colors as horizontal bars
2. [x] Add text samples section with normal, bold, italic, underline examples
3. [x] Add a framed section with nested content (demo pane is wrapped in Frame)
4. [x] Arrange sections in Column layout with proper spacing
5. [x] Run tests, lint, format

### Stage 8: Modal Integration

Add modal demonstration.

1. [x] Add 'm' key binding to show modal
2. [x] Create modal content (framed message with instructions)
3. [x] Apply dim effect to demo pane when modal shown
4. [x] Add Esc binding to dismiss modal
5. [x] Clear dim effect when modal dismissed (re-applies user effects)
6. [x] Test modal show/hide cycle
7. [x] Run tests, lint, format

### Stage 9: Polish

1. [x] Review and refine visual appearance
2. [x] Ensure consistent styling across themes (all themes have matching style paths)
3. [x] Add doc comments to Dropdown, Selector widgets and theme functions
4. [x] Run final tests, lint, format

### Stage 10: Bug Fixes (Post-Implementation)

Fixes for issues discovered during testing:

1. [x] Fix layout overlap - both panels now use Frame widgets for clear boundaries
2. [x] Simplify structure - removed redundant ControlPanel widget, use Frame directly
3. [x] Add Selector focus indicator - only shows highlight when widget has focus
4. [x] Add mouse click support to Selector - via `on_event` handler
5. [x] Add selector/dropdown style paths to all themes for proper visual feedback
6. [x] Run tests, lint, format

### Stage 11: Second Round Bug Fixes

Additional fixes based on user testing:

1. [x] Fix mouse click effect not applying immediately - Selector `on_event` returns `Ignore` to allow
   mouse bindings to fire
2. [x] Fix modal positioning - restructured with Container using Stack direction for modal overlay over
   right pane only
3. [x] Add mouse click support to Dropdown - via `on_event` handler
4. [x] Add focus indication - nested frames around Theme and Effects sections
5. [x] Add j/k global navigation bindings for moving between controls
6. [x] Add BackTab for reverse focus navigation
7. [x] Run tests, lint, format

### Stage 12: Third Round Bug Fixes

1. [x] Fix controls overlapping with frames - preserve Frame's padding (Edges::all(1)) when setting
   custom layouts via `with_layout_of`
2. [x] Fix dimming not affecting background - DemoContent now explicitly fills its background with root
   style via `rndr.fill("", rect, ' ')` so effects apply to empty space
3. [x] Fix modal background also being dimmed - ModalContent now fills its own background so the dimmed
   content doesn't show through
4. [x] Run tests, lint, format

### Stage 13: Dropdown Expansion

1. [x] Add `c.taint()` calls to Dropdown's toggle, confirm, cancel commands to trigger re-layout
2. [x] Add `ctx.taint()` call to Dropdown's on_event mouse handler
3. [x] Remove fixed_height(3) from theme frame so it can expand with dropdown
4. [x] Run tests, lint, format

### Stage 14: Terminal Attribute Leak Fix

1. [x] Fix crossterm backend apply_style() - was only adding attributes without resetting first, causing
   attributes to accumulate across style changes. Now always resets before setting colors and attrs.
2. [x] Run tests, lint, format
