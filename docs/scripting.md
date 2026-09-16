# Canopy Scripting

Canopy scripts are Luau programs evaluated against a finalized `Canopy` app.
They are automation code, not a separate runtime. Scripts inspect and mutate
the same tree, commands, bindings, focus, layout, render buffer, and fixtures
that Rust code uses.

## Generated API

`Canopy::finalize_api()` seals the command surface and renders the app's
`.d.luau` definition text. `Canopy::script_api()` returns that text.

Canopy renders the file from the same native modules it installs on the script
surface, in install order:

1. The header comment in `crates/canopy/luau/preamble.d.luau`.
2. The base `canopy` module, which declares `NodeId`, `CommandCall`, `Point`,
   `Size`, `Rect`, `SemanticIdentity`, `NodeInfo`, `TreeNode`, `CommandTarget`,
   `CommandTargetInfo`, `BindOptions`, `KeymapEntry`, `Keymap`,
   `UnbindSelector`, `MouseSpec`, `FixtureInfo`, `BindingInfo`,
   `CommandParamInfo`, `CommandInfo`, `ScreenCell`, `RouteTraceEntry`,
   `AvailableBinding`, `BindingSnapshot`, `ScriptAssertionInfo`,
   `ScriptJournalEntry`, `SemanticActionStatus`, `WidgetSemantics`,
   `NodeSnapshot`, `FrameSnapshot`, the `canopy` global, and `fixtures()`.
3. Each module registered through `Canopy::register_script_module`.
4. One module per widget owner, carrying its command table and default-binding
   helper.
5. The `command` global, with one constructor per node command grouped by
   owner. An owner named `command` fails API finalization.
6. Fixture comment lines.

The generated function signatures and the audited surface therefore cannot drift
apart.

Generated widget globals use the widget owner name. For a widget owner named
`editor`, commands appear as `editor.save(...)`, `editor.move_left(...)`, and
so on. An owner name that collides with a Luau keyword takes a `_cmd` suffix;
every other name is used unchanged.

Canopy renders command signatures from Rust command metadata. Primitive numbers
map to `number`, booleans to `boolean`, strings to `string`, `Option<T>` to
`T?`, vectors to `{T}`, string-keyed maps to `{[string]: T}`, and command enums
to Luau string unions when the command argument type declares one.

## Evaluation Model

Scripts run on the active app thread. A script callback may touch `Canopy` only
while Canopy has installed a script execution context for that stack frame. The
context is thread-local and stack-scoped. It is restored when callbacks return,
error, or panic.

Do not call script callbacks from arbitrary threads. Live MCP and other
automation entry points must marshal work back to the UI thread before touching
`Canopy` or `Core`.

The shared runtime driver polls detached Luau invocations. It releases application,
widget, and VM borrows between polls. Input, timers, node wakes, and bounded native
automation can progress while an evaluation waits. Resumed segments retain their
original script anchor. Focus-targeted calls resolve current focus when invoked.

Only one top-level evaluation may run at a time. Another evaluation or module
reload fails with structured `ScriptBusy`. Live callers submit an `EvalRequest`
through `AutomationHandle::submit_eval` and await the returned `EvalTicket` outside
the UI thread. Its completion contains the value or error, logs, and assertions.
Completion arrives through the original ticket after runtime preparation.

Script-created node IDs, binding IDs, and function handles are runtime
capabilities. They are valid only while the app, node, script host, and
registry entry remain live. Removing a node invalidates its `NodeId`. Unbinding
a script callback releases the function handle after the active callback stack
unwinds.

## Commands

Use explicit command targets in new scripts:

- `canopy.call_exact(node, "owner::command", ...)` invokes only that owner.
- `canopy.call_from(node, "owner::command", ...)` searches the origin subtree,
  then its ancestors.
- `canopy.call_focus("owner::command", ...)` resolves current focus at
  invocation.

These functions always use positional arguments. A table remains one argument,
including an empty table or a map whose keys match parameter names. Use
`canopy.call_named(id, fields, target?)` for explicit named arguments:

```luau
canopy.call_from(canopy.root(), "app::configure", { options = "dark" })
canopy.call_named("app::configure", { options = { options = "dark" } }, {
    kind = "exact", node = app_node,
})
```

Target tables accept `{ kind = "exact", node = id }`,
`{ kind = "from", node = id }`, or `{ kind = "focus" }`. Omission uses the
current script anchor.
`canopy.commands(target?)` uses the same target policies. Exact dispatch
rejects stale nodes, wrong owners, and free commands. It never searches for a
replacement target.

Legacy `owner.command(...)`, `canopy.cmd(id, ...)`, and `canopy.cmd_on(node,
id, ...)` retain their decoding rules. A single table is interpreted as named
arguments when its keys match user parameters. `cmd_on` retains relative
subtree-and-ancestor search. Unqualified calls use the script anchor, which is
root for top-level evaluation and the route node for bindings.

Discovery reports resolution, eligibility, disabled reason, and missing event
requirements separately. Dispatch rechecks eligibility before invoking the
command. Disabled state is not authorization and cannot guarantee that an
external operation will succeed.

Injected Rust parameters, such as context and events, are not supplied by
scripts. They are filled by command dispatch when available. Missing injections
fail the command.

Values crossing from Luau into command arguments follow one policy on
synchronous and asynchronous paths. Finite integral numbers in the `i64` range
become integers; other finite numbers remain floats; non-finite numbers fail
conversion. Strings must be valid UTF-8. Empty tables become maps, dense
positive integer tables become arrays, and string-keyed tables become maps.
Sparse, mixed-key, and unsupported-key tables fail with a path to the nested
value.

Live `NodeId` userdata retains its process-local identity. A marshaled Node ID
token is only an external data record and does not reconstruct that identity.

## Bindings

A binding action is a `CommandCall` or a function. The `command` table holds
one constructor for each node command, grouped by owner. A constructor has the
parameters of the owner function and returns a `CommandCall` that a binding
runs later. `command.file_select.select_by(1)` builds the call;
`file_select.select_by(1)` runs the command at once. A constructor checks its
arguments, so a bad argument fails the binding call before it installs a
binding. In a strict script the typechecker finds the same errors before the
script runs. Keep functions for composed actions.

Use `canopy.keymap` to write a keymap. The named fields of the table are the
options shared by every entry: `mode`, `path`, `phase`, and `tier`. The array
part holds the entries. An entry has `key` (one key spec or an array of key
specs), `mouse` (one mouse spec or an array), a required `description`, and an
`action`. An entry needs `key`, `mouse`, or both.

```luau
local fs = command.file_select

canopy.keymap({
    mode = "preview",
    { key = "j", description = "Scroll down", action = fs.pan_preview("Down") },
    { key = { "k", "Up" }, mouse = "ScrollUp", description = "Scroll up", action = fs.pan_preview("Up") },
    {
        key = "esc",
        description = "Leave the preview",
        action = function()
            canopy.set_mode("")
        end,
    },
})
```

`canopy.keymap` validates every option and entry before it installs a binding.
It rejects an unknown field in the table or an entry, an entry with neither
`key` nor `mouse`, a spec that does not parse, an empty spec array, and two
entries that bind the same input. A keymap with an error installs nothing. Within a call the entries install in
order, so a later entry wins a precedence tie. Every binding records the
`canopy.keymap` call site as its source. The call returns the binding IDs in
entry order, key bindings before mouse bindings within an entry.

Use `canopy.bind(key, options, action)` and `canopy.bind_mouse(mouse, options,
action)` for one binding. The options table is required. Its `description`
field must contain user-facing text. The optional `path` field limits a binding
to matching route paths. The optional `mode` field puts a binding in a named
mode. Set `tier = "global"` for a global binding; a global binding cannot also
name a mode, and its path must be anchored at both ends.

```luau
canopy.bind("?", {
    description = "Show key bindings",
    path = "/root/**/",
    tier = "global",
    phase = "before_widget",
}, command.root.toggle_help())
```

`canopy.set_mode(mode)` replaces the active modes with one mode, and the empty
string returns to the default mode. `canopy.push_mode(mode)` adds a mode above
the active modes, and `canopy.pop_mode()` removes the newest one. A key that
the newest mode does not bind falls through to the older modes and then to the
default scope.

Pass `{ transient = true }` to `canopy.push_mode` for a mode that takes only
the next key:

```luau
canopy.keymap({
    {
        key = "p",
        description = "Pane commands",
        action = function()
            canopy.push_mode("panes", { transient = true })
        end,
    },
})

canopy.keymap({
    mode = "panes",
    { key = "a", description = "Fit columns", action = command.file_select.autosize() },
})
```

The next key pops a transient mode. When the mode binds that key, the binding
runs after the pop, so it can enter another mode. It runs before the focused
widget sees the key, whatever its phase. Any other key only pops the mode, and
does not fall through to older modes or to the default scope. Global bindings
still apply. `Root` lists the keys of a transient mode in a small panel until
the mode ends.

Native Rust uses generated typed `Widget::call_command(arguments...)` builders
with `Canopy::bind_command`. `CommandCall::with_target` preserves exact,
relative, or focus targeting when a Button, List, or native binding stores the
action.

Set `phase = "before_widget"` to run before widget input, or `phase =
"after_widget"` to run after the widget ignores input. Key and mouse bindings
take either phase. The default phase is `after_widget`. Explicit phases do
not change scope, specificity, or insertion ordering.

The registry keeps one flat record format for application and framework
bindings. `canopy.bindings()` returns all records, including normalized input,
owner, scope, path, description, source, and target kind.
`canopy.available_bindings(node?)` returns an owned snapshot of the effective
bindings for the specified node or current focus. The snapshot contains the
focus path, active modes, the transient mode, the active exclusive framework
group, one winning record per key in `bindings`, and one winning record per
mouse input in `mouse_bindings`. Each winner includes its route path and
whether it runs before the widget or after the widget ignores the input.
Contextual help and automation use this same resolver as input routing.

A snapshot says what the named context would do with an input, not what the
next event will do. The mouse route starts at the requested node, as a click on
it would, and the pointer's own position plays no part: hit testing and mouse
capture choose the real target. Discovery also cannot know whether a widget's
own handler will consume an input before an `after_widget` binding sees it.

An `input` field is the spec a binding is written in, such as `Ctrl+LeftDown`
or `ScrollUp`, so a reported label parses back to the record it names.

`canopy.unbind(id)` removes one binding. `canopy.unbind_key(key, options?)`
removes matching application key bindings. Its optional selector has exact
`mode` and `path` filters. `canopy.clear_bindings()` removes every application
binding. Scripts cannot remove or replace framework-owned bindings.

Registered widget default bindings appear as `owner.default_bindings()` in the
generated API. Calling that helper installs the Rust-registered default binding
script for that owner.

## Persistent Modules

`CanopyBuilder` makes setup order explicit. Its owned `configure` callbacks
register commands, fixtures, and defaults before API finalization. Named
`bindings` and `config` sources run next, in insertion order. Owned `assemble`
callbacks then create the widget tree. `build()` consumes the builder and
returns no application on failure. Native and database effects are not rolled
back; retry with a fresh builder and suitable application resources.

Build does not prepare a frame or run startup. The first runtime preparation
runs startup, calculates geometry, then invokes `on_start` before publication.
The low-level `Canopy` setup APIs remain available.

Builder user and project script roots default to disabled. Register each root
with `user_script_root(path, ScriptTrust::TrustedLocal)` or
`project_script_root(path, ScriptTrust::TrustedLocal)` to enable it. A root
declared `ScriptTrust::Disabled` is not mounted, inspected, required, or executed.
An explicit `config(path)` call selects that local file for execution.

Trusted scripts can exercise the application's exposed native actions,
including filesystem and database effects. VM limits bound script execution;
they do not restrict the authority of those native actions. Explicit MCP launch
modes opt into trusted-local automation. A trusted socket requires an appropriate directory and host
filesystem permissions. Raw `serve_uds` is an explicitly trusted low-level API.

Canopy can mount existing user and project directories at `@user` and
`@project`. The roots are validated when the API is finalized. Scripts may use
explicit-root imports such as `require("@user/keymap")` and relative imports
within the current mount. Parent traversal, unknown mounts, ambiguous reverse
mappings, and symlink escapes are rejected.

Rooted config and startup files keep one source identity through typechecking,
compilation, loading, diagnostics, and tracebacks. `init.luau` maps to its
mount root (`@user` or `@project`), matching directory-module resolution.

`Canopy::invalidate_script_modules` refreshes one named root or every root.
Invalidation also removes application key and mouse bindings and pending
startup hooks because their retained function handles belong to the previous
source epoch. Framework-owned bindings remain installed. The next script load
prepares dependencies again, and re-running the startup scripts reinstalls the
application bindings.

## Startup Scripts

Startup scripts run once, after the app finalizes its script API. The layer
order is: app scripts registered with `Canopy::register_startup_script`, then
`@user/init.luau`, then `@project/init.luau`.

Every startup root must define:

```luau
function setup()
end
```

Canopy typechecks startup roots against an obligated surface before execution.
Missing or mismatched obligations fail startup with a diagnostic naming the
global and required type. App code may add more obligations with
`Canopy::require_startup_global(name, type_text)` before `finalize_api()`.

Keep top level startup code to imports, locals, and pure construction. Put side
effects such as bindings, mode setup, and command calls inside `setup()`.
Required modules loaded by startup scripts keep the ordinary paired `.d.luau`
conformance contract; they do not need their own `setup`.

## Fixtures

Fixtures are named setup functions registered by Rust code. Automation tooling
can apply a fixture before evaluation. The generated `.d.luau` file lists
fixture names and descriptions as comments, and `fixtures()` returns them at
runtime.

Headless MCP evaluation supports `fixture`. Live evaluation does not; live
callers must use the fixture tool before evaluating a script.

## Diagnostics

`canopy.log(value)` appends a log line to the evaluation result.

`canopy.assert(condition, message?)` records an assertion result. A failed
assertion also fails the script.

MCP evaluation returns:

- `success`
- `state`
- `value`
- `logs`
- `assertions`
- `diagnostics`
- `timing`
- `error`

`state` is `completed`, `failed`, `timed_out`, or `cancelled`.

## Waiting for State

Use `canopy.wait_for(predicate, timeout_ms?)`,
`canopy.wait_for_node(owner, timeout_ms?)`, or
`canopy.wait_for_screen_text(text, timeout_ms?)` for asynchronous state changes.
These helpers subscribe to snapshot publication and optional deadlines. They
recheck before parking to avoid a lost publication wake. The runtime services
input and automation between resumed segments. Wait helpers do not run their own
event loop or recursively service automation.

Screen observation reads published cells. Refreshing a snapshot does not advance
the backend's output baseline. A later visible frame still includes those changes.

## Typechecking

`Canopy::check_script(source_name, source)` checks Luau source against the
finalized canopy API using the ruau type checker and returns a
`ScriptCheckResult`. The source name appears in the diagnostics. Checking is
available unconditionally on every build target.

Diagnostics use `error` or `warning` severities and carry a source name when
Ruau associates them with a named source. Error diagnostics fail MCP evaluation
before execution. MCP evaluation reports `ScriptCheckDiagnostic` unchanged, so
the `source` field travels with each diagnostic in the `diagnostics` array.

After `finalize_api()`, every compile typechecks the source against the
finalized surface in every build. Error diagnostics fail compilation with a
parse error. Scripts compiled before finalization are only syntax-checked.

## The VM, sandboxing, and limits

Scripts run on the ruau Luau VM, a pure-Rust implementation. The VM is built
once at API finalization from a validated surface: the base module plus
per-owner command declarations are audited against the host functions actually
registered, so the typed surface and the runtime surface cannot drift apart.
The VM is sandboxed: globals are frozen, and each compiled script runs in its
own chunk environment, so global writes in one script are not visible to
another. Runtime compilation (`loadstring`) is not available. `require` is
available only through configured module sources such as the persistent roots
above.

Every script invocation runs under resource ceilings: a gas (instruction)
budget bounds runaway loops even without an explicit timeout, and a memory cap
bounds script allocations. Exhausting either fails the script with a runtime
error.

`print(...)` output lands in the evaluation log alongside `canopy.log`, bounded
by a per-invocation quota; output past the quota is dropped with a truncation
marker. Script-declared key and mouse bindings require an explicit description.
They record their declaration site (`script:LINE`) separately as the source
visible through `canopy.bindings()`.

## Timeouts

The driver includes parked time in each evaluation deadline. VM limits also check
running Luau at safepoints. Expiration surfaces as a structured `ScriptTimeout`
error. Explicit cancellation produces `ScriptCancelled` and MCP state `cancelled`.

Timeouts do not kill a thread or process. Rust callbacks must return to Luau
before the cancellation can be observed. A long native callback can therefore
run past the requested timeout. Infinite Luau loops time out with `state =
"timed_out"` and `error.type = "timeout"`.

## Testing

Use `canopy.snapshot()` to read one completed frame. It returns detached records
for cells, focus, and every live arena node, including detached nodes. Reading a
snapshot runs no hooks and does not advance its `frame_id`. It returns `nil`
until a viewport has been prepared. A headless evaluation prepares its initial
frame before running user source.

After commands return, call `canopy.flush()` to publish pending changes. Calls
made while a native widget mutation callback holds its widget fail with
`InvalidPhase`. Old snapshots retain their values after later publications and
node removal; their node tokens do not become durable references.

Node snapshots distinguish `attached`, `displayed`, and `intersects_viewport`.
Displayed nodes have no hidden or `Display::None` ancestor. Offscreen displayed
nodes retain their computed rectangles; non-displayed nodes have no current
screen rectangle. Intersection includes ancestor clipping but makes no claim
about occlusion. Widget semantics expose only declared roles, labels, selection,
action status, and explicitly enabled values. Sensitive input values are omitted.

Legacy `screen`, `screen_cells`, and `screen_text` queries still prepare pending
changes. Use a snapshot for attachment, ancestor visibility, and clipping
decisions.

The generated API is test-covered by an exact golden tail that includes command
enums, optional named arguments, fixtures, and default bindings.

Script ABI tests cover positional and named dispatch, optional arguments, error
reporting, logs, assertions, nested callbacks, deferred release, unbind, and
event dispatch.

## Versioned replay

`canopyctl eval --journal-out trace.json` writes a `canopy.replay/1` envelope
from the evaluation's actual metadata. Recording requires an API digest; a
failure before app construction still produces evaluation JSON, but cannot
produce a complete replay envelope.

```json
{
  "schema": "canopy.replay/1",
  "app": "todo",
  "api_digest": "recorded-api-digest",
  "execution": "fresh-app-per-eval",
  "viewport": { "width": 120, "height": 40 },
  "fixture": "with_items",
  "reset": "isolated",
  "steps": [
    {
      "source": "todo.select_first(); todo.delete_item()",
      "expect": { "success": true }
    }
  ]
}
```

Replay checks application identity, API digest, execution mode, viewport, and
domain reset policy before applying a fixture or evaluating source. Headless
replay requests the recorded viewport. Live replay checks the current viewport
and uses the explicitly selected socket. For example:

```sh
canopyctl replay trace.json -- todo mcp :memory:
canopyctl replay live-trace.json --socket /path/to/live.sock
```

Each `fresh-app-per-eval` step constructs a new app. Put a sequence that requires
shared state in one step's source. `live-session` steps share the running app;
reconnecting does not reset its widgets or database. Live replay applies a named
fixture through the fixture tool before the selected steps. Headless replay
passes the fixture into each evaluation. A missing fixture implementation is an
error in either mode.

`--allow-mismatch` reports each differing compatibility field before execution.
It does not permit an unsupported schema, malformed data, invalid dimensions,
or a missing fixture. A file database's `external` reset policy must not be
treated as `isolated` merely because headless evaluation creates a new UI.

Recorded failure steps are skipped unless `--include-failed` selects them.
Selected steps compare actual success with `expect.success`: an expected
failure passes when evaluation fails, and fails when evaluation succeeds.
`--fail-fast` stops at the first outcome that differs from its expectation.

Source strings are durable replay steps. Live node tokens and session IDs are
not durable node references; use semantic keys and application identifiers.
