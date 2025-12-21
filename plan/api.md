# Canopy Public API Review - Remaining Recommendations

This document contains the remaining recommendations from the API review.
The critical errors have been fixed in commit `3c71667`.

---

## Warnings (Should Address)

### 1. Testing Utilities in Public API

**Location:** `crates/canopy/src/core/tutils/`

**Problem:** The entire `tutils` module is public, exposing:

- `tutils::ttree` - Test tree with nodes `R`, `Ba`, `Bb`, `BaLa`, `BaLb`, `BbLa`, `BbLb`
- `tutils::grid` - `Grid`, `GridNode` test helpers
- `tutils::harness` - `Harness`, `HarnessBuilder`
- `tutils::dummyctx` - `DummyContext`
- `tutils::buf` - `BufTest`
- `tutils::render` - `NopBackend`

**Impact:**
- Adds ~20+ types to the public API that most users will never need
- Test implementation details leak into documentation
- Increases API surface area to maintain

**Recommendation:** Gate behind a feature flag:

```rust
// core/mod.rs
#[cfg(any(test, feature = "test-utils"))]
pub mod tutils;
```

```toml
# Cargo.toml
[features]
test-utils = []
```

---

### 2. Public Fields on CommandSet

**Location:** `crates/canopy/src/core/commands.rs`

**Problem:**

```rust
#[derive(Debug, Default)]
pub struct CommandSet {
    pub commands: std::collections::HashMap<String, CommandSpec>,
}
```

**Impact:**
- Users can directly manipulate the internal HashMap
- Cannot change internal representation (e.g., to BTreeMap) without breaking API
- Bypasses intended `CommandSet::commands()` method

**Recommendation:**

```rust
#[derive(Debug, Default)]
pub struct CommandSet {
    commands: std::collections::HashMap<String, CommandSpec>,
}

impl CommandSet {
    pub fn new() -> Self { ... }
    pub fn add(&mut self, cmds: &[CommandSpec]) { ... }
    pub fn get(&self, name: &str) -> Option<&CommandSpec> { ... }
    pub fn iter(&self) -> impl Iterator<Item = &CommandSpec> { ... }
}
```

---

### 3. Public Fields on Data Structures

**Location:** Various command and event types

**Problem:** Several structs expose all fields publicly:

```rust
// CommandSpec
pub struct CommandSpec {
    pub node: NodeName,
    pub command: String,
    pub docs: String,
    pub ret: ReturnSpec,
    pub args: Vec<ArgTypes>,
}

// CommandInvocation
pub struct CommandInvocation {
    pub node: NodeName,
    pub command: String,
    pub args: Vec<Args>,
}

// ReturnSpec
pub struct ReturnSpec {
    pub typ: ReturnTypes,
    pub result: bool,
}
```

**Impact:**
- Cannot add fields without breaking API
- Cannot add validation logic

**Recommendation:** For types that are primarily constructed internally, consider making
fields private with accessor methods. For types users construct, the current design may
be acceptable.

---

### 4. Render Struct Exposes Style Field

**Location:** `crates/canopy/src/core/render.rs`

**Problem:**

```rust
pub struct Render<'a> {
    pub style: &'a mut crate::style::StyleManager,
}
```

**Impact:**
- Leaks internal style management
- Users can corrupt style state

**Recommendation:**

```rust
pub struct Render<'a> {
    style: &'a mut crate::style::StyleManager,
    // ... other private fields
}
```

---

## Suggestions (Consider)

### 5. Helper Functions as Module-Level Exports

**Location:** `crates/canopy/src/core/state.rs:17-25`

**Problem:**

```rust
pub fn valid_nodename_char(c: char) -> bool { ... }
pub fn valid_nodename(name: &str) -> bool { ... }
```

These are implementation details for `NodeName` validation.

**Recommendation:** Move into `NodeName`:

```rust
impl NodeName {
    pub fn is_valid_char(c: char) -> bool { ... }
    pub fn is_valid(name: &str) -> bool { ... }
}
```

Or make private if only used internally.

---

### 6. Editor Module Internal Exposure

**Location:** `crates/canopy/src/widgets/editor/`

**Problem:** The editor module exposes internal types:

- `editor::core::Core` - Editor core state machine
- `editor::CharPos` - Character position
- `editor::InsertPos` - Insert position
- `editor::Pos` - Position trait
- `editor::Window` - Window state

**Recommendation:** Review whether these types need to be public. If the `Editor` widget
is meant to be used as a black box, these should be internal.

---

### 7. Inconsistent Backend Exposure

**Location:** `crates/canopy/src/core/backend/`

**Problem:** The `backend::test` module exposes multiple test backends:
- `TestRender`
- `TestBuf`
- `CanvasRender`
- `CanvasBuf`

While `backend::crossterm` exposes production types.

**Recommendation:** Consider whether test backends should be in `tutils` instead, or
gated behind the `test-utils` feature.

---

## Recommended Priority

### Medium Priority

1. **Gate `tutils` behind feature flag** - Reduces API surface significantly
2. **Make `CommandSet.commands` private** - Proper encapsulation
3. **Make `Render.style` private** - Prevent state corruption

### Low Priority

4. **Review editor internal exposure**
5. **Clean up backend module organization**
6. **Move helper functions into types**
7. **Consider making CommandSpec/CommandInvocation fields private**

---

## Implementation Notes

### Feature Flag for Test Utilities

To implement the `test-utils` feature:

1. Add to `Cargo.toml`:
```toml
[features]
default = []
test-utils = []
```

2. Update `lib.rs`:
```rust
#[cfg(any(test, feature = "test-utils"))]
pub use core::tutils;
```

3. Update `core/mod.rs`:
```rust
#[cfg(any(test, feature = "test-utils"))]
pub mod tutils;
```

4. Users who need test utilities add to their `Cargo.toml`:
```toml
[dev-dependencies]
canopy = { version = "...", features = ["test-utils"] }
```

### Encapsulating CommandSet

```rust
impl CommandSet {
    /// Create an empty command set.
    pub fn new() -> Self {
        Self { commands: HashMap::new() }
    }

    /// Add commands to the set.
    pub fn add(&mut self, cmds: &[CommandSpec]) {
        for cmd in cmds {
            let key = format!("{}::{}", cmd.node, cmd.command);
            self.commands.insert(key, cmd.clone());
        }
    }

    /// Get a command by fully qualified name.
    pub fn get(&self, name: &str) -> Option<&CommandSpec> {
        self.commands.get(name)
    }

    /// Iterate over all commands.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CommandSpec)> {
        self.commands.iter()
    }

    /// Number of commands in the set.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
```
