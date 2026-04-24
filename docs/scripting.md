# Canopy Scripting

Canopy scripts are Luau programs evaluated against a finalized `Canopy` app. They are
automation code, not a separate runtime. Scripts inspect and mutate the same tree,
commands, bindings, focus, layout, render buffer, and fixtures that Rust code uses.

## Generated API

`Canopy::finalize_api()` seals the command surface and renders the app's `.d.luau`
definition text. `Canopy::script_api()` returns that text.

The definition file has two parts:

1. The static `canopy` preamble in `crates/canopy/luau/preamble.d.luau`.
2. Generated widget command tables, default-binding helpers, and fixture comments.

The preamble declares:

- `NodeId`
- `Point`, `Size`, `Rect`, `NodeInfo`, `TreeNode`
- `BindOptions`, `MouseSpec`, `FixtureInfo`, `BindingInfo`
- `CommandParamInfo`, `CommandInfo`
- `canopy`
- `fixtures()`

Generated widget globals use the widget owner name. For a widget owner named
`editor`, commands appear as `editor.save(...)`, `editor.move_left(...)`, and so on.
Owner names are normalized into Luau global names by replacing non-identifier
characters with `_`.

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

## Bindings

Scripts can create key and mouse bindings with `canopy.bind`, `canopy.bind_with`,
`canopy.bind_mouse`, and `canopy.bind_mouse_with`. These calls return numeric binding
IDs.

`canopy.unbind(id)` removes one binding. `canopy.unbind_key(key, options?)` removes
matching key bindings. `canopy.clear_bindings()` removes every binding.

Registered widget default bindings appear as `owner.default_bindings()` in the
generated API. Calling that helper installs the Rust-registered default binding script
for that owner.

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

`Canopy::check_script(source)` always exists. It finalizes the API if needed and
returns a `ScriptCheckResult`.

When Luau typechecking is available, diagnostics use source-bound `error` or
`warning` severities. Error diagnostics fail MCP evaluation before execution.

When typechecking is unavailable for the build target, `check_script` returns one
`unavailable` diagnostic at line and column `0`. That diagnostic is informational and
does not fail evaluation.

Debug builds typecheck scripts before compiling them after API finalization. Release
builds skip that enforcement.

## Timeouts

MCP timeouts are cooperative. Canopy installs a temporary Luau VM interrupt before
script execution and removes it afterward. The interrupt fails evaluation once the
deadline has passed and Luau reaches an interrupt boundary.

Timeouts do not kill a thread or process. Rust callbacks must return to Luau before
the deadline can be observed. A long native callback can therefore run past the
requested timeout. Infinite Luau loops time out with `state = "timed_out"` and
`error.type = "timeout"`.

## Testing

The generated API is test-covered by an exact golden tail that includes command
enums, optional named arguments, fixtures, and default bindings.

Script ABI tests cover positional and named dispatch, optional arguments, error
reporting, logs, assertions, nested callbacks, deferred release, unbind, and event
dispatch.
