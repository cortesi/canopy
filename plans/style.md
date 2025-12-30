# Fluent style builder for examples

**Status: Implemented**

This document describes the fluent style builder API for `StyleMap`, which provides a terse,
chainable pattern for defining style rules.

## Problem statement

- `StyleMap::add` required four arguments with repetitive `Some(..)` and `AttrSet::default()`
  boilerplate.
- Examples repeatedly spelled long path prefixes like `termgym/entry/selected/...`.
- We had `add_fg`/`add_bg`, but the moment we needed both fg and bg we fell back to the verbose
  form.

## Implemented API

### Core Types

```rust
/// A fluent builder for adding style rules to a StyleMap.
#[must_use = "call .apply() to commit rules"]
pub struct StyleRules<'a> { ... }

impl<'a> StyleRules<'a> {
    // Single path methods
    pub fn fg(self, path: &str, color: Color) -> Self;
    pub fn bg(self, path: &str, color: Color) -> Self;
    pub fn attr(self, path: &str, attr: Attr) -> Self;
    pub fn attrs(self, path: &str, attrs: AttrSet) -> Self;
    pub fn style(self, path: &str, style: impl Into<PartialStyle>) -> Self;

    // Multi-path methods
    pub fn fg_all(self, paths: &[&str], color: Color) -> Self;
    pub fn bg_all(self, paths: &[&str], color: Color) -> Self;
    pub fn attr_all(self, paths: &[&str], attr: Attr) -> Self;
    pub fn attrs_all(self, paths: &[&str], attrs: AttrSet) -> Self;
    pub fn style_all(self, paths: &[&str], style: impl Into<PartialStyle>) -> Self;

    // Prefix management
    pub fn prefix(self, prefix: &str) -> Self;
    pub fn no_prefix(self) -> Self;

    pub fn apply(self);
}

impl StyleMap {
    pub fn rules(&mut self) -> StyleRules<'_>;
}
```

### Standalone Style Builder

```rust
/// A builder for creating reusable style specifications.
#[derive(Clone, Default)]
pub struct StyleBuilder { ... }

impl StyleBuilder {
    pub fn new() -> Self;
    pub fn fg(self, color: Color) -> Self;
    pub fn bg(self, color: Color) -> Self;
    pub fn attr(self, attr: Attr) -> Self;
    pub fn attrs(self, attrs: AttrSet) -> Self;
}

impl From<StyleBuilder> for PartialStyle { ... }
```

## Usage examples

### Simple foreground-only rules

```rust
cnpy.style
    .rules()
    .fg("red/text", solarized::RED)
    .fg("blue/text", solarized::BLUE)
    .fg("list/selected", solarized::BLUE)
    .apply();
```

### Multiple properties via merging

Rules for the same path are automatically merged, so you can set fg and bg separately:

```rust
cnpy.style
    .rules()
    .fg("statusbar/text", solarized::BASE02)
    .bg("statusbar/text", solarized::BASE1)
    .apply();
```

### Composite styles with StyleBuilder

For reusable styles or when you prefer a single expression, use `StyleBuilder`:

```rust
cnpy.style
    .rules()
    .style(
        "statusbar/text",
        StyleBuilder::new().fg(solarized::BASE02).bg(solarized::BASE1),
    )
    .apply();
```

### Shared styles with prefixes

```rust
let normal = StyleBuilder::new()
    .fg(solarized::BASE0)
    .bg(solarized::BASE03);

let selected = StyleBuilder::new()
    .fg(solarized::BASE3)
    .bg(solarized::BLUE)
    .attrs(selected_attrs);

cnpy.style
    .rules()
    .prefix("intervals/entry")
    .style_all(&["border", "fill", "text"], normal)
    .style_all(&["selected/border", "selected/fill", "selected/text"], selected)
    .no_prefix()
    .style("statusbar/text", StyleBuilder::new().fg(BASE02).bg(BASE1))
    .apply();
```

## Design decisions

- **Direct methods on `StyleRules`**: Instead of chaining `.rule(path).fg(color)`, we use
  `.fg(path, color)` directly. This is terser for the common case of single-property rules.
- **Automatic merging**: Multiple calls to the same path merge their styles. This allows
  `.fg("x", RED).bg("x", BLUE)` without needing `StyleBuilder`. Later calls override earlier ones
  for the same property.
- **Two types only**: `StyleBuilder` for composing reusable styles, `StyleRules` for applying them.
  The previous `StyleRule` and `MultiRule` intermediate types were eliminated.
- **`_all` suffix for multi-path**: Clear naming distinguishes single-path vs multi-path methods.
- **`#[must_use]` on builder**: Compiler warns if you forget `.apply()`.
- **`prefix()` as method**: Simpler than nested `scope` closures; state is linear and readable.
- **`StyleBuilder` for reusable styles**: Still useful when you want to define a style once and
  apply it to multiple paths via `style_all()`.
- **Removed `add`, `add_fg`, `add_bg`**: Replaced entirely by the fluent API.
