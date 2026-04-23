# Canopy Architecture

Canopy is a terminal UI runtime. `Core` owns the node arena, layout, rendering,
focus, commands, polling, and scripting. Widgets own local state. They use context
traits; they do not keep arena references.

Treat this file as the current contract. If it disagrees with code, fix one before
adding behavior.

## Tree Model

`Core` stores `Node`s in a `SlotMap<NodeId, Node>`. A `NodeId` is valid only while
its node remains in the arena. Removed IDs are invalid for application code,
scripts, bindings, and tests.

The root node always exists. It has no parent and anchors the attached tree. A
node is attached when its parent chain reaches the root without cycles. Detached
nodes may exist during assembly or reparenting. The runtime does not render,
hit-test, or focus them.

A node stores a parent, ordered children, keyed children, a widget slot, layout
and view caches, and mount and polling flags. Parent links, child lists, and keys
must agree: parents list their children, children point back, and keys point only
at direct children.

## Node Lifecycle

Nodes start detached. Attaching a subtree under an attached parent mounts its
unmounted nodes in pre-order.

Removing a subtree runs `pre_remove` in pre-order, runs `on_unmount` in
post-order, then deletes the nodes. Every `NodeId` in the subtree becomes invalid.

Replacing a widget keeps the node ID and children, but resets mount and polling
state. Replacing a subtree deletes descendants first, then replaces the target
widget.

Detaching clears the parent link but leaves the subtree in the arena. Detached
nodes may keep stale lifecycle and layout caches until code attaches and lays
them out again.

## Invariants

`Core::validate_invariants()` checks invariants that do not mutate widgets. Tests
and smoke tests should call it after tree mutations and layout-sensitive flows.

It checks the root, widget slots, reciprocal links, duplicate children, cycles,
keys, focus, mouse capture, pending help targets, lifecycle flags, layout caches,
and computed view caches.

It does not run layout. Run layout before using screen coordinates.

## Layout

Layout starts at the root with the terminal size. Each node gets an outer
rectangle relative to its parent's content origin. Padding produces content size.
The parent's direction, sizing, gap, alignment, display, and overflow settings
place its children.

Measurement is a widget hook. A widget returns a fixed content size or asks layout
to wrap visible children. Layout may measure a widget several times in one pass.

The canvas is the scrollable content extent. It is at least the content size.
Layout clamps scroll after every pass.

Hidden nodes and `Display::None` nodes do not participate in visible layout.
Layout clears their subtree caches.

Layout errors must surface. Re-entrant widget access, missing nodes, and failed
widget hooks must not become zero measurements or fallback canvases.

## Rendering

Rendering consumes current layout and view state. Canopy renders visible nodes in
tree order into an offscreen buffer, applies the cursor overlay, and diffs against
the previous buffer when possible.

Widgets draw through `Render` in local coordinates. The runtime clips to the view,
translates to terminal coordinates, and applies style effects.

If a pre-render hook marks layout dirty, Canopy runs layout again before
rendering. Rendering must not rely on stale views.

## Event Routing

Input arrives as typed events. Keys resolve bindings first, then go to the focused
node if no binding handles them. Mouse events go to the capture node when capture
is active; otherwise hit-testing chooses the target.

Widget events bubble from target to root until a widget handles or consumes them.
Command scopes expose the originating event and target.

Routing is public behavior. Command availability, help, diagnostics, key handling,
and mouse handling should share one resolver.

## Focus and Mouse Capture

Focus is `Option<NodeId>`. A valid focus node exists and is attached to the root.
After removal, recovery prefers the next focusable node, then the previous node,
then a focusable ancestor.

Mouse capture is also `Option<NodeId>`. A valid capture node exists and is
attached to the root. Detaching or removing it clears capture.

Widgets define focusability. Directional focus depends on computed view
rectangles, so it depends on layout.

## Scripting Ownership

Scripts share the runtime state used by native Rust code. A script callback may
touch the tree only while Canopy has installed an execution context for that
callback. Canopy must restore the context when the callback returns an error.

Script-owned IDs, function handles, and binding handles are runtime capabilities.
They remain valid only while the app, node, script host, and registry entry remain
alive.

MCP and live automation cross the event-loop boundary. Work submitted from another
thread must marshal back to the UI thread before touching `Canopy` or `Core`.

## Failure and Panic Policy

Public Canopy APIs report expected failures with `Result` or `Option`: invalid
node IDs, invalid tree edits, re-entrant widget access, script errors, command
errors, layout failures, render failures, and runloop misuse.

Panics are for tests and impossible internal bugs. A panic in public library code
needs a clear invariant and a test around the surrounding behavior.

Do not hide internal errors behind harmless defaults. If layout cannot measure a
node, canvas computation cannot access a widget, or the runloop has consumed its
event receiver, return a typed error with enough context to debug the phase and
node.

