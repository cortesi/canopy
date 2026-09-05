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
2. The base `canopy` module, which declares `NodeId`, `Point`, `Size`, `Rect`,
   `NodeInfo`, `TreeNode`, `BindOptions`, `UnbindSelector`, `MouseSpec`,
   `FixtureInfo`, `BindingInfo`, `CommandTarget`, `CommandParamInfo`,
   `CommandInfo`, `ScreenCell`, `RouteTraceEntry`, `AvailableBinding`,
   `BindingSnapshot`, `ScriptAssertionInfo`, `ScriptJournalEntry`, the `canopy`
   global, and `fixtures()`.
3. Each module registered through `Canopy::register_script_module`.
4. One module per widget owner, carrying its command table and default-binding
   helper.
5. Fixture comment lines.

The text and the audited surface therefore cannot drift apart.

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

Target tables accept `{ kind = "exact", node = id }`, `{ kind = "from", node =
id }`, or `{ kind = "focus" }`. Omission uses the current script anchor.
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

Use `canopy.bind(key, options, callback)` for key bindings and
`canopy.bind_mouse(mouse, options, callback)` for mouse bindings. The options
table is required. Its `description` field must contain user-facing text. The
optional `path` field limits a binding to matching route paths. The optional
`mode` field puts a binding in a named mode. Set `tier = "global"` for a global
binding; a global binding cannot also name a mode, and its path must be
anchored at both ends.

```luau
canopy.bind_command("?", {
    description = "Show key bindings",
    path = "/root/**/",
    tier = "global",
    phase = "before_widget",
}, "root::toggle_help")
```

Use `bind_command(key, options, id, ...)` for a single positional command
action. Keep callbacks for composed actions. Native Rust uses generated typed
`Widget::call_command(arguments...)` builders with `Canopy::bind_command`.
`CommandCall::with_target` preserves exact, relative, or focus targeting when a
Button, List, or native binding stores the action.

Set `phase = "before_widget"` to run before widget input, or `phase =
"after_widget"` to run after the widget ignores input. Mouse bindings accept
only `after_widget`. Omitted phases retain the legacy path-match rule. Explicit
phases do not change scope, specificity, or insertion ordering.

The registry keeps one flat record format for application and framework
bindings. `canopy.bindings()` returns all records, including normalized input,
owner, scope, path, description, source, and target kind.
`canopy.available_bindings(node?)` returns an owned snapshot of the effective
key bindings for the specified node or current focus. The snapshot contains the
focus path, active modes, active exclusive framework group, and one winning
record per key. Each winner includes its route path and whether it runs before
the widget or after the widget ignores the key. Contextual help and automation
use this same resolver as input routing.

`canopy.unbind(id)` removes one binding. `canopy.unbind_key(key, options?)`
removes matching application key bindings. Its optional selector has exact
`mode` and `path` filters. `canopy.clear_bindings()` removes every application
binding. Scripts cannot remove or replace framework-owned bindings.

Registered widget default bindings appear as `owner.default_bindings()` in the
generated API. Calling that helper installs the Rust-registered default binding
script for that owner.

## Persistent Modules

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

`state` is `completed`, `failed`, or `timed_out`.

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

MCP timeouts are wall-clock watchdogs layered per invocation on top of the gas
budget. The watchdog cancels execution at the next VM safepoint; the failure
surfaces as a structured `ScriptTimeout` error.

Timeouts do not kill a thread or process. Rust callbacks must return to Luau
before the cancellation can be observed. A long native callback can therefore
run past the requested timeout. Infinite Luau loops time out with `state =
"timed_out"` and `error.type = "timeout"`.

## Testing

The generated API is test-covered by an exact golden tail that includes command
enums, optional named arguments, fixtures, and default bindings.

Script ABI tests cover positional and named dispatch, optional arguments, error
reporting, logs, assertions, nested callbacks, deferred release, unbind, and
event dispatch.
