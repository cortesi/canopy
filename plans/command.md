# Spec 1 — Typed Command System with Context Injection and Rhai Interop

## 1. Scope

This specification defines a typed command system that:

* Extracts parameter and return types from `#[command]` function definitions at compile time.
* Supports **context injection** (e.g., current `Event`, `mouse::Event`, list row context) without bespoke per-command APIs.
* Supports a **broad set of common argument/return types**.
* Supports **user-defined types** via `serde` (map/array representations), interoperable with Rhai object maps.
* Preserves existing idioms: `ctx.dispatch_command(&...invocation())`, `TermGym::cmd_*()` factory functions, and `call_with(...)` style invocation building.

Commands are synchronous and execute immediately when dispatched.

---

## 2. Terminology

* **Command**: A registered callable unit generated from a Rust function/method annotated with `#[command]`.
* **Invocation**: A runtime request to execute a command with a specific set of user-supplied arguments.
* **Injected parameter**: A parameter populated from `Context` / ambient command scope, not from the invocation’s user arguments.
* **User argument parameter**: A parameter populated from the invocation’s user arguments (positional or named).
* **ArgValue**: The canonical dynamic value representation for command arguments and return values, with a lossless mapping to/from Rhai values.
* **Command scope**: A stack-scoped set of ambient values (e.g., current event snapshot) available during command execution and nested dispatch.

---

## 3. Execution invariants

### 3.1 Synchronous execution

* `ctx.dispatch_command(&invocation)` executes the command **synchronously** on the current call stack.
* Nested dispatch is supported; each dispatch pushes a new command scope frame.

### 3.2 Scope stacking

* `Context` maintains a **stack** of command scope frames.
* Reading injected data (`ctx.current_event()`, `ctx.current_list_row()`, etc.) always uses the **topmost** frame.
* On nested dispatch, the new frame inherits the parent frame by default unless explicitly overridden.

### 3.3 Deterministic binding

* Parameters are bound left-to-right by the generated wrapper.
* For each parameter:

  1. If its type is injectable, it is injected.
  2. Otherwise, it is a user argument.
* A required injected parameter (non-`Option<_>`) **must** be present in scope, otherwise invocation fails with a structured error.
* User argument arity must match, except for `Option<T>` user params (see 6.4).

### 3.4 No implicit ambient dependence requirement

* Commands may use injected parameters, but must remain callable without them by using `Option<InjectedType>` when appropriate.

---

## 4. Canonical dynamic value type

### 4.1 `ArgValue`

`ArgValue` is the command system’s canonical dynamic representation.

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<ArgValue>),
    Map(std::collections::BTreeMap<String, ArgValue>),
    Bytes(Vec<u8>), // optional; can be feature-gated if not needed
}
```

### 4.2 Mapping to/from Rhai

A Rhai bridge converts between `rhai::Dynamic` and `ArgValue`:

* `()` / unit → `ArgValue::Null`
* `bool` → `Bool`
* `i64` → `Int`
* `f64` → `Float`
* `String` → `String`
* `Array` → `Array` (recursive)
* `Map` → `Map` (recursive)
* Unsupported Rhai types (custom Rust types stored inside `Dynamic`) must error unless explicitly registered via the user-defined type pathway (Section 7).

---

## 5. Command declaration and registration

### 5.1 `#[command]` macro responsibilities

For each `#[command]` function/method, the macro generates:

1. A **static descriptor** (`CommandSpec`) including:

   * Command id
   * Name
   * Parameter list (names, kinds, expected types)
   * Return type metadata (for scripting/introspection)
2. An **invoke wrapper** that:

   * Performs injection and argument decoding
   * Calls the underlying Rust function
   * Encodes the return value into `ArgValue`

### 5.2 Descriptor types

```rust
pub struct CommandSpec {
    pub id: CommandId,
    pub name: &'static str,
    pub params: &'static [CommandParamSpec],
    pub ret: CommandReturnSpec,
    pub invoke: fn(target: &mut dyn std::any::Any, ctx: &mut dyn Context, inv: &CommandInvocation) -> Result<ArgValue>,
}

pub struct CommandParamSpec {
    pub name: &'static str,
    pub kind: CommandParamKind,      // Injected or User
    pub ty: CommandTypeSpec,         // for introspection/help/errors
}

pub enum CommandParamKind { Injected, User }

pub enum CommandReturnSpec {
    Unit,
    Value(CommandTypeSpec),
}

pub struct CommandTypeSpec {
    pub rust: &'static str,          // e.g. "isize", "Option<mouse::Event>"
    pub doc: Option<&'static str>,   // optional
}
```

`CommandId` may be a stable string or a stable hash; the spec requires it be unique and deterministic within the program.

### 5.3 Existing factory pattern retained

For a method `select_terminal`, the macro continues to generate:

```rust
impl TermGym {
    pub fn cmd_select_terminal() -> &'static CommandSpec { /* ... */ }
}
```

---

## 6. Invocation building and dispatch

### 6.1 `CommandInvocation`

An invocation may carry positional or named args.

```rust
pub struct CommandInvocation {
    pub id: CommandId,
    pub args: CommandArgs,
}

pub enum CommandArgs {
    Positional(Vec<ArgValue>),
    Named(std::collections::BTreeMap<String, ArgValue>),
}
```

### 6.2 Builder API (Rust call sites)

```rust
impl CommandSpec {
    pub fn call_with(&'static self, args: impl Into<CommandArgs>) -> CommandCall { /* ... */ }
}

pub struct CommandCall {
    spec: &'static CommandSpec,
    args: CommandArgs,
}

impl CommandCall {
    pub fn invocation(self) -> CommandInvocation { /* ... */ }
}
```

Compatibility: existing call sites like `cmd.call_with([index as isize])` remain supported by providing `Into<CommandArgs>` for arrays/slices of supported primitives (Section 7).

### 6.3 Dispatch API

`Context` continues to expose:

```rust
trait Context {
    fn dispatch_command(&mut self, inv: &CommandInvocation) -> Result<ArgValue>;
}
```

If the existing system returns `Result<()>`, it is extended to return `Result<ArgValue>`; `ArgValue::Null` represents unit.

### 6.4 Optional user arguments (`Option<T>`)

For user argument parameters of type `Option<T>`:

* Positional: if the argument is missing, bind `None`.
* Named: if the name key is missing, bind `None`.
* If an argument is present, it must decode as `T`.

Non-`Option<_>` user arguments remain strict: missing → arity error.

---

## 7. Type support

### 7.1 Built-in conversions (required)

The system must support conversion between `ArgValue` and these Rust types:

**Primitives**

* `bool`
* Signed ints: `i8`, `i16`, `i32`, `i64`, `isize`
* Unsigned ints: `u8`, `u16`, `u32`, `u64`, `usize`
* Floats: `f32`, `f64`
* `String`

**Containers**

* `Option<T>`
* `Vec<T>`
* `std::collections::BTreeMap<String, T>`
* `std::collections::HashMap<String, T>` (key must be `String`)

**Tuples (positional only)**

* Up to arity 4: `(A,)`, `(A,B)`, `(A,B,C)`, `(A,B,C,D)`
  (Tuple support is optional if not needed by current call sites; include if you want Rhai ergonomics.)

**Result types**

* Commands may return:

  * `()`
  * `Result<()>`
  * `T`
  * `Result<T>`
    Where `T` is convertible to `ArgValue`.

### 7.2 Range and lossiness rules

* Converting `ArgValue::Int(i64)` to narrower ints must range-check and error on overflow.
* Converting `ArgValue::Float(f64)` to `f32` range-checks.
* Converting float to int is **not implicit**; it must error unless explicitly requested by a future “coercions” feature (out of scope here).

### 7.3 String enums

For Rust enums intended to be script-facing, two options are supported:

* **String mapping** via a derive macro (recommended):

  * `#[derive(CommandEnum)]` generates string ↔ enum conversion
* **Serde mapping** (Section 7.5) via `serde(rename = ...)`

### 7.4 Injection types (required)

The command wrapper must treat the following parameter types as injectable:

* `&mut dyn Context`
* `&dyn Context`
* `Option<&Event>` and `&Event`
* `Option<&mouse::Event>` and `&mouse::Event`
* `Option<ListRowContext>` and `ListRowContext` (defined below)

Injected parameters do **not** consume user arguments.

### 7.5 User-defined types via serde (required)

User-defined types are supported by converting between `ArgValue::{Map,Array,...}` and a serde representation.

#### 7.5.1 Opt-in mechanism

A type is eligible for serde-based conversion if it implements:

```rust
pub trait CommandSerde: serde::Serialize + serde::de::DeserializeOwned + 'static {}
```

Users opt in by either:

* Implementing `CommandSerde` manually, or
* Deriving it via a helper derive macro (recommended):

  * `#[derive(CommandSerde)]` (or `#[derive(CommandArg)]`)
    (The derive may be a no-op that just asserts bounds and implements the marker trait.)

#### 7.5.2 Encoding/decoding rules

* `ArgValue ↔ serde_json::Value` conversion is structural:

  * `Null/Bool/Int/Float/String/Array/Map` map recursively.
* Decoding:

  * `ArgValue` → `serde_json::Value` → `serde_json::from_value::<T>()`
* Encoding:

  * `serde_json::to_value(&t)` → `ArgValue`

#### 7.5.3 Rhai compatibility

Because Rhai object maps naturally represent structured data, a Rhai `Map` converts to `ArgValue::Map`, enabling user-defined serde types to be passed as Rhai maps.

Example Rhai call passing a user struct:

```rhai
cmd("open_terminal", #{ id: 3, title: "prod", pinned: true })
```

---

## 8. Context scope for injection

### 8.1 Required Context accessors

`Context` must expose:

```rust
trait Context {
    fn current_event(&self) -> Option<&Event>;
    fn current_list_row(&self) -> Option<ListRowContext>;
}
```

Both values are maintained in the command scope stack and may be `None`.

### 8.2 Scope lifetime rules

* During widget event dispatch, the dispatcher pushes a scope frame containing the current `Event` snapshot and (if applicable) current list row context.
* During command execution, the same top scope frame is visible to commands.
* Nested dispatch inherits the scope unless overridden.

---

## 9. Error model

Command binding/decoding errors are structured and machine-usable:

```rust
pub enum CommandError {
    UnknownCommand { id: CommandId },
    ArityMismatch { expected: usize, got: usize },
    MissingNamedArg { name: String },
    TypeMismatch { param: String, expected: &'static str, got: &'static str },
    MissingInjected { param: String, expected: &'static str },
    Conversion { param: String, message: String },
    Exec(anyhow::Error),
}
```

Errors originating from command bodies propagate as `CommandError::Exec`.

---

## Addendum A — Staged implementation plan (checklist)

### Stage 1 — Core `ArgValue` and conversion traits

* [ ] Introduce `ArgValue` enum (Section 4.1).
* [ ] Add `TryFrom<ArgValue>` / `Into<ArgValue>` implementations for required primitives.
* [ ] Add container conversions: `Option<T>`, `Vec<T>`, `BTreeMap<String, T>`, `HashMap<String, T>`.
* [ ] Add structured error type `CommandError` and plumb through `dispatch_command`.

### Stage 2 — Command scope + injection plumbing in `Context`

* [ ] Add scope stack to `Context` implementation.
* [ ] Implement `ctx.current_event()` as scope-backed.
* [ ] Implement `ctx.current_list_row()` as scope-backed (may return `None` until list work is implemented).
* [ ] Add dispatcher plumbing to push/pop event scope frames around widget event dispatch.

### Stage 3 — `#[command]` macro: signature extraction + wrapper generation

* [ ] Extend `#[command]` macro to emit `CommandSpec` with parameter metadata (names/types).
* [ ] Implement wrapper binding algorithm: injected params vs user args.
* [ ] Support `CommandArgs::Positional` and `CommandArgs::Named` binding.
* [ ] Support return value normalization to `ArgValue` (`()`, `Result<()>`, `T`, `Result<T>`).

### Stage 4 — Injected parameter types

* [ ] Add injection support for `&mut dyn Context` / `&dyn Context`.
* [ ] Add injection support for `&Event` / `Option<&Event>`.
* [ ] Add injection support for `&mouse::Event` / `Option<&mouse::Event>`.
* [ ] Add injection support for `ListRowContext` / `Option<ListRowContext>` (initially always `None`, list work will populate).

### Stage 5 — Rhai bridge

* [ ] Implement `rhai::Dynamic ↔ ArgValue` conversion layer.
* [ ] Add script entrypoint to dispatch commands by name/id with Rhai args.
* [ ] Convert command return `ArgValue` back to Rhai for scripts.

### Stage 6 — User-defined types via serde

* [ ] Add `CommandSerde` marker trait + (optional) derive macro.
* [ ] Implement `ArgValue ↔ serde_json::Value` converter.
* [ ] Implement `ArgValue ↔ T` for `T: CommandSerde` (decode/encode).
* [ ] Add tests: nested structs, enums, optional fields, vec/map fields.

### Stage 7 — Tooling and test coverage

* [ ] Introspection APIs: list commands, show signatures, show param names/types.
* [ ] Golden tests for error messages: arity mismatch, type mismatch, missing injected.
* [ ] Fuzz tests for `ArgValue` conversion correctness (optional but recommended).

---

