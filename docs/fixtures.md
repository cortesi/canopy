# Fixture Inventory

Canopy fixtures are named, reproducible application states. Automation uses them
in two ways:

1. Headless MCP evaluation may pass `fixture` with `script_eval`.
2. Live MCP sessions apply a fixture first, then evaluate scripts against the
   running app.

## Execution and reset contracts

Each bootstrap and eval response includes `metadata` with the application name,
execution mode, session ID, viewport, reset policy, and API digest. The modes are
`fresh-app-per-eval` and `live-session`. A fresh UI does not imply fresh domain
data. Each headless eval receives a new session ID; a live listener keeps its ID
across evaluations and client reconnects.

Factories declare identity and reset behavior with
`app_factory(build).with_metadata(AppMetadata { app, reset })`.
`ResetPolicy::External` permits persistent domain state, `Isolated` declares
independent state per factory call, and `Fixture` describes an explicitly applied
domain fixture. The conservative factory default is app `canopy` with external
state; application integrations should supply their own declaration.

Todo declares `isolated` for `:memory:` and `external` for file databases.
An explicitly applied fixture changes an external headless request's effective
reset policy to `fixture`; isolated requests remain isolated. Live sessions report
`external` until a successful explicit `apply_fixture` call, then `fixture`.
Reconnecting never resets domain state. Failed fixture calls do not change the
reported reset policy.

Live fixture callbacks execute inside a runtime turn. They must use native
mutation APIs or typed command dispatch. Starting a synchronous top-level eval
from that callback returns `ScriptBusy`; execute subsequent scripts through
`script_eval` after the fixture completes. Headless fixture setup runs before
the eval turn and can use synchronous evaluation. Fixtures intended for both
adapters should use native setup code.

Headless bootstrap and eval requests accept an optional
`viewport: { width, height }`, defaulting to 120 by 40 cells. Empty or excessive
dimensions fail before factory construction. App-specific render limits are
checked before fixture application. Live requests can confirm their current
viewport but cannot resize it. Smoke suites accept the metadata-bearing
`AppFactory` directly and preserve its declaration in every outcome.

## Todo Example

The Todo example is the reference workflow suite for CLI, MCP, and widget smoke
coverage.

| Fixture | State | Covered Workflows |
| --- | --- | --- |
| `empty` | Fresh store with no todo items. | Add an item through the input widget and verify the list updates. |
| `with_items` | Store seeded with representative todo items. | Navigate the list and delete an item. |
| `modal_open` | Store seeded with items and the new-item modal open. | Verify the modal is visible and its input has focus. |

Root-level smoke scripts run without a fixture:

| Script | Purpose |
| --- | --- |
| `bootstrap.luau` | Verify the app starts, renders, mounts the todo tree, and takes focus. |
| `fixtures.luau` | Verify the fixture catalog is visible to Luau automation. |
| `help_modal.luau` | Verify the global help binding isolates application bindings, focuses the binding list, pages, and restores exact focus on close. |

Fixture directories map directly to fixture names. For example,
`examples/todo/smoke/with_items/navigation.luau` runs after applying the `with_items`
fixture.

## Guardrails

`cargo xtask smoke` discovers every `.canopyctl.toml` file and runs its configured
smoke suite. The Todo suite is currently the only checked-in suite. New examples
should add a `.canopyctl.toml`, at least one root smoke script, and fixture-specific
scripts for every non-trivial state they register.

`cargo xtask smoke` is the executable guardrail for this inventory. That keeps fixture
coverage visible when examples and smoke scripts change.
