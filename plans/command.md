# Spec — Typed Command System with Extractor-Style Injection and Rhai Interop

## 1. Scope

This specification defines a typed command system that:

* Extracts parameter and return types from `#[command]` function definitions at compile time.
* Supports extractor-style injection (axum-inspired) from a command-scope stack without bespoke
  per-command APIs.
* Supports a broad set of common argument and return types.
* Supports user-defined types via serde (map/array representations), interoperable with Rhai object
  maps.
* Preserves existing Rust call-site idioms: `ctx.dispatch_command(&invocation)`, `Type::cmd_*()`
  factories, and `call_with(...)` invocation building.

Commands are synchronous and execute immediately when dispatched.

---

## 2. Terminology

* **Command**: A callable unit generated from a Rust function/method annotated with `#[command]`.
* **CommandSpec**: Static metadata for a command (id/name/params/return) and an erased invoke
  entrypoint.
* **Invocation**: A runtime request to execute a command with specific user-supplied arguments.
* **Injected parameter**: A parameter populated from the current command scope rather than
  invocation arguments.
* **User argument parameter**: A parameter decoded from invocation arguments (positional or named).
* **ArgValue**: Canonical dynamic representation for command arguments and return values with a Rhai
  mapping.
* **Command scope**: A stack-scoped set of ambient values available to injectors and commands
  (e.g., current event snapshot, list row context).

---

## 3. Execution model

### 3.1 Synchronous execution

* `ctx.dispatch_command(&invocation)` executes synchronously on the current call stack.
* Nested dispatch is supported; nested calls observe a new command-scope frame that inherits the
  parent values by default.

### 3.2 Scope stacking and explicit scoping

* The context maintains a stack of scope frames.
* "Current" injected values are always resolved from the topmost frame.
* The API supports explicit execution under an override frame:

```rust
pub struct CommandScopeFrame {
    pub event: Option<Event>,
    pub mouse: Option<MouseEvent>,
    pub list_row: Option<ListRowContext>,
}

impl Default for CommandScopeFrame { /* all None */ }

pub trait Context: ViewContext {
    fn dispatch_command(&mut self, inv: &CommandInvocation) -> Result<ArgValue, CommandError>;

    fn dispatch_command_scoped(
        &mut self,
        frame: CommandScopeFrame,
        inv: &CommandInvocation
    ) -> Result<ArgValue, CommandError>;
}
```

`dispatch_command_scoped` pushes `frame` for the duration of the dispatch and then pops it, even if
the command errors.

---

## 4. Canonical dynamic value type

### 4.1 `ArgValue`

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
}
```

---

## 5. Encoding and decoding traits

Rust orphan rules prohibit implementing `TryFrom<ArgValue>`/`Into<ArgValue>` for primitives and
external types. This system uses crate-local traits.

```rust
pub trait ToArgValue {
    fn to_arg_value(self) -> ArgValue;
}

pub trait FromArgValue: Sized {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError>;
}
```

* The macro-generated command wrapper uses `FromArgValue` to decode user arguments.
* `call_with([a, b, ...])` uses `ToArgValue` to encode typed values.

---

## 6. Command IDs, descriptors, and registration

### 6.1 `CommandId`

`CommandId` is a `'static` string in the format `"OwnerType::command_name"`.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandId(pub &'static str);
```

### 6.2 Descriptor types

```rust
pub enum CommandParamKind { Injected, User }

pub struct CommandTypeSpec {
    pub rust: &'static str,            // e.g. "isize", "Injected<MouseEvent>"
    pub doc: Option<&'static str>,     // optional
}

pub struct CommandParamSpec {
    pub name: &'static str,            // binding name for named args
    pub kind: CommandParamKind,
    pub ty: CommandTypeSpec,
    pub optional: bool,                // true for Option<T> user args or Option<Injected<_>>
    pub default: Option<&'static str>, // stringified default value for diagnostics
}

pub enum CommandReturnSpec {
    Unit,
    Value(CommandTypeSpec),
}
```

### 6.3 Erased invoke entrypoint

Command invocation is stored in the `CommandSpec` via an erased function pointer that supports both
free functions and methods.

```rust
pub type InvokeFn = fn(
    target: Option<&mut dyn std::any::Any>,
    ctx: &mut dyn Context,
    inv: &CommandInvocation,
) -> Result<ArgValue, CommandError>;

pub enum CommandDispatchKind {
    /// Invoke with `target = None`.
    Free,
    /// Routed to a node/owner instance; invoked with `target = Some(&mut owner)`.
    Node { owner: &'static str },
}

pub struct CommandSpec {
    pub id: CommandId,
    pub name: &'static str,
    pub dispatch: CommandDispatchKind,
    pub params: &'static [CommandParamSpec],
    pub ret: CommandReturnSpec,
    pub invoke: InvokeFn,
}
```

* For method commands, `invoke` downcasts the provided `target` and calls the method.
* For free commands, `invoke` ignores `target` and executes directly.

### 6.4 Registration

Commands are registered into a `CommandSet`:

```rust
pub struct CommandSet { /* id -> spec */ }

impl CommandSet {
    pub fn add(&mut self, specs: &'static [&'static CommandSpec]);
    pub fn get(&self, id: &str) -> Option<&'static CommandSpec>;
}
```

`CommandSet::get` supports `&str` lookup from scripting without allocations.

---

## 7. Invocation building and argument containers

### 7.1 Invocation types

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

### 7.2 Builder API

```rust
impl CommandSpec {
    pub fn call_with(&'static self, args: impl Into<CommandArgs>) -> CommandCall {
        CommandCall { spec: self, args: args.into() }
    }
}

pub struct CommandCall {
    spec: &'static CommandSpec,
    args: CommandArgs,
}

impl CommandCall {
    pub fn invocation(self) -> CommandInvocation {
        CommandInvocation { id: self.spec.id, args: self.args }
    }
}
```

### 7.3 `CommandArgs` conversions

Implement `From<…> for CommandArgs` (enabling `Into<CommandArgs>` automatically):

* `From<()>` → positional empty
* `From<Vec<ArgValue>>` → positional
* `From<BTreeMap<String, ArgValue>>` → named
* `From<[T; N]>` where `T: ToArgValue` → positional (encode each element)
* `From<Vec<T>>` where `T: ToArgValue` → positional

### 7.4 Named args macro

Provide a macro for ergonomic Rust named-arg calls:

```rust
named_args! { key1: value1, key2: value2, ... } -> CommandArgs::Named(...)
```

Keys are stringified identifiers (e.g., `index:` becomes `"index"`).

---

## 8. Binding semantics (positional and named)

### 8.1 Parameter classification

The macro classifies each parameter as injected or user argument using the rules in Section 10.4.

`expected_user_arity` is the number of user parameters excluding injected parameters, with trailing
`Option<_>` user parameters and parameters with defaults considered optional for positional binding.

### 8.2 Positional binding

* User parameters bind left-to-right from `args[0..]`.
* For a user parameter of type `Option<T>`:

  * missing arg → `None`
  * `Null` → `None`
  * otherwise decode as `T` → `Some(t)`
* For a user parameter with a default:

  * missing arg → use the default value
  * otherwise decode as normal
* For non-`Option` user parameters without defaults:

  * missing arg → `CommandError::ArityMismatch`
* Extra positional arguments beyond the last user parameter → `CommandError::ArityMismatch`

### 8.3 Named binding

* Each user parameter binds by its `CommandParamSpec.name`.
* Key normalization: during matching, map keys are normalized by replacing `-` with `_` and matching
  is case-insensitive (see Addendum B.3).
* For user parameters of type `Option<T>`:

  * missing key → `None`
  * present `Null` → `None`
  * otherwise decode `T`
* For user parameters with defaults:

  * missing key → use the default value
  * otherwise decode as normal
* Unknown named keys (after normalization) → `CommandError::UnknownNamedArg { name, allowed }`

---

## 9. Attribute-based defaults

### 9.1 Syntax

Parameters can specify default values using the `#[arg]` attribute:

```rust
#[command]
fn scroll(
    ctx: &mut dyn Context,
    #[arg(default = 1)] count: isize,
    #[arg(default = "down")] direction: Direction,
) -> Result<()> { ... }
```

Two forms are supported:

* `#[arg(default = expr)]` — use `expr` as the default value
* `#[arg(default)]` — call `T::default()` for types implementing `Default`

### 9.2 Evaluation semantics

Default expressions are evaluated at compile time. Only literals and const expressions are
supported. For non-const defaults, use `Option<T>` with `.unwrap_or_else(|| ...)` in the function
body.

### 9.3 Interaction with `Option<T>`

`Option<T>` parameters already have an implicit default of `None` when the argument is missing.
Using `#[arg(default = ...)]` on an `Option<T>` parameter is a compile-time error. If a non-`None`
default is needed, use a non-`Option` type with an explicit default.

### 9.4 Generated code behavior

When a parameter has a default:

1. The generated `CommandParamSpec` includes `optional: true` and `default: Some("expr_str")`.
2. During binding, if the argument is missing (positional) or the key is absent (named), the
   default expression is used instead of raising `ArityMismatch` or `MissingNamedArg`.
3. If the argument is present but fails to decode, it is still an error (the default is not used as
   a fallback for decode failures).

### 9.5 Rhai and introspection

* Default values are exposed in `CommandParamSpec::default` as a stringified representation for
  introspection and tooling.
* From Rhai, commands can be called with fewer arguments and defaults apply transparently.

---

## 10. Error model

```rust
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("unknown command: {id}")]
    UnknownCommand { id: String },

    #[error("arity mismatch: expected {expected}, got {got}")]
    ArityMismatch { expected: usize, got: usize },

    #[error("missing named argument: {name}")]
    MissingNamedArg { name: String },

    #[error("unknown named argument: {name}; allowed: {allowed:?}")]
    UnknownNamedArg { name: String, allowed: Vec<&'static str> },

    #[error("type mismatch for parameter `{param}`: expected {expected}, got {got}")]
    TypeMismatch { param: String, expected: &'static str, got: String },

    #[error("missing injected value for parameter `{param}`: expected {expected}")]
    MissingInjected { param: String, expected: &'static str },

    #[error("conversion error for parameter `{param}`: {message}")]
    Conversion { param: String, message: String },

    #[error("command execution failed: {0}")]
    Exec(#[from] anyhow::Error),
}
```

---

## 11. Injection system (axum-inspired)

### 11.1 `Inject` trait and error type

Injection does not mention parameter names; the macro fills them in.

```rust
#[derive(Debug)]
pub enum InjectError {
    Missing { expected: &'static str },
    Failed { expected: &'static str, message: String },
}

pub trait Inject: Sized {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError>;
}
```

### 11.2 Blanket implementation for `Option<T>`

```rust
impl<T: Inject> Inject for Option<T> {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        match T::inject(ctx) {
            Ok(v) => Ok(Some(v)),
            Err(InjectError::Missing { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
```

### 11.3 Explicit wrappers

#### `Injected<T>`

Explicitly marks a parameter as injectable (required for user-defined injectables; optional for
built-ins).

```rust
#[derive(Debug, Clone, Copy)]
pub struct Injected<T>(pub T);

impl<T: Inject> Inject for Injected<T> {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        T::inject(ctx).map(Injected)
    }
}
```

#### `Arg<T>`

Explicitly marks a parameter as a user argument even if its identifier collides with built-in
injectable shorthand.

```rust
#[derive(Debug)]
pub struct Arg<T>(pub T);
```

The macro treats `Arg<T>` as a user argument and decodes `T` from invocation arguments.

### 11.4 Built-in injectable types

The following types are injectable by shorthand (without `Injected<T>`) and by explicit wrapper:

* `Event` (owned clone)
* `MouseEvent` (Copy)
* `ListRowContext` (Copy)

Implementations:

```rust
impl Inject for MouseEvent {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        ctx.current_mouse_event().ok_or(InjectError::Missing { expected: "MouseEvent" })
    }
}

impl Inject for ListRowContext {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        ctx.current_list_row().ok_or(InjectError::Missing { expected: "ListRowContext" })
    }
}

impl Inject for Event {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        ctx.current_event()
            .cloned()
            .ok_or(InjectError::Missing { expected: "Event" })
    }
}
```

### 11.5 Required `Context` methods for injection

```rust
pub trait Context: ViewContext {
    fn current_event(&self) -> Option<&Event>;
    fn current_mouse_event(&self) -> Option<MouseEvent>;
    fn current_list_row(&self) -> Option<ListRowContext>;

    fn dispatch_command(&mut self, inv: &CommandInvocation) -> Result<ArgValue, CommandError>;
    fn dispatch_command_scoped(
        &mut self,
        frame: CommandScopeFrame,
        inv: &CommandInvocation
    ) -> Result<ArgValue, CommandError>;
}
```

### 11.6 Macro binding algorithm

For each parameter, in order:

1. If parameter type is `&mut dyn Context` or `&dyn Context` → pass directly.
2. If parameter pattern/type is `Injected<T>` (or `Option<Injected<T>>`) → call
   `Injected::<T>::inject(&*ctx)` and map `InjectError` to `CommandError` using the parameter name.
3. If parameter type matches built-in injectable shorthand (`Event`, `MouseEvent`,
   `ListRowContext`, including `Option<_>`) → call `T::inject(&*ctx)` and map errors using the
   parameter name.
4. If parameter type is `Arg<T>` → decode user arg as `T`.
5. Otherwise → decode as a user arg (applying defaults if `#[arg(default = ...)]` is present and
   the arg is missing).

---

## 12. Type support

### 12.1 Built-in `ToArgValue` / `FromArgValue` (required)

**Primitives**

* `bool`
* Signed integers: `i16`, `i32`, `i64`, `isize`
* Unsigned integers: `u16`, `u32`, `u64`, `usize`
* Floats: `f32`, `f64`
* `String`

**Containers**

* `Option<T>` (`Null` or missing ↔ `None`)
* `Vec<T>` ↔ `Array`
* `BTreeMap<String, T>` ↔ `Map`
* `HashMap<String, T>` ↔ `Map`

**Tuples**

* `(A,)`, `(A,B)`, `(A,B,C)`, `(A,B,C,D)` ↔ `Array` (positional)

### 12.2 Range and lossiness rules

* `Int(i64)` → narrower integer: range-check; overflow errors.
* `Float(f64)` → `f32`: range-check.
* No implicit float→int coercion.

### 12.3 Script-facing enums

`#[derive(CommandEnum)]` generates `ToArgValue`/`FromArgValue` using case-insensitive string
matching on variant names.

---

## 13. User-defined types via serde (`CommandArg`)

### 13.1 Marker trait

```rust
pub trait CommandArg: serde::Serialize + serde::de::DeserializeOwned + 'static {}
```

### 13.2 Encoding/decoding rules

User-defined types are encoded/decoded via `serde_json::Value` as an intermediate:

* Decode: `ArgValue` → `serde_json::Value` → `T`
* Encode: `T` → `serde_json::Value` → `ArgValue`

### 13.3 Rhai compatibility

Rhai object maps become `ArgValue::Map`, enabling structs to be passed naturally as positional
values.

---

## 14. Rhai integration

### 14.1 Value mapping

Rhai `Dynamic` converts to/from `ArgValue` as:

* `()` ↔ `Null`
* `bool` ↔ `Bool`
* `i64` ↔ `Int`
* `f64` ↔ `Float`
* `String` ↔ `String`
* `Array` ↔ `Array`
* `Map` ↔ `Map`

### 14.2 Script entrypoints

Register the following Rhai functions:

* `cmd(name)` → no-arg positional
* `cmd(name, a1, a2, ...)` → positional (maps are treated as positional values)
* `cmdv(name, array)` → positional from array
* `cmd_named(name, map)` → named args from map
* `cmd_pos(name, value)` → forces a single positional argument even when it is a map (escape hatch)

### 14.3 Optional convenience behavior for `cmd(name, map)` (if enabled)

If you keep a single `cmd(name, map)` convenience overload, it must be signature-aware:

* Look up the `CommandSpec` for `name`.
* If the map's (normalized) keys are a subset of the command's user-parameter names and at least
  one key matches, interpret as named args.
* Otherwise interpret as a single positional map value.

This preserves ergonomic named args while allowing serde-struct positional maps to work naturally.

---

## Addendum A — Staged implementation plan

### Stage 1 — `ArgValue` and local encode/decode traits

* [ ] Implement `ArgValue`.
* [ ] Define `ToArgValue` / `FromArgValue`.
* [ ] Implement both traits for primitives, containers, and tuples.
* [ ] Implement and test range-checking and error reporting.

### Stage 2 — Command scope stack + `dispatch_command_scoped`

* [ ] Add command-scope stack and `CommandScopeFrame`.
* [ ] Implement `current_event()`, `current_mouse_event()`, `current_list_row()` reads from the top
      frame.
* [ ] Implement `dispatch_command_scoped(frame, inv)` with push/pop.
* [ ] Wire event dispatch to push a frame containing an event snapshot and derived mouse/list-row
      context.

### Stage 3 — Injection system

* [ ] Define `InjectError`, `Inject`, and blanket `Inject for Option<T>`.
* [ ] Define `Injected<T>` and (marker) `Arg<T>`.
* [ ] Implement `Inject` for `Event`, `MouseEvent`, `ListRowContext`.
* [ ] Ensure `InjectError` mapping to `CommandError::{MissingInjected,Conversion}` is performed by
      generated wrappers (parameter name filled by macro).

### Stage 4 — `#[command]` macro: extraction + wrapper generation

* [ ] Generate `CommandSpec` with `'static` id and metadata including `optional` and `default`.
* [ ] Generate erased `invoke(target: Option<&mut dyn Any>, ...)`.
* [ ] Implement binding algorithm (Context refs, Injected<T>, built-in shorthand, Arg<T>, user
      args).
* [ ] Implement attribute-based defaults (`#[arg(default = ...)]`).
* [ ] Normalize return values to `ArgValue` for `()`, `T`, `Result<()>`, `Result<T>`.

### Stage 5 — CommandSet and routing

* [ ] Implement `CommandSet::{add,get}`.
* [ ] Define/confirm routing logic for `CommandDispatchKind::Node { owner }`.
* [ ] Implement free-command dispatch path (invoke with `target = None`).

### Stage 6 — Rhai bridge

* [ ] Implement `Dynamic ↔ ArgValue` conversion.
* [ ] Register `cmd`, `cmdv`, `cmd_named`, `cmd_pos`.
* [ ] If enabling signature-aware `cmd(name, map)`, implement subset-of-param-names heuristic.

### Stage 7 — serde user types + enum derive

* [ ] Implement `CommandArg` derive and serde bridging (`ArgValue ↔ serde_json::Value`).
* [ ] Implement `ToArgValue` / `FromArgValue` for `T: CommandArg`.
* [ ] Implement `CommandEnum` derive generating `ToArgValue` / `FromArgValue`.
* [ ] Add coverage tests: nested structs, enums, optional fields, collections.

### Stage 8 — Tooling and diagnostics

* [ ] Introspection APIs: list commands, show signature, show which params are
      injected/user/optional/defaulted.
* [ ] Golden tests for arity/type/injection errors (including unknown named args and normalized
      keys).
* [ ] Integration tests for scripting.

---

## Addendum B — Design decisions

### B.1 Command routing semantics

When dispatching a command to a `Node { owner }`, the routing strategy is:

1. **Subtree-first**: Starting from the focused node (or dispatch origin), search the subtree rooted
   at that node in pre-order DFS. The first matching node by owner name receives the command.
2. **Ancestor fallback**: If no match is found in the subtree, walk up the ancestor chain from the
   origin. Each ancestor is checked (not its subtree). The first matching ancestor receives the
   command.
3. **Not found**: If no node matches after exhausting ancestors, return
   `CommandError::UnknownCommand`.

This matches the existing Canopy dispatch behavior and provides predictable, local-first semantics:
commands naturally target the "nearest" matching widget in the focused region before falling back to
ancestors.

### B.2 Event injection

Injecting `Event` clones the event. This is acceptable for the current use cases. If cloning
becomes a performance concern, more fine-grained injectable types (`MouseEvent`, `KeyEvent`,
`Modifiers`, `Point`) are already available and should be preferred for hot paths.

### B.3 Named arg key normalization

Named argument keys are normalized using both transformations:

1. Replace `-` with `_` (kebab-case to snake_case)
2. Case-insensitive matching

This maximizes script ergonomics: `scroll-count`, `scroll_count`, `ScrollCount`, and `SCROLL_COUNT`
all match a parameter named `scroll_count`.
