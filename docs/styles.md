# Stock widget styles

One rule names every stock widget's styles:

- A widget's style prefix is its node name.
- Leaves, composites, and surfaces push that name as a layer and paint bare
  part roles beneath it.
- Containers around foreign content push nothing and paint `<name>/<part>`,
  so the content keeps its own resolution.
- A widget never does both.
- Cursor rows paint `roles::selection(active)`.

`canopy::style::roles` holds the shared part names:

| Role constant | Paint path | Example rule |
| --- | --- | --- |
| `roles::TEXT` | `text` | `button/active/text` |
| `roles::BACKGROUND` | `background` | `input/focused/background` |
| `roles::BORDER` | `border` | `button/focused/border` |
| `roles::KEY` | `key` | `status_bar/key` |
| `roles::PROMPT` | `prompt` | `input/focused/prompt` |
| `roles::CURSOR` | `text/cursor` | `input/focused/text/cursor` |
| `roles::TITLE` | `title` | |
| `roles::THUMB` | `thumb` | |

Leaves that push their layer: `button`, `input`, `picker`, `confirm`, `help`,
`status_bar`, `selector`, `dropdown`, `editor`, and `diff_view`. Containers
that paint prefixed paths: `frame`, `tabs`, `columns`, and `root`.

The built-in themes are captured in `crates/canopy/src/core/style/themes.golden`.
Set `UPDATE_GOLDEN` when running the style tests to rewrite that capture from
the themes rather than editing it.

Input cursor styling applies to the painted grapheme before the central cursor
overlay; block cursors then exchange its foreground and background. Styling
preserves combining characters and wide-cell continuations. An absent cursor
rule falls back to the input text role.

## Fields and results

A text field beside a result list must show which part takes the keyboard.
Retain the query and selected row when focus moves between them:

- The active field paints its whole row, including empty space. Its prompt
  uses the accent and bold text, and its caret marks where typing lands.
- The inactive field keeps its text visible with subdued colors and no caret.
- The active list gives its selected row the accent background. An inactive
  list retains that row with a subdued background.

`Input` applies the field treatment automatically from its actual focus. All
built-in themes supply both states. Use `Input::new("").with_prompt(" Glob: ")`
to add a visible, noneditable prompt. The prompt participates in measurement
and cursor placement but does not change the value or semantic label.

Custom lists can paint their selected row with `roles::selection(active)`.
It returns `selection` or `selection/dimmed`, both supplied by the built-in
themes. Usually `active` is `context.is_focused()`. A composite that routes
keys through an owner must instead use that owner's actual active part. Being
an ancestor of the focused field does not make the result list active.
Unselected rows keep their normal item styles.

`Picker` follows this pattern for its filter and list. Its list uses the
shared selection role; its filter retains the `picker/filter/active` style
paths. Hosts can override these paths and `input/focused/*` together for a
consistent application palette. Use the standard roles instead of copying a
second focus state into every row or resetting selection when focus moves.

## Tabs

`Tabs` paints a one-row bar above its pages. The bar fill uses `tabs/bar`, each
label uses `tabs/tab`, and the active label uses `tabs/tab/active`. While focus
is within the tabs, the active label uses `tabs/tab/active/focused`, which falls
back to `tabs/tab/active`. The built-in themes define all four paths.

## Status bars

`StatusBar` is a container for one row. It pushes `status_bar`, fills its
`background` on the bar's ground, keeps the widgets added with
`StatusBar::with_left` at the start, and pins the widgets added with
`with_right` to the end. Everything in the bar resolves beneath that layer, so
a plain `Text` resolves `status_bar/text` and inherits the `status_bar` ground,
and `KeyHint` paints `key` and `text` for its two parts. The built-in themes
give the bar the panel ground, its text a quiet label, and hint keys the
accent.

## Selectors and dropdowns

`Selector` and `Dropdown` push their node names. The cursor row paints
`roles::selection(active)`, the chosen item paints `chosen`, and other items
paint `text`. The built-in themes define `selector/chosen` and
`dropdown/chosen`; the cursor takes the shared selection colours.

## Diffs

`DiffView` pushes `diff_view` and paints `context`, `added`, `removed`,
`header`, `gap`, `missing`, and `separator`. The built-in themes colour added
rows green, removed rows red, headers with the accent, and gaps muted.

## Notices

`Root` shows the newest notice in one row at the bottom of the main pane,
painted with `root/notice`. The built-in themes give it the red role on the
panel ground, so a failure the application survived reads as an error without
hiding what the row covers for long: the row goes at the next input.

## Frames

`Frame` draws its border with `frame`, or `frame/focused` while focus is within
the frame, and its title with `frame/title`. Scroll thumbs on the right and
bottom borders use `frame/thumb`. While a drag holds a thumb, the thumb uses
`frame/thumb/active`, which falls back to `frame/thumb`. The built-in themes
tint the thumb from the theme's base toward its accent, and give a held thumb
the full accent, so the position carries a hint of colour without competing
with selection. The built-in themes define every frame path. The help overlay
pushes a `help` layer, so its frame also uses the `help/frame` paths.

## Buttons

`Button` paints its border with `border`, its label with `text`, and the one
label character an accelerator names with `key`, all beneath the `button` layer
and its state layers. So `button/key` styles every accelerator, and
`button/focused/border` the border of a focused button. The built-in themes
give the accelerator the help overlay's key colour and leave the button on
whatever ground it sits on.

## Confirm

`Confirm` pushes a `confirm` layer, so the frame around it resolves the
`confirm/frame` paths and shares the dialog's background rather than sitting on
the view behind it. The question paints `message`, which resolves
`confirm/message`. Its answers are
ordinary buttons, so they resolve `confirm/button/border`,
`confirm/button/text`, and `confirm/button/key` before the plain `button` paths.
The built-in themes define all of them, taking the panel background so the
dialog reads as one surface.

## Columns

`Columns` draws the divider after each pane with `columns/divider`. Where a
pane's scrolling node reaches the divider, the divider beside its visible rows
is a track: the thumb uses `columns/thumb`, and `columns/thumb/active` while a
drag holds it. Thumbs follow the same tint-toward-accent rule as frame thumbs.
The trailing column after the last pane shows only a track. The built-in themes
define all three paths.

## States and fallback

`canopy::style::WidgetState::layer()` maps states to ordinary layer strings:

| State | Layer |
| --- | --- |
| `Focused` | `focused` |
| `Selected` | `selected` |
| `Disabled` | `disabled` |
| `Pressed` | `active` |

Button pushes `button`, then `active` when active, then `focused` when focus is
within the button, then `disabled` when its configured command is disabled.
For example, `button/active/focused/disabled/text` targets a disabled active
button label with focus, and `button/disabled/key` a disabled accelerator. Active state does not imply command eligibility or
selection. Input pushes `input`, then `focused` when it holds focus. Custom
widgets can use `selected` independently of focus.

The resolver searches paint-path prefixes from longest to shortest. For each
prefix, it tries the whole layer stack, then the stack with its outer layers
dropped one at a time, then the stack with its inner layers dropped, and
finally no layers. Each style component uses its first matching rule. So a
widget's own rules apply wherever it is mounted: an editor under a host's
layer still resolves `editor/text`. A context rule such as
`confirm/button/border` still beats `button/border`, and a host rule such as
`file_select/selection` still fills what an inner layer leaves unset. Existing
`button/active/text` rules remain fallbacks when focused or disabled rules are
absent.

## Semantic values

Input, Button, and List publish semantic roles in frame snapshots. Input and
List support `with_label`; Button uses its displayed label. List publishes the
domain key under its cursor as `selected_keys` and its checked keys as
`checked_keys`, and configured actions publish their command status. Selector
publishes its chosen label as its value.

Input values are omitted by default. Use
`Input::with_value_exposure(ValueExposure::Public)` to publish a value.
`ValueExposure::Omit` and `ValueExposure::Sensitive` always omit it. This policy
controls semantic values; it does not mask text painted on screen. Snapshot
readers receive the published data without running widget hooks again.
