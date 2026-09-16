# Stock widget styles

Stock widgets use named paint roles and the existing hierarchical style resolver.
These names describe the painted part, independently of decorative child node
names. Extra containers that do not push style layers do not change resolution.
Custom string layers remain supported.

| Component layer | Role constant | Paint path | Example rule |
| --- | --- | --- | --- |
| `button` | `roles::BUTTON_LABEL` | `text` | `button/active/text` |
| `button` | `roles::BUTTON_BORDER` | `border` | `button/active/border` |
| `button` | `roles::BUTTON_KEY` | `key` | `button/disabled/key` |
| `input` | `roles::INPUT_TEXT` | `text` | `input/text` |
| `input` | `roles::INPUT_CURSOR` | `text/cursor` | `input/focused/text/cursor` |

The built-in themes are captured in `crates/canopy/src/core/style/themes.golden`.
Set `UPDATE_GOLDEN` when running the style tests to rewrite that capture from
the themes rather than editing it.

The constants are available through `canopy::style::roles`. Button labels and
borders retain their existing paths. Input cursor styling applies to the painted
grapheme before the central cursor overlay; block cursors then exchange its
foreground and background. Styling preserves combining characters and wide-cell
continuations. An absent cursor rule falls back to the input text role.

## Tabs

`Tabs` paints a one-row bar above its pages. The bar fill uses `tabs/bar`, each
label uses `tabs/tab`, and the active label uses `tabs/tab/active`. While focus
is within the tabs, the active label uses `tabs/tab/active/focused`, which falls
back to `tabs/tab/active`. The built-in themes define all four paths.

## Frames

`Frame` draws its border with `frame`, or `frame/focused` while focus is within
the frame, and its title with `frame/title`. Scroll thumbs on the right and
bottom borders use `frame/thumb`. While a drag holds a thumb, the thumb uses
`frame/thumb/active`, which falls back to `frame/thumb`. The built-in themes
define every frame path. The help overlay pushes a `help` layer, so its frame
also uses the `help/frame` paths.

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
the view behind it. The question uses `confirm/message`. Its answers are
ordinary buttons, so they resolve `confirm/button/border`,
`confirm/button/text`, and `confirm/button/key` before the plain `button` paths.
The built-in themes define all of them, taking the panel background so the
dialog reads as one surface.

## Columns

`Columns` draws the divider after each pane with `columns/divider`. Where a
pane's scrolling node reaches the divider, the divider beside its visible rows
is a track: the thumb uses `columns/thumb`, and `columns/thumb/active` while a
drag holds it. The trailing column after the last pane shows only a track. The
built-in themes define all three paths.

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
prefix, it searches layer prefixes from deepest to root. Each style component
uses its first matching rule. Thus existing `button/active/text` rules remain
fallbacks when focused or disabled rules are absent. Decorative wrappers that
push custom layers participate in this same documented resolution order.

## Semantic values

Input, Button, and List publish semantic roles in frame snapshots. Input and
List support `with_label`; Button uses its displayed label. List publishes its
selected domain keys, and configured actions publish their command status.

Input values are omitted by default. Use
`Input::with_value_exposure(ValueExposure::Public)` to publish a value.
`ValueExposure::Omit` and `ValueExposure::Sensitive` always omit it. This policy
controls semantic values; it does not mask text painted on screen. Snapshot
readers receive the published data without running widget hooks again.
