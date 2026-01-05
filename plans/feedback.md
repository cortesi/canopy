## 1) Fix style layer popping (correctness) and add layer scoping

### What’s wrong

`StyleManager` tracks a render `level`, plus `layers` and `layer_levels`. The render traversal in
`Canopy::render_recursive` calls `styl.push()` once per node and `styl.pop()` once per node, while
widgets can push multiple layers at the same level (for example, `Button` pushes `"button"` and,
when selected, `"selected"`). `StyleManager::pop()` currently removes only one layer for the
current level, so earlier layers remain on the stack and leak into siblings until the next
`reset()`.

This is a correctness bug that can create non-local styling issues.

### Recommendation

1. Fix `StyleManager::pop()` to pop **all** layers pushed at the current level.
2. No additional API is required; once `pop()` drains the current level, the core traversal
   provides the necessary scoping guarantees.

### Before → After (minimal fix)

**Before** (`crates/canopy/src/core/style/mod.rs`, `StyleManager::pop`):

```rust
pub fn pop(&mut self) {
    if self.level != 0 {
        if self.layer_levels.last() == Some(&self.level) {
            self.layers.pop();
            self.layer_levels.pop();
        }
        self.level -= 1
    }
}
```

**After**:

```rust
pub fn pop(&mut self) {
    if self.level != 0 {
        while self.layer_levels.last() == Some(&self.level) {
            self.layers.pop();
            self.layer_levels.pop();
        }
        self.level -= 1;
    }
}
```

### Add a targeted regression test

In `core/style/mod.rs` (same module so it can see privates), add:

```rust
#[test]
fn pop_pops_all_layers_at_level() {
    let mut sm = StyleManager::default();
    sm.reset();
    sm.push(); // enter level 1

    sm.push_layer("button");
    sm.push_layer("selected");

    sm.pop(); // leave level 1

    assert!(sm.layers.is_empty());
    assert_eq!(sm.layer_levels, vec![0]);
}
```

### Ergonomics follow-up: no additional API

With `pop()` draining all layers at the current level, the traversal already provides the
scoping guarantee. Keeping `push_layer` avoids extra API surface without losing safety.

---

## 2) Expand tab handling in `Text` (correctness + UX)

### What’s wrong

`Text` wraps by calling `textwrap::wrap(&self.raw, width)` and slices by calling
`text::slice_by_columns` on the wrapped lines. There is no tab expansion step, while other
widgets do handle tabs (`input` has `expand_tabs`, and the editor has `tab_width` helpers).
The project TODO also notes that the pager example misbehaves on special characters (tabs are
called out as a likely culprit), which fits this path.

### Recommendation

Add a `tab_stop` field to `Text` (default 4 or 8) and expand `\t` into spaces before wrapping,
measuring, and slicing. Consider moving the tab expansion helper into `canopy::text` so `Text`
and `input` can share it.

---

## 3) Consolidate backend lifecycle and remove global side effects from `runloop`

### What’s wrong

`core/backend/crossterm.rs::runloop` manually enables raw mode, enters the alternate screen,
installs a global panic hook, and handles Ctrl+C by dumping the tree and calling `exit(130)`.
At the same time, there is a `CrosstermControl` implementing `BackendControl`, but `runloop`
mostly bypasses it. This duplicates responsibilities and forces global side effects onto all
library users.

### Recommendation

1. Centralize terminal session lifecycle in a single owned guard:

   - `TerminalSession::new()` calls `start()`.
   - `Drop` calls `stop()`.

2. Make side effects optional in `runloop`:

   - opt-in panic hook installation,
   - configurable Ctrl+C behavior (or deliver as input and let bindings decide).

3. Ensure the same start/stop path is used by both:

   - the runloop itself,
   - `Context::exit` / `Context::stop`.

---

## 4) Detect duplicate command IDs and improve “no target” errors

### What’s wrong

- `CommandSet::add` inserts by `CommandId` into a `HashMap` and silently overwrites duplicates.
  `Canopy::add_commands` returns `()` so callers get no signal that two specs shared an ID.
- `commands::dispatch` returns `CommandError::UnknownCommand` both when a command ID is missing
  **and** when a node-routed command can’t find a matching target in the subtree/ancestors. This
  makes “not found” vs “no target node” indistinguishable.

### Recommendation

1. Change `CommandSet::add` to return `Result<()>` and error on duplicates.
2. Add `CommandError::DuplicateCommand` and `CommandError::NoTarget` (or similar), and update
   `dispatch` to return `NoTarget` for the “nothing matched” case.
3. Thread the `Result` through `Canopy::add_commands` so registration errors surface early.

---

## 5) Make command targeting explicit (fix ambiguity when multiple widgets share a name)

### What’s wrong

Node-routed commands dispatch by searching:

1. subtree of the start node (pre-order), then
2. ancestors

The first match wins. This is fragile when multiple nodes share a name (multiple `Text` nodes,
multiple `Editor`s, overlays/modals, etc.), and the outcome changes under tree refactors.

### Recommendation

Introduce an explicit dispatch policy on `CommandInvocation` so callers can request:

- `FirstInSubtreeThenAncestors` (current behavior),
- `NearestOnFocusPath`,
- `UniqueInSubtree` (error unless exactly one match),
- `ExactNodeOnly`.

Then implement `dispatch_with_policy` and a builder helper so bindings can opt in:

```rust
Binder::new(c).key(
    Key::Left,
    Editor::cmd_cursor_left().call().with_policy(DispatchPolicy::NearestOnFocusPath),
);
```

---

## 6) Upgrade input modes: add a mode stack and standard mode commands

### What’s wrong

`InputMap` stores a single `current_mode: String`. This makes modal UIs and nested temporary
modes awkward (command palettes, drag states, modal stacks, etc.).

### Recommendation

Replace `current_mode` with a `mode_stack: Vec<String>` where the top is active, and add
`push_mode`, `pop_mode`, and `set_mode` APIs. `pop_mode` should never pop the default base mode.

Add root-level commands (or generic commands) so scripts and bindings can manage modes:

- `root::push_mode("insert")`
- `root::pop_mode()`
- `root::set_mode("normal")`

---

## 7) Make bindings composable without scripting

### What’s wrong

`BindingTarget` is currently either `Script(ScriptId)` or `Command(CommandInvocation)`. Anything
beyond a single command forces users into scripts for sequencing or fallback behavior, which is
harder to refactor and less type-safe.

### Recommendation

Extend `BindingTarget` with composition primitives:

```rust
pub enum BindingTarget {
    Script(script::ScriptId),
    Command(CommandInvocation),
    Sequence(Vec<BindingTarget>),
    // Optional:
    // Try(Vec<BindingTarget>), // swallow CommandError::NoTarget (or similar)
}
```

Then execute recursively in `Canopy::key` / `Canopy::mouse`.

---

## 8) Add explicit `Style` overloads for `Render::fill` and `Render::text`

### What’s wrong

`Render::fill` and `Render::text` require a style name and go through `StyleManager`, even when
callers already have a concrete `Style`. Lower-level helpers (`put_cell`, `put_grapheme`) accept
`Style`, but there’s no higher-level equivalent.

### Recommendation

Add explicit overloads that accept `Style` and apply effects, and let the name-based versions
delegate to them. This makes dynamic styling easier without pre-registering style paths.

```rust
pub fn fill_style(&mut self, style: Style, rect: Rect, c: char) -> Result<()> { ... }
pub fn text_style(&mut self, style: Style, line: Line, txt: &str) -> Result<()> { ... }
```
