# Canopy API Review

This document tracks the status of API review findings and recommendations for the `canopy` crate.

## Status Summary

- [x] **Gate `testing` utilities behind feature flag** (Renamed from `tutils` to `testing` and gated behind `testing` feature)
- [x] **Make `Frame` fields private**
- [x] **Make `CommandSet.commands` private**
- [x] **Make `Render.style` private**
- [ ] **Review `CommandSpec`, `CommandInvocation`, `ReturnSpec` public fields** (Low Priority)
- [ ] **Review `Editor` module internal exposure** (Low Priority)
- [ ] **Address multiple public export paths** (e.g. `canopy::widgets::Root` vs `canopy::Root`)

---

## Outstanding Recommendations

### 1. Encapsulate `CommandSet`

**Location:** `crates/canopy/src/core/commands.rs`

**Problem:** `CommandSet` exposes its internal `HashMap` publicly.

```rust
pub struct CommandSet {
    pub commands: HashMap<String, CommandSpec>,
}
```

**Recommendation:** Make `commands` private and provide accessors.

```rust
pub struct CommandSet {
    commands: HashMap<String, CommandSpec>,
}

impl CommandSet {
    pub fn new() -> Self { ... }
    pub fn add(&mut self, cmds: &[CommandSpec]) { ... }
    pub fn get(&self, name: &str) -> Option<&CommandSpec> { ... }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CommandSpec)> { ... }
}
```

### 2. Encapsulate `Render.style`

**Location:** `crates/canopy/src/core/render.rs`

**Problem:** `Render` exposes the `StyleManager` mutable reference publicly.

```rust
pub struct Render<'a> {
    pub style: &'a mut crate::style::StyleManager,
    // ...
}
```

**Recommendation:** Make `style` private to prevent state corruption.

### 3. Public Fields on Data Structures (Low Priority)

**Location:** `crates/canopy/src/core/commands.rs`

**Problem:** `CommandSpec`, `CommandInvocation`, and `ReturnSpec` expose all fields publicly. This limits future evolution and validation.

**Recommendation:** Consider making fields private with builders/accessors for `CommandSpec` (constructed by users). `CommandInvocation` and `ReturnSpec` are often created internally, but also by users for testing/scripting.

### 4. Duplicate Exports

**Location:** `lib.rs`, `widgets/mod.rs`

**Problem:** Inconsistent export paths.
- `canopy::widgets::Root` exists.
- Check if `canopy::Root` exists or should exist.
- Ensure a single canonical path for types.

## Historical Findings (Resolved or Deferred)

### Findings (Errors) - *Resolved/Checked*
- `Frame` fields are now private.
- `tutils` (now `testing`) is gated behind `#[cfg(any(test, feature = "testing"))]`.

### Findings (Warnings)
- **Allocation-heavy signatures**: `CommandNode::commands() -> Vec<CommandSpec>` forces allocation. *Decision: Keep for now for dynamic commands.*
- **Name collisions**: `Input` (event) vs `Input` (widget). *deferred*

### Findings (Suggestions)
- **GAT-based traversal**: Deferred.
- **`Cow<str>` for text**: Deferred.
- **`FromStr` for Path/NodeName**: Deferred.