# Luau Scripting Refactoring Plan

Replace the Rhai scripting engine with Luau (via mlua), expose the complete application API surface
as a dynamically-rendered `d.luau` definition file, and use that definition as the single source of
truth for type-checked scripts, MCP-driven automation, and smoke tests.


## Goals

1. **Complete API surface**: Every canopy app can render a single `d.luau` file that describes its
   entire scriptable surface — framework primitives plus app-specific commands. Models, humans, and
   the type checker all consume the same file.
2. **Strict Luau**: All scripts run in strict mode. During development and CI, type errors are
   caught before execution via `luau-analyze`. The `--api` dump provides the definition file
   that editors and external tooling can also check against.
3. **Dynamic definition rendering**: The `d.luau` is rendered at runtime from registered
   `CommandSpec` metadata plus a static framework preamble. Different apps produce different
   definitions.
4. **Declarative key bindings**: Key binding setup moves from Rust builder calls to Luau setup
   scripts that reference typed command modules.
5. **MCP connector**: A standard (but optional) MCP server exposes `script_eval` and `script_api`
   tools, following the eguidev pattern.
6. **Smoke tests**: A test runner discovers `.luau` scripts and executes them against a live app
   instance, using assertions to verify behavior.


## Architecture

```
                        ┌──────────────────────────────────────┐
                        │           canopy crate               │
                        │                                      │
                        │  ┌────────────┐   ┌───────────────┐  │
                        │  │ CommandSet │──▶│ d.luau render │  │
                        │  │ (specs)    │   │               │  │
                        │  └────────────┘   └───────┬───────┘  │
                        │                           │          │
                        │  ┌────────────┐   ┌───────▼───────┐  │
                        │  │ LuauHost   │◀──│ definitions   │  │
                        │  │ (mlua)     │   │ (d.luau text) │  │
                        │  └─────┬──────┘   └───────────────┘  │
                        │        │                             │
                        │        │ dispatches                  │
                        │        ▼                             │
                        │  ┌────────────┐                      │
                        │  │ Core       │                      │
                        │  │ (arena,    │                      │
                        │  │  commands, │                      │
                        │  │  focus)    │                      │
                        │  └────────────┘                      │
                        └──────────────────────────────────────┘
                                        │
            ┌───────────────────────────┼───────────────────────────┐
            │                           │                           │
   ┌────────▼─────────┐    ┌───────────▼──────────┐    ┌──────────▼──────────┐
   │  Setup scripts   │    │  canopy-mcp crate     │    │  Smoke test runner  │
   │  (key bindings,  │    │  (optional)           │    │  (discovers .luau   │
   │   config)        │    │                       │    │   scripts, runs     │
   │                  │    │  ┌─────────────────┐  │    │   them against app) │
   │  Run once at     │    │  │ script_eval     │  │    │                     │
   │  app init after  │    │  │ script_api      │  │    │  Uses same LuauHost │
   │  Loader::load()  │    │  │ (MCP tools)     │  │    │  + d.luau surface   │
   └──────────────────┘    │  └─────────────────┘  │    └─────────────────────┘
                           │                       │
                           │  ┌─────────────────┐  │
                           │  │ Launcher (opt.)  │  │
                           │  │ lifecycle mgmt   │  │
                           │  └─────────────────┘  │
                           └───────────────────────┘
```

**Key principles:**

- The `d.luau` is rendered *after* all `Loader::load()` calls complete, when the full `CommandSet`
  is known.
- `LuauHost` replaces `ScriptHost` (Rhai). It owns an `mlua::Lua` instance configured in sandbox +
  strict mode.
- Framework functions (focus, tree queries, input simulation) are registered as Luau globals.
- Per-widget-owner command modules are registered as Luau global tables (e.g., `todo.enter_item()`).
- The same d.luau serves all consumers: setup scripts, MCP callers, smoke tests, and
  `luau-analyze`.


## Widget Registration and App Lifecycle

### The Problem

The widget tree is dynamic — nodes are created and destroyed at runtime. A todo app starts empty and
populates its list lazily in `poll()`. But the *types* of widgets the app will ever use are known
statically. To render the complete d.luau, every widget type that could appear in the tree must
pre-register its commands before the API surface is finalized.

### The Existing Mechanism: `Loader::load()`

The `Loader` trait is the existing registration point. Each widget type that has commands implements
`Loader`, and calls `canopy.add_commands::<Self>()` plus recursively loads any widget types it
depends on:

```rust
impl Loader for Todo {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Todo>()?;           // This widget's commands
        c.add_commands::<List<TodoEntry>>()?; // Widgets it will create dynamically
        c.add_commands::<Input>()?;           // More dynamic widgets
        Ok(())
    }
}
```

`Root::load()` also chains into framework widgets (`Inspector`, `Help`). The root loader thus
transitively registers everything the framework provides. The app's root loader registers everything
the app provides. Together, the full `CommandSet` is populated before any widget instance exists.

This mechanism already works correctly for our purposes. The key rule is: **if a widget type has
commands, its `Loader::load()` must be called at startup, even if no instance of that widget exists
yet.** This is already the established convention — every `Loader` implementation registers its own
type plus all widget types it will ever instantiate dynamically.

**Generic widget types.** The `owner_name` derivation uses only the struct name (e.g., `List<W>` →
`"list"`), so multiple instantiations of the same generic type (`List<TodoEntry>`,
`List<LogEntry>`) share the same owner name and command set. `CommandSet::add()` rejects duplicate
command IDs, so only the first `add_commands::<List<T>>()` call succeeds — subsequent calls for
different `T` should be silently deduplicated (skip if already registered). This is already the
case in practice: each app has only one `List<T>` specialization, and the commands are identical
regardless of `T`. If an app ever needs multiple generic instantiations, the command set is shared
and dispatch resolves to the correct instance via the focus-relative subtree search.

### The App Lifecycle

The app lifecycle has well-defined phases. The d.luau is rendered at the boundary between setup and
execution:

```
Phase 1: Construction
    Canopy::new()                      Create empty canopy with LuauHost

Phase 2: Registration
    Root::load(&mut cnpy)?             Register framework widget commands
    <App as Loader>::load(&mut cnpy)?  Register app widget commands
    style(&mut cnpy)                   Set up styles (orthogonal to scripting)

Phase 3: API Finalization
    cnpy.finalize_api()                Render d.luau from complete CommandSet
                                       Register command modules as Luau globals
                                       Enable sandbox
                                       Cache the definitions text

Phase 4: Binding Setup
    DefaultBindings applied            Root, Inspector, Help bind via key_command()
    cnpy.run_default_script(SCRIPT)?   Run the app's built-in binding script
    cnpy.run_config(path)?             Run user's config file (bindings + hooks)

Phase 5: Widget Tree Construction
    let root_widget = App::new()?      Create root widget instance
    Root::install(&mut cnpy, root_id)  Install into tree

Phase 6: First Render and Runtime
    runloop(cnpy)?                     Drive the first render
    ├─ First successful Canopy::render() completes
    ├─ Runloop drains on_start hooks (tree is live, full API available)
    ├─ If hooks mutate state, an immediate follow-up render runs
    └─ Normal event processing / later renders (bindings can change at any time)
```

`finalize_api()` is the key transition. Before it, `add_commands()` calls accumulate specs in the
`CommandSet`. After it, the d.luau is rendered, command modules are registered in the Luau
runtime, and the sandbox is enabled. Calling `add_commands()` after `finalize_api()` is an error —
the API surface is sealed.

**Pre-finalization bindings.** The Rust `Binder` API's `key_command()` and `mouse_command()` methods
store `BindingTarget::Command(CommandInvocation)` directly — they never touch the script host. These
work at any point after command registration. Only Luau script compilation (`canopy.bind()` closures
and `eval_script()`) requires the API to be finalized, because the Luau runtime needs the command
modules registered as globals.

**DefaultBindings timing.** Today, `DefaultBindings` impls (Root, Help, Inspector) are applied when
`Binder::new(cnpy).defaults::<Root>()` is called from the app's `setup_bindings()` function. In
every existing example, this runs *after* all `Loader::load()` calls. Currently these use
`Binder::key()` with script strings like `"root::quit()"` — compiling Rhai immediately. As part of
Stage 1, all `DefaultBindings` implementations are migrated to `Binder::key_command()`, which stores
`BindingTarget::Command` without touching the script host. After migration, `DefaultBindings` can
run at any time after command registration — the natural place remains Phase 4, alongside the Luau
setup scripts, but they no longer depend on the script engine being ready.

**DefaultBindings migration required.** Today's `DefaultBindings` implementations (`Root`, `Help`,
`Inspector`) all use `Binder::key()` with script strings like `"root::quit()"` — not
`key_command()`. These compile Rhai scripts immediately, so they cannot survive as-is in the new
lifecycle where script compilation requires `finalize_api()` to have run. As part of Stage 1, these
must be migrated. The vast majority are single command calls that map 1:1 to
`Binder::key_command(Foo::cmd_bar().call())`. A few cases require different treatment:

- **`print()` calls** (cedit, focusgym, listgym): debug artifacts — remove or convert to `log()`.
- **Compound scripts** like `"selector::toggle(); stylegym::apply_effects()"` in stylegym (6
  compound bindings in `stylegym.rs:443-465`): these cannot be expressed as a single
  `key_command()`. To avoid splitting each example into two binding registration paths (Rust for
  simple + Luau for compound), add `Binder::key_commands()` accepting a `&[CommandCall]` sequence
  that stores a new `BindingTarget::CommandSequence(Vec<CommandInvocation>)`. This keeps compound
  bindings in Rust without requiring Luau. Alternatively, migrate all example bindings to Luau
  setup scripts consistently — but that's better done in Stage 4 when the Luau binding API exists.
  For Stage 1, `key_commands()` is the pragmatic choice.
- **Commands with args** like `"editor_gym::scroll_to(0, 0)"`: use
  `key_command(EditorGym::cmd_scroll_to().call_with([0u32, 0u32]))` (framegym already does this).

### `--api`: Dumping the API Definition

Every canopy app should support dumping its rendered d.luau. The framework provides the method;
the app wires it to a CLI flag:

```rust
impl Canopy {
    /// Return the rendered Luau API definition for this app.
    /// Panics if called before finalize_api().
    pub fn script_api(&self) -> &str { ... }
}
```

The app integrates this into its CLI:

```rust
#[derive(Parser)]
struct Args {
    /// Print the Luau API definition and exit
    #[arg(long)]
    api: bool,

    /// Path to a Luau config file
    #[arg(long, short)]
    config: Option<PathBuf>,

    path: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut cnpy = setup_app()?;  // phases 1-3

    if args.api {
        print!("{}", cnpy.script_api());
        return Ok(());
    }

    cnpy.run_default_script(DEFAULT_BINDINGS)?;  // phase 4a
    if let Some(config) = &args.config {
        cnpy.run_config(config)?;                 // phase 4b
    }

    let exit_code = runloop(cnpy)?;               // phases 5-6
    process::exit(exit_code);
}
```

This means `my-app --api > my-app.d.luau` produces a file that models, editors, and `luau-analyze`
can all consume. The `--api` and `--config` flags follow the same per-app wiring pattern as the
existing `--commands` flag. This is boilerplate, but the alternative (framework-level CLI parsing)
would constrain how apps structure their own argument handling.

### Config Files: Layered Binding Customization

Every canopy app naturally accepts a config file, which is simply a Luau script that executes in the
app context after the default bindings. It can override, clear, or extend any binding:

**Layer 1 — Default bindings** (embedded in the binary as a `&str` constant):
```luau
canopy.bind("q", function() root.quit() end)
canopy.bind("j", function() todo.select_by(1) end)
canopy.bind("k", function() todo.select_by(-1) end)
```

**Layer 2 — User config file** (loaded from disk, runs second):
```luau
-- Option A: Selective override — just rebind specific keys.
-- bind() on an already-bound key replaces the existing binding with the
-- same path filter, so no unbind_key() is needed for the common case.
canopy.bind("j", function() todo.select_by(5) end)   -- page by 5
canopy.bind("k", function() todo.select_by(-5) end)

-- Option B: Start fresh — clear all defaults and define everything.
-- clear_bindings() removes ALL bindings, including framework bindings
-- (inspector toggle, help toggle). The user must re-declare anything they
-- want to keep.
canopy.clear_bindings()
canopy.bind("q", function() root.quit() end)
canopy.bind("n", function() todo.select_by(1) end)
canopy.bind("p", function() todo.select_by(-1) end)

-- Option C: Extend — add bindings without removing any
canopy.bind("ctrl-x", function()
    todo.delete_item()
    canopy.log("deleted!")
end)
```

Config scripts run in Phase 4, before the widget tree is constructed in Phase 5. At that point they
can use **binding, hook, and configuration functions** (`canopy.bind()`, `canopy.on_start()`,
`canopy.clear_bindings()`, etc.) but **not** tree inspection or command dispatch
(`canopy.focused()`, `todo.enter_item()`, etc.) because there is no live tree yet. For startup
logic that needs the tree, config scripts register an `on_start` hook — a closure that fires once
after the first render completes, when the tree is fully constructed:

```luau
-- In a config file: set initial focus after the tree is ready
canopy.on_start(function()
    local ed = canopy.find_node("editor")
    if ed then
        canopy.set_focus(ed)
    end
end)
```

**Runtime binding changes.** Bindings can be added, removed, and replaced at any time — including
from within event handler closures. The `InputMap` supports this natively. This enables modal
editing, context-sensitive bindings, and dynamic plugin patterns:

```luau
-- Toggle between normal and "delete mode" at runtime
local delete_mode = false
canopy.bind("ctrl-d", function()
    if delete_mode then
        canopy.unbind_key("d")
        canopy.unbind_key("Escape")
        delete_mode = false
    else
        canopy.bind("d", function() todo.delete_item() end)
        canopy.bind("Escape", function()
            canopy.unbind_key("d")
            canopy.unbind_key("Escape")
            delete_mode = false
        end)
        delete_mode = true
    end
end)
```

Implementation:

```rust
impl Canopy {
    /// Evaluate a Luau source string in the app context.
    /// Available after finalize_api(). Named `eval_script` to avoid collision
    /// with the existing `run_script(node_id, script_id)` which executes a
    /// pre-compiled script by ID on a specific node.
    pub fn eval_script(&mut self, source: &str) -> Result<()> { ... }

    /// Evaluate a Luau script from a file path.
    pub fn run_config(&mut self, path: &Path) -> Result<()> {
        let source = std::fs::read_to_string(path)?;
        self.eval_script(&source)
    }

    /// Evaluate the app's built-in default bindings script.
    pub fn run_default_script(&mut self, source: &str) -> Result<()> {
        self.eval_script(source)
    }
}
```


## The d.luau API Surface

The rendered `d.luau` has two sections: a **static framework preamble** (checked into the canopy
crate, always the same) and a **dynamic command section** (rendered per-app from `CommandSpec`
metadata).

### Static Framework Preamble

```luau
--!strict

-- Canopy scripting API.
-- This file is the canonical reference for the app's complete scriptable surface.
-- It is dynamically rendered from the app's registered commands and the framework
-- built-ins.
--
-- Strict mode is always enforced. Use luau-analyze with this file as definitions
-- to type-check scripts before execution.
--
-- Framework functions live under the `canopy` namespace. App command modules are
-- bare globals named after their widget owner (e.g., `todo`, `editor`).

-- ===== Framework Types =====

--- Opaque node identifier. Backed by userdata wrapping the slotmap key.
--- Cannot be constructed from scripts; only obtained from framework functions.
declare class NodeId end

--- Information about a node in the widget tree.
export type NodeInfo = {
    id: NodeId,
    name: string,
    focused: boolean,
    on_focus_path: boolean,
    visible: boolean,
    children: {NodeId},
    rect: Rect?,
}

--- Geometry types
export type Point = { x: number, y: number }
export type Size = { w: number, h: number }
export type Rect = { x: number, y: number, w: number, h: number }

--- Options for bind_with / bind_mouse_with.
export type BindOptions = {
    mode: string?,
    path: string?,
    --- Explicit help label. Use this for bindings that should have a stable,
    --- human-readable help entry; otherwise the help system falls back to
    --- "script".
    desc: string?,
}

--- Mouse input specification for binding. Combo string with optional modifiers,
--- optional button, and required action. Grammar:
---   [modifiers "-"] [button] action
---   modifiers: "ctrl", "shift", "alt" (joined with "-")
---   button: "Left", "Right", "Middle"
---   action: "Down", "Up", "Drag", "Moved", "ScrollDown", "ScrollUp",
---           "ScrollLeft", "ScrollRight"
--- Examples: "ScrollUp", "LeftDown", "ctrl-LeftDown", "ctrl-shift-RightDrag"
export type MouseSpec = string

-- ===== Framework Namespace =====
-- All framework functions are accessed via the `canopy` table to avoid name
-- collisions with app command modules (which are bare globals).

declare canopy: {
    -- Tree Navigation
    root: () -> NodeId,
    focused: () -> NodeId?,
    node_info: (id: NodeId) -> NodeInfo,
    find_node: (pattern: string) -> NodeId?,
    find_nodes: (pattern: string) -> {NodeId},
    parent: (id: NodeId) -> NodeId?,
    children: (id: NodeId) -> {NodeId},

    -- Focus Management
    set_focus: (id: NodeId) -> boolean,
    focus_next: () -> (),
    focus_prev: () -> (),
    focus_dir: (dir: "Up" | "Down" | "Left" | "Right") -> (),

    -- Input Simulation
    send_key: (key: string) -> (),
    send_click: (x: number, y: number) -> (),
    send_scroll: (direction: "Up" | "Down", x: number, y: number) -> (),

    -- Generic Command Dispatch (dynamic escape hatch — prefer typed modules)
    cmd: (name: string, ...any) -> any,
    --- Dispatch a command against a specific node. Use this when multiple
    --- instances of the same owner type exist and the default focus-relative
    --- resolution would be ambiguous.
    cmd_on: (id: NodeId, name: string, ...any) -> any,

    -- Key Binding — handler is always a closure so the body is type-checked
    -- against the app's command modules by luau-analyze.
    -- bind() on an already-bound key with the same path filter replaces the
    -- existing binding, making overrides in config files zero-friction.
    bind: (key: string, handler: () -> ()) -> number,
    bind_with: (key: string, options: BindOptions, handler: () -> ()) -> number,

    -- Mouse Binding
    bind_mouse: (mouse: MouseSpec, handler: () -> ()) -> number,
    bind_mouse_with: (mouse: MouseSpec, options: BindOptions, handler: () -> ()) -> number,

    -- Binding Management
    unbind: (id: number) -> boolean,
    unbind_key: (key: string, options: BindOptions?) -> (),
    --- Remove ALL bindings, including framework bindings (inspector, help).
    --- Use in config files that want to define every binding from scratch.
    clear_bindings: () -> (),

    -- Lifecycle
    --- Register a hook that fires once after the first render, when the widget
    --- tree is fully constructed. Multiple hooks execute in registration order.
    on_start: (handler: () -> ()) -> (),

    -- Diagnostics
    log: (message: any) -> (),
    assert: (condition: boolean, message: string?) -> (),
}
```

**`NodeInfo.rect`** is nullable because a node's geometry is only known after layout. After the
first render, every visible node has a rect. The `ReadContext::view()` method provides this data
on the Rust side. Smoke tests and MCP scripts that need to verify layout or simulate clicks on
specific widgets use this field.

**`canopy.cmd_on(id, name, ...)`** provides targeted command dispatch against a specific node,
bypassing the focus-relative subtree search. This is the canonical way to address a specific widget
instance from eval scripts when multiple instances of the same owner type exist. See the "Command
dispatch context" section for details on when this is needed.

**No `canopy.quit()`.** Exit is handled exclusively through `root.quit()`, which is always present
since `Root` is a required framework widget. `Root::quit()` has layered dismiss logic (close help
first, then inspector, then exit with code 0), and a separate `canopy.quit()` with different
semantics would create confusing dual paths.

**Input string parsers.** The key and mouse combo string formats above (`"ctrl-s"`, `"PageDown"`,
`"ctrl-LeftDown"`, etc.) require new parsers — the current codebase has `Key` and `Mouse` types but
no string→type parser. These parsers are a prerequisite for `canopy.bind()` and `canopy.send_key()`
and must be implemented in Stage 3. The grammar follows the pattern established by eguidev's key
spec: optional modifiers joined by `"-"`, then the key/action name. Key names are case-insensitive
for multi-character names (`enter`, `ArrowUp`, `ESCAPE`), case-sensitive for single characters
(`a` ≠ `A`).

### Dynamic Command Section

Generated from `CommandSet` at runtime. Each widget owner becomes a declared global table whose
fields are functions matching the owner's commands.

**Naming policy.** Because framework functions live under the `canopy` namespace table, command
owner names are free to use bare globals without collision. The owner name from
`CommandDispatchKind::Node { owner }` is used directly as the Luau global name (e.g., `root`,
`todo`, `editor`). If an owner name happens to collide with a Luau keyword (`if`, `for`, etc.),
the renderer appends `_cmd` (e.g., `end_cmd`). This should be vanishingly rare since widget names
are nouns, not Luau keywords.

For the todo example app, this section would render as:

```luau
-- ===== Application Commands =====
-- Auto-generated from registered CommandSpecs.

--- Commands for widget "root"
declare root: {
    --- Quit the application.
    quit: () -> (),
}

--- Commands for widget "todo"
declare todo: {
    --- Toggle the add item modal.
    enter_item: () -> (),
    --- Delete the selected item.
    delete_item: () -> (),
    --- Accept the add operation.
    accept_add: () -> (),
    --- Cancel the add operation.
    cancel_add: () -> (),
    --- Select the first item.
    select_first: () -> (),
    --- Select item by delta offset.
    select_by: (delta: number) -> (),
    --- Page in a direction.
    page: (dir: "Up" | "Down" | "Left" | "Right") -> (),
}

--- Commands for widget "input"
declare input: {
    --- Move cursor left.
    left: () -> (),
    --- Move cursor right.
    right: () -> (),
    --- Delete character before cursor.
    backspace: () -> (),
}

--- Commands for widget "list"
declare list: {
    --- Select the next item.
    select_next: () -> (),
    --- Select the previous item.
    select_prev: () -> (),
}
```

**Generic widget types and the `list` table.** `List<TodoEntry>` and `List<LogEntry>` both derive
owner name `"list"` and share the same command set. The d.luau renders a single `declare list`
table. At runtime, `list.select_next()` dispatches to whichever `List` instance the focus-relative
search finds first. This is the correct behavior for bound scripts (the focused `List` handles the
command). For eval scripts where ambiguity matters, use `canopy.cmd_on(id, "list::select_next")`
with a specific node obtained via `canopy.find_node()`.

### Type Mapping

Rust types from `CommandTypeSpec.rust` are mapped to Luau types:

| Rust type                       | Luau type                                             |
| ------------------------------- | ----------------------------------------------------- |
| `bool`                          | `boolean`                                             |
| `i8..i64`, `isize`, `u8..u64`, `usize` | `number`                                      |
| `f32`, `f64`                    | `number`                                              |
| `String`, `&str`                | `string`                                              |
| `()`                            | `()`                                                  |
| `Option<T>`                     | `T?`                                                  |
| `Vec<T>`                        | `{T}`                                                 |
| `BTreeMap<String, T>`           | `{[string]: T}`                                       |
| `CommandEnum` variants          | String literal union (`"A" \| "B" \| "C"`)            |
| `CommandArg` (serde)            | Mapped structurally or falls back to `any`             |

To enable accurate mapping, we extend the command metadata:

1. **`CommandTypeSpec`** gains an optional `luau: Option<&'static str>` field.  When present, the
   renderer uses it verbatim instead of parsing the Rust type string.
2. **`CommandEnum` derive** auto-generates the Luau literal union string (e.g.,
   `"Next" | "Prev" | "Up" | "Down"`) and populates `luau`.
3. A fallback heuristic parses the Rust type string for primitive types, `Option`, `Vec`, and map
   types. **Struct-typed `CommandArg` parameters fall back to `any`.** Today `#[derive(CommandArg)]`
   is a marker trait with no field metadata, so the renderer cannot produce structural Luau type
   declarations for struct parameters. Extending the `CommandArg` derive to emit field descriptions
   (and generating Luau `export type` declarations from them) is a future enhancement — the initial
   implementation uses `any` and relies on doc comments to describe the expected shape.


## Scripting Examples

### Setup Script (key bindings)

```luau
--!strict

-- Every binding is a closure — the body is type-checked by luau-analyze
-- against the app's d.luau definition. A typo like `todo.selct_by(1)` is
-- caught at check time, not at runtime.
--
-- App setup scripts should always use bind_with() with desc so the help pane
-- shows readable labels. Luau closures are opaque — without desc, the help
-- pane falls back to "script".

canopy.bind_with("q", { desc = "Quit" }, function() root.quit() end)
canopy.bind_with("d", { desc = "Delete item" }, function() todo.delete_item() end)
canopy.bind_with("a", { desc = "Add item" }, function() todo.enter_item() end)
canopy.bind_with("g", { desc = "First item" }, function() todo.select_first() end)
canopy.bind_with("j", { desc = "Next item" }, function() todo.select_by(1) end)
canopy.bind_with("k", { desc = "Previous item" }, function() todo.select_by(-1) end)
canopy.bind_with("Down", { desc = "Next item" }, function() todo.select_by(1) end)
canopy.bind_with("Up", { desc = "Previous item" }, function() todo.select_by(-1) end)
canopy.bind_with("Space", { desc = "Page down" }, function() todo.page("Down") end)
canopy.bind_with("PageDown", { desc = "Page down" }, function() todo.page("Down") end)
canopy.bind_with("PageUp", { desc = "Page up" }, function() todo.page("Up") end)

-- Mouse bindings
canopy.bind_mouse("ScrollUp", function() todo.select_by(-1) end)
canopy.bind_mouse("ScrollDown", function() todo.select_by(1) end)

-- Input widget bindings
canopy.bind_with("Left", { path = "input", desc = "Cursor left" }, function()
    input.left()
end)
canopy.bind_with("Right", { path = "input", desc = "Cursor right" }, function()
    input.right()
end)
canopy.bind_with("Backspace", { path = "input", desc = "Delete char" }, function()
    input.backspace()
end)

canopy.bind_with("Enter", { path = "input", desc = "Confirm new item" }, function()
    todo.accept_add()
end)
canopy.bind_with("Escape", { path = "input", desc = "Cancel add" }, function()
    todo.cancel_add()
end)
```

### Runtime Script (MCP or smoke test)

```luau
--!strict

-- Add a todo item
todo.enter_item()
canopy.send_key("H")
canopy.send_key("e")
canopy.send_key("l")
canopy.send_key("l")
canopy.send_key("o")
todo.accept_add()

-- Verify it was added
local root_id = canopy.root()
local info = canopy.node_info(root_id)
canopy.assert(info.name == "root", "root node should be named 'root'")

-- Navigate
todo.select_by(1)
local f = canopy.focused()
canopy.assert(f ~= nil, "something should be focused")
```

### User Config File (overrides defaults)

```luau
--!strict

-- Override navigation to page by 5 — bind() replaces existing bindings
-- with the same key and path filter, so no unbind_key() needed
canopy.bind("j", function() todo.select_by(5) end)
canopy.bind("k", function() todo.select_by(-5) end)

-- Add a complex binding — use desc so help shows something readable
canopy.bind_with("ctrl-d", { desc = "Delete focused todo" }, function()
    local f = canopy.focused()
    if f then
        local info = canopy.node_info(f)
        if info.name == "todo_entry" then
            todo.delete_item()
        end
    end
end)
```

### User Config File (complete replacement)

```luau
--!strict

-- Start from scratch — this removes ALL bindings, including framework
-- bindings like inspector toggle and help toggle. You must re-declare
-- anything you want to keep.
canopy.clear_bindings()

-- Define everything from zero
canopy.bind("q", function() root.quit() end)
canopy.bind("n", function() todo.select_by(1) end)
canopy.bind("p", function() todo.select_by(-1) end)
canopy.bind("a", function() todo.enter_item() end)
canopy.bind("d", function() todo.delete_item() end)
canopy.bind_with("Enter", { path = "input" }, function() todo.accept_add() end)
canopy.bind_with("Escape", { path = "input" }, function() todo.cancel_add() end)
```

### Eval Script with Targeted Dispatch (multi-instance)

```luau
--!strict

-- When multiple widgets of the same type exist, use find_node + cmd_on
-- to target a specific instance
local main_list = canopy.find_node("todo/list")
local sidebar_list = canopy.find_node("sidebar/list")

if main_list then
    canopy.cmd_on(main_list, "list::select_next")
end
```


## Implementation

### Stage 1: Foundation — Replace Rhai with mlua

Replace the Rhai scripting engine with mlua configured for Luau in sandbox + strict mode. This is a
like-for-like replacement of the command dispatch mechanism and Core access pattern, with two
binding storage paths: `BindingTarget::Command` (unchanged) for Rust-side bindings and a new
`BindingTarget::LuauFunction` (a host-owned handle backed by an mlua registry entry) for Luau
closures.

1. [x] Add `mlua` dependency (features: `luau`, `serialize`, `vendored`) to canopy crate.
       Remove `rhai` and `scoped-tls` dependencies.
2. [x] Rewrite `crates/canopy/src/core/script.rs`:
       - `LuauHost` struct replacing `ScriptHost`: owns `mlua::Lua`, compiled script cache
         (`HashMap<ScriptId, mlua::Function>`), the definitions text, a closure registry keyed
         by `LuauFunctionId`, and a `finalized: bool` flag.
       - `LuauHost::new()`: creates `Lua::new_with(StdLib::ALL_SAFE, LuaOptions::default())`.
         Does **not** enable sandbox yet — sandbox freezes globals, preventing
         `register_commands()` from creating per-owner tables later. Registers only the `cmd()`,
         `cmd_on()`, `log()`, and `assert()` globals at this point.
       - `LuauHost::finalize(specs)`: registers per-owner command tables as Luau globals, then
         enables sandbox (`lua.sandbox(true)`). After this, no new globals can be created. Sets
         `finalized = true`.
       - `LuauHost::compile(source) -> Result<ScriptId>`: compiles strict-mode Luau
         (`--!strict\n` + source), caches the resulting function.
       - `LuauHost::execute(core, node_id, script_id) -> Result<()>`: sets up the scoped Core
         access via a thread-local `*mut Core` pointer (same pattern as the existing
         `SCRIPT_GLOBAL`), evaluates the cached function. See "Core access pattern" below for
         rationale.
       - `LuauHost::execute_value(core, node_id, script_id) -> Result<ArgValue>`: same as
         `execute` but captures and returns the script's final expression value, converted to
         `ArgValue`. Mirrors the existing `ScriptHost::execute_value()` which returns
         `Result<rhai::Dynamic>`. Needed by the MCP `script_eval` tool (Stage 6).
       - `LuauHost::register_commands(specs)`: for each owner, creates a global Luau table with
         one function per command. Each function converts mlua args to `CommandArgs`, dispatches
         through `commands::dispatch`, converts the result back to mlua values.
       - `LuauHost::release_function(id)`: decrements the live reference count for a registered
         closure. When the last reference disappears (for example from `unbind()`,
         `clear_bindings()`, or after a drained `on_start` hook runs), remove its
         `RegistryKey` immediately with `Lua::remove_registry_value()`. After bulk removals, call
         `Lua::expire_registry_values()` as a defensive sweep.

       **Core access pattern.** All Luau callbacks (command table functions, framework API
       functions, bind handler closures) need access to `&mut Core` at invocation time. We use the
       same thread-local `*mut Core` pattern as the existing Rhai `SCRIPT_GLOBAL`. mlua's
       `Lua::scope()` is the idiomatic safe alternative, but scoped functions cannot be stored
       in the registry — they borrow from the scope's lifetime. Since bind handlers are persistent
       closures stored as `RegistryKey`s, they cannot use `scope()`. Rather than maintaining two
       different access patterns (scope for one-shots, thread-local for persistent closures), we
       use the thread-local pattern uniformly. The unsafe is contained within `LuauHost::execute()`
       and the command dispatch functions, matching the existing safety boundary.

3. [x] Update all callers of the old `ScriptHost` API:
       - `Core` field type change.
       - `Canopy::bind_mode_key` and friends: compile scripts through `LuauHost`.
       - `Canopy::add_commands`: **remove** the eager `script_host.register_commands(cmds)` call.
         After this change, `add_commands()` only accumulates specs in `CommandSet`. Luau global
         registration happens later in `finalize_api()` → `LuauHost::finalize()`.
       - Test helpers in `testing/`.
4. [x] Redesign `BindingTarget` for the binding paths:
       - `BindingTarget::Command(CommandInvocation)` — unchanged, used by Rust `key_command()`.
       - `BindingTarget::CommandSequence(Vec<CommandInvocation>)` — new variant for compound
         Rust bindings that execute multiple commands in order. Used by `Binder::key_commands()`.
       - `BindingTarget::LuauFunction(LuauFunctionId)` — new variant for closures created by
         `canopy.bind()`. `LuauFunctionId` is a `Copy` newtype around `u64` — a stable handle
         into `LuauHost`'s function registry. This indirection is necessary because
         `mlua::RegistryKey` is not `Clone`, but `BindingTarget` must be `Clone` (it is cloned
         on every `resolve_match` lookup in `InputMap`). `LuauHost` owns the mapping from
         `LuauFunctionId` to a `StoredLuauFunction { key: RegistryKey, label: Option<String>,
         refs: usize }`.
       The old `BindingTarget::Script(ScriptId)` variant is removed. `ScriptId` remains only
       for `eval_script()` one-shot evaluation (compile source → cache → execute → discard).
5. [x] Add `Binder::key_commands()` and `Binder::mouse_commands()` methods accepting a
       `&[CommandCall]` sequence, storing `BindingTarget::CommandSequence`. This covers compound
       bindings (like stylegym's `"selector::toggle(); stylegym::apply_effects()"`) without
       requiring the Luau engine.
6. [x] Migrate all `DefaultBindings` implementations (`Root`, `Help`, `Inspector`) and example
       app `bind_keys()` / `setup_bindings()` functions from `Binder::key(script_string)` to
       typed Rust binding calls. Three cases:
       - **Simple command calls** (the majority — `Root`, `Help`, `Inspector` defaults, and most
         example bindings): map 1:1 to `Binder::key_command(Foo::cmd_bar().call())`.
       - **Compound scripts** (6 bindings in `stylegym.rs:443-465` like
         `"selector::toggle(); stylegym::apply_effects()"`): use the new
         `Binder::key_commands(&[Selector::cmd_toggle().call(), StyleGym::cmd_apply_effects().call()])`.
       - **Argument-bearing calls** (e.g., `"editor_gym::scroll_to(0, 0)"` in editorgym): use
         `key_command(EditorGym::cmd_scroll_to().call_with([0u32, 0u32]))` — framegym already
         demonstrates this pattern.
       - **Debug `print()` bindings** (cedit, focusgym, listgym): remove outright.
7. [x] Rewrite all script-based tests to Luau syntax. The current tests use Rhai's `::` dispatch
       syntax (e.g., `script_target::set(12)`, `list::select_next()`) and Rhai-specific constructs
       (`cmd_named("...", #{count: 7})`, `cmdv("...", [...])`). These become Luau dot-call syntax
       (`script_target.set(12)`, `list.select_next()`) and Luau table constructors
       (`canopy.cmd("script_target::set", {count = 7})`). Affected files:
       - `crates/canopy/tests/test_script_commands.rs` (command dispatch tests)
       - `crates/canopy-widgets/src/list.rs` (list widget tests)
       - `crates/examples/src/tests/listgym.rs` (listgym integration tests)
       - `crates/examples/src/tests/framegym.rs` (framegym integration tests)
       - `crates/canopy-widgets/src/root.rs` (root test helpers)
       - Any test calling `harness.script(...)` with Rhai syntax

### Stage 2: d.luau Rendering and API Finalization

Implement the machinery to render the complete `d.luau` from framework preamble + `CommandSet`,
and the `finalize_api()` transition that seals the API surface.

1. [x] Add `CommandTypeSpec::luau: Option<&'static str>` field. Default to `None` in existing
       code. Update `canopy-derive` codegen to populate it for `CommandEnum` types (render variant
       names as `"A" | "B" | "C"` union).
2. [x] Create `crates/canopy/src/core/script/defs.rs` (or a `defs` submodule):
       - `fn render_definitions(commands: &CommandSet) -> String`: renders the complete d.luau.
       - Static preamble is `include_str!("defs_preamble.luau")` — the framework declarations
         checked into the crate source tree.
       - Dynamic section iterates `CommandSet`, groups specs by owner, renders each owner as a
         `declare` table with typed function fields. Multiple generic instantiations sharing the
         same owner name produce a single table (the command sets are identical).
       - Type mapping function: `fn rust_type_to_luau(spec: &CommandTypeSpec) -> String` — uses
         `luau` field if present, falls back to heuristic parsing of `rust` string.
       - Include doc comments from `CommandDocSpec::short` as `---` prefixed lines.
3. [x] Create `crates/canopy/luau/preamble.d.luau` containing the static framework declarations
       (the "Static Framework Preamble" section from above).
4. [x] Implement `Canopy::finalize_api() -> Result<()>`:
       - Renders d.luau from the current `CommandSet` and caches the text.
       - Calls `LuauHost::finalize(specs)` to register per-owner command tables as Luau globals
         and enable the sandbox.
       - Marks the API as sealed — subsequent `add_commands()` calls return an error.
       - Must be called exactly once, after all `Loader::load()` calls and before any script
         execution.
5. [x] Implement `Canopy::script_api() -> &str` that returns the cached d.luau text. Panics
       if called before `finalize_api()`.
6. [x] Make `add_commands()` check the sealed flag and error if the API is already finalized.
       Also make `add_commands()` silently skip if all command IDs are already registered (to
       support multiple `add_commands::<List<T>>()` calls for different `T`).
7. [x] Write tests: render definitions for a test widget with known commands, assert the output
       contains expected Luau declarations. Verify that `add_commands()` after finalization fails.

### Stage 3: Framework API Functions

Expose the tree navigation, focus management, input simulation, and state inspection functions as
fields on the `canopy` Luau table. These are the functions declared in the static preamble.

1. [x] Implement `NodeId` as mlua userdata wrapping the slotmap `NodeId` key. Register it so
       Luau scripts receive and pass opaque node handles without serialization.
2. [x] Register tree navigation functions on the `canopy` table: `root()`, `focused()`,
       `node_info()`, `find_node()`, `find_nodes()`, `parent()`, `children()`. Each function
       accesses Core through the thread-local context. `node_info()` populates `NodeInfo.rect`
       from `ReadContext::view()` when layout data is available (nil before first render).

       **Root-relative semantics.** All `canopy.*` framework functions use root-relative behavior,
       not current-node-relative. Specifically, `find_node()` and `find_nodes()` search from the
       tree root (using `ReadContext` constructed with `root_id()`), not from
       `SCRIPT_GLOBAL.node_id`. This ensures consistent behavior regardless of whether a script
       runs as a bound handler (focused node context) or an eval script (root context). The
       underlying `ReadContext::find_node()` is relative to `self.node_id()`, so the Luau wrapper
       must explicitly construct a root-scoped context.

3. [x] Register focus management functions: `set_focus()`, `focus_next()`, `focus_prev()`,
       `focus_dir()`. These call the `*_global()` variants (`focus_next_global()`,
       `focus_prev_global()`, `focus_dir_global()`) which operate on the entire tree from root,
       not the current-node-relative defaults. This matches the principle that `canopy.*` functions
       provide consistent whole-tree behavior.
4. [x] Implement key and mouse combo string parsers: `parse_key("ctrl-s") -> Key` and
       `parse_mouse("ctrl-LeftDown") -> Mouse`. These are new code — the current codebase has
       `Key`/`Mouse` types but no string-to-type parsers. The grammar follows eguidev's pattern:
       optional modifiers joined by `"-"`, then the key/action name.
5. [x] Register input simulation functions: `send_key()`, `send_click()`, `send_scroll()`. These
       parse the combo string, synthesize `Event` values, and feed them through the event
       dispatch pipeline.
6. [x] Register `cmd_on(id, name, ...)` on the `canopy` table. This dispatches a command against
       a specific node ID instead of using the focus-relative subtree search. The implementation
       calls `commands::dispatch(core, node_id, &invocation)` with the provided node ID as the
       starting point.
7. [x] Register lifecycle function: `on_start()`. `on_start` accumulates closures in a `Vec` on
       the host. **Hook firing:** after the first successful `Canopy::render()` completes, the
       *caller* (runloop or test harness) drains and executes all registered `on_start` hooks in
       order, releases their `LuauFunctionId`s from `LuauHost`, and clears the list. Specifically:
       - `runloop_with_options()` drains hooks after the first render returns, before entering the
         event loop. If hooks mutate state, a follow-up render runs before event processing begins.
       - Test helpers drain hooks explicitly after calling `canopy.render()`, via a new
         `Canopy::drain_on_start_hooks()` method.
       This keeps `render()` itself pure (no side effects beyond rendering) and avoids reentrancy.
8. [x] Register diagnostic functions: `log()`, `assert()`. `log()` records to a `Vec<String>` on
       the host. `assert()` records pass/fail and throws on failure.
9. [x] Write integration tests: create a test widget tree, execute Luau scripts that call each
       framework function, verify correct behavior.

### Stage 4: Key Bindings in Luau and Config File Support

Move key binding setup from Rust `Binder` calls to Luau setup scripts. Add the layered config
file mechanism so users can override, extend, or replace default bindings.

1. [x] Register binding functions on the `canopy` table in `LuauHost`: `bind()`, `bind_with()`,
       `bind_mouse()`, `bind_mouse_with()`, `unbind()`, `unbind_key()`, `clear_bindings()`.
       - All binding functions take a **closure** as the handler. The closure body calls through
         the typed command modules (`root.quit()`, `todo.select_by(1)`, etc.), so luau-analyze
         type-checks the binding against the app's d.luau surface. No string-based command
         references — the type system covers the entire path from binding to dispatch.
       - **Replace semantics for `bind()`.** `bind(key, handler)` parses the key combo string.
         If an existing binding for the same key and path filter (default `""`) exists, it is
         replaced — the old closure is released. If no existing binding matches, a new one is
         created. This makes the common config override case (`canopy.bind("j", ...)`) work
         without a preceding `unbind_key()`. `bind_with()` with an explicit `path` option only
         replaces an existing binding with the *same* path filter, so path-scoped variants can
         still stack alongside the default.
       - `bind_mouse(mouse, handler)` / `bind_mouse_with(mouse, options, handler)` work
         identically for mouse specs, with the same replace semantics.
       - `unbind_key(key, options?)` removes all bindings for a key combo, optionally filtered
         by mode/path. Use this for targeted removal when replace-by-default isn't sufficient.
       - `clear_bindings()` removes **all** bindings — both Luau closures and Rust-side
         `BindingTarget::Command` entries, including framework bindings (inspector toggle, help
         toggle). This is intentional: a config file that calls `clear_bindings()` takes full
         ownership of the binding surface and must re-declare everything it wants. The
         "complete replacement" config example demonstrates this.
       - `unbind()`, `unbind_key()`, and `clear_bindings()` release removed `LuauFunctionId`s back
         to `LuauHost`; when the last reference disappears, the host removes the corresponding
         `RegistryKey` immediately and uses `expire_registry_values()` after bulk sweeps.
       - All bind functions return a `BindingId` (as number).

       **Help labels for closures.** `LuauHost`'s function registry stores each closure's
       `RegistryKey` alongside an `Option<String>` label. The label comes from `BindOptions.desc`
       when provided, otherwise falls back to `"script"`. The updated `binding_label()` function
       handles the new `BindingTarget::LuauFunction(id)` variant by looking up that stored label
       from `LuauHost`.

       Today, `help::binding_label()` derives meaningful labels from script source strings —
       for a binding like `"root::quit()"`, it extracts the command ID and looks up `doc.short`.
       Luau closures are opaque, so this introspection is lost. To maintain help pane quality:
       - **`DefaultBindings` (Root, Help, Inspector)** remain `BindingTarget::Command` after
         Stage 1 migration, so their labels continue to derive automatically from `CommandSpec.doc`.
       - **App setup scripts** (the Luau `DEFAULT_BINDINGS` constant) must use `bind_with()` with
         `desc` for every binding that should show a readable help label. This is a convention,
         not enforced — bindings without `desc` fall back to `"script"`.
       - **User config files** showing `"script"` for custom bindings is acceptable.
       The setup script examples below follow this convention, using `bind_with()` with `desc`
       throughout.
2. [x] Implement `Canopy::eval_script(source: &str) -> Result<()>` — evaluates a Luau source
       string in the app context. Works at any phase after `finalize_api()`. Named `eval_script`
       to avoid collision with the existing `run_script(node_id, script_id)` method.
       Also implement `Canopy::eval_script_value(source: &str) -> Result<ArgValue>` — same but
       captures and returns the script's final expression value. Used by MCP `script_eval`
       (Stage 6).
3. [x] Implement `Canopy::run_config(path: &Path) -> Result<()>` — reads a Luau file from disk
       and executes it via `eval_script()`.
4. [x] Implement `Canopy::run_default_script(source: &str) -> Result<()>` — convenience wrapper
       for running the app's embedded default binding script.
5. [x] Migrate the todo example's `bind_keys()` function to a Luau setup script embedded as a
       `const DEFAULT_BINDINGS: &str`. Update `main.rs` to follow the standard lifecycle:
       `finalize_api()` → `run_default_script()` → optional `run_config()` → `runloop()`.
6. [x] Keep the Rust `Binder` API available for widgets that set up bindings programmatically
       (e.g., `DefaultBindings` trait impls). After Stage 1 migration, all `DefaultBindings` use
       `key_command()` / `key_commands()`, which store `BindingTarget::Command` /
       `BindingTarget::CommandSequence` without touching the script host. These are applied in
       Phase 4 (binding setup) — the same phase as Luau setup scripts — but they could technically
       run any time after command registration.
7. [x] Write tests: set up bindings via Luau script, override them via a second script (config),
       simulate key presses, verify the overridden commands dispatch correctly. Test replace
       semantics: bind a key, bind again with same key, verify only the new handler fires.

### Stage 5: Type Checking with luau-analyze

Integrate `luau-analyze` for pre-execution type checking of scripts.

1. [x] Add `luau-analyze` as an optional dependency of the canopy crate (behind a feature flag,
       `typecheck`). The `luau-analyze` crate (github.com/cortesi/luau-analyze) wraps the
       Luau C++ analysis frontend and provides in-process type checking via `Checker::new()`,
       `add_definitions()`, and `check()`. Add it as a git dependency
       (`luau-analyze = { git = "...", optional = true }`) or publish to crates.io first. For
       local development, use a `[patch]` section or workspace-relative path override.

       The feature flag is justified despite the general guideline against feature gating:
       `luau-analyze` compiles the Luau C++ frontend, adding significant build time. Apps that
       just want to run pre-validated scripts shouldn't pay this cost. The `#[cfg]` surface is
       small — only the `check_script` method and its tests are gated.

2. [x] Add `LuauHost::check_script(source: &str) -> Result<CheckResult>` that:
       - Creates a `luau_analyze::Checker`.
       - Loads the rendered d.luau definitions via `checker.add_definitions()`.
       - Checks the script via `checker.check()`.
       - Returns structured diagnostics (errors and warnings with locations).
3. [x] Optionally call `check_script` before `compile` in debug/development builds. In release
       builds, skip type checking for performance (scripts are already validated during
       development).
4. [x] Expose type checking through the MCP `script_eval` tool (stage 6) — check before eval
       and return diagnostics in the response.
5. [x] Write tests: submit scripts with intentional type errors, verify diagnostics are returned
       with correct locations.

### Stage 6: MCP Integration

Add an optional MCP server for programmatic control of canopy apps. This follows the eguidev
pattern in spirit, but the final implementation keeps lifecycle management inside each canopy app
instead of introducing a separate generic launcher binary. The app embeds a minimal MCP server
with `script_eval` and `script_api`, and app-specific `mcp` / `smoke` subcommands provide the
automation entrypoints.

This is a new crate: `canopy-mcp`.

1. [x] Create `crates/canopy-mcp/` with dependencies on `canopy`, `tmcp`, `tokio`, and the
       serialization/schema support needed for automation responses.
2. [x] Implement the app-side MCP server (embedded in the canopy app process):
       - `script_eval` tool: accepts Luau source, evaluates via `Canopy::eval_script_value()`
         (Stage 4), returns JSON result with value, logs, assertions, timing.
       - `script_api` tool: returns the rendered d.luau text verbatim.
3. [x] Replace the planned standalone launcher with per-app subcommands:
       - `todo mcp <db>` serves stdio MCP from the app process.
       - `todo smoke <db> [scripts...]` runs the shared smoke runner against fresh app instances.
       - This keeps the automation lifecycle close to app-specific setup/config handling.
4. [x] Define `ScriptEvalRequest` and `ScriptEvalOutcome` types for the MCP surface:
       - Request: `script`, `timeout_ms`.
       - Outcome: `success`, `value`, `logs`, `assertions`, `timing`, `error`.
       Request-local `args` were dropped from the final surface because the Luau runtime does not
       expose an `args` table and the automation path now relies on the real typed API instead of
       stringly-typed parameter injection.
       The `value` field is populated from `eval_script_value()` (Stage 4), which delegates to
       `LuauHost::execute_value()` (Stage 1). Both return `ArgValue`. Today `ArgValue` has no
       public JSON conversion — the existing `arg_value_to_json()` in `commands.rs` is a private
       helper. As part of Stage 6, make `ArgValue`'s JSON conversion public (either
       `impl From<ArgValue> for serde_json::Value` or a public `to_json()` method). Non-finite
       floats (`NaN`, `Inf`) should map to `null` following the JSON spec.
5. [x] Integrate type checking (stage 5) into `script_eval`: check before eval, include
       diagnostics in the response.
6. [x] Expose a reusable stdio entry point through `canopy-mcp::serve_stdio(...)` so each app can
       opt into MCP automation without a background-thread attach API.
7. [x] Update the todo example to expose the MCP server through its CLI.

### Stage 7: Smoke Test Infrastructure

Build a smoke test runner that discovers and executes `.luau` test scripts.

1. [x] Add a smoke test runner to `canopy-mcp`:
       - `SuiteConfig`: suite directory, per-script timeout, fail-fast.
       - `run_suite(canopy, config) -> SuiteResult`: discovers `.luau` files, executes each
         against the app, collects results.
       - `ScriptResult`: path, status (pass/fail), elapsed, message, logs.
2. [x] Test scripts follow the same API as MCP scripts — they call commands, query state,
       use `assert()` for verification.
3. [x] Add per-app smoke CLI commands (`todo smoke ...`) instead of a separate launcher binary.
4. [x] Write example smoke tests for the todo app:
       - Bootstrap: verify widget tree structure.
       - Add item: enter_item, type text, accept_add, verify state.
       - Delete item: select, delete_item, verify removal.
       - Navigation: select_by, page, verify focus movement.
5. [x] Integrate smoke tests into `cargo test` via the todo smoke integration test.


## Design Decisions and Rationale

### Why per-owner global tables instead of methods on node handles?

In eguidev, all widgets share a uniform interface (click, hover, set_value) so a single `Widget`
type with methods works well. In canopy, each widget owner has different commands (todo has
`enter_item`, input has `backspace`). Making commands methods on a generic `Node` type would lose
type safety — the type checker couldn't tell you that `input.enter_item()` doesn't exist.

Global tables per owner (`todo.enter_item()`) give precise types and match the existing
`owner::command` dispatch model. Framework functions live under the `canopy` namespace table,
which cleanly avoids name collisions between framework globals and command owner tables. The
generic `canopy.cmd("owner::command", ...)` function remains available as an escape hatch.

**Multi-instance limitation.** Per-owner tables conflate widget *type* with widget *instance*. When
an app has multiple widgets of the same type (e.g., two `Input` widgets), the global `input` table
dispatches to whichever instance the focus-relative search finds first — which is the correct
behavior for key bindings (the focused widget handles the command). For eval scripts (MCP, smoke
tests) running against the root node, `input.left()` hits the first `Input` in pre-order tree
traversal, which may not be the intended one. The `canopy.cmd_on(id, "input::left")` function and
`canopy.find_node()` provide targeted dispatch for this case. This is a known limitation of the
type-per-table design, but the alternative (instance-based dispatch for all scripts) would sacrifice
the type safety that makes the API valuable.

### Why `Loader::load()` is sufficient for widget registration

The widget tree is dynamic, but the set of widget *types* is static. A todo app might create and
destroy `TodoEntry` widgets at runtime, but it always knows at compile time that it uses
`TodoEntry`, `List<TodoEntry>`, and `Input`. The existing `Loader` convention — each widget type's
loader registers itself and all types it will dynamically instantiate — already captures the
complete command surface. No new registration mechanism is needed; we just formalize the rule that
`finalize_api()` seals the set and makes it an error to register after finalization.

### Why config files are just Luau scripts, not a DSL

A config file could be TOML/YAML with a binding table, but that limits what users can do. Since
the config file is a Luau script, users can write conditional bindings, define helper functions,
or compose bindings programmatically. The same script API that MCP and smoke tests use is available,
so there's no separate configuration language to learn or maintain.

### Why render d.luau at runtime instead of build time?

The command surface depends on which widgets the app loads (`Loader::load()`), which is a runtime
decision. A build-time approach would require either proc-macro integration across crate boundaries
or a build script that inspects all possible widget combinations. Runtime rendering after loader
execution is simpler and always correct.

### Why keep the Rust Binder API alongside Luau bindings?

Some widgets (like the editor) have complex default binding sets that are tightly coupled to their
implementation. The Rust `Binder` API's `key_command()`, `key_commands()`, and `mouse_command()`
methods store `BindingTarget::Command` / `CommandSequence` directly without touching the script
host, so they work at any point after command registration. After Stage 1 migration, all
`DefaultBindings` implementations use these methods. The Rust Binder API remains for widget
authors who want to set up bindings programmatically from Rust. App-level bindings (the ones
users customize) move to Luau setup scripts and config files.

### Why a separate canopy-mcp crate?

MCP requires `tmcp` and `tokio` (async runtime), which are heavy dependencies. Keeping them
optional via a separate crate means apps that don't need MCP don't pay the compile cost. The
core canopy crate stays lightweight and sync-only.

### Why luau-analyze behind a feature flag?

`luau-analyze` compiles the Luau C++ frontend, which adds significant build time. Apps that
just want to run pre-validated scripts don't need it. Development tooling and CI enable the
feature; release builds skip it. The `#[cfg]` surface is minimal — only `check_script` and its
tests are gated.

### Command dispatch context

Currently, scripts run in the context of a specific node (via `SCRIPT_GLOBAL.node_id`). For
MCP-driven scripts and smoke tests, there is no "current node" — the script operates on the whole
tree. We adopt a model where:

- **Bound scripts** (key bindings) run in the context of the focused node, matching current
  behavior.
- **Eval scripts** (MCP, smoke tests) run in the context of the root node, with full tree access
  via the framework API.

For bound scripts, command dispatch searches the subtree of the focused node first, then walks
ancestors — this naturally finds the closest matching widget. For eval scripts running against root,
the subtree search covers the entire tree and finds the *first* matching owner in pre-order.
When only one instance of an owner type exists (the common case), this is unambiguous. When
multiple instances exist, eval scripts should use `canopy.cmd_on(id, ...)` with a specific node
obtained via `canopy.find_node()` to target the intended instance.

### Core access pattern: thread-local vs mlua::scope()

All Luau callbacks need access to `&mut Core` at invocation time. Two approaches exist:

1. **Thread-local pointer** (current Rhai approach): a `scoped_thread_local` holding `*mut Core`
   is set for the duration of script execution. Callbacks read it via `with()`.
2. **`mlua::Lua::scope()`**: creates a temporary scope where callbacks can borrow from the caller's
   stack frame, with lifetime safety enforced by the type system.

We use the thread-local approach uniformly. While `scope()` is safer, it has a fundamental
limitation: scoped functions cannot be stored in the Lua registry. Bind handler closures *must* be
stored persistently (as `RegistryKey`s) because they outlive any single script execution. Rather
than maintaining two access patterns (scope for one-shots, thread-local for persistent closures),
we use one pattern throughout. The unsafe is contained within `LuauHost` and the command dispatch
functions, matching the existing safety boundary.

### Bind replace semantics

`bind()` on an already-bound key *replaces* the existing binding with the same path filter, rather
than stacking a new binding alongside it. This makes the most common config file operation
(override a key) zero-friction: `canopy.bind("j", ...)` in a config file just works without
needing a preceding `unbind_key("j")`. Path-scoped bindings (`bind_with` with a `path` option)
only replace bindings with the *same* path filter, so multiple path-scoped bindings for the same
key still work correctly.
