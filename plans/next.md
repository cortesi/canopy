# Canopy: the agent-native terminal framework

The thesis: **the agent is the interface**. A canopy application is not a TUI that
happens to be scriptable. It is a typed, scriptable control surface that happens
to render to a terminal. Every app exposes one Luau surface, derived from the
reflective command system and audited against the runtime at startup, and that
single surface is consumed by four audiences through one mechanism:

- **Agents** drive the app over a narrow MCP surface that accepts whole typed
  Luau programs.
- **Users** customize and configure with persistent scripts: init files,
  keybindings, and modules.
- **Tests** are smoke scripts executed through the same eval path agents use.
- **The framework** runs on its own surface: default bindings are Luau scripts,
  and the inspector is a client of the script API.

Deriving a command publishes a typed agent API. This is canopy's structural
advantage over hand-authored contract systems: the `.d.luau` is generated from
Rust metadata, so the contract cannot drift from the implementation. The work
below makes the contract trustworthy end to end, opens the persistent-scripting
avenue, and turns agent activity into a first-class, legible part of the product.

Design principles, distilled from verber, eguidev, and the bring-your-own-agent
pattern:

- One powerful script-execution tool beats many narrow MCP tools. A scenario is
  one round trip: fixture, act, wait, assert, report.
- The generated `.d.luau` is the real API documentation. Scripts strict-typecheck
  against it before execution; failures come back as structured diagnostics,
  never strings.
- The surface is the binary, not the screen: the contract is fixed for the life
  of the process; mount state and focus only affect whether a call resolves.
- Scripts are first-class records: logged, replayable, diffable, and visible
  inside the app.
- One surface, no tiers: ephemeral agent evals, durable user scripts, and tests
  all see the same typed API. There is no separate "test API".
- The app stays authoritative: state, validation, focus, and input invariants
  live in Rust. The script layer composes behavior; it cannot corrupt invariants.

## Surface semantics: the binary, not the screen

What is the app's API when a widget exposes commands but is not mounted, not
visible, or not focused? The agent needs one stable contract it can read once,
yet the widget tree changes constantly. The resolution is a guarantee the code
already half-enforces, promoted here to an explicit contract:

**The surface is a property of the binary, not of the screen.** Command
registration is sealed at `finalize_api()` (`Canopy::add_commands` errors after
it), so the contract is exactly the union of every command the `Loader`
registered at startup, rendered once and immutable for the life of the process.
Mounting, unmounting, hiding, and focus never change the contract — they change
only whether a given dispatch resolves to a target. Three rules make the static
contract honest about dynamic state:

1. **Declaration is total.** Every owner table is always declared and always
   present at runtime. `editor.insert(...)` typechecks and is callable even when
   no editor is mounted. The contract an agent fetches via `script_api` (or the
   MCP bootstrap tool) at startup is still accurate at shutdown.

2. **Dispatch resolves at call time; failure is typed.** A node command resolves
   anchor-relative — pre-order through the anchor's subtree, then up the
   ancestors, matching by owner name (`CommandResolver::resolve_owner`). From an
   agent eval the anchor is the root; from a key binding it is the bound node;
   `canopy.cmd_on(node, "owner::cmd", ...)` sets the anchor explicitly, which is
   also how multiple instances of one widget are disambiguated. When nothing
   resolves, the call raises a structured error a script can branch on, not a
   string:

   ```luau
   local ok, err = pcall(function() editor.insert("hello") end)
   if not ok and err.kind == "no_target" then
       -- err.command == "editor::insert", err.owner == "editor"
   end
   ```

3. **Availability is a query, not a second API.** "What would resolve right
   now?" is a runtime question asked of the same contract. Rust already computes
   this (`Canopy::command_availability_from_focus`, `CommandAvailability`,
   `CommandResolution::{Free, Subtree, Ancestor}`); the script surface exposes
   it: `canopy.commands()` records gain `available: boolean` and
   `target: NodeId?`, and `canopy.resolve(owner)` previews dispatch for one
   owner. Waiting for availability then composes from primitives:

   ```luau
   if not canopy.resolve("editor") then
       workspace.open_editor("notes.md")
       canopy.wait_for(function() return canopy.resolve("editor") ~= nil end)
   end
   editor.insert("hello")
   ```

Together: one document to read once, a cheap predicate for "can I do this now",
and a typed failure when the answer was no — instead of a contract that mutates
as the UI changes.

## Current verification notes

- `cargo metadata --locked` succeeds; the live sibling path dependencies are
  oxau, tmcp, itty, and itty-script.
- The registry is already sealed at `finalize_api()`, so the static-surface
  guarantee holds today. What's missing is the script-side half: `NoTarget`
  surfaces as a flat string, and `CommandResolver::availability` exists in Rust
  but is not exposed to scripts.
- The base `canopy` API registration and declaration are generated from one Rust
  table (`base_api.rs::CANOPY_FUNCTIONS`), but each entry still carries a
  hand-written Luau signature *string* — typed by convention, not construction.
- `#[derive(CommandArg)]` emits `LUAU_TYPE = "any"`. `Option`/`Vec`/string-map
  support is three enumerated macro lists in `core/commands.rs` because
  `CommandType::LUAU_TYPE` is a const string, which blocks generic impls.
- Command returns reach owner declarations via `CommandReturnSpec`, but the
  `CommandInfo` records from `canopy.commands()` carry no return info, and
  `CommandTypeSpec.doc` is always `None`.
- `NodeId` is declared `declare class NodeId` but crosses the boundary as a
  forgeable number: `node_id_to_arg` packs the slotmap key into an int and
  `node_id_from_value` accepts any whole non-negative number.
- oxau has no declaration model: `NativeModule::declaration` and
  `HostTypeBuilder::declaration` take strings, `SurfaceSpecBuilder::
  declaration_global` does a `format!`. The `SurfaceSpec` audit parses and
  structurally diffs declarations against runtime bindings, but composes
  nothing. Userdata marshaling is hardcoded to `Opaque("userdata")`
  (`oxau-vm/src/value_marshal.rs`) and JSON conversion rejects opaques, so
  userdata cannot cross an MCP eval boundary at all today.
- oxau already has the pieces stages 3-4 need: `ModuleSource` (async-first, epoch
  invalidation, `FilesystemModuleSource`, `.luaurc`-style aliases),
  `ConformanceCheck` with fingerprints, structured `RuntimeError`s with typed
  payloads recoverable at `MarshaledScriptError`, and async host functions with
  `HostCtx::call_protected` for predicate waits (`examples/eguidev_host.rs` is a
  worked template). Canopy wires in none of them; its VM is driven synchronously
  (`vm.step_with_limits` in `run_target`).
- The app VM has no durable `require` source, no mode stack (flat `InputMap`
  modes with a default-mode fallback), no script journal, and no MCP bootstrap
  tool. MCP tools today: `script_eval`, `script_api`, `fixtures`, plus live
  `apply_fixture`.
- Stack layout gives overlapping children and reverse hit-testing, but focus
  traversal has no subtree inertness and there is no projection-hosted popup.

Items are ordered by priority; stages 1-4 are the core agent-native thrust. Each
stage is a coherent slice. After any stage the workspace passes `cargo xtask
tidy` and the relevant focused tests. Items marked **(oxau)** are extensions to
the oxau runtime itself. During development the sibling path dependencies stay
as-is; the release path is deferred to the end of this document.

1. The declaration model

Today every `.d.luau` in the stack is assembled by hand: oxau modules return raw
declaration strings, `HostTypeBuilder::declaration` takes a snippet, canopy's
`base_api.rs` pairs handlers with signature strings, and `defs.rs` concatenates
text. oxau parses and audits declarations but composes nothing; eguidev and
verber hand-assemble the same way. Build the typed model once, in oxau, and
render everything through it. Later stages (structural `CommandArg` types,
return metadata, NodeId-as-class, app modules) all need this model, so it lands
first.

Design — a new `oxau::decl` module (no VM dependency):

```rust
pub enum Ty {
    Boolean, Number, String, Nil, Any, Unit,
    Named(Cow<'static, str>),          // NodeId, OpenOpts
    Literal(Cow<'static, str>),        // "Up" (string-literal type)
    Optional(Box<Ty>),                 // T?
    Array(Box<Ty>),                    // {T}
    Map(Box<Ty>, Box<Ty>),             // {[K]: V}
    Union(Vec<Ty>),                    // A | B
    Table(Vec<Field>),                 // { name: T, ... }
    Function(Box<FnSig>),              // (a: T) -> R
}
pub struct Field { pub name: Cow<'static, str>, pub ty: Ty, pub doc: Option<Cow<'static, str>> }
pub struct Param { pub name: Cow<'static, str>, pub ty: Ty, pub doc: Option<Cow<'static, str>> }
pub struct FnSig { pub params: Vec<Param>, pub returns: Vec<Ty> }

pub struct DeclBuilder { /* items, name registry */ }
impl DeclBuilder {
    pub fn type_alias(&mut self, name: &str, doc: &str, ty: Ty);   // export type Name = ...
    pub fn class(&mut self, name: &str) -> ClassBuilder<'_>;       // declare class (methods/props)
    pub fn global(&mut self, name: &str, doc: &str, ty: Ty);       // declare name: ...
    pub fn function(&mut self, name: &str, doc: &str, sig: FnSig); // declare function ...
    pub fn render(&self) -> Result<String, DeclError>;
}
```

Properties: deterministic ordering; duplicate names with identical bodies dedup
silently (two commands may share one `OpenOpts`), differing bodies are an error;
docs render as `---` and `--- @param` lines with one formatter; `render`
self-validates by re-parsing its output with `allow_declaration_syntax` (oxau
already has the parser). The `SurfaceSpec` audit keeps consuming rendered text,
so there is no second source of truth to drift.

Canopy then re-grounds command metadata on the model. `CommandType` moves from a
const string to functions — fn pointers keep `CommandSpec` const-constructible —
which finally permits generic container impls:

```rust
pub trait CommandType {
    fn luau_ty() -> Ty;
    fn luau_decls(reg: &mut DeclRegistry) {}     // named types this value contributes
}
impl<T: CommandType> CommandType for Option<T> {
    fn luau_ty() -> Ty { Ty::Optional(Box::new(T::luau_ty())) }
    fn luau_decls(reg: &mut DeclRegistry) { T::luau_decls(reg) }
}
pub struct CommandTypeSpec {
    pub rust: &'static str,
    pub ty: fn() -> Ty,                          // was: luau: Option<&'static str>
    pub decls: fn(&mut DeclRegistry),
    pub doc: Option<&'static str>,
}
```

1. [ ] **(oxau)** Add `oxau::decl`: `Ty`/`Field`/`Param`/`FnSig`, `DeclBuilder`
       with type aliases, classes, globals, table globals, and doc comments;
       name dedup/conflict detection; render self-check by re-parsing. Accept
       the model in `HostTypeBuilder::declaration` and
       `SurfaceSpecBuilder::declaration_global` alongside the string forms.
2. [ ] Convert `CommandType` to `fn luau_ty() -> Ty` plus `fn luau_decls`, store
       fn pointers in `CommandTypeSpec`, and replace the
       `impl_option/vec/string_map_command_type` macro lists in
       `core/commands.rs` with generic `Option<T>`/`Vec<T>`/map impls.
3. [ ] Render canopy declarations through the builder: `BaseFunction` carries a
       typed `FnSig` instead of a signature string; `defs.rs` builds a
       `DeclBuilder` model for the canopy table, owner tables, and collected
       type aliases; migrate the `preamble.d.luau` record types (`NodeInfo`,
       `TreeNode`, `CommandInfo`, ...) into the model so nothing ships as
       unchecked text.
4. [ ] Prove the API on a second consumer: port eguidev's or verber's
       hand-assembled declarations onto `oxau::decl` and fold what that teaches
       back into the builder before stage 2 leans on it.

2. A contract you can trust

The generated surface still leaks: structs type as `any`, returns are invisible
to `canopy.commands()`, node ids are forgeable numbers, and host errors collapse
to strings. Close every fidelity gap, on the stage-1 model, so an agent can rely
on the contract completely — the typechecker is the rail the program runs on.

1. [ ] Make `#[derive(CommandArg)]` generate structural types. From

       ```rust
       /// Options controlling `editor::open`.
       #[derive(CommandArg, Serialize, Deserialize)]
       pub struct OpenOpts {
           /// Path to open.
           pub path: String,
           /// One-based line to jump to.
           pub line: Option<u32>,
       }
       ```

       derive `luau_ty() -> Ty::Named("OpenOpts")` and a `luau_decls` that
       registers the alias (recursing into field types), rendering as

       ```luau
       --- Options controlling `editor::open`.
       export type OpenOpts = {
           --- Path to open.
           path: string,
           --- One-based line to jump to.
           line: number?,
       }
       ```

       Names are unqualified; two distinct types sharing a name is a
       finalize-time error telling the author to rename. Do the same for
       `derive(CommandEnum)` literal unions via `Ty::Union` of `Ty::Literal`.
2. [ ] Finish return metadata: add `ret`/`ret_doc` to the records built by
       `command_info_to_arg`, declare them on the model-generated `CommandInfo`
       type (one definition, so record and declaration cannot drift), and
       populate `CommandTypeSpec.doc` for parameters and returns from doc
       comments instead of leaving discovery docs empty.
3. [ ] Make `NodeId` unforgeable: represent node ids script-side as an oxau
       `HostType` userdata (`NodeHandle`), add an `ArgValue::Node(NodeId)`
       variant so handles survive calls and returns, and validate generation on
       the way in. **(oxau)** Add a per-`HostType` marshal hook, e.g.
       `.marshal(|h: &NodeHandle| MarshaledValue::String(token(h)))` — today
       `value_marshal.rs` hardcodes `Opaque("userdata")` and JSON conversion
       rejects it, so without the hook a script returning a NodeId breaks the
       MCP eval boundary.
4. [ ] Adopt typed host errors end to end: raise command failures with
       `RuntimeError::structured` carrying `kind`/`command`/`owner` script
       fields (`no_target`, `unknown_command`, `type_mismatch`, ...) and a
       `with_payload(error::Error)` for the host side; recover the payload via
       `payload_ref` at `ScriptError` and `MarshaledScriptError`/MCP exit
       boundaries so `ScriptEvalOutcome.error` reports structured categories.
       Stop collapsing `canopy_to_host` failures to strings.
5. [ ] Expose the surface-semantics runtime pieces: `canopy.resolve(owner) ->
       NodeId?` built on `CommandResolver`, `available: boolean` and
       `target: NodeId?` on `canopy.commands()` records, and anchor-semantics
       documentation in the generated preamble so the contract states the rules
       agents operate under.

3. Persistent scripting: configuration, customization, modules

Today the app surface has no durable module source; user customization is limited
to whatever `run_config` happens to eval after startup. Open the persistent avenue
using the same typed surface agents see. Customization, configuration, and
automation are the same activity at different lifetimes.

1. [ ] Wire oxau `ModuleSource` into the app surface with two durable roots:
       `@user` (per-user, e.g. `~/.config/<app>/`) and `@project` (nearest
       `.canopy/` directory), registered as resolver aliases over a
       `FilesystemModuleSource` and installed via
       `SurfaceSpecBuilder::module_source`. Canopy's host is synchronous, so
       bridge with `SyncModuleSource`; invalidate deliberately by bumping the
       source epoch. Conformance-check paired `.luau`/`.d.luau` modules with
       `CheckedFrontend::check_conformance`, cached by `ConformanceFingerprint`.
2. [ ] Layer startup scripts as app defaults, `@user/init.luau`, and
       `@project/init.luau`, each strict-checked and run against the full surface
       at startup. Replace the ad-hoc `run_config` path so keybindings, mode
       setup, and app settings are all ordinary scripts.
3. [ ] Replace the flat input mode with a push/pop mode stack: explicit
       inheritance rules, binding resolution that walks the stack before the
       default mode fallback, `canopy.push_mode`/`canopy.pop_mode` script APIs,
       and updated editor, command-mode, and binding-discovery call sites.
4. [ ] Let apps register typed oxau `NativeModule`s beyond the derived command
       surface — for example a document-like buffer API with search and
       diff-based edits — with declarations built on `oxau::decl` and audited
       through the same `SurfaceSpec` as the generated surface.

4. The agent loop: drive, wait, observe, record

Make a single eval a complete, reliable scenario: setup, act, wait, assert,
report. Agent activity should be a durable record rather than an invisible side
effect.

1. [ ] Add a zero-boilerplate agent entry point, such as `canopy::launch(Loader)`,
       that wires the app root, fixtures, config, MCP server, smoke runner, eval,
       and API output for every binary. `canopyctl` already has the command set;
       the todo example should become a consumer of the harness, not the template.
2. [ ] Add predicate waits on the live path: `canopy.wait_for(fn)` plus node and
       screen variants. Concretely: evaluate scripts through oxau's async driver
       (`call_protected_owned_async`) polled from the canopy runloop — while the
       eval future is `Pending`, pump events and redraw; `wait_for` is an
       `AsyncHostFunction` that re-enters the stashed predicate between pumps
       via `HostCtx::call_protected` (`eguidev_host.rs` is a working template).
       Timeouts use `Cancel::after` and surface as the typed `ScriptTimeout`;
       no sleeps anywhere.
3. [ ] Enrich observation: expose styled screen capture as cells with attributes,
       node-region cropping, `route_trace`, `diagnostic_dump`, and `help_snapshot`
       through the script API. Keep a text-only screen helper for simple tests.
4. [ ] Add a script journal: the live runtime records every eval, with source,
       origin, outcome, logs, assertions, and timing, as a durable replayable
       record. Add `canopyctl replay` so successful agent sessions can become
       smoke tests and inspector evidence.
5. [ ] Align the MCP surface on bootstrap plus exec: add a `bootstrap` tool that
       returns the operating guide, the surface (or its digest), and current
       availability; keep contract discovery available before app startup
       through the headless factory path. Fixtures and API discovery should also
       be reachable from the script surface, not only as side MCP tools.
6. [ ] Write the agentic development loop guide: fixtures, smoke scripts, MCP
       eval, waits, screen assertions, replay, and promotion into tests. Extend
       `cargo xtask tidy` so checked-in `.luau` files strict-typecheck against
       their app's generated surface; examples and smoke scripts must not drift.

5. Runtime hardening and observability

These items reduce drift and make behavior easier to prove as the runtime grows.
They land before the overlay work. In particular, the unified resolver is the
foundation the overlay input semantics build on.

1. [ ] Wire the bench targets (`crates/canopy/benches/core.rs`,
       `crates/canopy-widgets/benches/{editor,rendering}.rs`) into `xtask` with
       a committed baseline and a regression check.
2. [ ] Unify routing so command availability, help, diagnostics, key handling,
       mouse handling, and route traces share one resolver. `CommandResolver`,
       `routing.rs`, and `help_snapshot` each walk the tree today.
3. [ ] Formalize external event injection for async apps: document and bless the
       existing `AutomationHandle`, callback queue, and `Wake` event pattern
       before considering a full async runtime.
4. [ ] Add conceptual docs for layout and the widget-author contract so users do
       not need to infer those rules from code. Fold the stale `cargo xtask docs`
       reference in `docs/fixtures.md` into whatever doc-check command actually
       exists or is added.

6. Overlays on draw order, and the inspector

Overlap needs no z-order. Draw order is tree traversal order; mouse hit-testing
already resolves topmost-first for overlapping stack children; keys are
focus-routed, and focus traversal walks the same tree order. Three mechanisms
complete the picture: subtree-level focus inertness so a modal can sit over a
visible-but-inert background, a full-area transparent backdrop so outside-click
dismissal is plain hit-testing, and a projection service so a deep widget can ask
an ancestor to host its popup. The overlay still draws only within that host's
managed space. The detailed spec is deliberately deferred until the script drive
and runtime hardening are done. The inspector is the payoff: agent activity
becomes legible to the human.

1. [ ] Spec the overlay contract after stages 1-5: draw order as the stacking
       model; subtree-level focus inertness for modal trapping; backdrop-based
       dismissal; and non-modal overlays that see keys first and fall through
       what they do not handle. `accept_focus` gives per-node inertness today,
       but `Preorder` descends unconditionally, so choose a focus scope or a
       subtree-pruning focus flag stored in the tree.
2. [ ] Build the overlay projection service: a widget asks an ancestor host
       (root, or any widget managing a full view) to lay out an overlay subtree
       painted after its other children, positioned relative to the source node's
       screen rect, and torn down when the source unmounts. Avoid manual plumbing
       through intermediate widgets.
3. [ ] Rebase `modal.rs`, `dropdown.rs`, and `Root`'s help/inspector handling on
       the overlay service. Dropdown expansion currently shifts layout; root
       help uses stack overlap plus hand-managed hidden flags, dimming, and focus
       toggles. Replace those local mechanisms with the shared service.
4. [ ] Rebuild the inspector as a script-surface client in an overlay: tree,
       commands, bindings, route traces, logs, script journal, replay, and diff.
       Do not create a parallel internal API.

## Release path (deferred)

Publishing canopy requires the sibling path dependencies — oxau, tmcp, itty,
itty-script — to be published or vendored first. Nothing in stages 1-6 changes
that calculus; revisit after stage 4 lands, when the oxau extensions marked
**(oxau)** have stabilized into a publishable API.
