# Canopy Scripting

Canopy scripts are Luau programs evaluated against a finalized `Canopy` app. They are
automation code, not a separate runtime. Scripts inspect and mutate the same tree,
commands, bindings, focus, layout, render buffer, and fixtures that Rust code uses.

## Generated API

`Canopy::finalize_api()` seals the command surface and renders the app's `.d.luau`
definition text. `Canopy::script_api()` returns that text.

Canopy renders the file from the same native modules it installs on the script surface, in
install order:

1. The header comment in `crates/canopy/luau/preamble.d.luau`.
2. The base `canopy` module, which declares `NodeId`, `Point`, `Size`, `Rect`, `NodeInfo`,
   `TreeNode`, `BindOptions`, `MouseSpec`, `FixtureInfo`, `BindingInfo`, `CommandParamInfo`,
   `CommandInfo`, the `canopy` global, and `fixtures()`.
3. Each module registered through `Canopy::register_script_module`.
4. One module per widget owner, carrying its command table and default-binding helper.
5. Fixture comment lines.

The text and the audited surface therefore cannot drift apart.

Generated widget globals use the widget owner name. For a widget owner named
`editor`, commands appear as `editor.save(...)`, `editor.move_left(...)`, and so on.
An owner name that collides with a Luau keyword takes a `_cmd` suffix; every other name is
used unchanged.

Canopy renders command signatures from Rust command metadata. Primitive numbers map
to `number`, booleans to `boolean`, strings to `string`, `Option<T>` to `T?`,
vectors to `{T}`, string-keyed maps to `{[string]: T}`, and command enums to Luau
string unions when the command argument type declares one.

## Evaluation Model

Scripts run on the active app thread. A script callback may touch `Canopy` only while
Canopy has installed a script execution context for that stack frame. The context is
thread-local and stack-scoped. It is restored when callbacks return, error, or panic.

Do not call script callbacks from arbitrary threads. Live MCP and other automation
entry points must marshal work back to the UI thread before touching `Canopy` or
`Core`.

Script-created node IDs, binding IDs, and function handles are runtime capabilities.
They are valid only while the app, node, script host, and registry entry remain live.
Removing a node invalidates its `NodeId`. Unbinding a script callback releases the
function handle after the active callback stack unwinds.

## Commands

Scripts can dispatch commands in three forms:

- `owner.command(...)`
- `canopy.cmd("owner::command", ...)`
- `canopy.cmd_on(node, "owner::command", ...)`

Command calls accept positional arguments. A single table argument is treated as named
arguments when its keys match the command's user parameters. Named argument keys use
the same normalization as Rust command dispatch.

Injected Rust parameters, such as context and events, are not supplied by scripts.
They are filled by command dispatch when available. Missing injections fail the
command.

Values crossing from Luau into command arguments follow one policy on synchronous and
asynchronous paths. Finite integral numbers in the `i64` range become integers; other finite
numbers remain floats; non-finite numbers fail conversion. Strings must be valid UTF-8. Empty
tables become maps, dense positive integer tables become arrays, and string-keyed tables become
maps. Sparse, mixed-key, and unsupported-key tables fail with a path to the nested value.

Live `NodeId` userdata retains its process-local identity. A marshaled Node ID token is only an
external data record and does not reconstruct that identity.

## Bindings

Scripts can create key and mouse bindings with `canopy.bind`, `canopy.bind_with`,
`canopy.bind_mouse`, and `canopy.bind_mouse_with`. These calls return numeric binding
IDs.

`canopy.unbind(id)` removes one binding. `canopy.unbind_key(key, options?)` removes
matching key bindings. `canopy.clear_bindings()` removes every binding.

Registered widget default bindings appear as `owner.default_bindings()` in the
generated API. Calling that helper installs the Rust-registered default binding script
for that owner.

## Persistent Modules

Canopy can mount existing user and project directories at `@user` and `@project`. The roots are
validated when the API is finalized. Scripts may use explicit-root imports such as
`require("@user/keymap")` and relative imports within the current mount. Parent traversal, unknown
mounts, ambiguous reverse mappings, and symlink escapes are rejected.

Rooted config and startup files keep one source identity through typechecking, compilation,
loading, diagnostics, and tracebacks. `init.luau` maps to its mount root (`@user` or `@project`),
matching directory-module resolution.

`Canopy::invalidate_script_modules` refreshes one named root or every root. Invalidation also
removes every key and mouse binding and every pending startup hook, because their retained
function handles belong to the previous source epoch. The next script load prepares dependencies
again, and re-running the startup scripts reinstalls the bindings.

## Startup Scripts

Startup scripts run once, after the app finalizes its script API. The layer order is:
app scripts registered with `Canopy::register_startup_script`, then `@user/init.luau`,
then `@project/init.luau`.

Every startup root must define:

```luau
function setup()
end
```

Canopy typechecks startup roots against an obligated surface before execution. Missing or
mismatched obligations fail startup with a diagnostic naming the global and required type. App
code may add more obligations with
`Canopy::require_startup_global(name, type_text)` before `finalize_api()`.

Keep top level startup code to imports, locals, and pure construction. Put side effects
such as bindings, mode setup, and command calls inside `setup()`. Required modules
loaded by startup scripts keep the ordinary paired `.d.luau` conformance contract; they
do not need their own `setup`.

## Fixtures

Fixtures are named setup functions registered by Rust code. Automation tooling can
apply a fixture before evaluation. The generated `.d.luau` file lists fixture names
and descriptions as comments, and `fixtures()` returns them at runtime.

Headless MCP evaluation supports `fixture`. Live evaluation does not; live callers
must use the fixture tool before evaluating a script.

## Diagnostics

`canopy.log(value)` appends a log line to the evaluation result.

`canopy.assert(condition, message?)` records an assertion result. A failed assertion
also fails the script.

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

`Canopy::check_script(source_name, source)` checks Luau source against the finalized canopy
API using the ruau type checker and returns a `ScriptCheckResult`. The source name appears in
the diagnostics. Checking is available
unconditionally on every build target.

Diagnostics use `error` or `warning` severities and carry a source name when Ruau associates them
with a named source. Error diagnostics fail MCP evaluation before execution. MCP evaluation
reports `ScriptCheckDiagnostic` unchanged, so the `source` field travels with each diagnostic in
the `diagnostics` array.

Debug builds typecheck scripts before compiling them after API finalization. Release
builds skip that enforcement.

## The VM, sandboxing, and limits

Scripts run on the ruau Luau VM, a pure-Rust implementation. The VM is built once at
API finalization from a validated surface: the base module plus per-owner command
declarations are audited against the host functions actually registered, so the typed
surface and the runtime surface cannot drift apart. The VM is sandboxed: globals are
frozen, and each compiled script runs in its own chunk environment, so global writes
in one script are not visible to another. Runtime compilation (`loadstring`) is not available.
`require` is available only through configured module sources such as the persistent roots above.

Every script invocation runs under resource ceilings: a gas (instruction) budget
bounds runaway loops even without an explicit timeout, and a memory cap bounds
script allocations. Exhausting either fails the script with a runtime error.

`print(...)` output lands in the evaluation log alongside `canopy.log`, bounded by
a per-invocation quota; output past the quota is dropped with a truncation marker.
Script-declared key and mouse bindings record their declaration site (`script:LINE`)
as the default binding description, visible through `canopy.bindings()`.

## Timeouts

MCP timeouts are wall-clock watchdogs layered per invocation on top of the gas
budget. The watchdog cancels execution at the next VM safepoint; the failure
surfaces as a structured `ScriptTimeout` error.

Timeouts do not kill a thread or process. Rust callbacks must return to Luau before
the cancellation can be observed. A long native callback can therefore run past the
requested timeout. Infinite Luau loops time out with `state = "timed_out"` and
`error.type = "timeout"`.

## Testing

The generated API is test-covered by an exact golden tail that includes command
enums, optional named arguments, fixtures, and default bindings.

Script ABI tests cover positional and named dispatch, optional arguments, error
reporting, logs, assertions, nested callbacks, deferred release, unbind, and event
dispatch.
