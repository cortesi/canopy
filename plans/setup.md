# Replace Binder with Luau Default Binding Scripts

Remove the Rust `Binder` struct and `DefaultBindings` trait. Replace them with per-widget Luau
default binding scripts, giving canopy a single canonical binding surface.


## Motivation

Today there are two ways to declare key bindings:

1. **Rust `Binder` API** — a builder pattern in `binder.rs` (292 lines) that wraps `Canopy` binding
   methods. Used by all gym examples.
2. **Luau `canopy.bind()` API** — the scripting surface. Used by the todo example.

The majority of binding calls pass Luau strings: `.key('s', "block.split()")` is a Rust call that
compiles a Luau string — no Rust type checking (it's a string), no Luau tooling (it's embedded in
Rust). However, `Binder` also offers a typed command surface via `.key_command()` /
`.mouse_command()`, used in several examples (chargym, pager, framegym, widget) with invocations
like `Text::cmd_scroll_down()` and `Root::cmd_focus_next()`. These provide compile-time verification
that the command exists and its arguments are correct — a real safety benefit that the Luau surface
does not offer. Removing `Binder` means accepting that binding declarations are no longer
compile-time checked; the Luau type checker (`luau-analyze` against the generated `d.luau`) becomes
the replacement safety net, catching errors at app startup or CI rather than at compile time.

The todo example has already proven the Luau-first approach works. This proposal generalizes it,
eliminates the Rust binding surface, and introduces a composition mechanism
(`default_bindings()` functions) to replace the `DefaultBindings` trait hierarchy.


## Design

### Widget Default Bindings

Widgets may register a Luau script that provides their default key and mouse bindings. These are
*optional conveniences* — an app developer who doesn't want a widget's defaults simply doesn't call
the function. Widget authors must not put essential initialization in default binding scripts; they
are purely input configuration.

Each widget that has default bindings exports a Luau script constant and registers it during
`Loader::load()`:

```rust
// In canopy-widgets/src/root.rs
const DEFAULT_BINDINGS: &str = r#"
    inspector.default_bindings()
    help.default_bindings()
    canopy.bind_with("ctrl-Right", { path = "root", desc = "Toggle inspector" },
        function() root.toggle_inspector() end)
    canopy.bind_with("ctrl-shift-?", { path = "root", desc = "Toggle help" },
        function() root.toggle_help() end)
    canopy.bind_with("q", { path = "root", desc = "Quit" },
        function() root.quit() end)
    canopy.bind_with("a", { path = "inspector", desc = "Focus app" },
        function() root.focus_app() end)
"#;
```

```rust
// In canopy-widgets/src/inspector/mod.rs
const DEFAULT_BINDINGS: &str = r#"
    canopy.bind_with("Tab", { path = "inspector/", desc = "Next tab" },
        function() tabs.next() end)
    canopy.bind_with("C", { path = "logs", desc = "Clear" },
        function() logs.clear() end)
    canopy.bind_with("j", { path = "logs", desc = "Next" },
        function() logs.select_by(1) end)
    -- ...
"#;
```

Registration happens in `Loader::load()`:

```rust
impl Loader for Root {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.register_default_bindings("root", DEFAULT_BINDINGS)?;
        Inspector::load(c)?;
        Help::load(c)?;
        Ok(())
    }
}
```

The `register_default_bindings(name, script)` method stores the Luau source under the widget's
command namespace. After `finalize_api()`, calling `root.default_bindings()` from Luau executes the
stored script.

Since `canopy.bind_with()` replaces existing bindings with the same key and path filter,
`default_bindings()` is naturally idempotent — calling it twice just re-registers the same bindings.

The script source is compiled once at `finalize_api()` time and stored as a `ScriptId`. Subsequent
calls to `default_bindings()` execute the pre-compiled function via `run_script` rather than
re-compiling through `eval_script`. This avoids accumulating duplicate entries in the script cache
when `default_bindings()` is called repeatedly (e.g., from a user config that does
`canopy.clear_bindings()` then `root.default_bindings()` to reset).


### App Default Bindings

The app's default binding script becomes a short Luau program that calls widget default binding
functions and adds app-specific bindings:

```rust
// focusgym.rs
const DEFAULT_BINDINGS: &str = r#"
    root.default_bindings()

    canopy.bind_with("p", { desc = "Log" }, function() canopy.log("focus gym") end)
    canopy.bind_with("?", { desc = "Help" }, function() root.toggle_help() end)

    canopy.bind_with("Tab", { path = "focus_gym", desc = "Next focus" },
        function() root.focus_next() end)
    canopy.bind_with("Right", { path = "focus_gym", desc = "Focus right" },
        function() root.focus_right() end)
    canopy.bind_with("x", { path = "focus_gym", desc = "Delete focused" },
        function() focus_gym.delete_focused() end)

    canopy.bind_with("s", { path = "block", desc = "Split" },
        function() block.split() end)
    canopy.bind_with("a", { path = "block", desc = "Add" },
        function() block.add() end)
    canopy.bind_with("[", { path = "block", desc = "Shrink" },
        function() block.flex_grow_dec() end)
    canopy.bind_with("]", { path = "block", desc = "Grow" },
        function() block.flex_grow_inc() end)

    canopy.bind_mouse_with("Left", { path = "block", desc = "Focus" },
        function() block.focus() end)
    canopy.bind_mouse_with("Middle", { path = "block", desc = "Split" },
        function() block.split() end)
"#;
```

App startup:

```rust
fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    FocusGym::load(&mut cnpy)?;
    cnpy.finalize_api()?;
    cnpy.run_default_script(DEFAULT_BINDINGS)?;
    // ...
}
```

Note that `Root::load` is no longer called separately — `FocusGym::load` calls it (as today), and
the default binding hierarchy is expressed in Luau (`root.default_bindings()` calls
`inspector.default_bindings()` and `help.default_bindings()`) rather than via a Rust trait chain.

**Dispatch timing.** Default binding scripts only call `canopy.bind()` / `canopy.bind_with()` — they
register bindings in the `InputMap`, they do not dispatch commands to live nodes. This is important
because default binding scripts run *before* `Root::install()` creates the widget tree, just as the
current `Binder` calls do today. The `canopy.bind()` API writes directly to the `InputMap` and never
touches the node tree, so no live nodes are needed.

The `default_bindings()` functions themselves must not be dispatched through the normal node-owner
command resolution path (`commands::dispatch` walks the subtree/ancestor chain looking for a node of
the right owner type — which doesn't exist yet). Instead, `register_default_bindings()` stores the
script as a free function keyed by namespace name. At `finalize_api()`, each script is registered as
a global Luau function on the owner table (e.g., `root.default_bindings`) that executes the stored
source directly via `eval_script`, bypassing node dispatch entirely. This parallels how
`canopy.bind()` itself works — it's a global function, not a node command.


### Composition Model

The `DefaultBindings` trait hierarchy:
```
Root::defaults()
  └─ Inspector::defaults()
  └─ Help::defaults()
```

Becomes Luau function calls:
```
root.default_bindings()
  └─ inspector.default_bindings()
  └─ help.default_bindings()
```

This is functionally identical but more transparent — the developer can see the call chain, skip
default bindings they don't want, call them in a different order, or call individual functions
without the parent:

```luau
-- Skip inspector defaults, only set up help
help.default_bindings()
canopy.bind_with("q", { path = "root", desc = "Quit" }, function() root.quit() end)
-- ...define everything else manually
```

An app that doesn't want any widget defaults at all simply omits the `default_bindings()` calls and
writes all its bindings from scratch.


### User Config Override

The layered config model from the Luau plan is unchanged. User config scripts run after the default
script and can override, extend, or clear bindings:

```luau
-- Override a specific key
canopy.bind("j", function() todo.select_by(5) end)

-- Or start from scratch
canopy.clear_bindings()
help.default_bindings()  -- keep just help defaults
canopy.bind("q", function() root.quit() end)
-- ...
```

The `default_bindings()` functions are available to user config scripts too, so a user who wants to
reset to defaults for a specific widget can call its function.


## Implementation

### New API Surface

```rust
impl Canopy {
    /// Register a Luau script as the default bindings for a widget namespace.
    /// The script becomes callable as `<name>.default_bindings()` from Luau after finalize_api().
    pub fn register_default_bindings(&mut self, name: &str, script: &str) -> Result<()>;
}
```

**Integration with `d.luau` and owner tables.** Default binding functions are registered alongside
regular commands during `finalize_api()`. The `register_commands()` method in `script/mod.rs`
already builds per-owner Lua tables from `CommandSet` and sets them as globals. The implementation
extends this: after populating each owner table from `CommandSpec` entries, it checks for a
registered default bindings script for that owner and adds a `default_bindings` function to the same
table. The `d.luau` renderer similarly appends `default_bindings: () -> ()` to the owner's type
declaration.

If a widget defines a `#[command] fn default_bindings(...)`, `register_default_bindings()` returns
an error at registration time — the name is reserved for this mechanism. This is a hard conflict
rather than a silent override, caught during `Loader::load()` before the app starts.

`register_default_bindings()` requires that the owner already has at least one command registered via
`add_commands()`. In practice this is always the case — every widget that would have default
bindings also exposes commands (those bindings need something to call). If
`register_default_bindings()` is called for an unknown owner, it returns an error. This avoids the
need for `finalize_api()` to create owner tables and `d.luau` declarations for bindings-only owners.

The function uses a source string rather than a pre-compiled Luau function. This is simpler,
consistent with `run_default_script`, and performance is irrelevant since default bindings run once
at startup.

The default binding function appears in the generated `d.luau` like any other command:

```luau
declare root: {
    default_bindings: () -> (),
    quit: () -> (),
    toggle_inspector: () -> (),
    -- ...
}
```


### Removed API Surface

- `Binder` struct (292 lines)
- `DefaultBindings` trait
- `Binder::new()`, `.defaults()`, `.with_path()`, `.with_mode()`
- All `.key()`, `.mouse()`, `.key_command()`, `.mouse_command()` variants (24 methods)
- All `.try_*` and `*_id` variants
- `Canopy::bind_key()`, `bind_mouse()`, `bind_mode_key()`, `bind_mode_mouse()`,
  `bind_mode_key_command()`, `bind_mode_mouse_command()`, `bind_mode_key_commands()`,
  `bind_mode_mouse_commands()` — all Rust-side binding declaration methods on `Canopy`, including
  the convenience shorthands (`bind_key`, `bind_mouse`) and the mode-qualified variants that
  `Binder` wraps. These are superseded by the Luau `canopy.bind()` / `canopy.bind_with()` API.
- Re-exports of `Binder` and `DefaultBindings` from `crates/canopy/src/lib.rs` and
  `crates/canopy/src/prelude.rs`.
- Direct `Canopy::bind_key()` call sites outside of `Binder`: `crates/examples/examples/textgym.rs`
  and `crates/canopy/tests/test_viewport_scrolling_simple.rs`. These are migrated to Luau scripts
  as part of the example/test conversion in stage 3.

The `InputMap` and its binding storage/resolution remain unchanged — only the Rust-side binding
*declaration* surface is removed.


### Migration

Every example with a `setup_bindings(cnpy: &mut Canopy)` function gets converted:

1. Replace the `Binder::new(cnpy)...` chain with a `const DEFAULT_BINDINGS: &str` Luau script.
2. Replace `.defaults::<Root>()` with `root.default_bindings()` in the script.
3. Replace `.with_path("foo").key('x', "bar()")` with
   `canopy.bind_with("x", { path = "foo", desc = "..." }, function() bar() end)`.
4. Replace `.mouse(mouse::Button::Left, "bar()")` with
   `canopy.bind_mouse_with("Left", { path = "foo", desc = "..." }, function() bar() end)`.
5. Replace typed command bindings (`.key_command()` / `.mouse_command()`) with the equivalent Luau
   calls. The translation is mechanical — the Rust typed invocation maps directly to a Luau command
   call:
   - `.key_command('j', Text::cmd_scroll_down())` →
     `canopy.bind_with("j", { path = "...", desc = "Scroll down" }, function() text.scroll_down() end)`
   - `.key_command('g', Text::cmd_scroll_to().call_with([0u32, 0u32]))` →
     `canopy.bind_with("g", { path = "...", desc = "Scroll to top" }, function() text.scroll_to(0, 0) end)`
   - `.mouse_command(mouse::Action::ScrollDown, Text::cmd_scroll_down())` →
     `canopy.bind_mouse_with("ScrollDown", { path = "...", desc = "Scroll down" }, function() text.scroll_down() end)`

   This loses compile-time type checking on command names and arguments. The `d.luau` type
   definitions and `luau-analyze` in CI serve as the replacement — type errors surface at analysis
   time rather than compile time.
6. Call `cnpy.run_default_script(DEFAULT_BINDINGS)?` in main.

The `desc` field on every binding is new — this is a good opportunity to add descriptions to all
bindings, which feeds into the help system.

Widget crates (`canopy-widgets`) convert their `DefaultBindings` impls to `DEFAULT_BINDINGS`
constants and `register_default_bindings()` calls in `Loader::load()`.


### Staging

Each stage leaves the workspace buildable and all tests passing.

1. Implement `register_default_bindings()` on Canopy and the corresponding Luau-side dispatch.
2. Add `register_default_bindings()` calls to `canopy-widgets` (Root, Inspector, Help) in their
   `Loader::load()` impls, *keeping* the existing `DefaultBindings` impls in place. At this point
   both mechanisms coexist — the new scripts are registered but nothing calls them yet.
3. Convert all examples from `Binder` to Luau default scripts, replacing `.defaults::<Root>()` with
   `root.default_bindings()` and `Binder::new(cnpy)...` chains with `const DEFAULT_BINDINGS` Luau
   scripts.
4. Remove `DefaultBindings` impls from `canopy-widgets` and the `DefaultBindings` trait from canopy.
   No callers remain after stage 3.
5. Remove `Binder`, the Rust-side binding declaration methods on `Canopy`, and related imports/
   re-exports.
6. Update docs.
