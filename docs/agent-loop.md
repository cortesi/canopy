# Agent Loop

Canopy automation is built around one typed Luau eval surface. A useful agent
session should read the surface once, arrange state through fixtures or
scripts, act through typed commands, observe through the script API, and save
any script that should become a repeatable smoke test.

## Trust and launch policy

Local scripts and automation can invoke every exposed native action, including
application commands, input, fixtures, and native modules. VM isolation does not
reduce that native authority. These interfaces do not provide read-only eval or
command allowlists.

Builder script roots use `ScriptTrust::Disabled` or `ScriptTrust::TrustedLocal`.
Disabled roots are not mounted, required, or executed. Register a trusted root
only when its contents have the same authority as the application user.

Passing `LaunchMode::HeadlessMcp` or `LaunchMode::Run { mcp_socket: Some(...) }`
to `launch` opts into trusted-local automation. Ordinary interactive and
API-output modes open no automation listener. The third `launch` argument is a
`RunOptions` value that controls terminal input behavior.

Low-level `serve_stdio` and `serve_uds` calls are explicit trusted-local
operations. Socket placement and host filesystem permissions are deployment
responsibilities: use a private directory accessible only to intended clients.
The launcher does not provide a first-use approval interface or an application
permission profile.

## Terminal interrupts

`canopy::terminal::runloop(canopy, options)` runs the terminal adapter. With
`RunOptions::default()`, Ctrl+C restores the terminal, dumps the node tree, and
returns status 130. Set `RunOptions::interrupt_dump` to `false` to exit without
the dump. `Setup::on_interrupt` installs a hook that takes each Ctrl+C before
any widget, dialog, or binding sees it. The hook returns whether it took the
interrupt, for example to stop running work, and the adapter exits on one that
the hook declines. `Canopy::take_interrupt` offers an interrupt to the hook as
the adapter does. `InterruptPolicy::RouteToApplication` delivers Ctrl+C through normal
input routing instead, including to embedded terminal widgets.

`RunOptions::emergency_exit` optionally specifies a separate exact `Key` match.
It exits with status 130 before widget dispatch and restores the terminal once.
TermGym routes Ctrl+C and reserves Ctrl+Alt+Q for emergency exit. Its key value
uses lowercase `q` with Ctrl and Alt modifiers, matching ordinary terminal input.
These policies do not change process signal handlers.

## Bootstrap

Start with the MCP `bootstrap` tool, or with `canopyctl bootstrap`, before
choosing actions. The payload includes the operating guide, an API source
inventory, an API digest, fixture metadata, current command availability, and a
compact script-journal summary.

The `metadata` record declares `app`, `execution`, `instance_id`, `screen`,
`reset`, `fixture`, and `api_digest`. Headless `fresh-app-per-eval` requests
reconstruct the UI and receive independent instance IDs. `live-session`
requests reuse the app and retain one instance ID across client reconnects.
Domain isolation comes from
the declared reset policy, never from UI reconstruction. A pre-build failure
can omit the API digest. Node tokens remain temporary references within a
session, not durable replay targets.

Top-level evaluations start at the root. `default_commands` reports root-relative availability, while `focus_commands`
reports focus-relative availability. Choose an explicit target when multiple
widgets share a command owner. Use `canopy.call_exact(node, id, ...)` to keep a
selected owner stable across tree changes, or `canopy.call_focus(id, ...)` to
follow focus.

Availability includes eligibility and missing event requirements. A disabled
reason can change between discovery and invocation. Inspect the structured
invocation error instead of assuming discovery reserves an action.

```sh
cargo run -p canopyctl -- bootstrap -- cargo run -p todo -- mcp :memory:
```

Inside Luau, use `canopy.api()`, `canopy.commands()`, `canopy.bindings()`,
`canopy.available_bindings(node?)`, and `canopy.fixtures()` when a scenario must
inspect the app in the same eval that acts on it. `bindings()` returns the
complete registry. `available_bindings()` returns the effective key-binding
snapshot for one focus context.

## Fixtures

Fixtures are named states registered by the app. Headless evals can request a
fixture directly; live sessions apply fixtures before eval.

```sh
cargo run -p canopyctl -- eval --fixture with_items \
  'return #canopy.fixtures() > 0' \
  -- cargo run -p todo -- mcp :memory:
```

Smoke scripts inherit fixtures from their directory name. A script under
`smoke/with_items/navigation.luau` runs after the `with_items` fixture is
applied.

## Eval Shape

Prefer one eval per scenario step: check availability, call commands, and
assert the result in the same Luau program. Use typed command calls and runtime
observation before coordinate input. This example runs after the `with_items`
fixture:

```luau
local todo_node = canopy.target("todo")
canopy.assert(todo_node ~= nil, "todo widget should be mounted")
canopy.assert(
    canopy.screen_text():find("Buy milk") ~= nil,
    "fixture should render the first item"
)

todo.select_first()
todo.delete_item()
canopy.prepare()

local text = canopy.screen_text()
canopy.assert(text:find("Buy milk") == nil, "deleted item should disappear")
```

Observation helpers are script-visible:

- `canopy.screen_text(target?)` for text assertions, cropped to a node or a
  `Rect` when given one.
- `canopy.snapshot().cells` for styled cell assertions.
- `canopy.route_trace()` for the most recent key or mouse route. Each entry
  has a snake_case `kind`, such as `before_widget_binding` or `handled`. A
  route whose binding or widget handler failed ends with a `notice` entry.
- `canopy.notices()` for failures the app reported and survived, oldest first:
  a failed command or callback that a sent key ran, a widget handler, or a
  poll. Each record has a `message`, a `kind`, a `source`, and the `node` when
  known. A failed binding does not fail the evaluation that sent its key, so
  assert on the newest notice instead.
- `canopy.mode()` for the newest active mode, or the empty string for the
  default mode.
- `canopy.bindings()` for the complete application and framework binding
  registry.
- `canopy.available_bindings(node?)` for effective keys, active modes, and the
  framework group an open modal admits. Every widget predicts its keys, so the
  key list is exact for the current state.
- `canopy.explain_key(key, node?)` to inspect the prospective route for one
  key before sending it: the examined steps and the decisive outcome. It is
  advisory; `canopy.route_trace()` reports what actually happened.
- `canopy.send_key_checked(key, expectation)` to send a key only when the
  analyzed route matches a widget, binding, transient, or unhandled
  expectation. A mismatch is rejected without delivery.
- `canopy.diagnostic_dump(node?)` for tree, focus, binding, and route context.
- `canopy.script_journal()` for recent eval records.

For modal automation, inspect the application snapshot before opening the
modal. After opening, verify the framework group and focused modal node. Then
close the modal and verify the exact focus before sending the next application
input.

```luau
local origin = canopy.focused()
canopy.assert(origin ~= nil, "the app must have focus")
if origin == nil then
    return
end

local before = canopy.available_bindings(origin)
canopy.assert(before.framework_group == nil, "the app must not be isolated")

canopy.send_key("ctrl-g")
local modal = canopy.available_bindings()
canopy.assert(modal.framework_group == "root.help", "help must isolate its bindings")

canopy.send_key("ctrl-g")
canopy.assert(canopy.focused() == origin, "help must restore exact focus")
```

Use `canopy.wait_for(fn, timeout_ms?)` to observe asynchronous state.
Waits subscribe to snapshot publication and deadlines, then release runtime borrows
while parked. The shared driver continues terminal input, timers, node wakes, and
bounded native automation. Predicates resume after publication on a later turn.

Only one top-level evaluation may be active. Concurrent evaluation and module
reload receive `ScriptBusy`. Live automation submits typed evaluation tickets and
awaits completion outside the UI thread. Cancellation targets the ticket's `EvalId`.
The original ticket reports cancellation after runtime preparation.

## Startup Shape

Reusable app/user/project startup scripts are typed roots with an obligated
`setup: () -> ()` global. Top level startup code should import modules and
build pure locals; bindings, mode changes, and command calls belong inside
`setup()`. Canopy runs app-registered startup scripts first, then
`@user/init.luau`, then `@project/init.luau`. Required modules loaded by those
roots keep the ordinary `.d.luau` conformance contract and do not need their
own `setup`.

## The interactive app

A headless app is not the app a person runs. It has no terminal, so keys skip
terminal input parsing, and an app can run headless services in place of its
interactive ones. To check behavior as a person sees it, host the interactive
app in a terminal that canopyctl owns:

```sh
SOCK=$(cargo run -p canopyctl -- run --pty 120x40 --detach)
cargo run -p canopyctl -- keys --socket "$SOCK" ctrl+g --screen
cargo run -p canopyctl -- keys --socket "$SOCK" esc :/needle enter
cargo run -p canopyctl -- screen --socket "$SOCK"
cargo run -p canopyctl -- eval --socket "$SOCK" 'return canopy.mode()'
cargo run -p canopyctl -- stop --socket "$SOCK"
```

`run --pty WIDTHxHEIGHT` starts the configured `[app].run` command, or the
command after `--`, in a pseudo-terminal behind an Itty terminal emulator.
`--detach` returns once the app serves automation and prints its live socket.
The host serves a control socket beside it, at the socket path plus `.term`.

`keys` presses its input in order, through the terminal. Each entry is a key,
such as `j`, `enter`, `esc`, `up`, or `ctrl+c`, or `:` and text to type as it
stands, such as `:/needle`. A lone `:` presses the colon key. `--screen`
prints the screen once the app has drawn the keys, and `screen` prints it at
any time. `--until TEXT` on either waits until the screen shows the text, for
a result that arrives later, such as a worker's. `eval`, `smoke`, and
`replay` take `--socket` to run against the hosted app. `stop` hangs up the
terminal, which ends the app.

The host never judges from elapsed or quiet time whether the app is done.
It starts once the app reports ready: it listens on a socket that it names in
`CANOPY_READY_SOCKET`, and `canopy_mcp::launch` connects to it once the live
automation socket accepts connections. `canopyctl run` waits on the same
signal before it applies a fixture. Ready means the app takes input, not that
it has prepared its content: an app that loads a document in the background
may still be loading it. Wait for content with `screen --until TEXT`, or a
script that waits on the app's own readiness, before keys that depend on it.
After it sends keys, it waits on the live socket until `canopy.input_count()`
has risen by the keys it sent. A bare escape goes alone, and the next key
waits until the app has taken it, because a terminal reads an escape and the
key after it in one read as that key with Alt. To read the screen, the host
calls `canopy.terminal_apc` with a marker payload, which the app writes to the
terminal after its frame, and waits until the terminal has read the marker.
Terminal output is ordered, so the terminal has then read the frame. Its
timeouts only turn a hang into an error.

Rust tests drive an app the same way through the `canopy-term` crate, which
canopyctl uses. `TermHost::start` runs a command in a terminal, with
`{socket}` in its arguments replaced by a live automation socket, and
`keys`, `screen`, `screen_until`, and `eval` drive and read it. A package
tests its own binary through `env!("CARGO_BIN_EXE_<name>")`; see
`examples/gyms/tests/terminal.rs`. Use such a test for what headless input
skips: terminal key parsing, input timing, and interactive services.

`canopyctl mcp` has the same tools. `launch` hosts the app and connects the
script tools to it; `keys` and `screen` drive and read its terminal; and
`disconnect` stops it. An idle server stops its app after the `[mcp]`
`idle_shutdown_after_secs` setting, 1200 seconds by default, and keeps
serving. The global `-C DIR` option runs canopyctl as if started in `DIR`, so
an MCP configuration can select an app by its `.canopyctl.toml`. This
repository's `.mcp.json` registers a server for each example.

## Replay

Save an eval as a replay file when it captures a useful interaction:

```sh
cargo run -p canopyctl -- eval \
  --replay-out tmp/todo-delete.json \
  --fixture with_items \
  'todo.select_first(); todo.delete_item(); return canopy.screen_text()' \
  -- cargo run -p todo -- mcp :memory:
```

The replay file uses `canopy.replay/1` and records application/API identity,
execution mode, screen size, fixture and reset contracts, and ordered source/expected-success
steps. Replay uses the recorded fixture and dimensions:

```sh
cargo run -p canopyctl -- replay tmp/todo-delete.json \
  -- cargo run -p todo -- mcp :memory:
```

Use `--socket PATH` for a live target. Replay checks compatibility and fixture
availability before applying a fixture or executing sources. `--allow-mismatch`
reports compatibility overrides; malformed envelopes, invalid viewports, and
missing fixtures remain errors. `--include-failed` executes recorded expected
failures and compares the result with their expectation.

Each fresh-app step is a
separate eval; put a stateful sequence into one eval source when its steps must
share a fresh app. Live replay applies its fixture through `apply_fixture`, then
runs against the same live session.

Live fixture callbacks must use native mutation or typed dispatch. They execute
inside a runtime turn, so a nested synchronous eval returns `ScriptBusy`.
Run top-level scripts through `script_eval` after applying the fixture.

When a replay becomes part of the permanent workflow, move the Luau body into a
smoke script under the relevant fixture directory and run `cargo xtask smoke`.

## Gallery

A gallery shows the screens of an app as images, for review by a person or an
agent. `canopyctl gallery` runs each gallery script at each screen size against
a fresh headless app, draws the shots that the script returns as PNG images,
and writes a viewer, `index.html`, beside them. A script returns a list of
shots. Each shot has an `id`, an optional `title` and `caption`, and a
`capture` from `canopy.capture()`:

```luau
local shots = {}
canopy.send_key("ctrl-o")
table.insert(shots, {
    id = "options",
    title = "Options menu",
    caption = "Ctrl-O opens the options menu.",
    capture = canopy.capture(),
})
return shots
```

The `[gallery]` section of `.canopyctl.toml` sets the defaults:

| Key | Default | Meaning |
| --- | --- | --- |
| `suite` | `gallery` | Directory of gallery scripts. |
| `out` | `tmp/gallery` | Directory of the images and the viewer. |
| `sizes` | `["100x30"]` | Screen sizes as `WIDTHxHEIGHT`. |
| `timeout_ms` | none | Per-script timeout. |
| `font_size` | `14` | Font size in pixels, before the scale. |
| `scale` | `2` | Image pixels for each pixel of the font size. |
| `title` | the config directory name | Title of the viewer. |

`--size`, `--suite`, and `--out` replace the defaults for one run. Explicit
script paths run only those scripts, and keep the pages of the other scripts
from the last run, unless the font size or the scale changed. The run
publishes into `out` only after every image is written, and only over a
gallery or an empty directory. It publishes even when a script fails: the
viewer lists each failure, and the command exits with a failure status. The
viewer also names each character that the screenshot font lacks, which shows
as a box.
