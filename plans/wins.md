# PLAN: Canopy Clear Wins

## Description

This survey records clear improvements across Canopy's runtime, widgets, macros, automation, examples, and tooling. Each retained item has a demonstrated cause, a bounded repair, and specific proof. The checklist defines the complete implementation scope.

## Summary Checklist

- [ ] W1: Replace each search occurrence once
- [x] W2: Merge committed style components at canonical map keys
- [ ] W3: Preserve failed evaluation outcomes through the CLI client
- [ ] W4: Preserve non-socket files at live socket paths
- [ ] W5: Save the correct selection for redo
- [x] W6: Require every character to satisfy a text-style assertion
- [ ] W7: Preserve Todo entries when database deletion fails
- [ ] W8: Replace Todo fixture rows in one transaction
- [ ] W9: Propagate list activation errors
- [x] W10: Exclude hidden ancestors from focus candidates
- [x] W11: Clear layout state throughout excluded subtrees
- [x] W12: Ignore terminal key-release events
- [x] W13: Preserve errors from explicit two-parameter Result returns
- [x] W14: Isolate generated command bindings from parameter names
- [x] W15: Give recursive tree records the correct children type
- [ ] W16: Enforce read-only mode for undo and redo
- [ ] W17: Enter insert mode after a visual change
- [ ] W18: Insert below the current line at the correct position
- [ ] W19: Preserve line boundaries during linewise put
- [ ] W20: Exclude complete CRLF endings from logical line contents
- [ ] W21: Skip frame titles when the frame has no top edge
- [ ] W22: Keep the input caret visible at the right edge
- [ ] W23: Use content-relative mouse coordinates in the editor
- [ ] W24: Render and select the scrolled dropdown rows
- [ ] W25: Render and select the scrolled selector rows
- [ ] W26: Apply search styles only to overlapping graphemes
- [ ] W27: Apply inherited effects to terminal colors
- [ ] W28: Apply inherited effects to image colors
- [ ] W29: Preserve the released button in SGR mouse reports
- [ ] W30: Make terminal focus-report tests deterministic
- [ ] W31: Use terminal columns for double-click word selection
- [ ] W32: Reject failed live fixture acknowledgments
- [ ] W33: Serialize session replacement and disconnection
- [x] W34: Preserve full-width flex remainders
- [x] W35: Avoid overflow when calculating content origins
- [x] W36: Remove narrowing from page scrolling
- [x] W37: Clamp negative screen-region dimensions to zero
- [x] W38: Return before shift detection for identical terminal buffers
- [x] W39: Reuse the desired-key set in reconciliation
- [x] W40: Construct horizontal splits without an intermediate widths vector
- [x] W41: Resolve only the requested owner while waiting for a node
- [x] W42: Correct mouse-event coordinate documentation
- [ ] W43: Remove the transaction guard's permanently enabled flag
- [ ] W44: Align help keys by terminal width
- [ ] W45: Load Todo rows in explicit identifier order
- [ ] W46: Measure Todo text at its rendered width
- [ ] W47: Make Todo modal dimming repeatable
- [ ] W48: Correct the documented FNV-1a digest
- [ ] W49: Preserve Stylegym modal dimming when effects change
- [ ] W50: Measure FontGym input and its cursor in display columns
- [ ] W51: Keep FocusGym's root block after repeated deletion
- [ ] W52: Propagate font-directory entry errors
- [ ] W53: Document the current API check command

## Scope and Compatibility

The survey started from a clean tree at `9c018d9e044a714398a43757b80947924feaf685` on September 5, 2026. No existing plan occupied this path. Only this plan changed.

Three independent readers covered the core and geometry, reusable widgets, and automation/CLI/Todo areas. The coordinating reader covered derives, demos, documentation, and tooling, then reopened retained evidence.

All handwritten source, tests, benchmarks, Luau scripts, manifests, documentation, and configuration in this repository were covered. Generated API skeletons were searched as consumers. Binary media, third-party font licenses, generated snapshots, `Cargo.lock`, build outputs, and ignored scratch files were excluded from improvement proposals. Dependency implementations were read only where needed to establish a local contract. No upstream changes are proposed.

`api-surface/README.md` explicitly permits breaking changes and treats skeletons as review artifacts. These repairs nevertheless preserve public signatures, command names, CLI arguments, fixture names, and storage schemas. The FNV correction changes API digest values. Each item states any other behavior correction.

The 53 retained items are ordered broadly by impact, repair cost, and risk. Their order is not an execution sequence.

The initial survey was static. The execution record below tracks implementation and validation after approval.

## Execution Plan

The user approved all 53 items and coherent stage commits. Source work can proceed in independent areas. One driver owns Cargo and Ruskel execution.

1. Core runtime and geometry: W2, W6, W10–W12, W15, W34–W42.
2. Command macro dispatch: W13–W14.
3. Editor state, replacement, and coordinates: W1, W5, W16–W20, W23, W26, W43.
4. Widget rendering and input: W9, W21–W22, W24–W25, W27–W31, W44.
5. MCP and CLI outcomes and lifecycle: W3–W4, W32–W33, W48.
6. Todo persistence and presentation: W7–W8, W45–W47.
7. Demo behavior and API documentation: W49–W53.
8. Final workspace validation and generated artifact reconciliation.

Each stage ends with focused and package proof, checklist updates, and a commit. The final stage runs the full validation contract. The ignored run record is `tmp/wins.run.csv`.

## Execution Proof

- Core runtime and geometry (15 items): `ncode test -E 'package(canopy) | package(canopy-geom)'` passed all 386 tests. New regressions cover the kept contracts. Source review confirms the removed duplicate collection and traversal. Generated API documentation is refreshed before the stage commit.
- The stronger W6 assertion exposed an existing editor test whose cursor covered part of its highlighted span. Move the cursor beyond the asserted text. This preserves the whole-span assertion.
- The installed `ncode test -p` kept workspace test selection. Subsequent package proof uses explicit Nextest package expressions to select only the intended tests.

- Command macros (2 items): `ncode test -E 'package(canopy-derive)'` passed all 9 tests. Explicit error returns and positional/named binding collisions are covered.

## Checklist Adjustments

- Grouped the priority-ordered survey into seven related implementation stages. No retained item was removed or deferred.
- Validation initially stopped during dependency resolution: the configured tmcp sibling is now 0.6.0, while Canopy required 0.5.0. Align the local requirement and lockfile in a prerequisite compatibility commit. Keep all sibling sources unchanged.

## Items

### W1: Replace each search occurrence once

- Outcome: Replace-all terminates without revisiting inserted text or skipping adjacent original matches.
- Evidence: `crates/canopy-widgets/src/editor/search.rs:391` repeatedly calls `replace_match`. That helper rescans the edited buffer and chooses a match strictly after the old start. Replacing `a` with `aa` repeatedly revisits inserted text. Deleting adjacent `a` occurrences skips the next match when it moves to the same start.
- Change: Add a private `find_matches_from` helper that searches the suffix beginning at a supplied character position. Let ordinary `find_matches` start at `(0, 0)`. After replacement, search directly from the post-insertion cursor, return only remaining matches, and reset their index to zero. Use this same forward helper for `y` and `a`. Close read-only confirmation without editing.
- Constraints: Search the suffix itself, rather than filtering full-buffer non-overlapping matches. Preserve UTF-8 boundaries, literal non-overlapping search, replacement normalization, forward cursor/history order, and `n`/`q`. No iteration cap is needed.
- Proof: Cover `a → aa`, `aaaa / aa → a`, adjacent deletion, unchanged replacement, Unicode, newline-containing replacement, no matches, and read-only state. Test sequential `y` and `a` after `n`. Assert exact contents and prompt completion.

### W2: Merge committed style components at canonical map keys

- Outcome: Separate style-rule batches preserve unspecified components and cannot remove required root defaults.
- Evidence: `StyleMap::insert_style`, `crates/canopy/src/core/style/mod.rs:491`, replaces an existing partial style. Public rule methods promise merging at lines 517–558. A foreground-only update at the root removes its background and attributes, causing `PartialStyle::resolve` to panic. Equivalent canonical paths can also overwrite each other within one batch.
- Change: At canonical insertion, store `new.join(existing)` for an existing entry and insert directly otherwise.
- Constraints: Preserve new-component precedence and whole-field attribute replacement, including explicit empty attributes. Do not change attribute merging into set union.
- Proof: Test a foreground-only root update, separate foreground/background batches, equivalent slash spellings, and explicit replacements. Required defaults must remain. Built-in theme golden output must stay unchanged.

### W3: Preserve failed evaluation outcomes through the CLI client

- Outcome: Smoke and replay report script failures, and failed evals retain their JSON and requested journal.
- Evidence: `crates/canopyctl/src/session.rs:82` calls `call_tool_structured`. `ScriptEvalOutcome::to_tool_result` in `crates/canopy-mcp/src/script.rs:179` marks failed outcomes with `is_error`. The installed `../tmcp/crates/tmcp/src/client.rs:900` rejects that flag before decoding. This bypasses three existing consumers: smoke failure accounting, replay failure accounting, and eval output in `crates/canopyctl/src/main.rs:312,361,430`.
- Change: Use `call_tool("script_eval", request)`, then `extract_as::<ScriptEvalOutcome>(ToolResultMode::Structured)`. Decode valid outcomes regardless of the envelope's error flag. Attach context to missing or malformed structured content and preserve transport errors.
- Constraints: Keep the server's error flag. Other tools retain their current decoding policy. No tmcp change is needed.
- Proof: Test successful, failed, timed-out, missing, and malformed outcomes through an in-process MCP peer. Test failure followed by success in smoke and replay, with and without `--fail-fast`. A failed `eval --journal-out` must write JSON and the journal before returning nonzero.

### W4: Preserve non-socket files at live socket paths

- Outcome: A live MCP path cannot silently delete a regular file or symlink.
- Evidence: `serve_uds`, `crates/canopy-mcp/src/server.rs:254`, removes any existing path before binding. `crates/canopy-mcp/src/launch.rs:45` accepts this path from the application's MCP argument. One mistaken path can destroy an unrelated file.
- Change: Inspect with `symlink_metadata`. Permit the existing replacement behavior only for a socket file type. Return an `AlreadyExists` I/O error for other existing types, including symlinks. Accept `NotFound` and propagate other metadata failures.
- Constraints: Preserve stale-socket replacement. This item does not resolve active-listener replacement or concurrent path replacement, which remain on the watch list.
- Proof: In temporary directories, reject regular files and symlinks while preserving their bytes and links. Verify startup and shutdown for a new path and replacement of a stale socket.

### W5: Save the correct selection for redo

- Outcome: Redo restores the edit's actual resulting selection.
- Evidence: `TextBuffer::replace_range`, `crates/canopy-widgets/src/editor/buffer.rs:230`, records before advancing the cursor. Standalone `record_edit` at line 394 saves the old selection as both before and after. `redo` at line 203 installs that stale selection, potentially outside the shortened buffer.
- Change: Capture the original selection explicitly. Set the post-edit caret before recording, and construct standalone transactions with distinct before/after selections. Preserve active transaction grouping and its commit-time final selection.
- Constraints: Keep text edits and public signatures unchanged. Clamping the wrong redo selection is insufficient.
- Proof: Check text and selection before edit, after edit, after undo, and after redo for insertion, selected replacement, deletion, and multiline deletion. Retain grouped transaction coverage.

### W6: Require every character to satisfy a text-style assertion

- Outcome: A partially styled substring cannot pass a whole-substring style assertion.
- Evidence: `BufTest::contains_text_style`, `crates/canopy/src/core/testing/buf.rs:93`, sets its style flag when any character matches and never clears it for later style mismatches. Consumers include editor selection tests and Stylegym effect assertions.
- Change: Reject a candidate span on the first character or style mismatch. Succeed only when all matched characters satisfy every specified style component.
- Constraints: Preserve empty-string false, unsupported-gradient false, and current text matching. Broader grapheme matching is outside this item.
- Proof: A substring with only its first, middle, or last character red must fail an all-red assertion. Uniform red must pass. Verify foreground, background, and attributes remain conjunctive. Recheck existing consumers because this correction may expose false-positive tests.

### W7: Preserve Todo entries when database deletion fails

- Outcome: Failed database deletion leaves the selected Todo visible and selected.
- Evidence: `Todo::delete_item`, `examples/todo/src/lib.rs:317`, removes a list entry before the fallible SQLite delete at line 328. A rejecting trigger leaves storage and UI inconsistent. `accept_add` at line 339 already uses persistence-first ordering.
- Change: In the existing list callback, resolve the selected entry's database ID, delete its stored row, then remove the selected widget. Return without mutation when no entry is selected.
- Constraints: Preserve successful selection recovery and error reporting. This follows the add path's policy and does not introduce a transaction across SQLite and widget lifecycle hooks.
- Proof: Install a rejecting delete trigger in a Todo unit test. Verify command failure, unchanged storage and list length, and the same selected row. Retain successful delete and navigation smoke coverage.

### W8: Replace Todo fixture rows in one transaction

- Outcome: Failed fixture replacement preserves the previous database contents.
- Evidence: `Store::replace_todos`, `examples/todo/src/store.rs:71`, clears the table and commits each insertion separately. All three fixtures reach this method through `examples/todo/src/lib.rs:475`. A failure during either three-item fixture leaves a partial replacement.
- Change: Open `self.conn.unchecked_transaction()?`, perform the existing clear and inserts on that connection, then commit. Return accumulated records only after successful commit.
- Constraints: Preserve `Rc<Connection>`, schema, identifiers, and successful insertion order. The installed rusqlite supports transactions through a shared connection. No repository caller opens another transaction around this method.
- Proof: A trigger rejecting the second replacement item must leave original rows and IDs intact. Test successful replacement and an empty replacement. Keep the regression in the existing store test module.

### W9: Propagate list activation errors

- Outcome: Failed row activation reaches the normal event error path.
- Evidence: `List::dispatch_activate`, `crates/canopy-widgets/src/list.rs:443`, reduces `dispatch_command_scoped` to `.is_ok()`. Its only caller at line 421 discards that boolean, hiding every command failure.
- Change: Return `Result<()>` from the private helper, map a successful command result to unit, and propagate with `?` from mouse-up. Missing activation configuration remains a successful no-op.
- Constraints: Release mouse capture before dispatch, as today. Preserve command scope, row arguments, and canceled-drag behavior.
- Proof: Drive down/up with an activation command that returns an error. Assert that the error surfaces and capture is released. Retain successful activation and canceled-drag tests.

### W10: Exclude hidden ancestors from focus candidates

- Outcome: Focus traversal and recovery cannot select descendants of hidden or display-none containers.
- Evidence: `is_focus_candidate`, `crates/canopy/src/core/world/focus.rs:379`, checks only the node's own hidden flag and optional view. Recovery at line 151 can retain or select a hidden descendant. Pre-layout fallback omits the view check, so clearing cached views alone does not repair this behavior.
- Change: Reject a candidate when it or any ancestor is hidden or has `Display::None`, before asking its widget to accept focus.
- Constraints: Preserve explicit `set_focus`'s attached-node validation and visible pre-layout fallback policy. Do not add a requirement that visible candidates already have geometry.
- Proof: Hide a focused child's ancestor and verify recovery to a visible sibling. Repeat for `Display::None`, before layout, with no visible candidate, and after unhiding. Exercise normal and fallback traversal.

### W11: Clear layout state throughout excluded subtrees

- Outcome: Hiding a previously visible subtree clears every descendant's cached geometry and scroll state.
- Evidence: `visible_children`, `crates/canopy/src/core/world/layout_driver/mod.rs:641`, excludes hidden and display-none children before `layout_node` can clear them. `update_views` at line 163 resets only the excluded root's view. Descendants retain old rectangles, canvas, and scroll state. Existing hidden-layout coverage hides before initial layout.
- Change: Call existing recursive `clear_layout(node_id)` from the excluded branch of `update_views`.
- Constraints: Preserve the existing reset policy, tree contents, and visible-node layout. This repair is independent of focus candidate filtering.
- Proof: Lay out a nested subtree, scroll it, then hide its parent or set `Display::None`. Assert all descendant rectangles, content sizes, canvases, scroll positions, and views reset. Restore visibility and verify recomputation.

### W12: Ignore terminal key-release events

- Outcome: A press/release pair produces one application action while key repeats remain active.
- Evidence: `translate_event`, `crates/canopy/src/core/backend/crossterm.rs:592`, ignores `KeyEvent.kind`. Both `EventSource` ingestion paths translate all terminal keys. The selected crossterm Windows parser emits a Release event for key-up, so normal input can trigger twice.
- Change: Filter Release during terminal ingestion. Continue waiting or draining after ignored events in both awaited and ready/coalescing paths. Preserve Press and Repeat.
- Constraints: Preserve event errors, EOF, framework wakes, and mouse ordering. Do not turn ignored releases into synthetic wakes.
- Proof: Feed finite Press/Release/Repeat streams and assert only Press/Repeat emerge. Cover ignored release followed by error or EOF, internal wakes, and mouse coalescing across ignored releases.

### W13: Preserve errors from explicit two-parameter Result returns

- Outcome: Commands returning `Result<T, E>` preserve execution failures, including with `ignore_result`.
- Evidence: `parse_return_type`, `crates/canopy-derive/src/parse.rs:137`, uses an extractor restricted to one generic argument. `std::result::Result<T, E>` therefore sets `is_result` false. With `ignore_result`, `codegen.rs:194` emits a discarded call followed by successful `Null`, even when the method returns `Err`.
- Change: Add a Result-specific extractor that accepts exactly one or two type arguments and returns the success type. Preserve path-tail recognition. Keep Option extraction restricted to one type argument.
- Constraints: Preserve one-parameter result aliases, unit results, conversion errors, and ignored successful values. Do not broaden unrelated type parsing.
- Proof: Cover alias and qualified Result forms in parser tests. Invoke explicit `Result<Opaque, canopy::error::Error>` with `ignore_result`: success must return `Null`, failure `CommandError::Exec`. Also cover explicit `Result<String, Error>` without `ignore_result`.

### W14: Isolate generated command bindings from parameter names

- Outcome: Legal command parameters cannot shadow the generated receiver or argument containers.
- Evidence: `crates/canopy-derive/src/codegen.rs:71,129` emits original parameter identifiers into invocation locals. `invoke_tokens` at line 365 also uses `target`, `ctx`, `inv`, `values`, and `normalized`. A `target: String` parameter replaces the receiver. A `values` or `normalized` parameter can break conversion of later arguments.
- Change: Generate a private mixed-site binding identifier from each parameter's full declaration index. Use that identifier for context, injected, and user bindings and the final method arguments. Keep original names solely for metadata and named lookup. Keep positional indexing over user parameters separate.
- Constraints: Preserve command IDs, named keys, injection order, argument conversion, and original method signatures. This is internal macro hygiene, not an API rename.
- Proof: Extend derive integration tests with `target`, `values`, `normalized`, `ctx`, `inv`, and a name resembling the private prefix. Exercise positional and named calls, multiple user arguments, context, and injected arguments. Assert dispatched values and results.

### W15: Give recursive tree records the correct children type

- Outcome: Generated Luau types accurately describe recursive tree records.
- Evidence: `crates/canopy/src/core/script/defs.rs:127` defines `NodeInfo.children` as node handles. `TreeNode` at line 146 intersects NodeInfo with recursive children. An intersection constrains both members rather than replacing one. Runtime `tree_node_to_arg`, `script/records.rs:60`, replaces children with recursive records.
- Change: Build NodeInfo and TreeNode tables from one private common-field builder parameterized by the children type. Keep both existing alias names and their field documentation.
- Constraints: Preserve runtime record shapes and NodeInfo's handle array. Add no public alias solely for factoring. Update any exact generated-declaration expectations affected by the corrected type.
- Proof: Typecheck recursive traversal using child `name`, `id`, and `children`. NodeInfo children must still work as node handles. Passing a TreeNode record itself as a NodeId must fail.

### W16: Enforce read-only mode for undo and redo

- Outcome: History commands cannot modify a read-only editor.
- Evidence: Editing helpers guard `config.read_only`, but public undo/redo at `crates/canopy-widgets/src/editor/widget.rs:847` and vi `u`/Ctrl-r at `editor/vi.rs:357` do not. `set_config` permits enabling read-only after history exists.
- Change: Guard both command and vi history paths before any text or selection mutation.
- Constraints: Retain history for use after read-only is disabled. Preserve movement and search. Transaction boundaries remain a separate watch-list concern.
- Proof: Create writable undo and redo history, enable read-only, and exercise command and vi paths. Text and selection must remain unchanged. Disable read-only and verify history remains usable.

### W17: Enter insert mode after a visual change

- Outcome: Vi visual `c` immediately accepts replacement text.
- Evidence: `crates/canopy-widgets/src/editor/vi.rs:856` deletes the selection, enters Insert, then calls `exit_visual`. That helper at line 190 resets the mode to Normal.
- Change: Exit visual mode before entering Insert. Preserve the deletion and subsequent insertion in the active text-entry transaction.
- Constraints: Retain yank contents, read-only handling, and one undo group.
- Proof: Drive visual selection, `c`, replacement text, and Escape. Assert text, Insert/Normal transitions, and one-step undo. Repeat for visual-line selection.

### W18: Insert below the current line at the correct position

- Outcome: Vi `o` types into the newly opened line.
- Evidence: `crates/canopy-widgets/src/editor/vi.rs:259` inserts a newline at `line_end_position(line, true)`, the next line's start. The caret then enters the old next line. `RepeatableEdit::OpenBelow` repeats this calculation at line 1274.
- Change: Insert at the current line's end excluding its separator in both branches. The resulting caret then points into the new line.
- Constraints: Preserve multiline and read-only guards, insertion grouping, and `O` behavior.
- Proof: From `one\ntwo`, `o`, `X`, Escape must produce `one\nX\ntwo`. Cover EOF with and without a trailing newline and the empty-insert repeat path.

### W19: Preserve line boundaries during linewise put

- Outcome: Linewise `p` and `P` insert separate lines at EOF and before existing lines.
- Evidence: `yank_line`, `crates/canopy-widgets/src/editor/vi.rs:1021`, can capture a final line without a separator. `put_yank` at line 1031 inserts those bytes directly. On `one`, `yy p` produces `oneone`.
- Change: In multiline linewise put, terminate the yank when inserting before an existing logical line if it lacks a separator. When appending after an unterminated final line, insert a newline before the yank. Preserve existing yank bytes otherwise.
- Constraints: Keep characterwise put, single-line normalization, and one insertion transaction. Avoid duplicate separators, including at an empty final line.
- Proof: Assert complete strings for final-line `p`/`P`, terminated yanks, insertion before a middle line, and empty final lines. Verify undo and redo.

### W20: Exclude complete CRLF endings from logical line contents

- Outcome: CRLF text exposes the same logical line contents and navigation boundaries as LF text.
- Evidence: `crates/canopy-widgets/src/editor/buffer.rs:124` subtracts one character from nonfinal Rope lines, and line 140 pops one character. For `a\r\nb`, both helpers retain the carriage return. Navigation, wrapping, search, and rendering consume these helpers.
- Change: Share a private trailing-separator-length calculation between line length and line text. Remove two characters for CRLF and one for the existing other nonfinal-line case.
- Constraints: Preserve stored bytes and public character positions. Do not normalize files or expand the newline policy beyond CRLF.
- Proof: For `a\r\nb`, assert line text `a`, length one, right movement to `(1, 0)`, and backward deletion joining to `ab`. Undo/redo must restore exact CRLF bytes. Compare LF and CRLF wrapping and search.

### W21: Skip frame titles when the frame has no top edge

- Outcome: Titled frames render safely in tiny terminal allocations.
- Evidence: `Frame::render`, `crates/canopy-widgets/src/frame.rs:123`, unconditionally calls `f.top.line(0)`. `FrameRects::new`, `crates/canopy-geom/src/frame.rs:33`, returns empty rectangles when either dimension is at most two. `Rect::line` rejects line zero of that empty top edge.
- Change: Render the title only when `f.top` has nonzero width and height.
- Constraints: Preserve the geometry contract, normal title clipping, borders, and scrollbar handling.
- Proof: Render titled frames at zero, one, and two columns/rows and at the first valid size. All must succeed. Retain the normal title snapshot.

### W22: Keep the input caret visible at the right edge

- Outcome: A nonzero-width Input viewport always contains its caret.
- Evidence: `InputBuffer::ensure_cursor_visible`, `crates/canopy-widgets/src/input.rs:133`, scrolls to expose the caret, then resets scroll when text exactly fills the viewport. Its `text_width - 1` cap also hides the end caret in a one-column viewport.
- Change: Include the caret cell after the text in the scroll extent. Use `text_width.saturating_add(1).saturating_sub(view_width)` as the maximum scroll. Preserve the preceding cursor-following adjustment and zero-width handling.
- Constraints: Keep grapheme-aware column calculations and text editing behavior.
- Proof: Cover empty, exact-fit, and longer text at widths one and three, movement to both ends, deletion, and wide graphemes. Assert `cursor_display() < view_width` for every nonzero viewport.

### W23: Use content-relative mouse coordinates in the editor

- Outcome: Editor clicks and drags stay accurate when the editor itself has padding.
- Evidence: `crates/canopy/src/core/canopy/routing.rs:58` converts mouse locations relative to `view.content`. `crates/canopy-widgets/src/editor/widget.rs:604` subtracts `content_origin()` again, shifting positions and rejecting initial content cells. WidgetEditor installs padding on its editor child.
- Change: Add `view.tl` directly to the event location, then subtract only the line-number gutter. Remove the second padding subtraction and its rejection.
- Constraints: Preserve scroll offsets, gutter behavior, multi-click selection, and render coordinates.
- Proof: Click the first and later content cells of a padded editor. Repeat with line numbers and scrolling, and assert cursor positions and drag ranges.

### W24: Render and select the scrolled dropdown rows

- Outcome: Expanded Dropdown rendering and clicks refer to the visible canvas rows.
- Evidence: `crates/canopy-widgets/src/dropdown.rs:166` renders from item zero, while `handle_click` at line 100 treats viewport y as the item index. `canvas` advertises the complete list, so scrolling can make both operations target the wrong rows.
- Change: Start expanded items at `view.tl.y`, draw at viewport row positions, and add that offset for clicks. Draw in content-local coordinates. Slice labels and collapsed display by `view.tl.x` with `text::slice_by_columns`.
- Constraints: Preserve highlight/selection indices, collapse behavior, styles, and `EventOutcome::Ignore`, which allows subsequent mouse bindings.
- Proof: Expand a list longer than a short viewport, scroll, check labels, and click its first visible row. Assert the absolute selected index. Include padding, horizontal scroll, and collapsed rendering.

### W25: Render and select the scrolled selector rows

- Outcome: Selector renders and toggles the items visible at its scroll offset.
- Evidence: `crates/canopy-widgets/src/selector.rs:173` renders from item zero despite its complete-item canvas. `handle_click` at line 115 likewise ignores `view.tl.y`.
- Change: Map viewport rows to item indices using `view.tl.y`. Draw at the content origin and slice the composed checkbox/label text by the horizontal scroll offset.
- Constraints: Preserve selection order, empty-list behavior, styles, and `EventOutcome::Ignore`. This change is independent of the Dropdown repair and needs no shared abstraction.
- Proof: Scroll a short viewport over a longer selector, assert visible checkbox rows, and click to toggle the corresponding absolute item. Cover horizontal scrolling and ordered multiple selection.

### W26: Apply search styles only to overlapping graphemes

- Outcome: Other matches and syntax styles remain visible on the current match's line.
- Evidence: `crates/canopy-widgets/src/editor/widget.rs:764` enters the current-search branch whenever the line has a current match. Graphemes outside that match never reach other-match or syntax branches.
- Change: Include the overlap predicate in the branch condition. Retain precedence: selection, overlapping current match, overlapping other match, syntax, plain text.
- Constraints: Preserve highlight caching and styles.
- Proof: Render two matches and a separately colored syntax span on one line. Assert all three styles simultaneously. Add a selection and verify its precedence. Use direct cell assertions until the text-style assertion helper is repaired.

### W27: Apply inherited effects to terminal colors

- Outcome: Terminal runs respect inherited effects, including Root help dimming.
- Evidence: `render_run`, `crates/canopy-widgets/src/terminal.rs:905`, creates raw `ResolvedStyle` values and writes graphemes. `Render::put_grapheme` does not apply effects. `Render::apply_effects`, `crates/canopy/src/core/render/mod.rs:149`, explicitly supports externally sourced styles. Root adds a brightness effect to the terminal's containing pane.
- Change: Build raw `Style` values for normal and selection-inverted colors. Apply the renderer's effects once to each, resolve their solid colors, and select the appropriate result per grapheme.
- Constraints: Preserve ANSI colors, raw selection inversion, coordinates, and no-effect output. Apply inversion before effects so both styles receive the same inherited policy.
- Proof: Render a known colored run with normal and selected cells under brightness and an attribute effect. Assert foreground, background, and attributes directly. Retain unchanged output with no effects.

### W28: Apply inherited effects to image colors

- Outcome: Image pixels respect inherited effects, including Root help dimming.
- Evidence: `ImageView::render_cells`, `crates/canopy-widgets/src/image_view.rs:311`, passes sampled colors directly to `put_cell`, bypassing the explicit external-style effect path in `Render::apply_effects`.
- Change: For each sampled cell, construct a solid `Style`, apply effects once, resolve it, and write the half-block as before.
- Constraints: Preserve sampling, scaling, glyphs, alpha behavior, and no-effect output. This repair is independent of terminal rendering.
- Proof: Render known image colors under brightness and attribute effects and compare both cell colors and attributes. Verify no-effect output remains identical.

### W29: Preserve the released button in SGR mouse reports

- Outcome: SGR release reports retain the actual button identity.
- Evidence: `encode_mouse`, `crates/canopy-widgets/src/terminal.rs:1011`, replaces every release code with three before choosing SGR output. The selected Itty implementation, `../itty/crates/itty-core/src/backend/input.rs:80`, retains button codes zero, one, or two and uses suffix `m` for release.
- Change: Apply release code three only when SGR is disabled, before adding modifier bits. Preserve button identity in the SGR branch.
- Constraints: Retain coordinate clamping, wheel/motion codes, modifiers, and legacy bytes.
- Proof: Table-test each button's press and release in SGR and legacy modes, with modifiers. Assert exact bytes.

### W30: Make terminal focus-report tests deterministic

- Outcome: Focus-report tests prove enabled reporting and cannot silently pass after a timeout.
- Evidence: `focus_reports_follow_the_terminal_mode`, `crates/canopy-widgets/src/terminal.rs:1163`, sends mode bytes into PTY input. If a 200-step polling loop never observes enabled reporting, the test returns successfully. Ordinary selection and mouse tests also create shells despite needing only terminal state.
- Change: Add a test-only stream-backed Terminal fixture using the selected dependency's `Session::new_stream` and retain its input receiver. Feed mode bytes through synchronous `feed_stream_output`. Reuse this fixture in the ordinary double-click and encoding tests. Remove the polling helper.
- Constraints: Keep the ignored attached-driver integration test and real-process coverage separate.
- Proof: Assert disabled, enabled, and disabled-again focus reports synchronously. Run the ordinary focus, selection, and encoding tests without shell creation or sleeps. Verify every enabled-state assertion executes.

### W31: Use terminal columns for double-click word selection

- Outcome: Double-click selects the correct word around wide and combining characters.
- Evidence: `Terminal::select_word`, `crates/canopy-widgets/src/terminal.rs:436`, treats terminal x as an index into `Vec<char>`, then passes character indices back as cell columns. `render_run` advances by grapheme display width, so the coordinates disagree.
- Change: Build grapheme spans with start/end cell columns using `text::grapheme_width`. Find the clicked span by cell column, expand across adjacent non-whitespace spans, and select the inclusive first/last cells.
- Constraints: Test-fixture dependency: W30. Preserve whitespace-click behavior, the whitespace-delimited word definition, viewport rows, and scrollback conversion.
- Proof: Use the stream fixture to test CJK words, combining accents, and following ASCII words. Click each wide cell where applicable and compare copied text with the intended complete word.

### W32: Reject failed live fixture acknowledgments

- Outcome: A failed live fixture cannot produce a false applied acknowledgment.
- Evidence: `Session::apply_fixture`, `crates/canopyctl/src/session.rs:108`, discards a raw tool result. Raw `Client::call_tool` preserves error envelopes. The proxy emits `{ "applied": name }` after this method returns at `crates/canopyctl/src/main.rs:225`. The interactive run path also relies on it at line 282.
- Change: Check `is_error()` on the live result. Return `error_message()` with a fixture-specific fallback when it fails.
- Constraints: Keep successful acknowledgment fields optional. Preserve headless default-fixture selection.
- Proof: A fake MCP peer must prove successful and error-marked acknowledgments. Verify the proxy does not report an applied fixture after failure. Retain headless fixture coverage.

### W33: Serialize session replacement and disconnection

- Outcome: Concurrent proxy operations cannot overwrite an intervening session or finish disconnect before an earlier connection installs.
- Evidence: `SessionManager::connect_live`, `crates/canopyctl/src/session.rs:142`, releases the state mutex during shutdown and handshake. `session()` at line 186 can install a headless child during that gap. The final assignment overwrites it without shutdown. `disconnect()` can likewise observe the temporary empty state.
- Change: Hold one state guard through each entire connect or disconnect transition, including shutdown and connection installation. Remove `take_session`, whose complete-tree callers are these two methods.
- Constraints: Preserve shutdown-before-replacement and the empty state after failed connection. Keep the existing serialization of eval and fixture calls.
- Proof: Control a fake MCP handshake with channels. While it is pending, concurrent eval and disconnect must remain blocked. After release, verify operation order and final session state. Use synchronization, not sleeps.

### W34: Preserve full-width flex remainders

- Outcome: Large valid flex weights receive the correct largest-remainder allocation.
- Evidence: `allocate_flex_shares`, `crates/canopy/src/core/world/layout_driver/mod.rs:774`, narrows each remainder to u32 at line 789. With two remaining cells and weights `[3000000000, 4000000000, 4000000000]`, saturation turns the correct `[0, 1, 1]` into `[1, 1, 0]`. Both measurement and placement call this helper.
- Change: Retain `prod % total` as u64 through remainder sorting.
- Constraints: Preserve u32 output shares, stable index tie-breaking, and zero-weight handling.
- Proof: Add the exact large-weight case alongside flex allocation tests. Retain sum, proportionality, and equal-remainder checks.

### W35: Avoid overflow when calculating content origins

- Outcome: Extreme valid signed origins produce the correct local offset without overflow.
- Evidence: `View::content_origin`, `crates/canopy/src/core/view.rs:35`, subtracts in i32 before clamping. Outer `i32::MIN` and content `i32::MAX` overflow, although their nonnegative difference fits u32. `view_rect_local` and many rendering widgets use this result.
- Change: Widen both operands to i64 before subtraction on each axis, clamp negative differences to zero, and convert the representable nonnegative difference to u32.
- Constraints: Preserve public types and the current policy for reversed origins.
- Proof: Test equal, reversed, ordinary padded, and MIN-to-MAX origins on both axes. Assert `content_origin` and `view_rect_local` results.

### W36: Remove narrowing from page scrolling

- Outcome: Large page heights cannot reverse scroll direction or panic.
- Evidence: `Context::page_up` and `page_down`, `crates/canopy/src/core/context.rs:456`, cast unsigned content height to i32. Heights above i32::MAX change sign, and negating i32::MIN panics. Text, help, and demo page commands use these defaults.
- Change: Snapshot the view and call `scroll_to` with unchanged x and y adjusted using unsigned saturating subtraction or addition of content height.
- Constraints: Preserve existing canvas clamping and changed-result semantics. `update_scroll` already synchronizes `view.tl`, so consecutive calls must remain cumulative.
- Proof: Cover ordinary and zero-height pages, both boundary clamps, heights i32::MAX plus one and u32::MAX, and repeated calls before layout.

### W37: Clamp negative screen-region dimensions to zero

- Outcome: Negative crop dimensions return an empty region instead of the screen remainder.
- Evidence: `host_screen_region`, `crates/canopy/src/core/script/base_api.rs:1357`, converts negative dimensions with `u32::try_from(...).unwrap_or(u32::MAX)`. Thus a negative width or height becomes an enormous positive extent.
- Change: Clamp dimensions to the unsigned range before constructing the rectangle.
- Constraints: Preserve signed-origin clipping and positive-overflow saturation.
- Proof: Extend script observation tests with negative width, negative height, and zero extents. All return empty text. Retain ordinary crops, offscreen origins, and huge positive dimensions.

### W38: Return before shift detection for identical terminal buffers

- Outcome: Unchanged repeated rows emit no terminal operations.
- Evidence: `TermBuf::diff`, `crates/canopy/src/core/termbuf/mod.rs:511`, tries row shifts before testing equality. Identical blank 3-by-3 buffers match its shift heuristic on Crossterm. This emits an unnecessary scroll, exposed-row repaint, and flush. Current no-change coverage uses one row without shift support.
- Change: After canonical validation and size handling, return success when the cell vectors are equal.
- Constraints: Preserve invalid-buffer rejection, changed-size repaint, and real shift behavior. The comparison short-circuits on the first changed cell.
- Proof: Use a shift-capable recording backend with a flush counter. Equal multirow blank and repeated-row buffers must produce zero operations. Retain real-shift replay and malformed-buffer tests.

### W39: Reuse the desired-key set in reconciliation

- Outcome: Successful keyed reconciliation avoids a redundant set allocation and key copies.
- Evidence: `KeyedChildren::reconcile`, `crates/canopy/src/core/children.rs:91`, builds `seen` to validate desired keys. Line 126 builds the same owned-key set again for removal membership. List synchronization uses this method. For n desired keys, the second set costs one allocation, n clones, and n insertions.
- Change: Use the existing `seen` set for removal membership and remove `desired_set` construction.
- Constraints: Preserve duplicate validation before callbacks, creation/update/removal order, and rollback behavior.
- Proof: Retain successful reorder, duplicate-key, and rollback tests. Source review must show only one desired-key set construction. No benchmark claim or new clone-count test is required.

### W40: Construct horizontal splits without an intermediate widths vector

- Outcome: Horizontal splitting produces its result directly with one fewer allocation and traversal.
- Evidence: `Rect::split_horizontal`, `crates/canopy-geom/src/rect.rs:102`, allocates widths through private `split` at line 170, then builds rectangles. Complete-tree search finds no other helper caller. Splitting n sections allocates an unnecessary n-element u32 vector.
- Change: Preserve the zero-sections check, calculate quotient and remainder once, and build rectangles directly with result capacity n. Remove the private helper.
- Constraints: Preserve extra cells in the first remainder sections, zero-width sections when n exceeds width, saturating offsets, and public errors.
- Proof: Retain the coverage property. Add exact remainder order, zero sections, excess sections, and saturated-origin cases.

### W41: Resolve only the requested owner while waiting for a node

- Outcome: Each wait-for-node poll resolves one requested owner instead of every registered command.
- Evidence: `wait_for_node`, `crates/canopy/src/core/script/base_api.rs:723`, computes the full command availability vector, then selects one owner. `CommandResolver::availability`, `core/commands.rs:917`, traverses targets for every command, including repeated owners and unrelated commands.
- Change: First check the registry for a node-dispatched command belonging to the requested owner. If present, call `CommandResolver::resolve_owner` once, anchored at current focus or root as in `command_availability_from_focus`.
- Constraints: Preserve the registration requirement, focus-relative resolution, async polling, and timeout behavior. Do not broaden the function to arbitrary mounted owners. Keep all changes inside the existing host implementation.
- Proof: Cover available/unavailable registered owners, multiple commands for one owner, free commands, unregistered mounted owners, focus changes, and timeout. Review that each poll performs at most one owner traversal and allocates no availability vector.

### W42: Correct mouse-event coordinate documentation

- Outcome: Widget authors use the coordinate origin that routing actually supplies.
- Evidence: `MouseEvent::location`, `crates/canopy/src/core/event/mouse.rs:141`, says to add the outer origin. Routing at `core/canopy/routing.rs:58` subtracts the content origin. Frame explicitly adds its content offset when it needs outer-local coordinates.
- Change: Distinguish incoming screen coordinates from delivered content-local coordinates. Document saturation for events before the content origin, including captured and padding events.
- Constraints: Do not promise reversible conversion after saturation. No behavior or public type changes.
- Proof: Review routing and Frame's conversion against the new documentation. Refresh the generated API skeleton if its rustdoc changes. No new behavior test is needed.

### W43: Remove the transaction guard's permanently enabled flag

- Outcome: The scoped transaction guard contains no redundant state or dead conditional.
- Evidence: `TextBuffer::transaction`, `crates/canopy-widgets/src/editor/buffer.rs:181`, is the sole constructor and always sets `active: true`. The private field at line 470 is never modified. Its only read guards commit in `Drop` at line 489. Complete-tree searches include tests, examples, and snapshots.
- Change: Remove the field, initializer, and condition. Commit unconditionally on drop.
- Constraints: Preserve existing nested-transaction behavior. Do not redesign transaction ownership.
- Proof: Retain `transaction_guard_groups_edits_until_drop` and confirm the removed field has no remaining references. No new test is needed for the removed impossible branch.

### W44: Align help keys by terminal width

- Outcome: Unicode and ASCII keys align in the normal help layout.
- Evidence: `crates/canopy-widgets/src/help/binding_list.rs:127` computes key width in terminal cells. `binding_lines` at line 290 passes it to character-count string padding. A width-two key such as `界` receives an extra space. Existing wide-key coverage exercises only narrow layout.
- Change: Compute each key's terminal width and prepend exactly the difference from `max_key_width` as spaces.
- Constraints: Preserve sorting, narrow layout, description wrapping, and continuation indentation.
- Proof: Use a wide help layout with `a`, `界`, and a modified key. Assert that every description begins in the same terminal column.

### W45: Load Todo rows in explicit identifier order

- Outcome: Reopened Todo lists preserve append order independently of SQLite scan choices.
- Evidence: `Store::todos`, `examples/todo/src/store.rs:84`, has no `ORDER BY`. `Todo::new` and `ensure_tree` consume that order directly at `examples/todo/src/lib.rs:147,181`. Additions and fixtures append in ascending identifier order.
- Change: Use `SELECT id, item FROM todo ORDER BY id`.
- Constraints: Preserve schema, identifiers, and return type.
- Proof: Enable `PRAGMA reverse_unordered_selects = ON` on the test connection. Multiple rows must still load in ascending identifier order. Retain row-conversion error coverage.

### W46: Measure Todo text at its rendered width

- Outcome: Wrapped Todo rows have enough height to display their contents.
- Evidence: `TodoEntry::measure`, `examples/todo/src/lib.rs:59`, wraps at the full width. Rendering at line 90 reserves two columns, then truncates to measured height. At outer width eight, `aaaa bbb` measures as one line but renders into six text columns and needs two.
- Change: Use one two-column gutter constant in measurement and rendering. Measure wrapping at outer width minus the gutter. Retain height one when no text columns are visible. Keep the reported outer width and constraint clamping.
- Constraints: Preserve the selection indicator, spacer, and textwrap behavior.
- Proof: Render the width-eight example and assert both words remain visible. Cover widths zero, one, two, normal widths, and empty content in measurement and Todo harness tests.

### W47: Make Todo modal dimming repeatable

- Outcome: Repeated modal opening retains one dimming effect.
- Evidence: `sync_modal_state`, `examples/todo/src/lib.rs:218`, appends brightness 0.5 on every open state. Both repeated `todo.enter_item()` and repeated `modal_open` fixtures reach it. `Context::push_effect`, `crates/canopy/src/core/context.rs:1052`, appends effects, so two openings reduce brightness twice.
- Change: Clear effects on the Todo-owned main-content container before applying current modal state. Add exactly one brightness effect when open. Retain visibility updates and input reset.
- Constraints: The current closed branch already owns and clears all effects on this private container. Preserve modal focus behavior.
- Proof: Compare main-content cell styles after one and two openings and repeated `modal_open` fixtures. Closing must restore undimmed styles. Retain modal and help smoke coverage.

### W48: Correct the documented FNV-1a digest

- Outcome: The API identity token matches its documented 64-bit FNV-1a algorithm.
- Evidence: `BootstrapResponse::api_digest`, `crates/canopy-mcp/src/script.rs:167`, promises FNV-1a. `stable_digest` at line 392 uses `0x1000_0000_01b3`, with one extra hexadecimal zero. Its only caller builds bootstrap data. The current test checks only nonempty output.
- Change: Use the prime `0x0000_0100_0000_01b3`.
- Constraints: Preserve the offset basis, wrapping arithmetic, UTF-8 bytes, and 16-character lowercase output. Nonempty digests change. Complete-tree searches found no persisted digest or equality consumer.
- Proof: Assert the vectors `"" = cbf29ce484222325`, `"a" = af63dc4c8601ec8c`, and `"foobar" = 85944171f73967e8`.

### W49: Preserve Stylegym modal dimming when effects change

- Outcome: Changing selected effects while the modal is open preserves the modal's dimming.
- Evidence: `Stylegym::show_modal`, `crates/examples/src/stylegym.rs:297`, adds brightness 0.5 to `DemoContent`. `apply_effects` at line 379 clears that same effect list and restores only selected effects. The controls remain active while this demonstration modal is visible.
- Change: After applying selected effects, append brightness 0.5 when `modal_visible` is true. Keep dimming last so this matches opening the modal after selecting effects.
- Constraints: Preserve effect order, frame exclusion, and hide behavior. Repeated application must produce one dimming effect.
- Proof: In `crates/examples/src/tests/stylegym.rs`, show the modal, change an effect, and compare resolved content styles with applying that effect before opening. Repeat application and close the modal. Verify frame styles remain unaffected.

### W50: Measure FontGym input and its cursor in display columns

- Outcome: Wide and combining characters no longer displace the visible input cursor or measured width.
- Evidence: `FontGymInput::cursor`, `crates/examples/src/fontgym.rs:575`, uses a character index as a terminal column. `measure` at line 672 also counts characters. Rendering uses display columns. For `界a`, the final cursor is column three, but current code returns two.
- Change: Convert the character cursor to a byte boundary with existing `byte_index_for_char`. Measure that prefix and the complete value with `text::slice_by_columns(..., 0, usize::MAX).1`, matching existing FontLabel usage in this file.
- Constraints: Preserve character-based editing and ASCII behavior. Do not replace the demo input or add scrolling policy in this item.
- Proof: Add local tests for `界a`, combining text, and ASCII. Check cursor columns at start, intermediate positions, and end, plus unconstrained measurement width.

### W51: Keep FocusGym's root block after repeated deletion

- Outcome: Repeated delete commands cannot remove the demo's structural block and leave an unusable empty app.
- Evidence: `FocusGym::delete_focused`, `crates/examples/src/focusgym.rs:228`, removes any focused leaf beneath its root block. `Core::focused_leaf`, `crates/canopy/src/core/world/focus.rs:77`, includes the root itself. After its two children are deleted, the root becomes a focusable leaf and the next delete removes it. New blocks require an existing Block receiver.
- Change: Return successfully without deletion when the focused node equals `root_block`. Preserve descendant deletion and subsequent focus recovery.
- Constraints: Keep the initial tree and all split/add bindings. The root remains available for splitting after its last child disappears.
- Proof: Extend `crates/examples/src/tests/focusgym.rs` to delete beyond the initial child count. The root ID must remain live and focused. Split it again and verify two usable children.

### W52: Propagate font-directory entry errors

- Outcome: Font discovery reports enumeration failures instead of silently producing an incomplete cycle.
- Evidence: `load_font_sources`, `crates/examples/examples/widget.rs:234`, handles directory-open and file-read errors but drops per-entry errors through `filter_map(entry.ok())`. One failed directory entry can disappear from the loaded font set without explanation.
- Change: Use a fallible loop over directory entries. Attach the directory path to entry errors, retain the case-insensitive TTF filter, then sort paths and load their bytes as today.
- Constraints: Preserve font order, labels, and the existing empty-directory error. No discovery abstraction or dependency is needed.
- Proof: Review the fallible iteration path and verify valid fonts retain sorted order, mixed extensions remain filtered, and an empty font directory still fails. Do not add a test framework solely to synthesize a filesystem iterator error.

### W53: Document the current API check command

- Outcome: API maintenance instructions name a command that exists.
- Evidence: `api-surface/README.md:21` claims `cargo xtask ci` runs the API check. `xtask/src/main.rs:27` defines no `Ci` task. `Task::Checks` at line 57 runs the API and tracked Luau checks.
- Change: Replace that documentation reference with `cargo xtask checks`. Retain `cargo xtask api --check` for the API-only check.
- Constraints: This documentation repair does not choose a replacement hosted-CI policy.
- Proof: Check both command names against the live `Task` enum and dispatch. Run the Markdown checks available locally and `git diff --check`; compilation is unnecessary.

## Validation Contract

Each item names focused behavioral proof. During implementation, run the relevant `ncode test -p PACKAGE FILTER` checks and confirm they select tests. The packages are `canopy`, `canopy-geom`, `canopy-widgets`, `canopy-derive`, `canopy-examples`, `canopy-mcp`, `canopyctl`, and `todo`. These commands exclude doctests.

After a package's changes settle, run its package tests. For the complete checklist, finish with `ncode test`, `ncode tidy --check`, and `cargo xtask smoke`. The workspace's tidy hooks include feature, API/Luau, and benchmark compilation checks. Refresh API skeletons with `cargo xtask api` where public documentation or generated surfaces change, then review their diffs. Finish with `git diff --check`.

No item authorizes source changes in sibling dependencies. Report dependency or environment failures separately from local results. Do not use the removed `cargo xtask ci` command as a validation gate.

## Watch List

These observations do not enter the implementation checklist. They need a policy choice, broader repair, or additional validation.

- **Hosted CI:** `.github/workflows/ci.yml` documents unavailable sibling dependencies and still runs removed `cargo xtask ci`. It also references a missing toolchain file. Choose dependency provisioning and the hosted nanocode/toolchain installation policy before replacing the job. The API documentation item repairs only the locally settled command reference.
- **Live socket ownership:** `crates/canopy-mcp/src/server.rs:237,255,294` can unlink an active listener or a later replacement path. Shutdown also repeats through Drop. The retained non-socket guard does not settle active-socket takeover, inode ownership, or races.
- **Child processes and idle shutdown:** `crates/canopyctl/src/session.rs` has explicit shutdown but no drop cleanup. Standalone commands and early process exits can bypass it. `main.rs:479` records only request-start activity, so its watchdog can exit during a long request. These need one process-ownership and graceful-shutdown policy.
- **Failed-evaluation timing:** `crates/canopy-mcp/src/script.rs:269,426` reports zero build timing on construction failure and captures failed-typecheck timing before checking. Timing coverage and failure-stage accounting need a focused follow-up.
- **Todo store ownership:** `examples/todo/src/store.rs` stores one database in a thread-local slot. Constructing another app on that thread changes the store used by the older app. Connection ownership needs a broader decision.
- **Replay session semantics:** Headless replay creates a fresh app for each entry. A stateful journal replay contract would change the existing per-entry fixture model.
- **Editor transaction boundaries:** Bound history and cursor commands bypass text-entry commit behavior. Mouse events also return early. `Editor::set_text` replaces the buffer while retaining its transaction flag, vi state, and prompt. Settle continuation of vi insertion and buffer-replacement state before repairing these together.
- **Multiline syntax highlighting:** `editor/highlight.rs:59` starts a fresh syntax parser per line. Correct multiline comments and strings need a buffer-aware parser and invalidation contract.
- **Editor effect composition:** `editor/widget.rs:784` combines syntax colors with an already effect-applied background, then applies effects again. Select the raw-background source and cover interaction with selection/search styles before changing this path.
- **Additional newline and vi behavior:** Rope recognizes separators beyond LF/CRLF, while position advancement primarily splits LF. `cc`, word-end movement, visual-line direction, and repeat recording also have behavior gaps. Preserve stored text and settle those editing semantics separately.
- **Horizontal editor clipping:** A partially clipped grapheme is shifted to column zero through saturation. Gutter and wide-grapheme behavior need one combined rendering decision. Similar partial-grapheme alignment deserves follow-up in scrolled inputs.
- **Terminal reporting modes:** The terminal mouse path does not distinguish click-only, drag, and all-motion reporting when forwarding movement. Backend acquisition also does not enable every translated focus/paste event mode. Capability acquisition, rollback, and legacy platform behavior need a settled contract.
- **Root help failure handling:** `root.rs:300` changes ownership state before fallible subtree/token cleanup. Repeated `sync_layout` while help is open can also append dimming. Settle effect ownership and recovery before repairing the compound lifecycle path.
- **List activation identity:** Pending activation stores a row index. Row replacement between down/up can make that index identify another row. Decide whether activation follows a stable key or cancels on structural change.
- **Panes ownership:** Removing a pane changes bookkeeping and detaches through layout synchronization without explicitly deleting its subtree. Confirm whether the caller or Panes owns destruction.
- **Inspector log failures:** Polling suppresses flush errors. Retention, subscriber lifetime, and recovery need an explicit policy.
- **Script lifecycle and recursive values:** Compiled scripts and roots can remain retained beyond bounded journals. Callback ownership must guide unloading. Cyclic live Luau values also need an explicit rejection or identity policy before adding conversion guards.
- **Declaration discovery and visibility:** Recursive declaration discovery needs a symlink policy. `NodeInfo.visible` needs a definition covering ancestors, display state, and clipping before expanding its current own-node meaning.
- **Style and geometry edge contracts:** Empty public gradient-stop lists, incomplete default gradients, repeated attribute accumulation, event Display/parse round-trips, and extreme terminal-shift coordinates need explicit semantics. They are separate from the bounded numeric repairs above.

## Rejected

- Renames, module movement, and mechanical helper extraction were rejected when they retained the same complexity.
- Absent workspace consumers do not prove a public library API dead. Public surface removal and feature/format narrowing were rejected.
- New caches, cache rewrites, and broad rendering optimization lack measured evidence. Retained allocation changes instead identify exact redundant collections or traversals.
- Replacing FNV with a new hashing dependency adds no value over correcting its documented constant.
- Replacing all editor newlines on load would change stored file contents. Only CRLF logical-line interpretation is retained.
- Dropdown and Selector deliberately return Ignore after mouse handling so Stylegym bindings can apply changes. Converting those results to Handle would break consumers.
- Raw terminal/image writes were considered a possible effects exception. Render explicitly provides effect application for external styles, and Root dims the enclosing pane. No exception is documented, so the two concrete repairs are retained with no-effect equivalence checks.
- Polling-delay changes would retain false-success and lifecycle causes. No delay tuning is proposed.
- A broad Todo persistence redesign, stateful replay redesign, or hosted-CI dependency migration exceeds the settled local repairs.
- Earlier TermGym/Stylegym side-by-side layout concerns are already covered by current widget layouts and installed-Root regressions. No duplicate repair is proposed.

## Coverage

| Area | Covered handwritten material |
| --- | --- |
| `crates/canopy` | All runtime modules, layout, scripting, rendering, input, styles, testing helpers, inline/separate/integration tests, benchmark, Luau preamble, manifest. |
| `crates/canopy-geom` | All ten source modules, properties/tests, and manifest. |
| `crates/canopy-widgets` | Every control/container, editor, terminal, image/font, help, and inspector module; rendering tests, both benchmarks, both integration targets, six Luau fixtures, manifest. |
| `crates/canopy-derive` | Parser, metadata model, code generation, derives, integration tests, manifest. |
| `crates/canopy-mcp` and `crates/canopyctl` | Evaluation, protocol envelopes, launch/socket lifecycle, smoke discovery, configuration, sessions, replay, CLI/proxy, tests, manifests. |
| `crates/examples` | Every demo, shared host, both launchers, all six test modules, manifest. |
| `examples/todo` | App/store/launcher, all three test targets, all seven smoke scripts, CLI configuration, manifest. |
| Root, docs, tooling | Repository instructions, manifests, cargo/lint configuration, ignore rules, xtask and its tests, CI, README, four documents, API maintenance guide. |

Generated API skeletons and the theme golden were inspected for consumers and expected-output constraints. They were not treated as independently authored implementations. The plan has no unexamined handwritten subsystem and no count cap.
