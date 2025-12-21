# Canopy API Review Findings and Recommendations

This document records findings and recommendations from a `ruskel` inspection of the public
API surface of the `canopy` crate. It focuses on API design, ergonomics, and surface area.

## Findings (Errors)

- Multiple public export paths exist for the same items (single-path rule violated). Examples:
  `canopy::geom::Rect` and `canopy::Rect`, `canopy::commands::CommandSpec` and
  `canopy::CommandSpec`, `canopy::node::Node` and `canopy::Node`, `canopy::render::Render`
  and `canopy::Render`, `canopy::widgets::Root` and `canopy::Root`,
  `canopy::widgets::editor::Editor` and `canopy::widgets::Editor`.
- Public fields expose internal state and weaken invariants: `widgets::frame::Frame`
  (child/state/glyphs/frame/title), `widgets::Panes` (children/state), `widgets::Text`
  (state/raw), `widgets::Input` (textbuf), `widgets::list::List` (selected).
- Testing and debug utilities are part of the public API (`tutils`, `backend::test`, `dump`),
  which expands and couples the public surface to internal machinery.

## Findings (Warnings)

- Name collisions create ambiguity and reduce discoverability: `Input` enum (bindings) vs
  `widgets::Input` (widget), duplicate `Editor` paths, multiple `Root` paths.
- Allocation-heavy signatures reduce ergonomics: `Frame::with_title(String)`,
  `Tabs::new(Vec<String>)`, `Input::text() -> String`, `CommandSpec::fullname() -> String`.
- `CommandNode::commands() -> Vec<CommandSpec>` forces allocation on every call.
- `Path::new<T: AsRef<str>>(v: &[T])` is awkward and overly restrictive.
- `TermBuf` and `TextBuf` appear in public signatures/fields but are not clearly public types
  in the top-level API, which risks undocumented dependencies.

## Design Decisions

- Keep `CommandNode::commands() -> Vec<CommandSpec>` to support dynamic command registration at
  runtime. The allocation warning remains valid, but dynamic command generation is a primary
  use case, so we will not switch to a `'static` slice. If allocation cost becomes a concern,
  we can consider caching inside the caller or providing an opt-in helper API without
  restricting dynamic definitions.
- Keep `CommandSpec::fullname() -> String` for symmetry with dynamic command construction and
  ownership-friendly usage. The allocation warning remains valid, but callers that want to
  reuse a cached name can store the string themselves.

## Findings (Suggestions)

- Consider GAT-based child traversal to reduce callback boilerplate and improve type
  expressiveness for `Node::children`.
- Consider `Cow<'a, str>`, `Arc<str>`, or `SmolStr` for text-heavy APIs to reduce allocations.
- Provide `FromStr`/`TryFrom<&str>` for `Path` and `NodeName` and store validated or interned
  names for cheap clones.
- Replace string-only error variants with structured data and use `#[from]` for underlying
  errors to improve diagnostics.

## Recommendations

1. Choose a single export strategy: flat (`canopy::Rect`) or hierarchical
   (`canopy::geom::Rect`), not both.
2. Make widget fields private and expose minimal, intent-revealing accessors or methods.
3. Move testing/debug helpers into a separate crate (e.g., `canopy-test`) or a non-public
   internal module.
4. Replace `String`-only parameters with flexible inputs (`impl Into<Cow<'a, str>>`,
   `impl IntoIterator`) in text-heavy APIs.
5. Change `CommandNode::commands()` to return a static slice or borrowed iterator to avoid
   per-call allocation.
6. Implement `FromStr`/`TryFrom<&str>` for `Path` and `NodeName`, and use interned strings to
   keep cloning cheap.
7. Introduce a GAT-based child iterator or `ControlFlow`-style visitor for traversal APIs.
8. Rename ambiguous types to avoid collisions (`Input` event vs `Input` widget; duplicate
   `Editor` and `Root` paths).
9. Replace `String`-only error variants with structured, typed error data.
