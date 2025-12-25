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

Also, `PathMatcher` is regex-based and matches substrings unless anchored. The
“winner” is based on match end index, not segment specificity. This can produce
unintuitive precedence when multiple patterns match.

**Recommendation:** formalize input routing as explicit phases:

* **Capture phase**: bindings can intercept before widget.
* **Target/bubble phase**: widget handles first; bindings can be fallback.
* Optionally allow bindings to specify which phase they belong to.

And reimplement path matching as a **segment-aware glob** (not raw regex). That
will improve both correctness and ergonomics. If you keep regex, document the
exact precedence rules and caveats prominently.

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

### D) Rendering model allocates a buffer per node per frame

`Render::new` allocates a `TermBuf` of the node’s view size on every render
traversal.

That’s a very clean and safe approach, but it can become memory- and CPU-heavy
if:

* you have deep trees,
* many nodes have large views,
* you render frequently (which you do).

**Bigger recommendation:** move toward one of:

1. **Single composed buffer + clip stack**: pass a render context that writes
   into the final `TermBuf` with an active clip rect + translation offset stack.
   This avoids per-node allocations entirely.
2. **Buffer pool reuse**: cache per-node `TermBuf` allocations keyed by
   `(node_id, view_size)` and reuse across frames (still more complex than #1,
   but incremental).

If you keep per-node buffers for now, at least consider a `Vec<Cell>` arena
reused per frame to amortize allocations.

---

### E) Public extensibility is constrained by crate-private render APIs

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

### 2) Provide typed widget access helpers

Right now, every app ends up writing the same downcast boilerplate.

Add helpers like:

* `Context::with_widget_mut_typed<T: Widget + 'static>(...) -> Result<()>`
* `ViewContext::get_widget_typed<T: Widget + 'static>(...) -> Option<&T>`

Even if these are implemented via `Any` downcast under the hood, they
dramatically improve ergonomics and reduce user error.

### 3) Improve `PathMatcher` semantics (even without redesign)

If you don’t want to replace regex right now:

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

### 4) Rendering refactor to avoid per-node allocations

If you’re aiming for scalability and smoothness, this is the big one.

A global buffer with a render context containing:

* translation offset (canvas → screen),
* clip rect,
* style stack,

will give you:

* fewer allocations,
* less copying,
* simpler cursor injection.

---

### 5) Improve backend abstraction and expose rendering publicly

Make downstream-driven event loops and backends first-class:

* public render/step APIs
* optionally feature-gate crossterm backend + scripting to slim dependencies for
  embedding use cases.

---

### 6) Tighten the docs: your `docs/src/*` appear pre-refactor

Many docs refer to old types (`Root` structure, `Node::handle_key`, etc.) and
will mislead users of the new API. Shipping updated docs and a migration guide
will pay back immediately in fewer “mysterious behavior” issues.

---

## If you want a prioritized “next week” plan

1. **Expand `derive_commands` argument types** (biggest immediate scripting UX
   win).
2. **Add typed widget access helpers** (cleans up app code).
3. **Improve `PathMatcher` semantics** (reduce binding surprises without a full
   redesign).
4. Start a design doc for splitting node instance naming from command namespace,
   improving input routing phases, and clarifying command targeting.

If you want, I can also propose concrete API signatures for typed widget access,
capture/bubble routing, and command targeting in a way that keeps `Context`
object-safe and doesn’t explode your public surface.
