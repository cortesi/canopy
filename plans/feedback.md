## Architectural read of the current design

At a high level you have:

* **Retained UI tree**: `Core` owns a `SlotMap<NodeId, Node>`; each `Node` owns a
  boxed `Widget`, a `taffy::Node` id, children, and a `ViewPort` (canvas + view +
  position).
* **Layout**: `Core::update_layout(screen_size)` calls `taffy.compute_layout`
  and then `sync_viewports` to compute:

  * each node’s `vp` (canvas/view)
  * each node’s **screen projection** `node.viewport` (a `Rect` in screen
    coordinates used for hit testing and rebasing mouse events)
* **Rendering**: `Canopy::render`:

  1. recomputes layout every render (good for correctness, potentially expensive),
  2. runs `pre_render` (initialization via `poll` for uninitialized nodes, and
     focus validity checks),
  3. renders top-down with composition into a final `TermBuf`,
  4. diffs with the previous buffer and writes only deltas to a `RenderBackend`.
* **Event routing**:

  * Key: bubble from focus up to root; **keybindings are checked before the
    widget event** at each step.
  * Mouse: locate deepest node under cursor using view projection; bubble up;
    **widget event runs first, then bindings if ignored**.
* **Commands + scripting**:

  * `#[derive_commands]` generates `CommandNode` dispatch (string-keyed).
  * `ScriptHost` registers each widget’s command set into Rhai modules
    (namespace = `NodeName`), injecting a “Context” argument implicitly when
    needed.
  * Bindings map `(mode, path-filter, input)` → `"module::command(...)"` scripts.

That’s a coherent stack. The remaining work is mostly conceptual/structural and
ergonomics, outlined below.

---

## Conceptual / structural problems to address

### A) Node “name” is overloaded across multiple concerns

`Node.name` is:

* the *path component* for input binding resolution,
* the *script module namespace* (via `CommandSpec.node` / `NodeName`),
* the *selector key* for command dispatch (search subtree/ancestors for matching
  name).

This couples instance identity, widget type identity, and command namespace. It
works as long as:

* node names are essentially type names,
* you don’t want multiple instances that need distinct bindings beyond
  structural position.

**Recommendation:** split “type namespace” from “instance identifier”.

* Keep a **command namespace** (usually type-based) for scripting.
* Add a separate **instance name/tag/key** for path matching and inspector
  display.

  * e.g. `node.kind: NodeKind` (type-based) + `node.name: Option<NodeKey>`
    (instance label)

Then bindings can match on instance keys, and commands can remain type-based.

---

### B) Binding resolution is inconsistent, and specificity is underspecified

* Keys: binding checked *before* widget receives event.
* Mouse: widget gets event first; binding is fallback.

You can justify either, but the inconsistency is surprising to users implementing
widgets and writing bindings.

Also, `PathMatcher` is regex-backed and matches substrings unless anchored, and
`*` wildcards can span multiple segments. The “winner” is based on match end
index, not segment specificity. This can produce unintuitive precedence when
multiple patterns match.

**Recommendation:** formalize input routing as explicit phases:

* **Capture phase**: bindings can intercept before widget.
* **Target/bubble phase**: widget handles first; bindings can be fallback.
* Optionally allow bindings to specify which phase they belong to.

And reimplement path matching as a **segment-aware glob** (single-segment `*`,
`**` for multi-segment) rather than the current substring matcher. That will
improve both correctness and ergonomics. If you keep the current matcher,
document the exact precedence rules and caveats prominently.

---

### C) Command dispatch target resolution is ambiguous in non-trivial trees

`dispatch(core, node_id, cmd)`:

* searches the subtree of `node_id` for a matching node name,
* then walks parents, checking each ancestor.

This means the *same script* can target different widget instances depending on
where it’s executed from, and “first match in subtree traversal” is the
tie-breaker. That’s sometimes useful (“closest relevant widget”), but it’s also
a source of spooky action at a distance.

**Recommendations:**

* Provide an explicit way to target:

  * “this node” (no search)
  * “nearest ancestor of type X”
  * “nearest descendant of type X”
  * “node at path …”
* Expose these as script builtins or command addressing modes, instead of
  hardwiring subtree-then-ancestors search for all invocations.

Even a small change like supporting `@self`, `@parent`, `@root` as
pseudo-namespaces would reduce ambiguity a lot.

---

### D) Public extensibility is constrained by crate-private render APIs

`Canopy::render` is `pub(crate)`, meaning downstream crates cannot implement
custom backends or drive the event loop manually without using your crossterm
runloop.

If you want this to be a framework (not just an app), you’ll want:

* a public `Canopy::render(&mut self, backend: &mut dyn RenderBackend)` (or
  similar),
* and a public “step” API: `handle_event(Event)`.

This also improves testability for downstream users.

---

## Low-hanging fruit: features + ergonomics you can ship quickly

### 1) Expand `#[derive_commands]` argument types

Even without changing dispatch semantics, adding a few basic types would make
scripting feel far less “toy”:

* `bool`, `usize`, `String` are immediate wins.
* Optionally `char`, `i64`, and small enums.

### 2) Improve `PathMatcher` semantics (even without redesign)

If you don’t want to redesign matching right now:

* enforce anchors by default (match whole segments, not substring),
* define a simple specificity rule (e.g. longer pattern string wins; anchored
  wins over unanchored; fewer wildcards wins).

---

## Larger structural recommendations for long-term ergonomics

### 1) Decouple command namespace from node naming

As described earlier, split “instance identity used for matching” from “type
namespace used for command registration.”

A clean model is:

* `Widget::kind() -> &'static str` (type namespace; default from type name)
* `Widget::id() -> Option<NodeKey>` (instance label; default None)
* Path matching uses `id` when available, else `kind`.

Then scripts use `kind::command()` and bindings can target specific instances via
path filters.

---

### 2) Make input routing explicit and configurable

Adopt a DOM-like model:

* capture: root → target
* target: target handler
* bubble: target → root

Allow bindings to attach to capture or bubble. This removes the current key/mouse
inconsistency and makes behavior predictable.

**Before (current behavior)**

Key bindings run before the widget, so the binding can pre-empt the focused
widget even if the widget would have handled the key:

```rust
// Current API: key bindings are checked before the widget.
canopy.bind_key(key::Ctrl + 's', "editor/", "editor::save()")?;

// If focus is inside an input widget, the binding still fires first.
// The input widget only sees the key if no binding matched.
```

Mouse bindings run after the widget, so the binding is only a fallback if the
widget ignores the event:

```rust
// Current API: mouse bindings only run if the widget ignores the event.
canopy.bind_mouse(mouse::Button::Right, "list/", "list::open_context()")?;

// If the list widget handles right-click, the binding never runs.
```

**After (proposed behavior)**

Bindings declare a phase, so you can choose whether they pre-empt or defer to
the widget:

```rust
// Proposed API sketch.
canopy.bind_key(
    InputPhase::Capture,
    key::Ctrl + 's',
    "editor/",
    "editor::save()",
)?;

canopy.bind_mouse(
    InputPhase::Bubble,
    mouse::Button::Right,
    "list/",
    "list::open_context()",
)?;
```

That gives you predictable, explicit ordering:

```text
Capture:  root → ... → target  (bindings can intercept)
Target:   target widget        (widget handler runs)
Bubble:   target → ... → root  (bindings can act as fallback)
```

If you want today’s behavior, you can keep the defaults (key bindings in
capture, mouse bindings in bubble). If you want consistency, set both to the
same phase, or make it configurable per binding.

---

### 3) Rethink command targeting

Keep the current “search subtree then ancestors” as a default, but expose
explicit resolution modes:

* `this::cmd()`
* `ancestor::<type>::cmd()`
* `descendant::<type>::cmd()`
* `path("/root/...")::cmd()`

You can implement these as script builtins that set a “starting node id” and/or
“search strategy” for dispatch. It will greatly reduce surprises in complex
trees.

---

### 4) Improve backend abstraction and expose rendering publicly

Make downstream-driven event loops and backends first-class:

* public render/step APIs

---

### 5) Tighten the docs: your `docs/src/*` appear pre-refactor

Many docs refer to old types (`Root` structure, `Node::handle_key`, etc.) and
will mislead users of the new API. Shipping updated docs and a migration guide
will pay back immediately in fewer “mysterious behavior” issues.

---

## If you want a prioritized “next week” plan

1. **Expand `derive_commands` argument types** (biggest immediate scripting UX
   win).
2. **Improve `PathMatcher` semantics** (reduce binding surprises without a full
   redesign).
3. Start a design doc for splitting node instance naming from command namespace,
   improving input routing phases, and clarifying command targeting.

If you want, I can also propose concrete API signatures for capture/bubble
routing, command targeting, and the namespace/instance split in a way that
keeps `Context` object-safe and doesn’t explode your public surface.
