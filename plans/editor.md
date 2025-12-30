# Editor rewrite: spec and staged plan

This document specifies a new editor component for Canopy and provides a staged implementation plan
for building it. The intent is a robust, testable editor that supports both single-line inputs and
multi-line documents, offers a vi-like modal option, and stays consistent with Canopy's existing
event/key/render model.

## Current state review

- `crates/canopy/src/widgets/editor/*` is a minimal editor with chunk-based storage, wrapping, and
  cursor movement. It does not handle text input, selection, or editing commands beyond cursor
  shifts.
- `Editor` renders wrapped lines from `state::State` and exposes only cursor-shift commands.
- `state::State` stores text as `Vec<Chunk>` and wraps with `textwrap`, tracking a window of wrapped
  lines; cursor positions are byte offsets (not grapheme-aware).
- Undo/redo exists via `widgets/editor/core.rs` + `widgets/editor/effect.rs` for insert/delete, but
  there is no UI wiring for edits or mode handling.
- `widgets/input.rs` is a separate single-line input widget that is grapheme-aware and handles
  direct character input via `on_event`.
- `core/text.rs` already contains terminal-aware grapheme width + column slicing helpers used by
  rendering.

## Constraints and integration points

These framework details materially affect the editor design:

- **Key dispatch order**: `Canopy::key` resolves `InputMap` bindings before calling
  `Widget::on_event`. If a key is bound (for the current bubble path), the editor will not see it in
  `on_event`.
- **Key bubbling semantics**: keys bubble from the focused node to ancestors. During bubbling, the
  effective `Path` used for binding resolution is shortened one component at a time.
- **Scrolling**: scrolling is a core feature via `Widget::canvas()` and
  `Context::{scroll_to,scroll_by,page_up,page_down,...}`.
- **Rendering primitives**: `Render::text` draws a whole line in one style; `Render::put_cell`
  allows per-cell styling (needed for selection and syntax highlight) but can be slower.

The new editor reuses core scrolling (no editor-internal viewport) and keeps Canopy's bindings-first
model unchanged. Editor behavior is primarily driven by `Widget::on_event`, and applications should
use path-scoped bindings to avoid intercepting editor keystrokes unintentionally.

## Goals

- Provide a single editor component configurable for both single-line fields and multi-line
  documents.
- Support vi-like modal editing (normal/insert/visual, extensible) and a non-modal "text entry"
  mode.
- Support auto-growing text widgets that expand height as content wraps, within constraints.
- Be Unicode-correct for cursoring and deletion in terminal cell coordinates.
- Be robust for larger documents (efficient edits, incremental layout).
- Keep the API clean, minimal, and consistent with the rest of Canopy.
- Build a thorough test suite (unit, integration, render snapshots, property tests) covering edits,
  layout, and key handling.

## Non-goals (initial implementation)

- Full Vim parity (macros, registers, ex commands, complex motions) beyond an agreed subset.
- Rich text editing beyond syntax highlighting (no mixed fonts, no inline widgets, no markdown).
- Collaborative editing / CRDT support.
- Multi-cursor editing.

## Terminology (to avoid ambiguity)

- **Byte offset**: index into UTF-8 bytes.
- **Char index**: index into Unicode scalar values (what some ropes expose).
- **Grapheme**: user-perceived character (may be multiple scalars, e.g. ZWJ sequences).
- **Column**: terminal cell column; uses `core::text::grapheme_width` semantics (clamped cell
  width).
- **Logical line**: a line split on `\n` in the buffer.
- **Display line**: a wrapped segment of a logical line.

The editor's user-facing cursor and selection semantics are expressed in logical lines and display
columns. Internally, the buffer uses a rope-native index (byte/char depending on the rope) plus a
cached "preferred column" for vertical motion.

## Proposed design

### Architecture overview

Split the editor into three layers:

- **Buffer model**: text storage, edit operations, selections, undo/redo, change tracking.
- **Layout/view**: wrapping, cursor-to-screen mapping, scrolling, measuring, incremental caches.
- **Widget/controller**: event handling, mode state, rendering, configuration, commands.

The buffer model begins as an internal module within `crates/canopy` to keep the initial rewrite
small. If a second consumer emerges, it can be extracted into a separate crate later.

### Buffer model

The buffer layer is headless (no dependency on `canopy` rendering/layout types).

- **Storage**: use a rope-based buffer for scalable edits and line access. Evaluate `ropey`, `crop`,
  and `jumprope` early and choose one.
- **Edits**: primitive operations are insert/delete/replace on ranges, plus helpers for line-based
  transforms (join lines, insert newline, delete line).
- **Selections**: a single selection with `(anchor, head)`; an empty selection is a point cursor.
- **Undo/redo**: transaction model that can group multiple primitive edits into one undo step.
  - In vi mode, an "insert session" (enter insert mode, type, exit insert mode) is one undo step.
  - In text-entry mode, contiguous insertions are grouped into transactions where it is natural and
    safe to do so.
- **Change tracking**: monotonically increasing `revision` (or similar) to key layout caches.
- **Tabs**: store literal `\t` in the buffer; the layout layer expands tabs to a configurable tab
  stop width (default 4 columns) for wrapping and column mapping.

### Layout, wrapping, and scrolling

- **Column model**: use `core::text::grapheme_width` (clamped cell widths) for layout calculations,
  so rendering and layout agree.
- **Wrap modes**:
  - `WrapMode::None`: no wrapping; horizontal scrolling is enabled.
  - `WrapMode::Soft`: wrap display lines to the view width; horizontal scrolling disabled.
- **Wrap policy**: wrap by graphemes (break anywhere) in v1; optionally prefer word boundaries later
  if it materially improves UX.
- **Caching**: maintain an incremental cache keyed by buffer revision + wrap width, avoiding full
  rewrap on each edit. Rewrap only the touched logical lines (and adjust prefix sums / mappings).
- **Mapping**:
  - `text -> screen`: cursor/selection endpoints map to `(x, y)` in display coordinates.
  - `screen -> text`: mouse click maps `(x, y)` back to a buffer position.
- **Vertical motion**: preserve a "preferred column" when moving up/down (vim-like behavior).
- **Scrolling integration**: represent the editor's full content height via `Widget::canvas()` and
  use `Context::scroll_to`/`scroll_by` to keep the cursor visible. Do not maintain a parallel
  internal viewport.

### Widget/controller

- `Editor` owns the buffer and the caches needed for mapping and rendering.
- `EditorConfig` controls:
  - `multiline: bool`
  - `wrap: WrapMode`
  - `auto_grow: bool`, `min_height`, `max_height`
  - `mode: EditMode` (text entry, vi)
  - `read_only: bool`
  - `show_line_numbers: bool`
- Commands cover navigation, selection, edit operations, undo/redo, and mode transitions.
- `Widget::on_event` handles:
  - character insertion (when appropriate)
  - `Event::Paste(String)` (multi-character insertion)
  - vi multi-key sequences (e.g. `dd`, `gg`) via an internal key-sequence parser

Single-line newline behavior:

- In `multiline: false` mode, `KeyCode::Enter` does not insert a newline and is ignored so it can
  bubble to the application (submit/cancel bindings).
- In `multiline: false` mode, pasted newlines are normalized and replaced with spaces so paste
  remains useful without violating the single-line invariant.

The editor does not introduce new framework-level change notification. Parent widgets and
applications can query editor state when handling app-level commands (the existing Canopy pattern).
Validation and filtering are intentionally left to higher-level widgets in v1.

### Key handling and modes

- **Text entry mode**: direct insertion on printable keys; arrows/home/end; backspace/delete;
  standard shortcuts where they do not conflict with app-level bindings.
- **Vi mode**: internal mode state (normal/insert/visual) with a small command parser to interpret
  key sequences and apply buffer edits.

The editor does not ship a large default binding table. Instead, it is designed to consume relevant
keys in `on_event` when they are not intercepted by `InputMap`. Applications should avoid binding
printable keys in contexts where the editor should receive them, or scope those bindings so they do
not match when the editor is focused.

### Clipboard integration

- **Paste**: handle `Event::Paste(String)` as the primary paste mechanism (works in terminals that
  support bracketed paste).
- **Yank/put** (vi): implement an internal register for yanking and putting text.
- **OS clipboard**: not required for v1. If needed later, integrate via callback-based plumbing
  (similar to the terminal widget configuration) or a cross-platform crate.

### Cursor and selection rendering

- Cursor shape follows vim conventions (block in normal, line in insert, etc.). Cursor blinking
  remains enabled to match other Canopy widgets unless we have a strong reason to disable it.
- Selection and syntax highlighting require multi-style rendering. In v1 we accept per-cell
  rendering via `Render::put_cell`, and we add a benchmark to validate performance. If performance
  becomes a problem, consider a span-oriented render API extension.

### Mouse support (deferred)

Mouse support relies on correct `screen -> text` mapping and selection rendering. It should be
staged after the core editor is correct.

### Search and replace (deferred)

Search needs a UI for query entry and match navigation. The intended v1 UI is a vim-like overlay
modal with a single-line input at the bottom. Search results are highlighted in the editor and
navigation is via vi commands (`n`/`N`).

### Syntax highlighting (stretch)

- Define a `Highlighter` trait that returns styled spans for a given text range.
- Integrate optional highlighting into rendering without coupling to a specific engine.
- Stretch implementation can use `tree-sitter` (parsing) + theme mapping, or `syntect`.

## API sketch (subject to refinement)

Buffer module (internal to `crates/canopy` initially):

- `buffer.rs`: `TextBuffer`, `Snapshot`
- `position.rs`: position/index types + conversions
- `selection.rs`: `Selection`
- `edit.rs`: `Edit`, `Transaction`
- `error.rs`: error type (only if we expose fallible APIs)

In `crates/canopy/src/widgets/editor`:

- `layout.rs`: wrap/cache + mapping types, `WrapMode`
- `editor.rs`: `Editor`, `EditorConfig`, `EditMode`
- `highlight.rs` (stretch): `Highlighter`, `HighlightSpan`

## Testing strategy

- **Unit tests for buffer**:
  - insert/delete/replace across lines, undo/redo, multi-line edits, selection changes.
  - grapheme-aware cursor movement and deletion (emoji, accents, combining marks).
  - invariants: round-trip undo/redo, range queries match buffer text, cursor always in bounds.
- **Layout tests**:
  - wrapping with Unicode widths, tabs, and soft-wrap behavior.
  - `text -> screen` and `screen -> text` mapping (mouse positioning correctness).
  - vertical motion preferred-column behavior (vim-like).
  - auto-grow height calculations and cap behavior.
- **Widget integration tests** (using `testing/harness`):
  - simulate key sequences in both text-entry and vi modes; assert buffer + cursor + selection.
  - render snapshots for visible text (wrapping, scrolling, selections).
- **Property tests** (using `proptest`):
  - random edit sequences vs. a reference string model for small documents.
  - selection/cursor invariants after edits.
- **Performance benchmarks**:
  - insert/delete latency at 10K lines
  - rewrap costs for edits at top/middle/bottom
  - per-cell selection rendering cost vs. baseline

## Decisions (locked)

- **Key dispatch model**: keep Canopy's bindings-first behavior unchanged; editor consumes keys via
  `on_event` only when unbound.
- **Scrolling integration**: reuse core scrolling (`Widget::canvas` + node scroll offset); no
  editor-internal viewport.
- **Buffer packaging**: start as an internal module; extract a crate later only if reuse emerges.
- **Cursor/selection semantics**: user-facing logical line + display column; internal rope-native
  index plus preferred column for vertical motion.
- **Wrap policy**: grapheme wrap in v1; consider word-boundary preference later.
- **Tabs**: store `\t` and expand via configurable tab stops (default 4 columns) in layout.
- **Single-line behavior**: Enter bubbles (no newline insert); pasted newlines become spaces.
- **Change notification**: no new event plumbing; apps query editor state as needed.
- **Validation/filtering**: handled by higher-level widgets, not the editor core in v1.
- **Rendering**: accept per-cell selection/highlight rendering in v1; benchmark and revisit if slow.
- **Clipboard**: paste via `Event::Paste`; vi yank/put uses an internal register; OS clipboard
  later.
- **Search UI**: vim-like overlay modal with a single-line input at the bottom.

## Staged implementation plan

### Stage One: Scaffold and dependency selection

Set up module skeletons with tests and benchmark hooks, and select the rope implementation.

1. [x] Evaluate rope libraries (`ropey`, `crop`, `jumprope`) and select one.
2. [x] Add buffer module skeleton (internal) with unit test scaffolding.
3. [x] Add `proptest` as a dev dependency for the buffer module.
4. [x] Add new editor module skeletons in `canopy` and document `EditorConfig`/`EditMode`.
5. [x] Extend `testing/harness` helpers for editor key sequences + render snapshots (as needed).
6. [x] Add editor-specific benchmarks (criterion is already in the repo).

### Stage Two: Buffer model + undo/redo

Implement the text buffer core, selections, and transactions with exhaustive unit tests.

1. [x] Implement rope integration and the core text storage API.
2. [x] Implement position/index types and conversions.
3. [x] Implement selection + cursor semantics (including preferred column state for vertical
       motion).
4. [x] Implement insert/delete/replace primitives and line helpers.
5. [x] Implement undo/redo transactions and change tracking (`revision`).
6. [x] Add unit tests for Unicode graphemes, multi-line edits, and selection updates.
7. [x] Add property tests for edit invariants.

### Stage Three: Layout + mapping + scrolling

Add wrapping, mapping, and scrolling logic with incremental caching and layout tests.

1. [x] Implement wrap/mapping cache keyed by buffer revision + wrap width.
2. [x] Implement `text -> screen` and `screen -> text` mapping APIs.
3. [x] Integrate scrolling via core scroll offsets and cursor visibility adjustments.
4. [x] Implement horizontal scrolling for `WrapMode::None`.
5. [x] Add layout tests covering wrap behavior, mapping correctness, and preferred-column motion.

### Stage Four: Editor widget integration (text-entry mode first)

Wire buffer + layout into the widget, implement text entry mode, and update examples.

1. [x] Implement `Editor` rendering and measurement, including auto-grow behavior.
2. [x] Implement `on_event` handling for typing, backspace/delete, enter/newline rules, paste.
3. [x] Implement selection rendering (even if vi mode is not yet enabled).
4. [x] Add integration tests for render output and event sequences.

### Stage Five: Single-line input migration

Make `Input` share the editor buffer/mapping logic, without changing how apps use it.

1. [x] Rebuild `widgets/input.rs` on the new buffer/mapping utilities (or a single-line editor
       mode).
2. [x] Preserve `Input` public behavior and existing tests.
3. [x] Add regression tests for key handling interactions (app bindings vs text entry).

### Stage Six: Vi mode

Add modal behavior, a small key-sequence parser, and tests for normal/insert/visual transitions.

1. [x] Implement editor-internal mode state (normal/insert/visual).
2. [x] Implement core motions: `h/j/k/l`, `w/b/e`, `0/$`, `^`, `gg/G`, `gj/gk`.
3. [x] Implement mode entry/exit: `i/a/I/A/o/O`, `v/V`, `Escape`.
4. [x] Implement edit operators and commands: `x`, `dd`, `cc`, `D`, `C`.
5. [x] Implement visual mode ops: `d`, `y`, `c`, `x`, `>`/`<`.
6. [x] Implement yank/put using the internal register.
7. [x] Implement undo/redo: `u`, `Ctrl+r`.
8. [x] Implement repeat: `.`.
9. [x] Add mode/command tests, including cursor/selection invariants and preferred-column behavior.

### Stage Seven: Mouse support

Add mouse interactions for cursor positioning and selection.

1. [x] Implement click-to-position.
2. [x] Implement click-and-drag selection.
3. [x] Implement double/triple click selection (word/line).
4. [x] Add mouse interaction tests.

### Stage Eight: Search and replace

Implement incremental search with match highlighting and the overlay search UI.

1. [x] Implement the overlay search input UI (vim-like).
2. [x] Implement search state and match tracking.
3. [x] Implement `/` and `?` (vi), plus `n`/`N` navigation.
4. [x] Implement match highlighting in render.
5. [x] Implement search-and-replace with confirmation.
6. [x] Add search/replace tests.

### Stage Nine: Line numbers and polish

Add line number display and polish remaining features.

1. [x] Implement optional line number gutter.
2. [x] Add relative line number mode (for vi users).
3. [x] Run benchmarks and do an optimization pass.
4. [x] Documentation and examples.

### Stage Ten: Syntax highlighting (stretch)

Integrate highlighting behind a trait and add snapshot tests for styled rendering.

1. [x] Define `Highlighter` trait and integrate it into rendering.
2. [x] Evaluate and select highlighting backend.
3. [x] Implement the chosen backend and theme mapping.
4. [x] Add highlighting snapshot tests and example usage.
