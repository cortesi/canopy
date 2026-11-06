# Explicit state machines

## Description

This plan makes the implicit state machines of the workspace explicit. An
implicit state machine is a mode or a lifecycle that bools, Options,
counters, sentinel values, or parallel fields encode together, so that the
types admit combinations that the code must avoid by hand. Six parallel
searches read every crate and the examples on 2026-11-07. Each item under
Changes was checked at its cited lines. Items whose defect is marked
*reachable* were traced from a public entry point to the bad state. The
other items admit invalid states that no current path reaches.

The open decisions of the first draft were settled on 2026-11-07, and
are C33-C42. An external review (GPT-6.1 Sol) of that draft corrected
several items and added C43. Line numbers are those of the tree at `fc736573`. The changes are private
unless an item says otherwise. A public change migrates fh in the same
commit and refreshes `api/`.

## Practice

Each change follows these rules.

- **One enum for each machine.** Each variant holds only the data that is
  valid in its state. A field that has a meaning in only one state moves
  into that variant.
- **One writer for each transition.** The machine has a method for each
  event (`open`, `commit`, `observe_exit`). The method moves the state with
  `mem::replace` or `Option::take`. Other code reads the state through
  queries, such as `is_open()` or `dragging()`, and does not assign a
  variant.
- **Derive, do not mirror.** A flag that another field can answer
  (`dragging` against `drag_anchor`, `success` against `state`) becomes a
  method.
- **Scopes are guards.** A counter or a flag that is set for the length of a
  call (`callback_depth`, `draining`, `rolling_back_tree_edit`) is owned by
  an RAII guard, so an early return, a `?`, or an unwind restores it.
- **Effects leave the transition.** A transition that must post a command or
  release a resource returns it, as in `fn observe_exit(&mut self) ->
  Option<CommandCall>`. The caller, which holds the context, performs it.
  This keeps the machine testable without a harness.
- **Typestate only for build phases.** Runtime widget state stays an enum.
  Phases that run once in order, such as the script host's finalize, may
  use separate types.
- **Total queries.** A query on a machine returns a value for every state,
  and does not use `expect` to assume one.

## Testing

- **Regression first.** For each reachable defect, write a test that fails
  on the current tree, then make the change. Commit them together.
- **Transition tables.** A machine with a few states and events gets one
  test that drives each (state, event) pair through its transition method
  and asserts the next state and the returned effect. The machine is a plain
  value, so these tests need no harness.
- **Model tests.** The larger machines get proptest sequences of random
  events. Each step checks the invariants, and where it is cheap, a
  reference model. The workspace already uses proptest; extend the existing
  strategies where they exist:
  `crates/canopy-widgets/src/editor/tests/visual.rs:1141`,
  `crates/canopy-markdown/src/widget/cursor_tests.rs:896-937`, and
  `fuzz/fuzz_targets/vi_keys.rs`.
- **No wall clock.** Tests of polls, timeouts, and watchdogs use the
  runtime's manual clock, or tokio's paused time.
- **Regressions apart.** A test that fails on the current tree is named as
  a regression. A test of behaviour that already holds is a preservation
  test, and the plan does not call it a regression.

## Changes

### Editor (`crates/canopy-widgets/src/editor`, `text_buffer`)

#### C1: A vi insert session owns its undo group (reachable panic)

`ViState` (`vi/mod.rs:264-274`) keeps `mode`, `pending`, `insert_text`,
and `last_edit`. The undo group is a separate
`TextBuffer.transaction: Option<Transaction>` (`text_buffer/buffer.rs:54`).
Every entry to Insert calls `begin_text_entry_transaction()` and then
`vi.begin_insert()` by hand (`vi/mod.rs:546-567, 840-878`,
`vi/visual.rs:615-619`).

`Editor::undo` (`widget.rs:1345-1352`) does not commit an open
transaction. `TextBuffer::undo` (`buffer.rs:236-246`) then pops an older
transaction while newer edits remain uncommitted, and replays it by char
offset (`buffer.rs:494-503`). Insert mode does not claim Ctrl chords
(`vi/mod.rs:379-389`, `:510-513`), so a host binding of Ctrl+Z reaches
`undo` during an insert. *Reachable:* `iabc<Esc>A`, three Backspaces, then
undo calls `rope.remove(0..3)` on an empty rope, which panics. With
`ia<Esc>Ab` and an undo while the insert is still open, the editor removes
the older `a` and keeps `b`. The fuzz target sets Normal mode before it
undoes (`vi_keys.rs:173`), so it cannot find this.

The session also breaks in other places:

- `set_text` (`widget.rs:367-375`) and `publish_view` with `preserve`
  (`:409-411`) replace the buffer but keep Insert mode, so each later key
  is its own undo step.
- `set_config` from Text to Vi (`:355-359`) leaves Normal mode with an open
  transaction.

Text mode has the same gap. A typed character opens a transaction
(`widget.rs:880-885`), and other keys commit it. Mouse events
(`:993-1036`), `set_cursor` (`:1302-1306`), and `set_selection`
(`:450-456`) do not commit. So typing, a click, and more typing undo as one
step.

Change:

- `TextBuffer::undo` and `redo` commit an open transaction first. This
  alone removes the panic.
- `ViMode` stays the public `Copy` enum (`editor/mod.rs:20`,
  `vi/mod.rs:23-25`). The session lives in private state,
  `ViState.session: Option<InsertSession>`, and `ViMode` is derived from it.
- One `Editor::transition(to)` opens the transaction when it enters Insert,
  and commits it and records the repeat when it leaves. `set_vi_mode`
  (`widget.rs:1318`) goes through it.
- An undo or a redo during Insert commits the session's group and then
  undoes. The editor stays in Insert, the next key opens a new group, and
  the repeat record starts again from that key.
- `replace` keeps its contract (`widget.rs:1258`,
  `tests/mod.rs:2393`): it ends the current group, applies itself as its
  own step, and starts a new group, with the mode unchanged and no repeat
  recorded. fh relies on this for landed edits
  (`fh-audio/src/app.rs:763`).
- `set_text`, `publish_view`, `set_config`, and `clear` end an open session
  through `transition`.
- Text mode gets `entry: Entry { Idle, Typing }`. Every cursor relocation,
  including mouse, `set_cursor`, and `set_selection`, calls `end_entry()`.

Tests: regression tests for the panic, for `ia<Esc>Ab` with undo, and for
typing, a click, and more typing. Also test continued typing after an undo
in Insert, a redo straight after an undo, an undo of the first open group
with no older history, and `.` after an undo. Remove the forced Normal from
`vi_keys.rs`, so the fuzz target undoes in every mode. Add a proptest of
keys and undos against a model stack of texts.

#### C2: `.` repeats the edit that started the insert (reachable)

`end_insert` (`vi/mod.rs:335-342`) replaces `last_edit` with
`Insert { text }` whenever the session typed text. This overwrites the
`OpenBelow`, `ChangeLine`, or `ChangeToEnd` that `record_and_apply`
recorded (`:886-892`). `i`, `a`, `I`, and `A` record nothing
(`:546-567`). Visual delete always records `DeleteChar`
(`vi/visual.rs:597`), and visual change always records `ChangeLine`
(`:620`). *Reachable:*

- `oX<Esc>.` inserts `X` at the cursor, and does not open a line.
- `ccfoo<Esc>.` only inserts `foo`.
- `Afoo<Esc>.` inserts at the cursor, not at the end of the line.
- `Vjd.` deletes one character.
- A character-wise `vlc<Esc>.` deletes the whole line.

The recorder also misses edits made in Insert. Delete changes the buffer
without a record (`vi/mod.rs:678`), and a movement (`:688`) changes where
text goes, while the recorder only appends text and drops its last
grapheme.

Change: `Repeat { op: RepeatOp, keys: Vec<InsertKey> }`. `RepeatOp` names
the entry (`Insert`, `Append`, `InsertLineStart`, `AppendLineEnd`,
`OpenBelow`, `OpenAbove`, `ChangeLine`, `ChangeToEnd`) and the visual
extent (`VisualDelete { lines | chars }`, `VisualChange { .. }`).
`InsertKey` is a typed text, a Backspace, or a Delete. The session from C1
holds the op that started it, and `end_insert` fills in its keys instead of
replacing the op. A movement in Insert ends the recorded segment and starts
a new one with no op, as vi does. `repeat_empty_open_below_enters_another_new_line`
(`tests/mod.rs:1162`) keeps its behaviour.

Tests: a table of each entry key and each visual operator followed by `.`,
checked against the text that vi gives, with the mode and the undo steps
after each repeat. Add Delete and Backspace in a session, and a movement in
the middle of a session.

#### C3: An open prompt closes when its buffer changes (reachable)

`prompt: Option<PromptState>` (`widget.rs:97`) holds
`ReplaceConfirm { matches, index, replace_all }` (`search.rs:487-520`),
built from the buffer when the prompt opens. `set_text` (`widget.rs:367-375`)
resets the search but not the prompt, `publish_view(preserve)` keeps it
(`:409-418`), and `replace()` (`:1265-1280`) has no prompt guard, unlike
`insert()` (`:1248`). *Reachable:* after one of these, `y` replaces a
clamped range of the new text (`search.rs:654-657, 716-718`).
`replace_all` is set and used in the same call (`:670`, `:690-700`), so it
is not state.

The public `undo` and `redo` (`widget.rs:1346`, `:1356`) also change the
buffer under an open confirmation.

Change: one `close_prompt()` that `set_text`, `publish_view`, `replace`,
`set_config`, `undo`, and `redo` call. The confirmation's own replacements
do not close it. Remove the `replace_all` field and keep it a local.

Tests: for each of `set_text`, `publish_view(preserve)`, `replace`,
`undo`, and `redo`, open a replace confirmation, make the change, then send
`y`, and assert that the text is unchanged.

#### C4: An editor drag is one optional anchor

`selecting: bool` and `anchor: Option<TextPosition>` (`widget.rs:151-158`)
encode one drag. Up clears only `selecting` (`:1033-1035`). `set_text` in
the middle of a drag keeps the old anchor, and the next Drag sets a
selection that is clamped into the new text. Entering visual mode in the
middle of a drag lets Drag write a selection that is not snapped to
graphemes (compare `vi/visual.rs:208`).

Change: `drag: Option<TextPosition>`, which is `Some` only while the button
is down. Up, `set_text`, and a mode transition clear it.

Tests: a drag that `set_text` interrupts, and a drag across `v`.

#### C5: The editor search names its source

`SearchState` (`search.rs:42-55`) uses an empty query to mark an installed
set of matches (`:129`, `:158-187`). `direction` and `ignore_case` have a
meaning only for a query.

Change: `enum Source { None, Query { text, direction, ignore_case },
Installed }`, next to the matches, the revision, and the current index.

Tests: the existing installed-match tests, and a test that an edit after
installed matches drops them and reports `Source::None`.

### Widgets (`crates/canopy-widgets/src`)

#### C6: The terminal's life is one enum (reachable)

`session: Option<Session>` (`terminal.rs:156`), `exit_reported`
(`:168`), and the session's own `child_exited()` encode Unmounted,
Running, Exited, and Reported. *Reachable:*

- `poll` (`:662-670`) returns 16 ms forever, even after the exit is
  reported, so a dead terminal wakes the loop at 60 Hz.
- `classify_key` (`:463-476`) checks only that a session exists. After the
  exit, `key_outcome` (`:642`) claims every mappable key, and `handle_key`
  (`:445-453`) sends it to the dead PTY. A terminal that shows "Process
  exited" takes the host's keys.

Selection is a second implicit machine. `selection_active` and
`selection_anchor` (`:162-164`) encode one drag. When the child turns on
mouse reporting in the middle of a drag, `on_event` returns early
(`:581-588`) and leaves the drag active. After reporting is turned off,
the next left Drag extends from the old anchor.

Change:

- `enum Life { Unmounted, Running(Session), Exited { session, reported:
  bool } }`. One `fn observe_exit(&mut self) -> Option<CommandCall>`.
- `poll` returns `None` once the exit is reported, and while unmounted.
- After the exit, `classify_key` keeps the scrollback and copy keys, and
  returns `None` for `Forward`. Paste (`:570`), mouse reporting (`:584`),
  and focus reporting forward to the PTY only while `Running`. Scrollback
  and selection still work on the kept session.
- `selection_drag: Option<Point>`, which the mouse-reporting branch clears.

Tests: on a manual clock, an exited terminal asks for no further poll
after it reports, and ignores a printable key but still scrolls with
Shift+PageUp. After the exit, a paste is ignored, and a click with the
child's mouse reporting left on selects text. A drag that mouse reporting interrupts leaves no anchor.

#### C7: A table view's pointer gesture is one enum

`TableView.drag` and `scroll_drag` (`table_view/mod.rs:131-133`) and the
`drag: Option<Drag>` of each scrollbar (`scrollbar.rs:275`) are up to four
sources for one gesture. The view sets `scroll_drag` whenever a scrollbar
returns `Handle` (`:1029-1031`), including when the scrollbar did not
start a drag (`scrollbar.rs:575-577`). A second Down on a track during a
cell drag sets both flags. Esc ends the drag in the view
(`:597-605`) but leaves the scrollbar's drag until its next render. Each of
these heals on the next event.

Change:

- `enum Gesture { Idle, Cells, Scrollbar(ScrollAxis) }` in the view.
- `Scrollbar` gets `enum DragState { Idle, Held(Drag), Orphaned }` in place
  of `Drag.cancelled` (`scrollbar.rs:244-256`), and a public
  `is_dragging()`. The view enters `Gesture::Scrollbar` only when
  `is_dragging()` is true.
- `Scrollbar::cancel_drag()`, which `TableView::end_drag` (`:597-605`)
  calls, so Esc leaves the scrollbar idle at once, before a render.

Tests: a transition table for `Gesture`. A press on a full track does not
enter a scrollbar gesture. Esc during a scrollbar drag leaves both the
view and the scrollbar idle.

#### C8: A diff view shows a diff or a message (reachable)

`message: Option<String>` (`diff_view.rs:226`) sits beside the model.
`set_message` (`:316-319`) empties the model, but `with_message`
(`:275-278`) does not. *Reachable:* `DiffView::new(model).with_message(..)`
keeps the model behind the message, so the canvas size (`:756`, `:1193`),
`step_change` (`:538-548`), and the search marks work on hidden rows.

Change: `enum Content { Diff(Model), Message(String) }`. `loading` stays a
separate `Option<Instant>`.

Tests: `with_message` on a non-empty model gives the canvas of the message,
and `step_change` does nothing.

#### C9: An image view fits or holds a zoom (reachable)

`auto_fit: bool` and `zoom: f32` (`image_view/mod.rs:338-340`) encode one
mode. While `auto_fit` is set, `zoom` is a stale cache that only render
refreshes (`:405-414`). `zoom` settles it before it leaves fit
(`:707-711`), but `pan` (`:733-734`) does not. *Reachable:* `set_image`
and then `pan` before a render freezes the zoom at the placeholder 1.0, and
the image jumps.

Change: `enum Zoom { Fit, Manual(f32) }`, and
`fn take_manual(&mut self, view: Size) -> f32`, which every manual
operation calls.

Tests: `set_image` and then `pan` with no render between them keeps the
fitted zoom.

#### C10: A column chart's cursor follows or rests

`cursor: Option<usize>` and `following: bool` (`column_chart.rs:98-102`).
`set_columns(vec![], ..)` while following gives no cursor with
`following()` true (`:213-225`, `:263`), and the next non-empty
`set_columns` selects the newest column. That follows the documented
behaviour (`:210-213`), so this is a representation cleanup, not a defect.

Change: `enum Cursor { None, At(usize), Newest }`. `cursor()` and
`following()` derive from it, so the public API keeps its shape. `Newest`
on empty columns gives no cursor, and selects the newest column on refill.
A cursor that `set_columns` clamps onto the last column stays `At`, as now.

Tests: preservation tests, which pass today, of empty and refilled columns
while following, and of a clamped fixed cursor. A transition table of
`set_cursor` and `set_columns`.

#### C11: A picker's filter is off, editing, or applied (reachable)

`filtering: bool` and `filter: String` (`picker.rs:366-368`). Three tests
of "is there a filter" disagree: the field (`:683`), the title (`:709`),
and matching (`:605-609`, `:832`). *Reachable:* start the filter, type a
space, and press Enter. The field stays visible with a blank query, while
the title and the `WithoutFilter` items act as unfiltered.

Change: `enum Filter { Off, Editing(Query), Applied(Query) }`, where
`Query` normalizes its text once. An applied query that is empty after
normalization becomes `Off`.

Tests: the space-and-Enter sequence, and a transition table of
`start_filter`, `set_filter`, `clear_filter`, Enter, and Esc.

#### C12: A confirm dialog's question is one value

`Confirm` keeps four mounted parts as Options (`confirm.rs:179-185`) and
the question as `answers`, `decline`, `closing`, `tone`, and `token`
(`:187-195`). Under `ByCall`, the token stays `Some` after the call closes
the modal (`:283-287`), and `answer_status` (`:314-319`) does not check
that a question is open. `show` changes the row, the frame, and the text
before `open_modal` (`:253`), so a failed open leaves a half-built dialog.

Change: `parts: Mount<(), Parts>` (C42) and `question: Option<Question {
token, answers, decline, closing, tone }>`. `answer` and `answer_status`
act only on an open question. `show` replaces the answer buttons, the
title, the message, and the body layout before it opens the modal
(`:346-364`), and `Answers::set` removes the old buttons as separate tree
edits (`answers.rs:151`). So a failed open restores the previous parts as
well as the question: `show` keeps what it replaced, and puts it back when
`open_modal` fails.

Tests: first, a test of a scripted answer after a `ByCall` close. If it
posts the old call today, it is a regression test. Also test a failed open
after an earlier question, which leaves the dialog closed with the earlier
parts.

#### C13: A paged request has one state

`Paged` keeps `active: Option<Active>` with `dispatched: bool`,
`retry: Option<Work>`, and `status: Status` (`paged/mod.rs:159-166`,
`:221-225`). The types admit Loading with nothing active, and work that
is both active and waiting to retry. `retry()` (`:549-555`) takes the
retry work before it checks for an active request, so if both were set it
would drop the work. No current path sets both.

Change: `enum Request { Idle, Queued(Active), InFlight(Active),
Failed(Work), Stalled(Work) }`, with `status()` derived from it.

Tests: the existing suite, and a proptest of `first`, `navigate`,
`take_request`, `apply`, `fail`, `cancel`, and `retry` that checks after
each step that `status()` agrees with the request.

#### C14: Small widget machines

None of these has a reachable defect. Each removes a field that can
disagree with another.

- `SearchProgress` (`search_progress.rs:38-40`):
  `enum Run { Idle, Running { since: Option<Instant> } }`.
- `Answers.armed_at` (`answers.rs:104`), where `None` and a past instant
  both mean armed: `enum Arming { Armed, Until(Instant) }`, which `poll`
  sets back to `Armed`.
- The editor's `LayoutCache` (`editor/layout.rs:145-201`), where a wrap
  width of 0 means "never built": `built: Option<(Wrap, u64)>`.
- `Dropdown` (`dropdown.rs:28-37`): `enum Open { Collapsed,
  Expanded(RowCursor) }`. `toggle` closes through the same path as
  `cancel`.
- `Root.inspector_active` (`root.rs:94`) is the initial setting of
  `with_inspector` (`:143-149`) until `install` reads it (`:432`), and
  after that it duplicates the inspector node's hidden flag. Keep it as the
  initial setting only, and read the node's flag for live visibility. Test
  that `with_inspector(true)` still opens the inspector at start.

Tests: a transition table for `Open` and `Run`.

### Markdown reader (`crates/canopy-markdown/src`)

#### C15: The block-edit slot is one enum (reachable)

`Markdown.slot: Option<OpenSlot>` (`widget/mod.rs:250`) and
`OpenSlot.committed` (`widget/slot.rs:78`) are coupled to
`SurfaceState.awaiting` and `pending_mark` (`surface.rs:73-77`). The slot
reads its unit from `SurfaceState.marked` (`widget/slot.rs:155-185`).
*Reachable:*

- `select_block_at` (`widget/mod.rs:836-861`) is a command with no slot
  guard. With a slot open, it marks another unit, so the rows above and
  below the slot move around the new unit while the editor still edits the
  old one. About eighteen other entry points check `self.slot.is_some()`.
- When a committed slot's preparation fails, the error branch
  (`widget/mod.rs:2522-2534`) closes the slot but leaves `awaiting` and
  `pending_mark` set. Block steps (`surface.rs:538`), click-to-mark
  (`:1014`), and `editable` (`slot.rs:379`) stay off until an unrelated
  reschedule. A worker panic or `HeldWork::fail` (`widget/tests.rs:1925`)
  reaches it.
- `block_open`, `block_editor`, and `draft` test `!committed`
  (`widget/mod.rs:946-955`), but the guards test `slot.is_some()`. So
  `escape` reports handled while committed, and does nothing.

Change:

- `enum Edit { Idle, Open(OpenSlot), Committed(OpenSlot) }`, where
  `OpenSlot` stores its own unit and does not read `marked`. A committed
  slot keeps its child nodes and layout until the publication replaces it,
  because `finish` still uses them (`slot.rs:685-687`).
- Move `awaiting` and `pending_mark` into one
  `enum Pending { None, Mark(PendingMark), Edit(PendingMark) }` on the
  surface state, so the error path clears them together. This is the only
  copy of the mark: `Edit::Committed` holds none.
- Admission depends on the state, through two queries on `Edit`:
  - `Open` takes the slot editor's keys, and refuses reader commands,
    `select_block_at`, and `apply_edit` (`widget/mod.rs:1021`).
  - `Committed` takes no keys in the slot editor, as now. It lets keys reach
    the reader (`keys_after_a_commit_reach_the_reader_until_its_layout_shows`),
    and admits `apply_edit`.
  - `escape` reports handled only when it changes something.

Tests: `select_block_at` with a slot open leaves the slot on its unit. A
commit whose preparation fails gives back block steps, and keeps the
accepted text and any queued host edits. Repeated `apply_edit` calls while
a preparation is pending, and Esc on a committed slot. Extend the
`cursor_tests.rs` proptest `Action` with slot open, commit, discard, and
`select_block_at`.

#### C16: The inspector's stages are enums (reachable stuck state)

`reader`, `pending_source`, `display_source`, `detail`,
`detail_loading`, `work_search`, and `search_fresh` (`inspector.rs:401-429`)
encode a content stage and a search stage. `ready()` is a conjunction of
four fields (`:160-166`). The preparation error branch (`:1284-1289`)
leaves `pending_source`, `detail_loading`, and "Preparing…" set, so
`ready()` never becomes true. A worker panic reaches it.
`Markdown.inspected_link` (`widget/mod.rs:199`) lives outside the open
inspector, and `dismiss_inspector` must clear it in order (`:1761-1776`).

Change: `enum Stage { Preparing(SourceSnapshot), Source { display },
Table { detail: Detail }, Failed(String) }`, with `enum Detail { None,
Loading(SourceSnapshot), Shown { id, display } }`, and a separate
`enum Search { Idle, Running(WorkSearch), Fresh }`. `inspected_link` moves
into the open inspector.

- A failed first preparation moves to `Failed`. `ready()` is true, the view
  shows the error in place of "Preparing…", focus stays on the inspector,
  copy and search are off, and Esc closes.
- A failed cell detail moves back to `Table` with `Detail::None`, and posts
  a notice. The table stays usable.

Tests: under `HeldWork::fail`, separately for the first source and for a
cell detail. Assert that the pending preparation ends, that "Preparing…"
goes away, and that a failed detail leaves a table that takes keys. The
error report and the close on Esc pass today (`inspector.rs:741`,
`:1284`), so they alone do not test the change.

#### C17: Reader search results are one enum

`query_error`, `results`, `source_results`, `search_index`, and
`kept_match` (`widget/mod.rs:186-195`) are mirrored into
`overlays.search_current`. About nine sites reset the index and its mirror
together (`:510-513`, `:644-647`, `:1102-1117`, `:1418-1425`, `:1547`,
`:2620-2623`, `:2709-2842`). No path breaks them today.

`kept_match`, which `keep_search_position` sets (`widget/mod.rs:1513`) and
the next results take (`:2709`), lives across a pending search. fh uses it
on a reload: `view_watch.rs:231` in `canopy-fileselect` saves the place,
replaces the file, and restores it.

Change: `enum Matches { Idle, Pending { kept: Option<MatchPlace> },
Failed(String), Rendered { results, current }, Source { matches, current }
}`. A new query or a new text moves to `Pending` and keeps the place that
`keep_search_position` gave. The overlay reads its current match from it,
and keeps no copy.

Tests: the existing search tests, and a proptest of query, step, view
switch, and edit that checks that the overlay agrees with `Matches`. The
fh reload case: a kept place restores its match, also with two matches at
one byte, and with new results that fail or hold no match.

#### C18: Lists and quotes count depth apart (reachable)

`Context.depth` counts lists and quotes together (`document.rs:808-813`).
List depth is recovered as `depth - quote_depth` (`:897-906`,
`presentation/mod.rs:2316`), but the bullet glyph uses the combined depth
(`document.rs:861-870`). *Reachable:* for `> - a`, `Block.list_marker` is
`◦`, while the presentation draws `•` (`presentation/mod.rs:2332-2338`).

The two rules also differ below the third level: the document clamps to
`▪` (`document.rs:863-867`), and the presentation cycles by three
(`presentation/mod.rs:2334`).

Change: `list_depth` and `quote_depth` as separate counters, with `depth`
derived from them, and one bullet rule, which cycles by three, that both
use.

Tests: the list markers of `> - a`, `- > - a`, `> > - a`, and a list four
levels deep.

#### C19: A block's layout extra is one enum

`BlockLayout` keeps `table`, `flow`, `table_start`, `table_height`,
`cells`, and `image` (`presentation/mod.rs:257-284`). When
`table::prepare` fails after the flow preview is prepared, the flow is kept
(`:1488-1503`). It counts in the byte estimate (`:672-678`), but it never
paints (`:1130`).

Change: `enum Extra { None, Table { table, flow, start, height, cells },
Image(ImagePlacement) }`.

Tests: the reader's default widths and glyphs
(`presentation/mod.rs:1481`) do not reach the failure of `table::prepare`
(`table/mod.rs:902`). Either inject the failure, or check the structure:
no `Extra` holds a flow without a table.

#### C20: Small reader machines

None of these has a reachable defect.

- `action_status: String`, `action_failed`, and the `"Done"` sentinel
  (`widget/mod.rs:215-218`, `:2353`, `:3626`): `enum ActionStatus { None,
  Busy(&'static str), Done, Failed(String), Rejected(String) }` with
  `Display`.
- The executor lane (`executor.rs:320-341`), whose guard is copied at
  `:387-391` and `:411-412`. The worker and the preparation result are two
  separate dimensions: a copy may run while a preparation result is
  complete or waits for its acknowledgement (`:387-391`, `:760`, `:783`).
  So: `enum Worker { Idle, Preparing, Copying }`, `enum Prep { Idle,
  Completed(WorkResult), AwaitingAck }`, and the replaceable `desired`
  request, with one `fn can_prepare()`. Test a copy that runs beside a
  completed preparation, and beside one that waits for its acknowledgement.
- Publication (`widget/mod.rs:151`, `:170-180`), where `dirty` has about
  twelve writers: `enum Submission { Clean { last: Option<Key> }, Due {
  deadline: Option<Instant>, last: Option<Key> }, Sent(Key) }` with
  `schedule`, `sent`, and `invalidate` methods. `submit_work` dedupes
  against the last key (`:2450`), so `Due` keeps it. This also clears the
  deadline that the dedupe branch keeps (`:2450-2452`). Test a deduped
  submission and a retry after a rejected one.
- Host requests as membership of three sets (`host.rs:118-127`):
  `BTreeMap<u64, (ActionContext, Stage)>` with `enum Stage { Reserved,
  Ready(HostRequest), Delivered }`.

Tests: transition tables for `Prep` and `Submission`.

#### C21: xmark's tail watch

`TailWatch.open_quotes` (`crates/canopy-xmark/src/stream.rs:98-102`) is
written and never read: remove it. `open_leaves` and `in_run`
(`:48-59`) become `enum Run { Idle, Inline, Leaf(NonZeroUsize) }`.

#### C43: Markdown's scrollbar drag ends with its capture (reachable)

`Markdown.bar_drag` (`widget/mod.rs:231`) is set from
`ctx.has_mouse_capture()` after a scrollbar event (`:3469`), and is cleared
only by a later pointer event or Esc (`:1155-1157`). Opening a modal clears
the core's capture (`world/modal.rs:331`), and the scrollbar drops its own
drag when it sees no capture (`scrollbar.rs:347`, `:503`). *Reachable:*
help opened by a key during a scrollbar drag leaves `bar_drag` set. Then
`drag_active` (`widget/mod.rs:736-738`) reports a drag, `poll` defers every
completed publication (`:3396`), and view switches stay off, until the next
pointer event or Esc.

Change: `bar_drag` goes. `drag_active` asks the scrollbars through
`Scrollbar::is_dragging()` (C7), together with the capture. When the reader
sees that a drag it deferred for has ended, it resumes the deferred
publication.

Tests: a scrollbar drag that help interrupts, with a worker completion
while help is open, then help closes with no further mouse input: the
publication shows, and the view can switch.

### Runtime (`crates/canopy/src/core`)

#### C22: An evaluation keeps its ticket until it finishes (reachable)

`Driver.active: Option<ActiveEval>` and
`tickets: HashMap<EvalId, oneshot::Sender>` (`canopy/turn.rs:291-304`) are
separate, and `turn_inner` holds a taken eval in a local `completed`
(`:497`) until `finish_eval` (`:566-580`). *Reachable:* a `?` between the
take and the finish drops the eval: `abort_invocation(..)?` at `:516`,
`:541`, and `:544`, and `wake_registry.drain()?` and `poll_owners(due)?`
(`:526-527`) while `completed` holds a cancelled eval. A widget poll of
`Duration::MAX` makes `Poller::schedule` fail with "poll deadline overflow"
(`poll.rs:182-186`). Then the ticket never resolves until the `Canopy`
drops, the eval gets no journal entry, and its publication waiters keep
deadlines alive.

Change:

- `enum EvalSlot { Idle, Parked(ActiveEval), Finishing(ActiveEval,
  Result<ArgValue>) }`, with the reply sender inside `ActiveEval`. A dropped
  sender alone would leave no journal entry and keep the publication
  waiters, so it is not the cleanup.
- Every path from the take reaches `finish_eval` (`turn.rs:676`), which
  writes the journal, clears the waiters, and settles the ticket. The step
  does not use `?`.
- The order of results is fixed: an abort or poll failure replaces the
  eval's own result with that error; a frame failure then joins it, as
  `prepared.and(result)` does now; the turn returns the runtime error after
  the ticket settles.
- A separate commit makes `Poller::schedule` saturate an overflowing
  deadline to a far deadline that never fires, in place of the driver error
  at `poll.rs:182-186`.

Tests: in `turn_cleanup_tests.rs`, inject a fatal poll failure in the same
turn as a cancel, and an abort that fails. Each settles the ticket, writes
a journal entry, clears the waiters, and admits a later eval. These tests
must not use `Duration::MAX`, because the saturation removes that trigger.
Test the saturation on its own.

#### C23: Callback, dispatch, and cleanup scopes are guards

`callback_depth` (`world/mod.rs:92`, `:304-308`), completion `depth` and
`draining` (`completion.rs:123-126`, `:182-199`, `:348-352`),
`rolling_back_tree_edit` (`world/mod.rs:98`, `tree.rs:285-301`), and
`sealed` (`completion.rs:128`, `:288-290`) are incremented and reset by
hand. The widget cell has an RAII guard (`widget_access.rs:109`), but the
counters do not. A panic inside a callback leaves `callback_depth` at 1
forever. Then the completion queues never drain (`completion.rs:198`,
`:336`), deferred focus repair never runs, and `prepare()` always returns
`InvalidPhase`. The test `widget_slot_restores_after_callback_panic`
(`world/tests.rs:875`) does exactly this and checks only the cell. No
production path in the workspace catches a panic inside core, so this
matters to embedders and to that test's promise. These are recovery paths
for an embedder, not paths that the workspace takes today. The drain gate is written
twice (`completion.rs:198`, `:336`). The rollback unmount hooks get a bare
`CoreContext` (`tree.rs:298-299`) that bypasses `sealed` and
`callback_depth`.

`Driver::in_turn` (`canopy/turn.rs:485-492`) is set by hand too, so a
panic in a public `turn` that an embedder catches makes every later turn
report busy. A panic inside a tree-edit closure skips the take of the
journal (`tree.rs:256-258`), so later edits join the abandoned one.

Change:

- `CallbackScope` and `DispatchScope` guards own the counters, and the
  truncation of `notice_sources` and `event_scope`.
- A `TurnScope` guard owns `in_turn`.
- A `TreeEditScope` guard owns the journal. On an unwind, it rolls the edit
  back as an error would. A panic in an `on_unmount` during that rollback
  leaves the restored tree, and the guard does not run the remaining hooks
  twice.
- `enum Cleanup { None, Hooks(NonZeroU32), Rollback }`, entered through a
  guard, with `fn admits(op) -> Result<()>` in place of the checks at
  `completion.rs:324`, `tree.rs:227`, and `:582`.
- One `fn completion_ready(&self) -> bool`.

Tests: extend `widget_slot_restores_after_callback_panic` to post
completion work after the panic and assert that it drains. A caught panic
in a public `turn` is followed by a turn that runs. A caught panic in a
tree-edit closure is followed by an edit that commits alone. Add a panic in
`on_unmount` during rollback, followed by an edit that succeeds.
`canopy/snapshot_tests.rs:116`, which sets `callback_depth = 1` by hand,
enters a real `CallbackScope`.

#### C24: The mode stack holds its menu apart

`ActiveMode { name, transient }` in `mode_stack: Vec` (`inputmap/mod.rs:461-485`)
admits several transient entries anywhere. The guards at `:1011`, `:1035`,
and `:1056` keep at most one, at the top, and three readers scan the whole
stack anyway (`:774-782`, `:852-855`, `:1138-1142`).

Change: `struct ModeStack { modes: Vec<String>, menu: Option<String> }`,
where the menu is always the newest. The scans become reads of `menu`.
C41 builds on this.

Tests: the existing mode tests, and a proptest of enter, leave, open menu,
close menu, clear, and restore that checks the order of the reported
modes.

#### C25: A node's lifecycle is one enum

`initialized`, `mounted`, `attachment_generation`, `poll_lifetime`, and
`incarnation` (`node.rs:34-38`, `:87-92`) are set in `world/tree.rs:94-98`,
`:171-208`, and `:307-313`, and in `canopy/rendering.rs:111-134`. Only the
debug validator catches `initialized && !mounted` while attached
(`tree.rs:488-497`). `replace_subtree_inner` resets about fifteen fields by
hand (`tree.rs:171-187`).

Change: `enum Lifecycle { Unmounted, Mounted { polled: bool } }` and
`attachment: Option<NonZeroU64>`, with transition methods on `Node`. The
fields that belong to one incarnation move into a `NodeIncarnation`
struct, so a replacement is one assignment.

Tests: the validator becomes a check that the tests call after each
structural edit in the existing proptest trees.

#### C26: The script host's phases are types

`LuauHost` keeps `runtime: Rc<RefCell<Option<Runtime>>>`, `surface`,
`startup_surface`, and `finalized` (`script/mod.rs:395-431`). Each script
keeps `prepared` and `root` Options (`:214-216`). About eight sites check
the phase, each with its own message (`:651`, `:661`, `:743`,
`:780-788`, `:923`, `:963`, `:1053`). An invocation keeps
`handle: Option`, `origin_incarnation: Option`, and
`active: Option<ActiveEvalGuard>` (`invocation.rs:32-50`). Both production
callers bind the incarnation right after the start (`turn.rs:354`,
`mod.rs:1014`). Four places answer "is an eval running", each in its own
way (`turn.rs:328`, `:646`, `invocation.rs:163`, `mod.rs:960`).
`Driver.startup_attempted` (`turn.rs:301`) and `script_id: Option`, which
is unwrapped with `expect` (`scripting.rs:35-45`, `:326`), are the startup
phase.

Change:

- `enum HostPhase { Open, Staged(Surfaces), Ready(Surfaces, Runtime) }`.
- `enum ScriptLoad { Source, Prepared(PreparedGraph),
  Loaded(PreparedGraph, RootHandle) }`.
- `enum InvocationState { Pending { handle, guard }, Done { guard },
  Released }`, with the incarnation as a constructor argument.
- One `fn eval_admission(&self) -> Result<()>`.
- `enum Startup { Pending, Ran }`, and a `Finalized` struct that holds
  `script_id`, `api_text`, and `module_source`.

Tests: a transition table for `HostPhase`, and one admission test for each
busy cause.

#### C27: Small runtime machines

None of these has a reachable defect.

- Toasts keep `ended: HashSet<NoticeId>` beside the notices
  (`notice.rs:217`), kept in step at four sites: a private `dismissed` flag
  on each notice.
- `completion.failed` (`completion.rs:130`) is a side channel that only
  one caller takes (`canopy/mod.rs:530`): `drain_completion` returns
  `DrainError { source, post: Option<PostFailure> }`.
- `MotionClocks` (`style/animation.rs:115-120`) packs a ladder into three
  bools that one builder orders (`canopy/motion.rs:86-99`):
  `enum Rest { Moving, CursorResting, Idle, Unfocused }`.
- The crossterm event source (`backend/crossterm/input.rs:35-38`):
  `held: Option<Held>` with `enum Held { Wake, Error(Error) }`.
- `Slot` (`slot.rs:13-24`): `enum Held<T> { Empty, Waiting(T, u64),
  Taken(Arc<AtomicBool>) }`, plus a closed state.
- `OneShot` (`context.rs:269-299`): `enum { Ready(F), Done(R), Spent }`.
- The input source's `pending` and `deferred_error` are exclusive by
  construction: they become one `Option<Held>`, as above.

Tests: a transition table for `Rest`, which replaces the redundant checks
in `termbuf/motion.rs:71-74, 94`.

### Tools and options

#### C28: A detached terminal host survives a client (reachable)

`accept` (`crates/canopyctl/src/term.rs:95-129`) returns early with `?` on
any read or write error of a connection (`:103`, `:123`). The `TermHost`
then drops without `stop()` (`crates/canopy-term/src/lib.rs:323-340`).
*Reachable:* Ctrl-C on `canopyctl keys` while it waits for its reply ends
the whole detached app and leaves the app's socket file behind. A request
line that is not UTF-8 has the same result. `TermRequest`
(`term.rs:35-51`) admits `stop` with `keys`, which drops the keys
(`:107`), and `until` without `screen`, which implies `screen`
(`:67-70`).

A clean close in the middle of a line ends the read loop without an error,
so it is not a trigger. The triggers are a read error and a write error.
The loop also watches for the app's exit only between connections
(`:97-100`), so a client that holds a connection open hides the exit.

Change:

- One loop over `enum HostEvent { Request(TermRequest), ConnectionLost,
  AppExited }` that always ends in `host.stop()`. It watches the exit while
  it reads a connection, too. A connection error is logged, and the loop
  accepts the next connection.
- `enum TermRequest { Stop, Drive { keys, read: Read } }` with
  `enum Read { None, Now, Until(String) }`, inside the host. A detached
  host outlives the launcher that started it (`main.rs:571`), so a later
  canopyctl may talk to an older host. The JSON form stays as it is, and
  serde maps it to the enum.

Tests: drive `accept` over a socket. A line that is not UTF-8, and a client
that closes before the response is written, each leave the host serving the
next client. A client that holds an incomplete request while the app exits
does not keep the host alive. `Stop` removes both socket files.

#### C29: The MCP proxy's idle clock counts busy work (reachable)

`last_activity: Mutex<Instant>` (`crates/canopyctl/src/session.rs:167-174`)
is set when each operation starts (`:193`, `:209`, `:220`, `:232`,
`:251`). The watchdog (`main.rs:950-963`) compares it with
`idle_shutdown_after`. *Reachable:* with an idle limit below the build
time of `launch` (up to `LAUNCH_TIMEOUT`, 600 s, `main.rs:49`), the
watchdog's `disconnect()` queues on the state mutex during the launch, and
tears the session down as soon as the launch ends. The `keys` call that
follows fails with "no app is hosted".

Change:

- `enum Activity { Busy(NonZeroUsize), Idle(Instant) }`, updated by a guard
  that each MCP tool call holds for its whole length, from the handler's
  start to its result (`main.rs:359-367`, `:419`). A guard inside
  `SessionManager::session()` would end when it returns its lock, while the
  work goes on.
- The watchdog checks `Idle(since)` again after it takes the state lock, and
  releases only if it is still older than the limit. A new call that starts
  between the check and the lock keeps its session.
- The manager uses `tokio::time::Instant` in place of `std::time::Instant`,
  so the tests can pause the clock.

Tests: with tokio's paused time, a call that runs longer than the idle
limit is not released while it runs, and is released one limit after it
ends. A barrier holds the watchdog between its check and its lock while a
new call starts; the session survives. A cancelled call ends its guard.

#### C30: A canopyctl session's transport carries its data

`Session` keeps `child: Option<Child>`, `kind: SessionKind`, and
`default_fixture: Option<String>` (`session.rs:37-46`). This admits a live
session with a default fixture, and a headless session with no child, as
the tests build (`:339-344`, `:494`). Only `replay` calls `shutdown()`
(`main.rs:739`). The other paths drop the session and rely on the child
exiting when its stdin closes.

Change: `enum Transport { Headless { child: Option<Child>,
default_fixture: Option<String> }, Live }`, and `kill_on_drop(true)` on
the child. `None` is the in-process test peer (`session.rs:339-344`), which
runs the headless protocol without a process. A live session can no longer
hold a default fixture.

Tests: the fixture tests, built through the enum.

#### C31: Quick mode remembers a change on its level (reachable)

`PanelBody.quick` and `level_changed` (`crates/canopy-options/src/panel.rs:526-529`)
live on the body, but levels are a stack. *Reachable:* in quick mode,
toggle a member of a set level, press `S` to push the Save level, then
press Esc. `back()` (`:960-971`) pops Save, takes `level_changed`, and
closes the panel instead of returning to the set.

Change: `enum Close { Never, OnChange }` on the body, and `changed: bool`
on each `Level`. `back` reads the flag of the level that it pops.

Tests: the sequence above returns to the set level. Add a proptest of
panel keys that checks that quick mode closes only after a change on a
level that the user leaves.

#### C32: The rest of the options and the examples

Each bullet is its own commit.

- `EvalReport` (`crates/canopy-mcp/src/script.rs:153-178`) stores the
  outcome three times: `success`, `state`, and `error`. It is a public type
  that derives `Deserialize` and a schema, read in canopy-mcp, canopyctl,
  and canopy-term, but not in fh. The public type holds
  `outcome: Outcome { Completed(Option<JsonValue>), Failed(ScriptErrorInfo) }`,
  with `success()`, `state()`, `value()`, and `error()` as accessors, where
  `state()` derives from the error's type through the existing
  `From<ScriptErrorType>`. A private wire struct with today's fields does
  the serde and the schema, so the JSON stays the same. Test a round trip
  and the schema for a completed, a failed, a timed-out, and a cancelled
  eval, and for a completed eval whose value is null.
- The derive's `OptionAttrs` (`crates/canopy-options-derive/src/lib.rs:76-99`)
  holds `skip` and `flatten` as two bools, where neither means a leaf, and
  `skip` ignores the other attributes in silence (`:281-283`). Use
  `enum Kind { Skip, Flatten, Leaf }`. `skip` keeps `default`, which it
  applies (`:276`) and a test uses (`canopy-options/src/tests.rs:83`), and
  rejects the attributes that have no effect on a skipped field, as
  `flatten` does (`:287-301`). Add compile tests for `skip, default` and
  for a rejected pair.
- The todo's `accept_add` (`examples/todo/src/lib.rs:419-430`) does not
  check that its modal is open, so a script that calls it twice adds the
  item twice. Return early when `adder` is not an open modal.
- The Markdown gym's stream (`examples/gyms/src/markdown.rs:91-95`) keeps a
  counter, a bool, and display text: `enum Stream { Complete,
  Receiving(usize), Finished, Aborted }`, reset in `remount` and `load`.
- The gyms binary accepts `--headless --mcp X` and ignores `--mcp`
  (`examples/gyms/src/main.rs:156-172`, `:239-243`): add clap
  `conflicts_with`. canopyctl's hidden `host_socket` gets
  `requires = "pty"` (`crates/canopyctl/src/main.rs:97-118`).

### From the decisions

These changes settle the decisions of the first draft of this plan, as
discussed on 2026-11-07.

#### C33: Every due state hook runs

`Hooks::take_due` marks a generation seen before its hooks run
(`canopy/mod.rs:116-125`), and `run_state_hooks` stops at the first error
(`:616-628`). One failing hook skips the rest of the batch, across the
mode, binding, and notice hooks, and they never run for that generation.
The hooks are the status bar's key hints (`status_bar.rs:254`) and the
root's mode help and notice sync (`root.rs:469-470`).

Change: keep one run for each generation, but run every due hook and
return the first error. A failure does not cause a retry on the next frame.

Tests: two notice hooks, of which the first fails. The second still runs,
and neither runs again until the generation changes.

#### C34: The reader's interaction is one enum (reachable)

Every path that makes a text selection also enters Interact: the keyboard
at `surface.rs:603` and a mouse drag at `:1088`. Esc clears the selection
before it leaves Interact (`:629-641`). Only `set_mode(Scroll)`
(`widget/mod.rs:820-822`) calls `leave_interact` (`surface.rs:163-166`),
which drops the object cursor but keeps the selection. *Reachable:* make a
selection, then `set_mode(Scroll)`. The status shows Scroll, but `claims`
(`surface.rs:284-291`) still sends the arrows to the hidden selection.
`mode` has eleven writers (`surface.rs:164, 394, 443, 466, 502, 603, 681,
1053, 1088`, `widget/mod.rs:410, 539`). `pending_mark.mode` sets the mode
without the inspector check of `set_mode` (`surface.rs:443`,
`widget/mod.rs:811`). Search sets an object in Scroll on purpose
(`widget/mod.rs:2850`).

Change:

```rust
enum Interaction {
    Scroll { search_object: Option<u64> },
    Block { focus: Focus },
}
enum Focus {
    Unit,
    Object(u64),
    Selecting { range: ReadingRange, object: Option<u64> },
}
```

- A selection exists only in `Block`, so leaving the block cursor drops it.
- A selection can start from the object cursor and keep it
  (`surface.rs:603-618`). The first Esc clears the selection and a second
  clears the object (`:629-641`), and activation reads the object
  (`widget/mod.rs:1571`). So `Selecting` keeps the object it started from,
  and Esc moves back to `Object`.
- `marked` stays outside as the remembered unit.
- A press keeps its own `press: Option<ReadingPosition>`, because a press in
  Scroll starts a drag before it selects. `dragging` is `press.is_some()`.
  Leaving the block cursor ends the press and releases the capture, so a
  later Drag cannot make a new selection.
- `enter`, `leave`, `escape`, and `publish` are the only writers. `publish`
  applies `pending_mark` through the same checks as `set_mode`.
- `claims`, `has_item` (`widget/mod.rs:2178-2183`), `activate` (`:1571`),
  the copy fallback (`:2130`), and `selected_destination` (`:1810`) match
  on the enum.

Tests: a selection followed by `set_mode(Scroll)` leaves the arrows to the
viewport. An object, then a selection, then Esc, then activation opens the
object. `set_mode(Scroll)` during a captured drag, then a Drag, makes no
selection. Extend the `cursor_tests.rs` proptest `Action` with selections,
drags, and `set_mode(Scroll)`, with the invariant that Scroll holds no
selection.

#### C35: A closing modal reads closed

`close_modal_after_dispatch` (`completion.rs:219`) queues the close, and
the scope stays until the queue drains. Until then, `modal_is_open`
(`world/modal.rs:188-191`) reads true, and an `open_modal` on the same
subtree fails with "modal owner, subtree, and focus must form an admitted
route" (`modal.rs:296-312`). A queued close can also disappear: a failed
dispatch truncates the queue (`completion.rs:189`), a failed request
clears the rest of the drain (`:351`), and a tree-edit rollback truncates
it (`tree.rs:40`). Then the modal stays open.

Change:

- "Closing" is derived, not stored: a scope is closing while a
  `CloseModal(token)` request is queued. Each path that discards a request
  then gives back an open modal with no further code. A stored
  `ScopeState` would need an undo on each of those paths.
- `modal_is_open` is false for a closing scope.
- A separate `modal_retired(token)` is true only once the scope is gone.
  C36 uses it.
- `open_modal` on the subtree of a closing scope returns a new
  `Error::ModalClosing`. This is a public variant, so fh's error matches
  must be checked.

Tests: `close_modal` and then `modal_is_open` in the same dispatch reads
false. The modal reads open again after each discard path: a failed
dispatch, an earlier posted call that fails before the queued close, a
nested rollback, and a tree-edit rollback. Two closes of one token, and
the close of an ancestor modal. A reopen in the window gets
`ModalClosing`.

#### C36: A modal's owner hears that it closed

Closing a modal drains every scope above it (`world/modal.rs:372`). When
the options panel opens over another modal, such as the inspector, it
opens over the focused node and closes with that modal
(`canopy-options/src/widget.rs:262-271`). Nothing tells the panel's owner,
so `Options.token` goes stale, `panel_closed` (`store.rs:613-619`) never
runs, and followers do not restore the focus the panel held
(`follow.rs:87-88`). Any modal stacked on another has the same gap.

Change:

- `Context::wake_on_modal_close(token, NodeWakeHandle)`. The core wakes the
  handle once when the scope ends for any reason: a close, retirement with
  a lower modal, or removal of its owner (`retire_invalid_modals`,
  `modal.rs:413`). A context method, not a new `ModalOptions` field, leaves
  the 38 `ModalOptions` literals in canopy and fh unchanged.
- The core wakes the handle only when the scope's removal commits, after
  the drain, including a younger scope that retires with an older one. A
  close that a rollback discards wakes nothing.
- `Options` keeps `panel: PanelState { Closed, Open(ModalToken) }`. One
  `reconcile` method, which `poll`, `open`, and `close` call, moves `Open`
  to `Closed` when `modal_retired(token)` (C35) is true, and fires
  `panel_closed` exactly once. A closing panel is not retired yet.
- `is_open` stays a `&self` query: it reads `modal_is_open`, and changes
  nothing.

Tests: open the panel over the inspector, then close the inspector. The
follower restores focus in that turn, and `panel_closed` fires once. An
explicit close also fires once. A query during a close that a later
failure discards reads closed and then open, and fires nothing.

#### C37: Every store change waits for its round

Writes queue while subscribers run (`store.rs:588-600`), but `set_default`,
`revert`, `clear`, and `save` change the layers at once (`:654-774`) and
queue only their notification (`Deferred::Notify`, `:88-93`). *Reachable:*
a subscriber that calls `update(x = 1)` and then `revert(x)` ends with
`x = 1`, and a save in a round writes the file without the queued writes.
The comment in `a_write_can_read_the_store_and_a_callback_can_revert`
(`tests.rs:1054`) describes the behaviour that this change makes true.

Change:

- `enum Round { Idle, Notifying { queue: VecDeque<Deferred> } }` in place
  of `notifying` and `queue` (`store.rs:182-184`).
- `Deferred` becomes `Write`, `SetDefault`, `Revert`, `Clear`, and `Save`.
  One `apply` runs each in call order after the round, and notifies.
- `Backend::save` returns `Saved { Written(PathBuf), Deferred }`. A
  deferred save posts its outcome, or its error, as a notice when it runs.
  A call outside a round gets `Written`. A Rust subscriber can reach the
  `Options` widget through the context, so the commands can see `Deferred`
  too.
- `Backend::clear` returns `Cleared { Done(Option<PathBuf>), Deferred }`
  in the same way. A deferred clear posts its outcome, the file or "no file
  sets it", as a notice. The `options.clear` command returns the path or
  nil as now, and the string `"deferred"` for a deferred clear.

Tests: a subscriber's update and revert end reverted. A save in a round
writes the queued writes and posts a notice, and a save that fails in a
round posts the error. A deferred clear after a queued save. Two
subscribers in one round see the same store. The existing test asserts the
order, not just the final value.

#### C38: A list edit follows its item

`EditTarget::ListItem(Some(index))` (`panel.rs:431-436`, `:1052-1065`)
names the item by its position. A store write during the edit can make the
commit (`:1145-1160`) overwrite or delete another item.

Change: `EditTarget::ListItem(Option<ItemTarget { index, original }>)`.
The commit edits the item at `index` if it still equals `original`, or else
the only other item that equals it. If no item matches, or more than one
other item does, the commit changes nothing: a notice says that the item
changed while it was being edited, and the edit stays open with its text,
as it does for an integer that does not parse (`:1135-1140`).

Tests: a write that moves the item during the edit, a write that removes
it, and duplicates of the original for both a replacement and a deletion.
An unchanged item at the original index wins over a duplicate.

#### C39: An idle release remembers what it released

After an idle release, `session()` (`canopyctl/src/session.rs:250-260`)
starts a headless app without telling the agent, so an agent that drove a
live or hosted app runs its next scripts against an empty app.

`Current` (`session.rs:158-164`) keeps no record of where a session came
from: `connect_live` drops its socket after it connects, and `launch` drops
its command.

Change: each `Current` variant keeps its provenance from admission: the
socket of a live session, and the launch command of a hosted one. An idle
release moves it into `Current::Released { was: Released }`, with
`enum Released { Headless, Live(PathBuf), Hosted { command } }`. An
explicit `disconnect` (`:231`) leaves nothing, so the next call starts
headless, as now. The next call to `session()` after an idle release:

- after `Headless`, starts a headless app, as now;
- after `Live`, reconnects to the same socket, so the app keeps its state,
  and reports an error if the app is gone;
- after `Hosted`, reports an error that names the stopped app and the
  `launch` tool that restores it.

This uses the `Activity` guard of C29 to decide when to release.

Tests: with the factory peer, release a live session and call again: the
call reaches the same app. A live session whose socket is gone fails, and
keeps its provenance for a later retry. Release a hosted session: the call
fails with the error. After an explicit disconnect, the call starts
headless.

#### C40: A failed turn carries its exit

When dispatch fails, `turn_inner` returns before it takes
`exit_requested` (`canopy/turn.rs:589-592`). The request survives, and the
next turn's batch stops after its first event (`:449`).

Change:

- `Canopy::turn` returns `Result<TurnOutcome, TurnError>`, where
  `TurnError { error: Error, exit_code: Option<i32> }` implements
  `std::error::Error`, and `From<TurnError> for Error` keeps `?` working
  for every caller in canopy and fh.
- `turn_inner` takes the exit request on every path, including the early
  returns from polling and from frame preparation, so no request survives
  into the next turn.
- `?` converts. A caller that returns the turn's result directly needs an
  explicit conversion: `Harness::key` and `Harness::mouse`
  (`testing/harness.rs:176-190`) return `.turn(..).map(|_| ())`. The
  conversion keeps the inner error, and drops the code on purpose. List
  each such caller, and each match on the turn's error, in canopy and fh.
- The terminal runloop still reports the error. Headless hosts can read the
  code.

Tests: a command that requests an exit and then fails gives a `TurnError`
with its code. The same holds for a failure in a poll and in frame
preparation. The next turn dispatches its whole batch.

#### C41: A mode enters under a waiting menu

`enter_mode` refuses while a one-key menu waits
(`inputmap/mod.rs:1008-1014`), because the mode would bury the menu in the
one stack. This applies also while a framework modal suspends the menu
(`world/modal.rs:211-219`), when no key can reach it. With the separate
menu slot of C24, the reason goes away.

Change: a waiting menu no longer stops `enter_mode`. The mode joins
`modes`, and the menu stays the newest and still takes the next key. The
other rules stay:

- An empty name fails (`inputmap/mod.rs:20`).
- One name never names both a mode and a menu (`:1052`), so entering the
  waiting menu's own name fails.
- `leave_mode` of the menu's name fails.

Update the docs of `InputMap::enter_mode`, of the `Canopy` wrapper
(`canopy/mod.rs:582`), and of `docs/scripting.md`.

Tests: open a menu and enter a mode. The menu takes the next key, and then
the mode is active. Do the same while help suspends the menu. Entering the
menu's own name and an empty name fail. `clear_application` and
`restore_application` keep the menu newest. Help reports the modes in
order.

#### C42: A widget's mounted parts are one value

`SearchBar` (`search_bar.rs:218-240`), `Menu` (`menu.rs:272-274`),
`Picker` (`picker.rs:71-73`), `PickerList` (`:374-376`), `Confirm`
(`confirm.rs:179-185`), the `Options` widget (`widget.rs:36-42`),
`OptionsPanel.body` (`panel.rs:340`), the gym shell
(`examples/gyms/src/shell.rs:28-38`), and the image gym
(`imgview.rs:143-149`) keep what they build in `on_mount` as several
Options. Some move their callbacks out on mount, and the `Options` widget
refuses a second mount ("the options host mounts once"). If its
`add_modal` fails (`widget.rs:302`), it is left half-mounted and reports
"not mounted".

A second mount of one widget value can happen. A tree-edit snapshot shares
the widget cells (`tree.rs:26`), and a rollback runs `on_unmount` on the
widgets that the failed edit mounted (`tree.rs:283-301`). A widget that
existed before the edit stays, unmounted, and mounts again when it is next
attached.

`PanelBody.host` and `frame` (`panel.rs:521-523`) are not mount parts: the
panel sets `host` when it opens (`:554`), and the parent gives `frame`
after it builds the child (`:389`). They stay outside.

Change: `canopy::widget::Mount<P, M>`, with the variants `Pending(P)` and
`Mounted(P, M)`, in the runtime crate so that fh's widgets can use it too.

- `Mount::mount(&mut self, f: impl FnOnce(&mut P) -> Result<M>) ->
  Result<()>` keeps the pending value. A failure leaves `Pending`, so a
  retry can mount again.
- `Mount::unmount(&mut self)` drops the parts and returns to `Pending`.
  Each widget's `on_unmount` calls it, so a mount after a rollback starts
  from the pending value.
- Callbacks in `P` that a mount used to move out become shared (`Rc`) or
  rebuilt from `P`.
- `mounted()` and `mounted_mut()` return the parts, or an error that says
  "not mounted".
- Each widget above builds one parts struct and holds it in a `Mount`.

Tests: a transition table for `Mount`. A retained, detached composite
widget mounts, a later sibling's mount fails, and the widget attaches again
and works. A mount failure of the `Options` widget leaves it `Pending`.

## Order

These items share state or contracts, so they land in this order, or
together:

- C1 and C2 design the private insert session together. C3 and C4 then
  close the prompt and the drag on its transitions.
- C23 (the guards) and C25 (the node lifecycle) come before C42, which
  depends on how a rollback unmounts. C12 then uses C42.
- C35 and C36 land together, before C12 relies on them and before the
  todo fix in C32.
- C24 comes before C41.
- C7 adds `Scrollbar::is_dragging()` before C43 uses it.
- C29 and C39 land together.
- C15, C17, C20, and C34 share the surface's mark, the search restore, and
  the publication fields. Agree their transitions before any one of them
  changes those fields.
- C22's saturation of the poll deadline is a commit of its own, after the
  eval-slot fix.

## Left as they are

The searches also checked these, and found them explicit enough, or not
worth a change:

- The poller's two maps and the wake registry.
- `FrameState.emitted_buf`, where `None` means a full repaint.
- The parallel vectors of xmark's `Parsed` and `ParseCache`, which are
  built together.
- The upstream pulldown-cmark parser flags in xmark.
- `StyleManager`'s placeholder layer.
- `Animation.start` and `shown`: `AnimationStart` is public and `Copy`.
- `TurnOutcome.frame` and `motion` (`turn.rs:134-146`), which are public
  fields that the backend reads.
- `AvailableBinding` and `CommandAvailability` (`help.rs:69-76`,
  `commands.rs:932-934`). Their public fields flatten a sum type, but the
  help widgets and the Luau records read them, and one constructor keeps
  each consistent.
- `TableReader.reveal` and `BigText`'s roll, whose transitions are
  central.
- The pointer hover chain (`canopy/mod.rs:151`). `leave()` dispatches
  `Leave` to nodes behind a modal without `modal_admits`
  (`routing.rs:752-785`), while `route_input` checks it (`:378`). Add a
  comment there that says whether this is intended.
