# Fixture Inventory

Canopy fixtures are named, reproducible application states. Automation uses them in two
ways:

1. Headless MCP evaluation may pass `fixture` with `script_eval`.
2. Live MCP sessions apply a fixture first, then evaluate scripts against the running app.

## Todo Example

The todo example is the reference workflow suite for CLI, MCP, and widget smoke coverage.

| Fixture | State | Covered Workflows |
| --- | --- | --- |
| `empty` | Fresh store with no todo items. | Add an item through the input widget and verify the list updates. |
| `with_items` | Store seeded with representative todo items. | Navigate the list and delete an item. |
| `modal_open` | Store seeded with items and the new-item modal open. | Verify modal input focus and editing behavior. |

Root-level smoke scripts run without a fixture:

| Script | Purpose |
| --- | --- |
| `bootstrap.luau` | Verify the app starts, renders, and exposes commands. |
| `fixtures.luau` | Verify the fixture catalog is visible to Luau automation. |

Fixture directories map directly to fixture names. For example,
`examples/todo/smoke/with_items/navigation.luau` runs after applying the `with_items`
fixture.

## Guardrails

`cargo xtask smoke` discovers every `.canopyctl.toml` file and runs its configured smoke
suite. The todo suite is currently the only checked-in suite, so new examples should add a
`.canopyctl.toml`, at least one root smoke script, and fixture-specific scripts for every
non-trivial state they register.

`cargo xtask smoke` is the executable guardrail for this inventory. That keeps fixture
coverage visible when examples and smoke scripts change.
