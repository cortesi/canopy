# Luau API: Consolidation, Documentation, Inspection & Test Infrastructure

The `.d.luau` type definitions are the sole documentation for models interacting with
canopy via MCP. This plan addresses six interconnected concerns:

1. **MCP connectivity** via UDS sockets and `canopyctl` as a universal proxy/launcher.
2. **Fixture system** for reproducible named app states, supporting smoketests and MCP
   development.
3. **Consolidate** directional command variants into parameterized commands using Luau union
   types.
4. **Expand inspection** so scripts can query meaningful app state (tree structure, scroll
   position, canvas size, rendered buffer).
5. **Document** the `.d.luau` so it stands alone as a complete API reference.
6. **Migrate Rust tests** to Luau smoketests where feasible.


## 1. canopyctl

`canopyctl` is the developer's single entry point for running, testing, inspecting, and
automating canopy apps. It communicates with apps over MCP — headless via stdio, live via
UDS — so it works uniformly across all canopy apps without embedding tooling logic in each
app binary.

### Configuration (`.canopyctl.toml`)

`canopyctl` discovers `.canopyctl.toml` by walking up from the current directory, stopping
at the repository root. CLI flags override config values. Relative paths resolve against the
directory containing the config file.

```toml
[app]
# Command to start the app's headless stdio MCP server.
# Used by: smoke, fixtures, eval, api, mcp (auto-spawn).
headless = ["cargo", "run", "-p", "todo", "--", "mcp", "./dev.db"]

# Command to start the app interactively.
# Used by: run. canopyctl appends mcp_args to enable the UDS listener.
run = ["cargo", "run", "-p", "todo", "--", "./dev.db"]

# Args appended to [app].run to inject the UDS socket path.
# Each element may contain {socket}, replaced with the actual path.
# Default: ["--mcp={socket}"]
# mcp_args = ["--mcp", "{socket}"]        # space-separated flag
# mcp_args = ["serve-mcp", "{socket}"]    # subcommand form

# Working directory (default: directory containing .canopyctl.toml).
cwd = "."

# Environment variables merged into the app process.
env = { RUST_LOG = "info" }

[smoke]
# Directory containing .luau test scripts (default: "smoke").
suite = "./smoke"
# Stop after first failure.
fail_fast = false
# Per-script timeout in milliseconds (default: 30000).
timeout_ms = 30000

[mcp]
# Idle shutdown in seconds (default: 1200). canopyctl mcp exits after this
# long with no tool calls.
idle_shutdown_after_secs = 1200
```

All sections and fields are optional. Without a config file, commands that spawn the app
require `-- COMMAND` on the command line.

### Subcommands

Every subcommand that spawns the app accepts `-- COMMAND` to override the config's command
for that mode.

#### `canopyctl run [--fixture NAME]`

Run the app interactively. Spawns the `[app].run` command with `[app].mcp_args` appended
and terminal passthrough — the child inherits stdin/stdout/stderr for full TUI control.
canopyctl prints the socket path on stderr at startup so MCP clients can `connect` to the
running app.

With `--fixture`: canopyctl waits for the socket, connects, calls `apply_fixture(NAME)`,
and disconnects.

#### `canopyctl mcp`

Run canopyctl as an MCP server on stdio. Configure this once in your MCP client (Claude,
etc.) — one entry covers all canopy apps in a project.

**MCP tools exposed:**

Session management:
- `connect(socket: string)` — attach to a running app's UDS socket (e.g. one started by
  `canopyctl run`). Subsequent interaction tools proxy to that app.
- `disconnect()` — detach from the current session, killing any managed process.

Interaction (auto-spawn from `[app].headless` if no session active):
- `eval(script: string, fixture?: string)` — evaluate Luau on the connected app. Returns
  `ScriptEvalOutcome`. The `fixture` parameter is only supported on headless sessions
  (where each call builds a fresh app); on live sessions, use `apply_fixture` instead.
- `apply_fixture(name: string)` — apply a fixture to a live app. Runs the Rust fixture
  closure and re-renders. No script evaluation, no Luau host state interaction — just the
  fixture setup function. Safe to call on any session.
- `api()` — return the app's `.d.luau` API definitions.
- `fixtures()` — list registered fixtures.

Auto-spawn: when an interaction tool is called with no active session, canopyctl spawns the
`[app].headless` command and connects over stdio. The managed process persists across calls
and is killed on `disconnect()` or idle shutdown. Each `eval()` builds a fresh app instance
inside the headless process, so successive calls are isolated.

`connect()` produces a **stateful session**: successive `eval()` calls operate on the same
live app instance, accumulating state. This models interactive development where the MCP
client drives the app through a sequence of actions.

**Fixture semantics differ by session type.** On headless sessions, `eval(fixture: ...)`
builds a fresh app, applies the fixture, and runs the script — fully isolated. On live
sessions, `eval(fixture: ...)` returns an error because there is no clean way to reset
accumulated host state (bindings, Luau callbacks). Use `apply_fixture` instead: it runs the
Rust fixture closure and re-renders, but does not touch host state. This is the right tool
for setting up a live app (e.g. `canopyctl run --fixture`) or resetting the widget tree
mid-session.

Idle shutdown: canopyctl exits after `idle_shutdown_after_secs` with no tool calls (default
1200s), cleaning up managed processes.

#### `canopyctl smoke [SCRIPTS...] [--suite DIR] [--fail-fast] [--timeout-ms MS]`

Run the Luau smoke test suite. Spawns one headless app process (via `[app].headless`) and
sends each script as a `script_eval` call. Each call builds a fresh app instance inside the
process, so tests are isolated without per-test process overhead.

Script discovery: if `SCRIPTS` are given, run those (resolved against `[smoke].suite`).
Otherwise, recursively collect all `.luau` files in `[smoke].suite`, sorted
lexicographically.

Fixture selection by naming convention: the first path component under the suite root
determines the fixture. `with_items/navigation.luau` and `with_items/edge/corner.luau` both
use fixture `"with_items"`. Scripts at the suite root use no fixture.

Output: per-script pass/fail with elapsed time, followed by a summary. Non-zero exit on
failure.

The in-process smoke runner (`canopy-mcp/src/smoke.rs`, `run_suite()`) remains for Rust
test integration via `cargo test`. Both runners share `SuiteConfig` and script discovery
logic.

#### `canopyctl fixtures`

List registered fixtures. Spawns the headless app, queries the fixture registry, prints a
table of names and descriptions.

#### `canopyctl eval (-f FILE | SCRIPT) [--fixture NAME] [--timeout-ms MS]`

One-shot Luau evaluation. Pass a script as an inline string argument or via `-f` with a file
path (mutually exclusive). Spawns headless, runs the script, prints the result, and exits.
Useful for quick checks during development.

Output: return value (pretty-printed JSON), logs, assertion results. On failure, prints the
error and exits non-zero.

#### `canopyctl api`

Print the app's Luau API definitions (the rendered `.d.luau`). Spawns headless, calls
`script_api`, prints to stdout.

### App-side integration

**UDS listener**: The canopy runtime provides a library function to open a UDS MCP listener
at a given path. The framework itself imposes no CLI convention — how an app exposes the
listener is up to the developer. `canopyctl run` injects the socket path via `[app].mcp_args`
(default `["--mcp={socket}"]`), which covers the common flag form. Apps with different
conventions override `mcp_args` in the config. The app is responsible for socket cleanup on
exit.

For live apps, the crossterm runloop owns `&mut Canopy` exclusively. UDS tool requests must
be marshalled onto the UI thread via a channel: the UDS listener thread sends requests to
the runloop, which executes them between event/render cycles (as `poll()` callbacks already
do) and sends results back. This preserves the single-owner model.

**Headless stdio MCP**: Apps serve MCP over stdin/stdout using `canopy_mcp::serve_stdio`.
This is what canopyctl's headless spawning connects to. How the app exposes it (subcommand,
flag, etc.) is an app-level decision — the config's `[app].headless` captures the full
invocation. Also used directly by Rust test integration and CI.

### Rendered buffer inspection

Both headless and UDS modes support inspecting the rendered terminal buffer:

- `canopy.screen() -> { { string } }` — the rendered screen as a table of rows, each row a
  table of cell strings.
- `canopy.screen_text() -> string` — the rendered screen as plain text (rows joined by
  newlines).

`Canopy::render()` already materializes the full screen into a `TermBuf` stored on the
`Canopy` instance (`self.termbuf`), accessible via `Canopy::buf()`. This happens before
diffing into any backend, so `NopBackend` works unchanged — tests already use
`canopy.buf()` with `NopBackend` to inspect rendered content. The Luau `screen()` and
`screen_text()` functions read directly from `Canopy::buf()`.

### Implementation plan

1. [x] Add UDS MCP listener as a library function in the canopy runtime.
2. [x] Implement UDS→runloop request marshalling for live apps.
3. [x] Implement socket cleanup on app exit.
4. [x] Create `canopyctl` crate with CLI scaffolding (clap) and config discovery.
5. [x] Implement `canopyctl run` with terminal passthrough and UDS injection via `mcp_args`.
6. [x] Implement `canopyctl run --fixture` (wait for socket, connect, apply, disconnect).
7. [x] Implement `canopyctl mcp` — MCP server with `connect`/`disconnect`, `eval`/`api`/
   `fixtures` tools, auto-spawn, idle shutdown.
8. [x] Implement `canopyctl smoke` — suite discovery, headless execution, reporting.
9. [x] Implement `canopyctl fixtures`, `canopyctl eval`, `canopyctl api`.
10. [x] Add UDS support to the todo example (`--mcp=<path>` flag).
11. [x] Add `canopy.screen()` and `canopy.screen_text()` Luau functions (read from
   `Canopy::buf()`).
12. [x] Add Luau type definitions for screen inspection to preamble.


## 2. Fixture system and smoketest infrastructure

The eguidev project has a fixture abstraction that has proven valuable: named, reproducible
app states that both smoketests and MCP consumers can load. We build an analogous system for
canopy.

### Design

**Fixtures** are named app states. Each fixture has:
- A **name** (e.g. `"empty"`, `"with_items"`, `"modal_open"`)
- A **description** (human-readable, appears in `.d.luau` docs and MCP tool responses)
- A **setup function** (Rust closure that mutates a `Canopy` instance into the desired
  state)

Unlike eguidev, canopy's headless rendering is synchronous — after applying a fixture and
calling `render()`, the tree is in its final state. No readiness polling or anchors needed.

### Fixture registration

Apps register fixtures alongside their factory. The current `AppFactory` type is
`Arc<dyn Fn() -> Result<Canopy>>`. Fixtures extend this as named transformations applied to
a freshly-built app:

```rust
/// A named, reproducible app state.
pub struct Fixture {
    /// Fixture name (e.g. "empty", "with_items").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Setup closure applied to a fresh app instance. Boxed to allow capturing
    /// per-fixture state (prepared data sets, temp paths, app-specific handles).
    pub setup: Box<dyn Fn(&mut Canopy) -> Result<()> + Send + Sync>,
}
```

`AppEvaluator` gains a fixture registry. Each `script_eval` call can optionally specify a
fixture name; the evaluator builds the app, applies the fixture, renders, then runs the
script.

### Luau API

```luau
-- List all registered fixtures.
declare function fixtures(): { { name: string, description: string } }
```

`fixtures()` is a **top-level global** (not on the `canopy` table) because it operates on
the test harness, not on the app instance. This matches eguidev's convention.

Fixtures are applied at the **request level**, not mid-script. There is no in-script
`fixture()` call — resetting from inside a live Luau VM would need explicit semantics for
state outside the widget tree (keymap bindings, stored Luau callbacks), and the complexity
is not worth it. Each `script_eval` request specifies a fixture name; the evaluator builds
a fresh app, applies the fixture, renders, then runs the script. Different scenarios require
separate `script_eval` calls.

### MCP integration

The `script_eval` tool gains an optional `fixture` parameter:

```json
{ "script": "...", "fixture": "with_items", "timeout_ms": 5000 }
```

When present, the named fixture is applied before script execution. The `script_api` tool
response lists available fixtures alongside the API definitions.

A `fixtures` MCP tool returns the fixture catalog:
```json
[{ "name": "empty", "description": "App with no items" }, ...]
```

### Smoke suite changes

The smoke runner (`canopy-mcp/src/smoke.rs`) currently creates a fresh app per script.
With request-level fixtures, the runner can pass a fixture name per script, driven by:
- A naming convention (e.g. `with_items/navigation.luau` uses fixture `"with_items"`)
- Per-script config in the suite
- A default fixture for the suite

Each script still runs against a fresh app instance for isolation.

### Implementation plan

1. [x] Define `Fixture` struct in `canopy-mcp` (name, description, setup fn).
2. [x] Extend `AppEvaluator` to accept a fixture registry alongside the factory.
3. [x] Expose `fixtures()` as a Luau global in the script evaluation context.
4. [x] Add optional `fixture` field to `ScriptEvalRequest`; apply before script runs.
5. [x] Add `fixtures` MCP tool to `CanopyMcpServer`.
6. [x] Include fixture catalog in `script_api` output (as a comment block or separate
   section in the `.d.luau`).
7. [x] Add fixtures to the todo example:
   - `"empty"` — app with no items
   - `"with_items"` — app with pre-populated todos
   - `"modal_open"` — app with the add-item modal active
8. [x] Migrate existing todo smoketests to use fixtures instead of manual key sequences for
   setup.
9. [x] Add Luau type definition for `fixtures()` to the preamble.


## 3. Consolidate directional command variants

Replace directional command variants with parameterized commands. The derive macro and
command system already support enum parameters (`CommandEnum`), and `defs.rs` maps
`Direction` → `"Up" | "Down" | "Left" | "Right"` and `FocusDirection` →
`"Next" | "Prev" | "Up" | "Down" | "Left" | "Right"`.

Use `Direction` only where all four directions are meaningful — scroll on widgets with 2D
canvases, cursor movement in the editor, pan on image views. For vertical-only commands
(page), use `delta: number` (positive = down, matching the `select_by` convention). Leave
1D-only commands with just two variants (Input, HelpContent) as-is — parameterizing two
commands into one plus an enum type adds API surface rather than reducing it.

**Root** (`crates/canopy-widgets/src/root.rs`):
- [x] `focus_next`/`focus_prev`/`focus_up`/`focus_down`/`focus_left`/`focus_right`
  → `focus(dir: FocusDirection)`. The internal `focus()` method already takes
  `FocusDirection`; promote it to a `#[command]` and remove the six wrappers.

**List** (`crates/canopy-widgets/src/list.rs`):
- [x] `scroll_up`/`scroll_down`/`scroll_left`/`scroll_right` → `scroll(dir: Direction)`.
- [x] `page_up`/`page_down` → `page(delta: number)`.
- [x] Remove `select_next`/`select_prev` — trivial wrappers around `select_by(±1)`.

**Text** (`crates/canopy-widgets/src/text.rs`):
- [x] `scroll_up`/`scroll_down`/`scroll_left`/`scroll_right` → `scroll(dir: Direction)`.
- [x] `page_up`/`page_down` → `page(delta: number)`.

**Editor** (`crates/canopy-widgets/src/editor/widget.rs`):
- [x] `cursor_left`/`cursor_right`/`cursor_up`/`cursor_down` → `cursor(dir: Direction)`.

**ImageView** (`crates/canopy-widgets/src/image_view.rs`):
- [x] `zoom_in`/`zoom_out` → `zoom(dir: ZoomDirection)`.
- [x] `pan_up`/`pan_down`/`pan_left`/`pan_right` → `pan(dir: Direction)`.

**HelpContent** (`crates/canopy-widgets/src/help/mod.rs`):
- No change. Only two vertical scroll commands — not worth parameterizing.
  Keep `scroll_to_top`/`scroll_to_bottom` as-is (positional jumps).

**Logs (Inspector)** (`crates/canopy-widgets/src/inspector/logs.rs`):
- [x] `scroll_up`/`scroll_down`/`scroll_left`/`scroll_right` → `scroll(dir: Direction)`.
- [x] `page_up`/`page_down` → `page(delta: number)`.

**Dropdown** (`crates/canopy-widgets/src/dropdown.rs`):
- [x] Remove `select_next`/`select_prev` — trivial wrappers around `select_by(±1)`.

**Selector** (`crates/canopy-widgets/src/selector.rs`):
- [x] Remove `select_next`/`select_prev` — trivial wrappers around `select_by(±1)`.

**Input** (`crates/canopy-widgets/src/input.rs`):
- No change. Single-line widget with only `left`/`right` — not worth parameterizing.

**Panes** (`crates/canopy-widgets/src/panes.rs`):
- [x] Remove `next_column`/`prev_column` — trivial wrappers around `focus_column(±1)`.

**Tabs** (`crates/canopy-widgets/src/tabs.rs`):
- [x] `next`/`prev` → `select_by(delta: number)` to match List/Selector/Dropdown pattern.

After consolidation, update all DEFAULT_BINDINGS scripts and smoketests.


## 4. Expand inspection surface

Scripts can currently inspect node names, parent/child relationships, focus state,
visibility, and screen rects. This is insufficient for meaningful automated interaction.

1. [x] **Enrich `NodeInfo`**: Add fields to the table returned by `canopy.node_info()`:
   - `hidden: boolean` — the hidden flag (`visible` is `!hidden`, but a node can be
     not-hidden yet have zero rect).
   - `content_rect: Rect?` — inner area after padding.
   - `canvas: Size` — total scrollable content area.
   - `scroll: Point` — current scroll position (viewport top-left in canvas coords).
   - `accept_focus: boolean` — whether this node can receive focus.
   Update `preamble.d.luau` `NodeInfo` type accordingly.

2. [x] **`canopy.tree() -> table`**: Recursive table of the full node tree. Each entry
   contains enriched NodeInfo fields plus nested children. Gives a complete structural
   snapshot in one call — essential for MCP consumers that need to understand app state
   before deciding what to do.

3. [x] **`canopy.bindings()`**: Current binding table as structured data. Each entry:
   `input` (key or mouse spec string), `input_type` (`"key"` or `"mouse"`), `mode`, `path`,
   `desc`, `target` (human-readable string: `"root.quit()"`, `"[sequence: 2 commands]"`, or
   `"luau"`). The goal is discoverability for MCP consumers, not round-tripping or
   programmatic diffing. The binding system supports key and mouse bindings with command,
   sequence, and Luau closure targets; the string representation covers all three at the
   right fidelity.

4. [x] **`canopy.node_at(x: number, y: number) -> NodeId?`**: Hit-test a screen coordinate,
   returning the deepest visible node (or nil). Exposes `core.locate_node(...)` to scripts.
   Useful for verifying layout and for MCP consumers navigating by geometry.

5. [x] **`canopy.commands()`**: Registered command set as structured data:
   `{ { name: string, owner: string, doc: string?, params: ... } }`. More useful for
   programmatic exploration than the `.d.luau` text.


## 5. Improve `.d.luau` documentation

The `.d.luau` file is the *only* documentation an LLM has when interacting with a canopy
app via MCP.

1. [x] **Document preamble types**: Add `---` doc comments to every type and field in
   `preamble.d.luau`. Explain what `NodeInfo.on_focus_path` means, what `Rect` coordinates
   are relative to, what `BindOptions.mode` and `BindOptions.path` control, what
   `MouseSpec` format strings look like.

2. [x] **Document `canopy` table functions**: Each function needs a `---` doc comment.
   `find_node` should explain path pattern syntax. `send_key` should explain key spec
   format. `bind_with` should explain mode/path interaction.

3. [x] **Render parameter docs in generated definitions**: `defs.rs` currently renders
   `--- short` for commands but nothing for parameters. Add a `doc: Option<&'static str>`
   field to `CommandParamSpec`. Since Rust does not support doc comments on function
   parameters, the derive macro should parse `@param name description` tags from the
   method's doc comment and attach them to the corresponding `CommandParamSpec`. Then
   `render_definitions` can emit `--- @param name — description` lines. Example:
   ```rust
   /// Move the cursor.
   /// @param dir The direction to move.
   #[command]
   fn cursor(&mut self, c: &mut dyn Context, dir: Direction) -> Result<()>
   ```

4. [x] **Render long docs for commands**: `CommandDocSpec` has a `long` field currently
   ignored by `defs.rs`. Render it as additional `---` lines above the function.

5. [x] **Add a module-level doc block**: Expand the preamble comment into a guide explaining
   API structure: `canopy` global for framework operations, per-widget globals for commands,
   `default_bindings()` for loading keybindings, `fixtures()` for listing test states
   (applied at the request level via `eval()`'s `fixture` parameter), and assertion/logging
   for testing.


## 6. Migrate Rust tests to Luau smoketests

With fixtures and enriched inspection, many Rust integration tests can migrate to Luau
smoketests. This is valuable because Luau tests exercise the same surface that MCP consumers
use, validating that the scripting API is sufficient for real work.

**Good candidates** (test externally observable behavior through commands and node
inspection, using real app widgets):

1. [x] `crates/canopy-widgets/src/list.rs` tests — list append, select, navigation,
   removal. Use fixtures to populate lists, verify with commands and node inspection.

2. [x] `crates/canopy-widgets/src/dropdown.rs` tests — open/close, selection.

3. [x] `crates/canopy-widgets/src/root.rs` tests — focus direction commands. Already tested
   via script in the Rust tests; migrate to standalone Luau files.

**Stay as Rust tests** (require internal APIs, synthetic widgets, or coverage the Luau
surface cannot replicate):
- `tests/test_commands.rs` — builds test-only `TestLeaf`/`TestBranch` widgets, calls
  `commands::dispatch(...)` directly; fixtures cannot expose these to Luau.
- `tests/test_focus.rs` — drives `core.focus_dir()` against synthetic grids, tests
  zero-view node edge cases after layout mutation; the Luau surface does not expose layout
  mutation or zero-view node construction.
- `tests/test_tree.rs` — path-query portions could migrate, but hit-testing
  (`core.locate_node(...)`) needs `canopy.node_at(x, y)` from Section 4 plus a fixture
  with known geometry.
- Layout math (`test_layout.rs`, `test_core_grid_dimensions.rs`)
- Render buffer internals (`test_node_render.rs`)
- Editor buffer algorithms (`editor/buffer.rs`)
- Argument marshaling (`commands.rs` unit tests)
- Style/color parsing

For each migration, the Luau test must be at least as thorough as the Rust test it
replaces. After migration, the original Rust test can be removed.
