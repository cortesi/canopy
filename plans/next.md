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
- The registry is already sealed at `finalize_api()` (`Canopy::add_commands`
  returns `InvalidOperation` after it), so the static-surface guarantee holds
  today. What's missing is the script-side half: a `NoTarget` failure is
  flattened twice — `CommandError::NoTarget` is wrapped as
  `error::Error::Script(format!(...))` in `dispatch_command_by_name`, then
  collapsed to a plain string `RuntimeError` in `canopy_to_host` — and
  `CommandResolver::availability` exists in Rust
  (`command_availability_from_focus`, `CommandAvailability`,
  `CommandResolution`) but is not exposed to scripts.
- The base `canopy` API registration and declaration are generated from one Rust
  table (`base_api.rs::CANOPY_FUNCTIONS`, a `const` table of
  `BaseFunction { name, docs, signature, handler }`), but each entry carries a
  hand-written Luau signature *string* — typed by convention, not construction.
  Because the table is `const`, typed signatures must become `fn() -> FnSig`
  pointers (or the table stops being const).
- `#[derive(CommandArg)]` emits `LUAU_TYPE = "any"` for every type it touches.
  `#[derive(CommandEnum)]` already renders quoted literal unions, but as a
  hand-formatted string const. `Option`/`Vec`/string-map support is enumerated
  macro lists in `core/commands.rs` because `CommandType::LUAU_TYPE` is a const
  string, which blocks generic impls.
- Command returns reach owner declarations via `CommandReturnSpec`, but the
  `CommandInfo` records from `canopy.commands()` carry no return info, and
  `CommandTypeSpec.doc` is always `None`.
- `NodeId` is declared `declare class NodeId` but crosses the boundary as a
  forgeable number: `node_id_to_arg` packs the slotmap key into an int, and
  `node_id_from_value` accepts any integer (negatives wrap through `as u64`) or
  any whole non-negative float.
- oxau has no declaration model: `NativeModule::declaration` returns `&str`
  (in `oxau-vm-api`), `HostTypeBuilder::declaration` takes a snippet string (in
  `oxau-vm`), and `SurfaceSpecBuilder::declaration_global` stores text that
  `DeclarationGlobalSpec::source` `format!`s into a declaration. The
  `SurfaceSpec` audit (`validate_host_modules`) parses declarations with
  `allow_declaration_syntax` and structurally diffs them against runtime
  bindings, but composes nothing. `oxau-vm-api` has no internal dependencies —
  it is the deliberately stable embedding crate — so a declaration model it
  returns must itself be dependency-free; hence a new bottom crate re-exported
  as `oxau::decl`. Userdata marshaling is hardcoded to `Opaque("userdata")`
  (`oxau-vm/src/value_marshal.rs`) with no per-`HostType` hook, and JSON
  conversion rejects opaques, so userdata cannot cross an MCP eval boundary at
  all today.
- oxau already has the pieces stages 4-5 need: `ModuleSource` and
  `SyncModuleSource` with epoch invalidation, `FilesystemModuleSource` with the
  `FilesystemSourceEpoch` handle, `CheckedFrontend::check_conformance` with
  `ConformanceFingerprint`, `RuntimeError::structured`/`with_payload` with
  `payload_ref` recovery at `ScriptError` and `MarshaledScriptError`, async host
  functions with `HostCtx::call_protected` and `Vm::call_protected_owned_async`,
  and `Cancel::after` with timeouts surfacing as `RuntimeErrorKind::Deadline` at
  the VM layer and `EvalErrorKind::Timeout` at the eval layer
  (`examples/eguidev_host.rs` is a worked template). Canopy wires in none of
  them; its VM is driven synchronously (`vm.step_with_limits` in `run_target`)
  and its profile installs no `require` source.
- The app VM has no mode stack (flat `InputMap` modes with a default-mode
  fallback), no script journal, and no MCP bootstrap tool. MCP tools today:
  `script_eval`, `script_api`, `fixtures` on the headless server, plus
  `apply_fixture` on the live server; `canopyctl mcp` serves its own
  differently named set (`connect`, `disconnect`, `eval`, `apply_fixture`,
  `api`, `fixtures`). The only persistent-config hook is `run_config` — a bare
  `eval_script` of a file path during app setup, used by the todo example for
  keybinding overrides.
- Stack layout gives overlapping children and reverse (topmost-first)
  hit-testing, but focus has no subtree inertness: neither `accept_focus` nor
  the `hidden` flag is inherited, `Preorder` descends unconditionally, and
  although layout zeroes hidden subtrees' views, every focus helper falls back
  to `require_view = false`, which can reach into hidden subtrees. There is no
  projection-hosted popup.

Items are ordered by priority; stages 1-5 are the core agent-native thrust. Each
stage is a coherent slice. After any stage the workspace passes `cargo xtask
tidy` and the relevant focused tests. Items marked **(oxau)** are extensions to
the oxau runtime itself. During development the sibling path dependencies stay
as-is; the release path is deferred to the end of this document.

1. (oxau) The declaration builder

Today every `.d.luau` in the stack is assembled by hand: oxau modules return raw
declaration strings, `HostTypeBuilder::declaration` takes a `declare class`
snippet, `SurfaceSpecBuilder` formats global declarations from text, canopy's
`base_api.rs` pairs handlers with signature strings, and `defs.rs` concatenates
text. oxau parses and audits declarations but composes nothing; eguidev and
verber hand-assemble the same way. Build the typed model once, in oxau, and
render everything through it. Every later stage (structural `CommandArg` types,
return metadata, NodeId-as-class, app modules) needs this model, so it lands
first, as a self-contained oxau work package.

**Crate placement.** A new `oxau-decl` crate with zero dependencies,
re-exported as `oxau::decl`. It must sit below `oxau-vm-api`, because
`NativeModule::declaration` returns the model and `oxau-vm-api` is the
deliberately dependency-free stable embedding crate — `oxau-decl` inherits that
constraint: pure data plus a renderer, no parser. The model does not go into
`oxau-vm-api` itself because it is independently useful (derive macros, doc
tooling, embedders without the vm) and vm-api should stay a pure embedding
interface. Self-validation needs no parser dependency either: `oxau-decl` takes
`oxau-ast` as a dev-dependency for a render-then-reparse property test (every
rendered module must parse under `allow_declaration_syntax`), and at runtime
the existing `SurfaceSpec` audit re-parses everything anyway.

**Relation to existing type models.** oxau already has two type
representations: the syntactic `oxau-ast::ast::Type` (span-carrying,
parse-shaped) and the semantic `oxau-typecheck::types::TypeKind`
(arena/`TypeId`-based, inference-shaped), plus the derived `ModuleSchema`
surface in `oxau-typecheck::schema`. `decl::Ty` is deliberately a third: an
*authoring* model — owned, span-free, doc-carrying, cheap to construct from
derive macros and host-registration code. Rendered text stays the interchange
format between the models: the builder renders, the audit parses and diffs.
No model-to-model conversion, no second source of truth.

**The model.** Items are plain values built with `new(...)` plus with-style
methods; `DeclBuilder` is an ordered collection with validation. No closure
DSLs, no entry handles, no sub-builder lifetimes — the same `Class` value that
declares a host type in `oxau-vm` drops into a module declaration unchanged.

```rust
use std::borrow::Cow;

pub type Name = Cow<'static, str>;

/// A Luau type expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Boolean,
    Number,
    String,
    Nil,
    Any,
    /// Reference to an alias, class, or `extern_ty` name.
    Named(Name),
    /// String-literal singleton: `"Up"`.
    Literal(Name),
    /// `T?`
    Optional(Box<Ty>),
    /// `{T}`
    Array(Box<Ty>),
    /// `{ [K]: V }`
    Map(Box<Ty>, Box<Ty>),
    /// `A | B`
    Union(Vec<Ty>),
    /// `A & B` — e.g. `TreeNode = NodeInfo & { children: {TreeNode} }`
    Intersection(Vec<Ty>),
    /// `{ name: string, ... }`
    Table(Vec<Field>),
    /// `(a: string) -> number`
    Function(Box<FnSig>),
}

impl Ty {
    pub fn named(name: impl Into<Name>) -> Ty;
    /// Union of string literals — the `CommandEnum` shape.
    pub fn literals(values: impl IntoIterator<Item = impl Into<Name>>) -> Ty;
    pub fn table(fields: impl IntoIterator<Item = Field>) -> Ty;
    /// Flattens nested unions, dedups, folds `Nil` members into `Optional`.
    pub fn union(tys: impl IntoIterator<Item = Ty>) -> Ty;
    pub fn map(key: Ty, value: Ty) -> Ty;
    pub fn func(sig: FnSig) -> Ty;
    /// Idempotent: `t.optional().optional() == t.optional()`.
    pub fn optional(self) -> Ty;
    pub fn array(self) -> Ty;
}

pub struct Field { pub name: Name, pub ty: Ty, pub doc: Option<Name> }
pub struct Param { pub name: Name, pub ty: Ty, pub doc: Option<Name> }

pub struct FnSig {
    pub params: Vec<Param>,
    pub varargs: Option<Ty>,
    pub returns: Vec<Ty>,
}

impl FnSig {
    pub fn new() -> Self;
    /// `From<(&'static str, Ty)>` keeps undocumented params terse.
    pub fn param(self, param: impl Into<Param>) -> Self;
    pub fn varargs(self, ty: Ty) -> Self;
    /// Call repeatedly for multi-returns; never called means `()`.
    pub fn ret(self, ty: Ty) -> Self;
}

pub struct Alias  { /* name, doc, ty  — `export type Name = T` */ }
pub struct Global { /* name, doc, ty  — `declare name: T` */ }
pub struct Func   { /* name, doc, sig — `declare function name(...): R` */ }
pub struct Class  { /* name, doc, props: Vec<Field>, methods — `declare class` */ }
```

Two shape decisions keep invalid declarations unrepresentable. There is no
`Unit` type: a function's returns are a pack (`returns: Vec<Ty>`, empty renders
`()`), so unit cannot appear in a type position. And variadics are an `FnSig`
slot rather than a `Ty` variant, so `...any` cannot appear outside a parameter
list. `Func` stays distinct from `Global` with a function type because the
audit classifies `declare function` and value globals differently.

```rust
pub struct DeclBuilder { /* ordered items plus name registry */ }

impl DeclBuilder {
    pub fn new() -> Self;
    /// Banner comment in the rendered output (`-- ===== title =====`).
    pub fn section(&mut self, title: impl Into<Name>);
    pub fn alias(&mut self, alias: Alias);
    pub fn class(&mut self, class: Class);
    pub fn global(&mut self, global: Global);
    pub fn function(&mut self, func: Func);
    /// Declare a name as defined elsewhere (another module, the preamble).
    pub fn extern_ty(&mut self, name: impl Into<Name>);
    pub fn finish(self) -> Result<DeclModule, DeclErrors>;
}

pub struct DeclModule { /* validated, ordered items */ }

impl DeclModule {
    pub fn render(&self) -> String;
}
```

Construction reads top-down and every item, field, and param takes `.doc(...)`:

```rust
let mut decl = DeclBuilder::new();
decl.extern_ty("NodeId");
decl.alias(
    Alias::new("OpenOpts", Ty::table([
        Field::new("path", Ty::String).doc("Path to open."),
        Field::new("line", Ty::Number.optional()).doc("One-based line to jump to."),
    ]))
    .doc("Options controlling `editor::open`."),
);
decl.global(Global::new("editor", Ty::table([
    Field::new("open", Ty::func(
        FnSig::new().param(("opts", Ty::named("OpenOpts"))).ret(Ty::Boolean),
    )),
])));
let rendered = decl.finish()?.render();
```

renders as:

```luau
--- Options controlling `editor::open`.
export type OpenOpts = {
    --- Path to open.
    path: string,
    --- One-based line to jump to.
    line: number?,
}

declare editor: {
    open: (opts: OpenOpts) -> boolean,
}
```

**Correctness contract.**

- `finish` validates and reports *all* errors, not the first: item, param, and
  member names must be valid non-reserved Luau identifiers (table field names
  are exempt — the renderer bracket-quotes them); a name registered twice with
  an identical body dedups silently to its first position (two commands may
  share one `OpenOpts`), while differing bodies or differing kinds are a
  conflict error carrying both rendered forms; every `Ty::Named` must resolve
  to an alias, class, or `extern_ty` in the module, so a typo'd type reference
  fails at build time rather than at script typecheck time.
- `DeclModule::render` is infallible: invalid modules cannot be constructed, so
  rendering cannot fail. This splits the API cleanly — all fallibility lives in
  `finish`.
- Rendering is deterministic (insertion order, no hash-map iteration),
  parenthesizes by precedence (`(A | B)?`, function types inside unions),
  escapes string literals, and formats all docs through one formatter: wrapped
  `---` lines, `--- @param`/`--- @return` tags for functions.
- The render-then-reparse property test (dev-dep on `oxau-ast`) pins the output
  grammar; the `SurfaceSpec` audit keeps consuming rendered text at runtime, so
  there is no second source of truth to drift.

**Integration.** One enum makes migration mechanical and keeps the audit's
input unchanged:

```rust
pub enum DeclSource<'a> {
    Text(&'a str),
    Model(&'a DeclModule),
}

impl DeclSource<'_> {
    /// `Text` borrows; `Model` renders.
    pub fn render(&self) -> Cow<'_, str>;
}

// oxau-vm-api — implementors build their DeclModule once and return a borrow.
pub trait NativeModule: Send + Sync {
    fn name(&self) -> &str;
    fn declaration(&self) -> DeclSource<'_>;
    fn build(&self, builder: &mut dyn ModuleBuilder);
}

// oxau-vm — typed alternative to `declaration(String)`.
impl<T: Send + 'static> HostTypeBuilder<T> {
    pub fn class(self, class: decl::Class) -> Self;
}

// oxau
impl SurfaceSpecBuilder {
    pub fn declaration_global_ty(self, name: &str, ty: decl::Ty) -> Self;
}
```

All three consumers of declaration text — the `validate_host_modules` audit,
the `host_module_manifest_version` hash, and the checker's
`BuiltinDefinitionModule` injection — flow through the single
`declaration().render()` choke point, and `DeclSource::Text` keeps every
existing string declaration working during migration. Because `declaration()`
is called more than once per startup, implementors build the model at
construction time and return `DeclSource::Model(&self.decl)`.

1. [x] **(oxau)** Create the `oxau-decl` crate: `Ty`/`Field`/`Param`/`FnSig`,
       the `Alias`/`Global`/`Func`/`Class` items, `DeclBuilder`/`DeclModule`
       with finish-time validation (identifier checks, identical-body dedup,
       conflict detection, `Named`-reference resolution against declared and
       `extern_ty` names), the deterministic renderer with one doc formatter,
       and the render-then-reparse property test as a dev-dependency on
       `oxau-ast`. Re-export as `oxau::decl`.
2. [x] **(oxau)** Thread `DeclSource` through `oxau-vm-api::NativeModule::
       declaration_source`, add `HostTypeBuilder::class` and
       `SurfaceSpecBuilder::declaration_global_ty`, and route the audit, the
       manifest hash, and checker injection through one `render` call so text
       and model declarations are indistinguishable downstream. This lands as a
       staged adapter rather than changing the object-safe `declaration()` text
       method in the same patch; model-owning modules override
       `declaration_source()`.
3. [ ] **(oxau)** Migrate oxau's own surfaces onto the model:
       `GlobalValueModule`, `DeclarationGlobalSpec`, and the example hosts
       (`eguidev_host`, `verber_tool_host`, `agent_host`, `demo_server`,
       `embed_host`, `vm_only`, `analyze`) — the in-repo proving ground for
       construction ergonomics.
4. [ ] Prove the API on an external consumer: port eguidev's or verber's
       hand-assembled declarations onto `oxau::decl` and fold what that teaches
       back into the builder before stage 2 leans on it.

2. Re-ground canopy's command metadata on the model

Canopy's command system renders Luau types from const strings. Move it onto
`oxau::decl`. `CommandType` becomes function-based — fn pointers keep
`CommandSpec` and `CANOPY_FUNCTIONS` const-constructible — which finally
permits generic container impls:

```rust
pub trait CommandType {
    fn luau_ty() -> Ty;
    fn luau_decls(reg: &mut DeclRegistry) {}
}

impl<T: CommandType> CommandType for Option<T> {
    fn luau_ty() -> Ty { T::luau_ty().optional() }
    fn luau_decls(reg: &mut DeclRegistry) { T::luau_decls(reg) }
}

pub struct CommandTypeSpec {
    pub rust: &'static str,
    pub ty: fn() -> Ty,
    pub decls: fn(&mut DeclRegistry),
    pub doc: Option<&'static str>,
}
```

`DeclRegistry` is a thin canopy-side wrapper over `DeclBuilder` that tracks
in-flight registrations so recursive and shared types terminate; conflicting
redefinitions still error when the builder finishes inside `finalize_api()`.

1. [x] Convert `CommandType` to `fn luau_ty() -> Ty` plus `fn luau_decls`,
       store fn pointers in `CommandTypeSpec`, and replace the
       `impl_option/vec/string_map_command_type` macro lists in
       `core/commands.rs` with generic `Option<T>`/`Vec<T>`/map impls. Update
       `derive(CommandEnum)` to emit `Ty::literals` instead of a hand-quoted
       union string, and drop the `()` impl — unit returns already flow through
       `CommandReturnSpec::Unit`.
2. [x] Render canopy declarations through the builder: `BaseFunction.signature`
       becomes `fn() -> FnSig` (the table stays `const`); `defs.rs` builds one
       `DeclBuilder` covering the preamble record types (`NodeInfo`,
       `TreeNode`, `CommandInfo`, ...), the `NodeId` class, the canopy table,
       the fixtures function, and the owner tables, so nothing ships as
       unchecked text. Keep the rendered output sectioned
       (`section("Application Commands")`) for readability.

3. A contract you can trust

The generated surface still leaks: structs type as `any`, returns are invisible
to `canopy.commands()`, node ids are forgeable numbers, and host errors collapse
to strings. Close every fidelity gap, on the stage 1-2 model, so an agent can
rely on the contract completely — the typechecker is the rail the program runs
on.

1. [x] Make `#[derive(CommandArg)]` generate structural types. From

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

       derive `luau_ty() -> Ty::named("OpenOpts")` and a `luau_decls` that
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

       Names default to the Rust identifier, but the derive accepts an explicit
       script name when a collision would otherwise occur:

       ```rust
       #[derive(CommandArg, Serialize, Deserialize)]
       #[canopy(type_name = "EditorOpenOpts")]
       pub struct OpenOpts {
           pub path: String,
           pub line: Option<u32>,
       }
       ```

       Two distinct structural types that still render to the same script name
       are a `finalize_api()`-time error — exactly the stage-1 builder's
       conflict detection surfacing through the registry. Do the same for
       `derive(CommandEnum)` via `Ty::literals`, with the same naming override.
2. [x] Finish return metadata: add `ret`/`ret_doc` to the records built by
       `command_info_to_arg`, declare them on the model-generated `CommandInfo`
       type (one definition, so record and declaration cannot drift), and
       populate `CommandTypeSpec.doc` for parameters and returns from doc
       comments instead of leaving discovery docs empty.
3. [x] Make `NodeId` unforgeable: represent node ids script-side as an oxau
       `HostType` userdata (`NodeHandle`) and add an internal
       `ArgValue::Node(NodeId)` variant so command dispatch can carry handles
       without pretending they are JSON. Convert `ArgValue::Node` to a scoped
       userdata for script values, reject it from ordinary `ArgValue::to_json`
       unless an explicit external token has been requested, and validate
       `core.nodes.contains_key(id)` whenever a handle enters a command.

       ```rust
       #[derive(Clone, Copy)]
       struct NodeHandle {
           id: NodeId,
       }

       fn node_id_to_scoped<'s>(
           scope: &Scope<'s>,
           id: NodeId,
       ) -> Result<ScopedValue<'s>, RuntimeError> {
           Ok(scope.create_userdata(NodeHandle { id })?.into_lua(scope)?)
       }

       fn node_id_from_scoped(
           core: &Core,
           handle: &NodeHandle,
       ) -> Result<NodeId, RuntimeError> {
           if core.nodes.contains_key(handle.id) {
               Ok(handle.id)
           } else {
               Err(RuntimeError::structured(
                   "node_invalid",
                   [ScriptErrorField::new("kind", "node_invalid")],
               ))
           }
       }
       ```

       The landed path keeps oxau unchanged for now: Canopy's ordinary
       `ArgValue::to_json_value` rejects `NodeId`, while the MCP/smoke boundary
       calls `to_external_json_value`, which renders a descriptive opaque token
       that cannot be fed back into scripts as a forged handle.
4. [x] Adopt typed host errors end to end: raise command failures with
       `RuntimeError::structured` carrying `kind`/`command`/`owner` script
       fields (`no_target`, `unknown_command`, `type_mismatch`, ...) and attach
       a cloneable normalized host payload, not the raw `error::Error` enum
       (which can contain `anyhow::Error`). Stop pre-flattening in
       `dispatch_command_by_name` (which wraps `CommandError` into
       `error::Error::Script(format!(...))` before `canopy_to_host` ever sees
       it). Recover the payload via `payload_ref` at `ScriptError` and
       `MarshaledScriptError`/MCP exit boundaries so `ScriptEvalOutcome.error`
       reports structured categories.

       ```rust
       #[derive(Clone, Debug)]
       struct CanopyErrorPayload {
           kind: &'static str,
           command: Option<String>,
           owner: Option<String>,
           message: String,
       }

       fn canopy_to_host(err: &error::Error) -> RuntimeError {
           let payload = CanopyErrorPayload::from(err);
           let mut fields = vec![ScriptErrorField::new("kind", payload.kind)];
           if let Some(command) = payload.command.clone() {
               fields.push(ScriptErrorField::new("command", command));
           }
           if let Some(owner) = payload.owner.clone() {
               fields.push(ScriptErrorField::new("owner", owner));
           }
           RuntimeError::structured(payload.message.clone(), fields).with_payload(payload)
       }
       ```
5. [x] Expose the surface-semantics runtime pieces: `canopy.resolve(owner) ->
       NodeId?` built on `CommandResolver`, `available: boolean` and
       `target: NodeId?` on `canopy.commands()` records, and anchor-semantics
       documentation in the generated preamble so the contract states the rules
       agents operate under.

       ```luau
       local editor_node = canopy.resolve("editor")
       if editor_node == nil then
           error({ kind = "no_target", owner = "editor" })
       end
       canopy.cmd_on(editor_node, "editor::insert", "hello")
       ```

4. Persistent scripting: configuration, customization, modules

Today the app surface has no durable module source; user customization is
limited to `run_config`, a bare file eval during setup. Open the persistent
avenue using the same typed surface agents see. Customization, configuration,
and automation are the same activity at different lifetimes.

1. [x] Wire oxau `ModuleSource` into the app surface with two durable roots:
       `@user` (per-user, e.g. `~/.config/<app>/`) and `@project` (nearest
       `.canopy/` directory). Implement a Canopy-owned composite `ModuleSource`
       that dispatches those prefixes to `FilesystemModuleSource`s, install it with
       `SurfaceSpecBuilder::module_source`, and invalidate deliberately with the
       filesystem epoch handles. Conformance-check paired `.luau`/`.d.luau`
       modules with `CheckedFrontend::check_conformance`, cached by
       `ConformanceFingerprint`.

       ```rust
       struct CanopyModuleSource {
           user: FilesystemModuleSource,
           project: Option<FilesystemModuleSource>,
       }

       impl ModuleSource for CanopyModuleSource {
           fn resolve(
               &self,
               requester: Option<&ModuleId>,
               request: &[u8],
           ) -> ModuleSourceFuture<ModuleId> {
               ready(self.resolve_root(requester, request))
           }

           fn read(&self, id: &ModuleId) -> ModuleSourceFuture<Vec<u8>> {
               self.source_for(id).read(id)
           }
       }
       ```

       A user/project startup file then looks ordinary and typed:

       ```luau
       local keys = require("@user/keymap")
       local project = require("@project/project")

       keys.bind_editor_defaults()
       project.open_last_workspace()
       ```
2. [x] Layer startup scripts as app defaults, `@user/init.luau`, and
       `@project/init.luau`, each strict-checked and run against the full surface
       at startup. Replace the ad-hoc `run_config` path so keybindings, mode
       setup, and app settings are all ordinary scripts.

       ```rust
       let mut canopy = Canopy::new();
       canopy.set_user_script_root(config_dir.join("my-app"))?;
       canopy.discover_project_script_root_from(current_dir)?;
       canopy.register_startup_script("app", include_str!("defaults.luau"))?;
       canopy.run_startup_scripts()?;
       ```
3. [x] Replace the flat input mode with a push/pop mode stack: explicit
       inheritance rules, binding resolution that walks the stack before the
       default mode fallback, `canopy.push_mode`/`canopy.pop_mode` script APIs,
       and updated editor, command-mode, and binding-discovery call sites.
4. [x] Let apps register typed oxau `NativeModule`s beyond the derived command
       surface — for example a document-like buffer API with search and
       diff-based edits — with declarations built on `oxau::decl` and audited
       through the same `SurfaceSpec` as the generated surface.

5. The agent loop: drive, wait, observe, record

Make a single eval a complete, reliable scenario: setup, act, wait, assert,
report. Agent activity should be a durable record rather than an invisible side
effect.

1. [x] Add a low-boilerplate agent entry point. The concrete API lives in
       `canopy-mcp` as `launch(factory, LaunchMode)` rather than in `canopy::`
       because `canopy-mcp` already depends on `canopy`; putting MCP launch code
       in the core crate would create a dependency cycle. The helper wires API
       output, headless MCP, smoke suites, live MCP, and the crossterm runloop.
       The todo example now consumes the harness instead of hand-rolling that
       scaffolding.

       ```rust
       use canopy_mcp::{LaunchMode, app_factory, launch};

       let factory = app_factory(move || create_app_with_config(&path, config.as_deref()));
       let code = launch(factory, LaunchMode::run_with_mcp(socket_path))?;
       ```
2. [ ] Add predicate waits on the live path: `canopy.wait_for(fn)` plus node and
       screen variants. Concretely: evaluate scripts through oxau's async driver
       (`call_protected_owned_async`) and poll that future from the canopy
       runloop. While the eval future is `Pending`, one active-eval guard owns
       the VM; the app may pump Rust events and redraw, but concurrent script
       execution is rejected with a typed `script_busy` error or queued by an
       explicit policy. `wait_for` is an `AsyncHostFunction` that re-enters the
       stashed predicate via `HostCtx::call_protected` between event pumps
       (`eguidev_host.rs` is a working template). Timeouts use `Cancel::after`
       and surface through the typed eval-layer timeout (`EvalErrorKind::
       Timeout`); no sleeps anywhere.

       ```rust
       fn poll_active_eval(&mut self) -> Option<Result<ScriptEvalOutcome>> {
           let eval = self.active_eval.as_mut()?;
           match eval.poll_once(&mut self.cx) {
               Poll::Pending => {
                   self.pump_rust_events();
                   self.render_if_needed();
                   None
               }
               Poll::Ready(outcome) => {
                   self.active_eval = None;
                   Some(outcome)
               }
           }
       }
       ```

       ```luau
       workspace.open_editor("notes.md")
       canopy.wait_for(function()
           local node = canopy.resolve("editor")
           return node ~= nil and canopy.screen_text():find("notes.md") ~= nil
       end)
       editor.insert("hello")
       ```
3. [x] Enrich observation: expose styled screen capture as cells with attributes,
       node-region cropping, `route_trace`, `diagnostic_dump`, and `help_snapshot`
       through the script API (each exists today only as a Rust method). Keep a
       text-only screen helper for simple tests.
4. [x] Add an in-memory script journal: the live runtime records every eval,
       with source, origin, outcome, logs, assertions, and timing, and exposes
       that record through Rust and Luau. This gives the inspector and MCP
       bootstrap path one source of truth for recent script activity.
5. [x] Persist the script journal as a durable replayable record. Add
       `canopyctl replay` so successful agent sessions can become smoke tests
       and inspector evidence.

       ```sh
       cargo run -p canopyctl -- eval \
         --journal-out tmp/replay.json \
         'return canopy.api():find("declare canopy") ~= nil' \
         -- cargo run -p todo -- mcp :memory:

       cargo run -p canopyctl -- replay tmp/replay.json \
         -- cargo run -p todo -- mcp :memory:
       ```
6. [x] Align the MCP surface on bootstrap: add a `bootstrap` tool that returns
       the operating guide, the full surface plus stable digest, and current
       availability through both the headless factory path and a live app.

       ```rust
       let bootstrap = app.bootstrap()?;
       assert_eq!(bootstrap.api_digest.len(), 16);
       for command in bootstrap.commands {
           println!("{}.{}: {}", command.owner, command.name, command.available);
       }
       ```
7. [x] Converge canopyctl's divergent tool names (`eval`, `api`) with the
       canopy-mcp names. Fixtures and API discovery should also be reachable
       from the script surface, not only as side MCP tools.
8. [x] Write the agentic development loop guide: fixtures, smoke scripts, MCP
       eval, waits, screen assertions, replay, and promotion into tests.
9. [ ] Extend `cargo xtask tidy` so checked-in `.luau` files strict-typecheck
       against their app's generated surface; examples and smoke scripts must
       not drift.

6. Runtime hardening and observability

These items reduce drift and make behavior easier to prove as the runtime grows.
They land before the overlay work. In particular, the unified resolver is the
foundation the overlay input semantics build on.

1. [ ] Wire the bench targets (`crates/canopy/benches/core.rs`,
       `crates/canopy-widgets/benches/{editor,rendering}.rs`) into `xtask` with
       a committed baseline and a regression check. While wiring, declare the
       canopy core bench with `harness = false` — today it is auto-discovered
       under the default harness, which conflicts with criterion's argument
       parsing.
2. [ ] Unify routing: `CommandResolver` and `routing.rs` each implement their
       own tree walk today (subtree-then-ancestor DFS vs the ancestor bubble
       plus the separate `locate_recursive` hit-test); `help_snapshot` already
       delegates to `CommandResolver::availability`. Fold key/mouse routing and
       route traces onto the same resolver so command availability, help,
       diagnostics, and input handling share one walk.
3. [ ] Formalize external event injection for async apps: document and bless the
       existing `AutomationHandle`, callback queue, and `Wake` event pattern
       before considering a full async runtime.
4. [ ] Add conceptual docs for layout and the widget-author contract so users do
       not need to infer those rules from code. Fold the stale `cargo xtask docs`
       reference in `docs/fixtures.md` into whatever doc-check command actually
       exists or is added.

7. Overlays on draw order, and the inspector

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

1. [ ] Spec the overlay contract after stages 1-6: draw order as the stacking
       model; subtree-level focus inertness for modal trapping; backdrop-based
       dismissal; and non-modal overlays that see keys first and fall through
       what they do not handle. `accept_focus` gives per-node inertness today,
       but `Preorder` descends unconditionally and the focus helpers'
       `require_view = false` fallback can reach into hidden subtrees, so choose
       a focus scope or a subtree-pruning focus flag stored in the tree.
2. [ ] Build the overlay projection service: a widget asks an ancestor host
       (root, or any widget managing a full view) to lay out an overlay subtree
       painted after its other children, positioned relative to the source node's
       screen rect, and torn down when the source unmounts. Avoid manual plumbing
       through intermediate widgets.
3. [ ] Rebase `modal.rs`, `dropdown.rs`, and `Root`'s help/inspector handling on
       the overlay service. Dropdown expansion currently grows the widget in
       normal flow and shifts layout; root help uses stack overlap plus
       hand-managed hidden flags, dimming, and focus toggles. Replace those
       local mechanisms with the shared service.
4. [ ] Rebuild the inspector as a script-surface client in an overlay: tree,
       commands, bindings, route traces, logs, script journal, replay, and diff.
       Do not create a parallel internal API.
