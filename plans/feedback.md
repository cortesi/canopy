## 1) Fix style layer popping (this is a correctness bug) and add layer scoping

### What’s wrong

`StyleManager` tracks “render recursion level” (`level`) and a stack of style layers (`layers` + `layer_levels`). Widgets frequently push more than one layer per node render (e.g. `Button` pushes `"button"` and then `"selected"`). However, `StyleManager::pop()` only removes **one** layer for the current level, which means the earlier layer(s) remain permanently on the stack until the next `reset()`. That leaks styling across siblings within the same frame.

This is severe: it can produce non-local styling bugs that are hard to debug.

### Recommendation

1. Fix `StyleManager::pop()` to pop **all** layers pushed at the current level.
2. (Ergonomics upgrade) Add an RAII “layer guard” API to make layer usage explicit and self-balancing.

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

### Ergonomics follow-up: layer scope

Right now widgets call `rndr.push_layer("x")` with no corresponding pop (they rely on traversal levels). That’s fine, but it makes it easy to create subtle bugs if you later change traversal semantics.

Add something like this to `Render`:

```rust
impl<'a> Render<'a> {
    pub fn with_layer<T>(&mut self, name: &str, f: impl FnOnce(&mut Self) -> T) -> T {
        self.push_layer(name);
        // relies on traversal pop fix; optional: add explicit pop_layer support instead
        f(self)
    }
}
```

Then widget code becomes:

```rust
rndr.with_layer("button", |r| {
    if self.selected {
        r.with_layer("selected", |r| { /* render */ });
    } else {
        /* render */
    }
});
```

This is not strictly required after the `pop()` fix, but it makes “style scope” obvious in widget code.

---

## 2) Enforce core invariants: focus and capture must always target a node attached to `root`

### What’s wrong

There are multiple ways to detach nodes from the tree (`detach_child`, `set_children`, etc.). Detaching a subtree can leave:

* `Core.focus` pointing to a node that is no longer reachable from `Core.root`.
* `Core.mouse_capture` pointing to an orphaned node.

This is not just theoretical: `widgets::List::remove` detaches nodes and adjusts selection, but does not re-focus the new selection. Worse, detached nodes retain stale `node.view` and `node.rect` because layout traversal doesn’t visit them, so `ensure_focus_visible()` can incorrectly treat a detached focus as “visible” and never fix it.

That can cause input to go to invisible widgets, keybindings to resolve against an empty path, and mouse capture to “stick” to dead UI.

### Recommendation

1. Introduce an explicit “attached to root” predicate (or reuse `is_ancestor(root, node)`).
2. Upgrade `ensure_focus_visible()` into `ensure_focus_valid()`:

   * focus must exist,
   * focus must be in root subtree,
   * focus must not be hidden,
   * focus view must be non-zero.
3. Call this invariant enforcer after **any** structural mutation:

   * `detach_child`, `set_children`, `mount_child`, `set_hidden`, and after a subtree removal API (see next section).
4. Apply the same invariant to `mouse_capture`.

### Concrete implementation sketch

Add to `Core` (in `core/world.rs`):

```rust
fn is_attached_to_root(&self, node: NodeId) -> bool {
    self.is_ancestor(self.root, node)
}

fn ensure_focus_valid(&mut self) {
    let Some(focus) = self.focus else { return };

    let attached = self.nodes.contains_key(focus) && self.is_attached_to_root(focus);
    let visible = attached
        && self.nodes.get(focus).is_some_and(|n| !n.hidden && !n.view.is_zero());

    if visible {
        return;
    }

    if let Some(target) = self.first_focusable(self.root) {
        self.set_focus(target);
    } else {
        self.focus = None;
    }
}

fn ensure_mouse_capture_valid(&mut self) {
    if let Some(c) = self.mouse_capture {
        if !self.nodes.contains_key(c) || !self.is_attached_to_root(c) {
            self.mouse_capture = None;
        }
    }
}
```

Then call these in structural methods:

* `detach_child` after detaching
* `set_children` after reassignment
* `set_hidden` already calls `ensure_focus_visible`; switch to the stronger one
* anywhere else children/parents can change

### Before → After (List correctness fix)

In `List::remove`, after selecting the next item, also focus it:

**Before**:

```rust
self.update_selection(ctx, new_sel);
```

**After**:

```rust
self.update_selection(ctx, new_sel);
self.focus_selected(ctx);
```

Still do the Core-level invariant fix; the list-level focus fix preserves the more specific “list semantics” (focus stays on selected item) instead of falling back to “first focusable anywhere in app”.

---

## 3) Add subtree removal (drop nodes), and stop “detaching as deletion” from leaking memory and state

### What’s wrong

There is no way to delete nodes from the arena (`SlotMap`). Widgets can detach children, but detached nodes remain allocated and keep stale layout/view. If user code “removes” items by detaching and discarding the IDs, the arena grows unboundedly.

This is a design/ergonomics issue because it forces every user to invent a lifecycle convention (“detached means dead but still allocated”) and it’s easy to leak.

### Recommendation

Introduce first-class deletion APIs:

* `Core::remove_subtree(node_id: NodeId) -> Result<()>`
* `Context::remove_subtree(node_id: NodeId) -> Result<()>`
* Optional: `Core::remove_child(parent, child)` convenience

Also add a “drain but keep alive” concept explicitly if you want reuse:

* Keep `detach_child` for *reparenting/moving*.
* Use `remove_subtree` for *destroying*.

### Implementation plan (LLM-implementable)

1. Collect subtree ids (preorder) then remove in reverse (postorder) so children are removed first.
2. Ensure you detach from parent’s `children` list if attached.
3. Clean up focus and mouse capture if they point into the removed subtree.
4. After removal, call `ensure_focus_valid()` and `ensure_mouse_capture_valid()`.

Example implementation skeleton:

```rust
pub fn remove_subtree(&mut self, node_id: NodeId) -> Result<()> {
    if node_id == self.root {
        return Err(Error::Invalid("cannot remove root".into()));
    }
    if !self.nodes.contains_key(node_id) {
        return Ok(());
    }

    // Detach from parent if needed
    if let Some(parent) = self.nodes.get(node_id).and_then(|n| n.parent) {
        self.detach_child(parent, node_id)?;
    }

    // Gather nodes
    let mut stack = vec![node_id];
    let mut ids = Vec::new();
    while let Some(id) = stack.pop() {
        if let Some(n) = self.nodes.get(id) {
            stack.extend(n.children.iter().copied());
        }
        ids.push(id);
    }

    // Clear focus/capture if they’re inside this subtree
    if let Some(f) = self.focus {
        if ids.contains(&f) || self.is_ancestor(node_id, f) {
            self.focus = None;
        }
    }
    if let Some(c) = self.mouse_capture {
        if ids.contains(&c) || self.is_ancestor(node_id, c) {
            self.mouse_capture = None;
        }
    }

    // Remove in reverse (children before parents)
    for id in ids.into_iter().rev() {
        self.nodes.remove(id);
    }

    self.ensure_focus_valid();
    self.ensure_mouse_capture_valid();
    Ok(())
}
```

### API design follow-up for `List`

Right now `List::remove` returns the detached ID. That implicitly chooses “detach as delete”.

A better split:

* `take(index) -> Option<TypedId<W>>` (detach and return for reuse)
* `remove(index) -> bool` (destroy and don’t leak)

Example:

```rust
pub fn take(&mut self, ctx: &mut dyn Context, index: usize) -> Result<Option<TypedId<W>>> { ... }

pub fn remove(&mut self, ctx: &mut dyn Context, index: usize) -> Result<bool> {
    let Some(id) = self.take(ctx, index)? else { return Ok(false); };
    ctx.remove_subtree(id.into())?;
    Ok(true)
}
```

---

## 4) Make command dispatch strict-by-default (don’t silently succeed on “no target”) and detect duplicate commands

### What’s wrong

* `commands::dispatch(...) -> Result<Option<ReturnValue>>` returns `Ok(None)` when no node receives the command. Many call sites ignore the `Option` and treat “no-op” as success.
* `CommandSet::add` silently overwrites duplicates (`HashMap::insert`), which can happen via `NodeName` collisions or repeated loads.

This reduces correctness and makes debugging keybindings/scripts painful: you can bind a key to a command that never runs and get no signal.

### Recommendation

1. Introduce a strict variant: `commands::dispatch_required(...) -> Result<ReturnValue>` (or `Result<()>` for void) that errors if no target is found.
2. Update key/mouse binding execution and script wrappers to use strict dispatch by default (configurable if you need optional behavior).
3. Change `CommandSet::add` to return `Result<()>` and error on duplicates (or at least log with enough context to fix).

### Concrete changes

**New error variant** in `core/error.rs`:

```rust
#[error("command not dispatched: {0}")]
CommandNotDispatched(String),

#[error("duplicate command: {0}")]
DuplicateCommand(String),
```

**New helper** in `core/commands.rs`:

```rust
pub fn dispatch_required(
    core: &mut Core,
    id: NodeId,
    cmd: &CommandInvocation,
) -> Result<ReturnValue> {
    dispatch(core, id, cmd)?.ok_or_else(|| Error::CommandNotDispatched(cmd.fullname()))
}
```

**Update Canopy.key / Canopy.mouse** to call `dispatch_required` for `BindingTarget::Command`:

```rust
BindingTarget::Command(cmd) => {
    commands::dispatch_required(&mut self.core, nid, &cmd)?;
    changed = true;
}
```

**Detect duplicates** in `CommandSet::add`:

```rust
pub fn add(&mut self, cmds: &[CommandSpec]) -> Result<()> {
    for cmd in cmds {
        let key = (cmd.node.clone(), cmd.command.clone());
        if self.cmds.contains_key(&key) {
            return Err(Error::DuplicateCommand(cmd.fullname()));
        }
        self.cmds.insert(key, cmd.clone());
    }
    Ok(())
}
```

Then propagate through `Canopy::add_commands::<T>() -> Result<()>` (breaking change but worth it).

---

## 5) Expand the command system beyond `isize` (huge power/ergonomics win)

### What’s wrong

The derive macro currently supports:

* args: `&mut dyn Context`, plus zero or more `isize`
* returns: `()`, `String`, `Result<()>`, `Result<String>`

This is a major limitation. Many useful commands need:

* `bool` (toggle)
* `String` (set text, open file)
* `usize/u32` (indices)
* `f64` (zoom scale)
* structured options (eventually)

It also forces awkward workarounds: invent commands that only take integers, or rely on global state in the widget.

### Recommendation

1. Extend `ArgTypes` and `Args` to cover at least: `Bool`, `Int`, `Float`, `String`.
2. Extend return values similarly: `Bool`, `Int`, `Float`, `String`, `Void`.
3. Upgrade `canopy-derive` to accept those types in `#[command]` methods.
4. Update the Rhai bridge to pass those types seamlessly.

### Concrete API design

In `core/commands.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgTypes {
    Context,
    Int,
    Bool,
    Float,
    String,
}

pub enum Args<'a> {
    Context(&'a mut dyn Context),
    Int(i64),
    Bool(bool),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReturnValue {
    Void,
    Int(i64),
    Bool(bool),
    Float(f64),
    String(String),
}
```

### Before → After: authoring a command

**Before (impossible today):**

```rust
#[derive_commands]
impl Text {
    #[command]
    pub fn set_raw(&mut self, _ctx: &mut dyn Context, text: String) {
        self.raw = text;
    }
}
```

**After (supported):**

```rust
#[derive_commands]
impl Text {
    #[command]
    pub fn set_raw(&mut self, _ctx: &mut dyn Context, text: String) {
        self.raw = text;
    }
}
```

Then:

* Script: `text::set_raw("hello")`
* Rust binding: `Binder::new(c).key(Key::Char('r'), Text::cmd_set_raw("hello"))`

### Make typed calls ergonomic (remove the `Vec<isize>` footgun)

Right now typed calls require `call_with(Vec<isize>)`, which is clunky and locks you into ints.

Instead, have the macro generate “fully-applied calls”:

**Current:**

```rust
Binder::new(c).key(Key::Char('x'), Editor::cmd_cursor_down().call());
```

**Proposed (same for 0-arg):**

```rust
Binder::new(c).key(Key::Char('x'), Editor::cmd_cursor_down());
```

**And for args:**

```rust
Binder::new(c).key(Key::Char('z'), ImageView::cmd_zoom_by(1.25));
Binder::new(c).key(Key::Char('t'), Text::cmd_set_raw("hello"));
```

Implementation: in `canopy-derive`, instead of generating `cmd_*() -> CommandRef<Self>`, generate `cmd_*(...) -> CommandInvocation` (or `CommandCall<Self>`). This avoids a second “call” step and lets the macro bake arguments into the invocation with the correct types.

### Rhai bridge implementation guidance

Today `ScriptHost::load_commands` uses hand-written closures for arity 0 or 1 int. You can generalize either by:

* **Preferred**: use Rhai’s “raw function” registration (if available in your Rhai version) to accept `&mut [Dynamic]` and convert based on `ArgTypes`.
* **Fallback**: generate closures for arities up to N (say 4) and for allowed primitive types.

Either way, you already have `CommandSpec.args` so you can:

1. Convert Rhai args into `Args` variants in order,
2. Insert `Args::Context(&mut ctx)` at position 0 if the command expects context,
3. `dispatch_required` and map `ReturnValue` back to Rhai `Dynamic`.

---

## 6) Make command targeting explicit (fix ambiguity when multiple widgets share a `NodeName`)

### What’s wrong

Command dispatch searches:

1. subtree of the “start node” (pre-order), then
2. ancestors

This is workable for simple trees, but becomes ambiguous fast:

* multiple `Text` nodes under the same container,
* multiple `Editor` instances,
* overlays/modals with duplicated widget types.

The current behavior (“first match in pre-order”) is not stable under tree refactors and is hard to reason about.

### Recommendation

Introduce an explicit dispatch policy for command invocations/bindings.

A minimal, high-value set:

* `NearestOnFocusPath`: prefer the focused node or its ancestors, then fallback to subtree
* `FirstInSubtree`: current behavior
* `UniqueInSubtree`: error if not exactly one match (great for debugging)
* `ExactNodeOnly`: only call if the start node matches the command’s node name

### Concrete design

Add to `CommandInvocation`:

```rust
pub enum DispatchPolicy {
    FirstInSubtreeThenAncestors,
    NearestOnFocusPath,
    UniqueInSubtree,
    ExactNodeOnly,
}

pub struct CommandInvocation {
    pub node: NodeName,
    pub command: String,
    pub args: Vec<ArgTypes>,
    pub policy: DispatchPolicy,
}
```

Default it to `FirstInSubtreeThenAncestors` for backward compatibility.

Then implement a new dispatcher:

```rust
pub fn dispatch_with_policy(core: &mut Core, start: NodeId, cmd: &CommandInvocation) -> Result<ReturnValue>;
```

### Example: disambiguating editor commands

**Before (ambiguous):**

```rust
Binder::new(c).key(Key::Left, Editor::cmd_cursor_left());
```

**After (stable):**

```rust
Binder::new(c).key(
    Key::Left,
    Editor::cmd_cursor_left().policy(DispatchPolicy::NearestOnFocusPath),
);
```

Where `.policy(...)` is a helper on `CommandInvocation` (or `CommandCall<T>`).

---

## 7) Upgrade input modes: add a mode stack and standard mode commands

### What’s wrong

`InputMap` has a single `current_mode: String`. Many TUIs need:

* modal editing (vi-like),
* transient “command palette” mode,
* temporary capture modes (dragging, selection),
* nested modals (open modal inside modal).

A single string mode forces every widget to coordinate globally and makes nested modals awkward.

### Recommendation

Replace `current_mode: String` with `mode_stack: Vec<String>`:

* top of stack is active mode
* `push_mode`, `pop_mode`, `set_mode` APIs
* `pop_mode` never pops the default base mode

Then add `Root` commands (or generic commands) so scripts and bindings can manage it:

* `root::push_mode("insert")`
* `root::pop_mode()`
* `root::set_mode("normal")`

### Concrete changes

In `InputMap`:

```rust
pub struct InputMap {
    modes: HashMap<String, InputMode>,
    mode_stack: Vec<String>, // base is DEFAULT_MODE
}
```

Add:

```rust
pub fn push_mode(&mut self, mode: impl Into<String>) { ... }
pub fn pop_mode(&mut self) { ... }
pub fn current_mode(&self) -> &str { ... }
```

Update `resolve_match` to consult:

1. active mode,
2. fallback to default mode (same as today)

This is a contained change with large ergonomic payoff.

---

## 8) Make bindings composable without scripting (reduce Rhai dependence, improve type safety)

### What’s wrong

`BindingTarget` is either `Script(ScriptId)` or `Command(CommandInvocation)`. Anything beyond “one command” forces users into a script string:

* sequences of commands,
* “try command else fallback”,
* conditional behavior based on state.

This pushes a lot of app logic into strings, which hurts refactoring and tooling.

### Recommendation

Extend `BindingTarget` with composition primitives:

```rust
pub enum BindingTarget {
    Script(script::ScriptId),
    Command(CommandInvocation),
    Sequence(Vec<BindingTarget>),
    // Optional:
    // Try(Vec<BindingTarget>), // swallow CommandNotDispatched errors
}
```

Then execute recursively in `Canopy::key` / `Canopy::mouse`.

### Before → After example

**Before (needs Rhai):**

```rust
Binder::new(c).key(Key::Char('j'), "list::select_by(1); editor::cursor_down()");
```

**After (typed, refactorable):**

```rust
use canopy::BindingTarget;

Binder::new(c).key(
    Key::Char('j'),
    BindingTarget::Sequence(vec![
        BindingTarget::Command(List::<Text>::cmd_select_by().call_with([1]).invocation()),
        BindingTarget::Command(Editor::cmd_cursor_down()),
    ]),
);
```

This also makes it feasible to compile the library without `rhai` for “pure Rust bindings” use cases.

---

## 9) Add child/node ergonomics: `Child<T>` handles + “ensure child” utilities

### What’s painful today

Many widgets store `Option<TypedId<...>>` for children and do manual “ensure_tree” logic (create orphans, mount them, keep IDs, etc.). This is workable but verbose and repetitive, and it scatters tree invariants across widget code.

### Recommendation

Provide a small “child handle” utility in core (or widgets) that standardizes this pattern:

```rust
pub struct Child<T: Widget + 'static> {
    id: Option<TypedId<T>>,
}

impl<T: Widget + 'static> Child<T> {
    pub fn ensure(&mut self, ctx: &mut dyn Context, build: impl FnOnce() -> T) -> Result<TypedId<T>>;
    pub fn id(&self) -> Option<TypedId<T>>;
    pub fn with_mut<R>(&self, ctx: &mut dyn Context, f: impl FnOnce(&mut T, &mut dyn Context) -> Result<R>) -> Result<R>;
}
```

Then widgets like `Button` become much smaller and less error-prone.

### Before → After sketch (Button)

**Before** (pattern repeated across widgets):

```rust
pub struct Button {
    box_id: Option<TypedId<widgets::Box>>,
    center_id: Option<TypedId<Center>>,
    text_id: Option<TypedId<Text>>,
    // ...
}
```

**After**:

```rust
pub struct Button {
    box_: Child<widgets::Box>,
    center: Child<Center>,
    text: Child<Text>,
    // ...
}
```

And `ensure_tree` becomes:

```rust
fn ensure_tree(&mut self, ctx: &mut dyn Context) -> Result<()> {
    let box_id = self.box_.ensure(ctx, || widgets::Box::new())?;
    let center_id = self.center.ensure(ctx, || Center::new())?;
    let text_id = self.text.ensure(ctx, || Text::new(&self.label))?;

    ctx.mount_child_to(box_id.into(), center_id.into())?;
    ctx.mount_child_to(center_id.into(), text_id.into())?;
    ctx.mount_child(box_id.into())?;
    Ok(())
}
```

This is not “declarative UI”, but it meaningfully improves ergonomics and consistency.

---

## 10) Consolidate backend lifecycle and remove global side effects from `runloop`

### What’s wrong

`core/backend/crossterm.rs::runloop`:

* manually starts/stops the terminal state, **and**
* also installs a `BackendControl` in `Core`.

That creates duplication and confusion about the “one true owner” of terminal state. It also sets a global panic hook and installs `color_backtrace` unconditionally, which is intrusive for a library.

### Recommendation

1. Centralize terminal session lifecycle in a single owned guard:

   * `TerminalSession::new()` calls `start()`
   * `Drop` calls `stop()`
2. `runloop` should be a thin coordinator and should:

   * not set global panic hooks by default,
   * make ctrl-c behavior configurable (or just deliver it as an input event and let bindings decide).
3. Ensure the same start/stop path is used by both:

   * the runloop itself,
   * `Context::exit` / `Context::stop`.

### Concrete design sketch

```rust
pub struct RunLoopOptions {
    pub install_panic_hook: bool,
    pub ctrl_c_behavior: CtrlCBehavior,
}

pub fn runloop(mut cnpy: Canopy, opts: RunLoopOptions) -> Result<()> {
    let mut backend = CrosstermBackend::new();
    let _session = TerminalSession::new(&mut backend)?;
    cnpy.register_backend(Box::new(CrosstermControl::new(/*...*/)));
    // ...
}
```

---

## 11) Low-hanging rendering ergonomics: accept explicit `Style` for text/fill, and fix tab handling for `Text`

### What’s missing

`Render::text` and `Render::fill` require a `style_name: &str`. If you want dynamic colors, you either:

* pre-register style paths into `StyleMap`, or
* use effects (not always appropriate).

You already support explicit `Style` in `put_cell`/`put_grapheme`; extend that to higher-level helpers.

Also, the TODO mentions pager issues with special characters (tabs are a likely culprit). The `Text` widget currently wraps/slices without tab expansion.

### Recommendation A: add `*_style` overloads

In `core/render.rs`:

**After:**

```rust
pub fn fill_style(&mut self, style: Style, rect: Rect, c: char) -> Result<()> {
    self.dest_buf.fill(style, rect, c);
    Ok(())
}

pub fn fill(&mut self, style_name: &str, rect: Rect, c: char) -> Result<()> {
    let style = self.resolve_style_name(style_name);
    self.fill_style(style, rect, c)
}

pub fn text_style(&mut self, style: Style, line: u32, txt: &str) -> Result<()> {
    self.dest_buf.text(style, line, txt)?;
    Ok(())
}

pub fn text(&mut self, style_name: &str, line: u32, txt: &str) -> Result<()> {
    let style = self.resolve_style_name(style_name);
    self.text_style(style, line, txt)
}
```

### Recommendation B: tab expansion for `Text`

Add an option to `Text`:

```rust
pub struct Text {
    tab_stop: usize, // default 4 or 8
    // ...
}
```

Then in wrap/slice logic, expand `\t` to spaces (like the editor already models via `tab_stop`). This will fix a class of rendering/wrapping inconsistencies without users reinventing it.

---

## 12) Practical next steps: a high-leverage sequence of changes

### Phase 0: correctness + invariant hardening (do these first)

1. Fix `StyleManager::pop()` to pop all layers at a level (+ regression test).
2. Implement `Core::ensure_focus_valid()` and `ensure_mouse_capture_valid()`.
3. Call these invariant checks after detaches/child reorders/hide/show.
4. Fix `List::remove` to refocus selection.

### Phase 1: lifecycle and deletion

5. Implement `remove_subtree` APIs in `Core` + `Context`.
6. Audit widgets that “remove” nodes (lists, modals, inspector panes) to use deletion where appropriate or to offer `take` vs `remove`.

### Phase 2: power + ergonomics unlocks

7. Make command dispatch strict by default and detect duplicate commands.
8. Expand command arg/return types (`bool`, `string`, `float`, `int`) and adjust `canopy-derive` + Rhai bridge.
9. Add command dispatch policies to resolve ambiguity.

### Phase 3: developer experience

10. Mode stack for input, plus standard mode commands.
11. Composable binding targets (sequence, try).
12. Child handles (`Child<T>`) to reduce boilerplate.

If you want, I can go one level deeper and produce a concrete “patch plan” by file (exact function signatures, data structure migrations, and mechanical edits needed across the codebase) for the top 3 phases.
