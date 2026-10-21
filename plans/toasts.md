# Toasts for every notice

## Description

Canopy shows a notice in one of two ways, by its level:

- An info or warning notice is a toast (`canopy-widgets/src/toast.rs`). A
  toast is a one-row box at the bottom right, as wide as its text and cut
  at the end. It ends after 2 s.
- An error notice is never a toast (`NoticeLevel::is_toast`,
  `core/notice.rs`). `Root`'s `NoticeBar` draws it as a red row across the
  bottom of the window (`canopy-widgets/src/root.rs`). The row covers the
  app's footer, shows only the first line, and stays until the next key,
  click, or paste.

Every failure becomes an error notice: a command or a script that fails in
a binding, a widget handler, a poll, and a posted call. Thus, a refusal
such as fh's "no vault here" covers the footer until the next key. A
success such as "saved" leaves after 2 s.

This plan shows each notice as a toast, with a style for its level. It
also moves the errors that apps draw elsewhere into the same toasts. The
notice log (`notice_log.rs`) keeps every notice, as now.

## Changes

### C1: Errors are toasts

- `NoticeLevel::is_toast` goes, because every level is a toast. The log
  keeps its live set, its limit of 4 toasts (`TOAST_LIMIT`), and its
  repeats. A post that matches the newest entry joins it, counts `×N`, and
  starts its timer again.
- Each level gets a default duration: info 2 s, warning 4 s, error 6 s.
  `NoticePost::with_duration` sets another duration for each level, also
  for an error.
- `NoticeBar`, `NoticeLog::shown_error`, `dismiss_error`, and the dismissal
  on input (`dismiss_notice` in `routing.rs`) go. A click on a toast still
  ends it, and an error ends on its timer.
- The style `root/notice` goes, and `root/toast/error` takes its place.

### C2: Card toasts

Each toast is a card in the stack at the bottom right, the newest lowest,
as the stack is now:

```text
                              ▌ no vault here

                              ▌ saved to ~/.fh/options.toml

                              ▌ the git worker could not
                              ▌ start: permission denied
```

- The card has a ground of its own and an edge at its left in the color
  of its level. The edge is a cell with a background color, not a block
  glyph, because fonts draw some block glyphs wide. A blank row parts the
  cards.
- The text wraps at word boundaries, at a width of at most 48 columns, on
  at most 3 lines. A word wider than a line breaks across lines. The last
  line ends with `…` when the text does not fit, and the `×N` count stays
  whole.
- The card is as wide as its longest line, so a short notice gives a
  narrow card.
- Styles: `root/toast/info`, `root/toast/warning`, `root/toast/error`, and
  `root/toast/count`, with an `edge` style for each level, such as
  `root/toast/error/edge`. The default themes give info the accent, warning
  yellow, and error red.
- `Root::set_toast_floor` keeps its role: the stack rests above a band of
  the app's, such as an editor's command line.

### C3: Apps post their errors as notices

Each error that an app draws in a status row of its own moves to a
notice. These change:

- **The Markdown reader** (`canopy-markdown/src/widget/mod.rs`): a failed
  action ("Failed: …"), a copy error, and a worker failure post an error
  notice. The footer items `markdown.action` and `markdown.failure` keep
  only progress, such as "Copying".
- **The Markdown inspector** (`canopy-markdown/src/inspector.rs`): "Search
  failed" and "Preparation failed" post error notices, and "Copied" and
  "Search canceled" post info notices. The inspector's status row keeps
  live counts.

These stay where they are, because they describe what a widget shows now,
not an event:

- the search bar's match count and "invalid regex"
- the find field's status, and its red field for an invalid query
- footer state, such as loading, unreadable counts, and the pane labels
- empty states, such as `<no changes>` and `<empty>`
- a view that cannot read its file, which shows the error in the view
- a terminal's exit status in its pane

### C4: Documents and tests

- `docs/architecture.md`, `docs/scripting.md`, and `docs/styles.md`
  describe one toast for each notice, the durations, and the styles. The
  doc comments that say that an error shows until the next input change:
  `core/notice.rs`, `core/context.rs`, and `script/base_api.rs`.
- The Luau `Notice.live` field keeps its meaning: a notice that shows now.
- Canopy tests change from the error row to toasts:
  `root_shows_the_newest_notice_until_the_next_input` (`root.rs`),
  `an_error_shows_until_dismissed_and_a_toast_until_its_time`
  (`notice.rs`), and the dismissal tests in `canopy/tests.rs`. New tests
  cover the error toast and its timer, wrapping, the `…` cut, and the
  edge styles.
- fh: the `notice()` and `toast()` helpers in
  `commander/tests_support.rs` split notices by level, not by
  `is_toast()`, so the tests that call them keep their meaning. The
  gallery shows an error toast.

## Execution Plan

### Stage 1: Errors are toasts

- [x] C1: Remove `is_toast`, the error row, and the dismissal on input.
  Add the durations for each level, and `root/toast/error`. A script's
  `canopy.notify` with a level takes that level's duration.
- [x] Update the Canopy tests of the error row and of dismissal, and the
  doc comments. Run the Canopy tests and `cargo xtask smoke`.

### Stage 2: Card toasts

- [x] C2: the edge, the ground, wrapping on at most 3 lines, the `…` cut,
  and the styles in the default themes.
- [x] Layout and style tests in `toast.rs`. Run `cargo xtask gallery` in
  Canopy and in fh, and review the toasts.

### Stage 3: Apps

- [x] C3: the Markdown reader's and the inspector's errors as notices.
  A command's `Error::Invalid` reports its message without the "invalid"
  label.
- [x] fh: the `notice()` and `toast()` helpers, and the tests and smoke
  suites that read notices.

### Stage 4: Documents

- [x] C4: `docs/architecture.md`, `docs/scripting.md`, `docs/styles.md`.
- [x] Run `ncode tidy`, `ncode test`, the smoke suites, and the galleries
  in both repositories. Run `ncode api` in both, and review the diffs.
