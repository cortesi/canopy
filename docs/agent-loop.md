# Agent Loop

Canopy automation is built around one typed Luau eval surface. A useful agent
session should read the surface once, arrange state through fixtures or scripts,
act through typed commands, observe through the script API, and save any script
that should become a repeatable smoke test.

## Bootstrap

Start with the MCP `bootstrap` tool, or with `canopyctl bootstrap`, before
choosing actions. The payload includes the operating guide, generated API text,
an API digest, fixture metadata, current command availability, and a compact
script-journal summary.

```sh
cargo run -p canopyctl -- bootstrap -- cargo run -p todo -- mcp :memory:
```

Inside Luau, use `canopy.api()`, `canopy.commands()`, `fixtures()`, and
`canopy.help_snapshot()` when a scenario needs to inspect the app from inside
the same eval that will act on it.

## Fixtures

Fixtures are named states registered by the app. Headless evals can request a
fixture directly; live sessions apply fixtures before eval.

```sh
cargo run -p canopyctl -- eval --fixture with_items \
  'return #fixtures() > 0' \
  -- cargo run -p todo -- mcp :memory:
```

Smoke scripts inherit fixtures from their directory name. A script under
`smoke/with_items/navigation.luau` runs after the `with_items` fixture is
applied.

## Eval Shape

Prefer one eval per scenario step: check availability, call commands, and assert
the result in the same Luau program. Use typed command calls and runtime
observation before coordinate input.

```luau
local todo_node = canopy.resolve("todo")
canopy.assert(todo_node ~= nil, "todo widget should be mounted")

todo.select_first()
todo.delete_item()

local text = canopy.screen_text()
canopy.assert(text:find("Write agent loop docs") == nil, "deleted item should disappear")
```

Observation helpers are script-visible:

- `canopy.screen_text()` for simple text assertions.
- `canopy.screen_cells()` for styled cell assertions.
- `canopy.screen_region(x, y, w, h)` and `canopy.node_region(node)` for crops.
- `canopy.route_trace()` for the most recent key or mouse route.
- `canopy.diagnostic_dump(node?)` for tree, focus, binding, and route context.
- `canopy.script_journal()` for recent eval records.

Async predicate waits run on the Ruau async driver. Use
`canopy.wait_for(fn, timeout_ms?)`, `canopy.wait_for_node(owner, timeout_ms?)`,
or `canopy.wait_for_screen_text(text, timeout_ms?)` when an eval must observe
state that may arrive through automation while the script is active. The wait
helpers service automation between predicate checks; broader terminal event
redraw during a pending eval remains the outstanding live-loop refinement.

## Startup Shape

Reusable app/user/project startup scripts are typed roots with an obligated
`setup: () -> ()` global. Top level startup code should import modules and build pure
locals; bindings, mode changes, and command calls belong inside `setup()`. Canopy runs
app-registered startup scripts first, then `@user/init.luau`, then `@project/init.luau`.
Required modules loaded by those roots keep the ordinary `.d.luau` conformance contract
and do not need their own `setup`.

## Replay

Save an eval as a replay journal when it captures a useful interaction:

```sh
cargo run -p canopyctl -- eval \
  --journal-out tmp/todo-delete.json \
  --fixture with_items \
  'todo.select_first(); todo.delete_item(); return canopy.screen_text()' \
  -- cargo run -p todo -- mcp :memory:
```

Replay that journal against a fresh app:

```sh
cargo run -p canopyctl -- replay tmp/todo-delete.json \
  --fixture with_items \
  -- cargo run -p todo -- mcp :memory:
```

When a replay becomes part of the permanent workflow, move the Luau body into a
smoke script under the relevant fixture directory and run `cargo xtask smoke`.
