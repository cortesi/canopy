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

Leaves that push their layer: `button`, `input`, `picker`, `confirm`, `help`, `dialog`,
`status_bar`, `selector`, `dropdown`, `editor`, and `diff_view`. Containers
that paint prefixed paths: `frame`, `tabs`, `columns`, and `root`.

The built-in themes are captured in `crates/canopy/src/core/style/themes.golden`.
Set `UPDATE_GOLDEN` when running the style tests to rewrite that capture from
the themes rather than editing it.

Input cursor styling applies to the painted grapheme before the central cursor
overlay; block cursors then exchange its foreground and background. Styling
preserves combining characters and wide-cell continuations. An absent cursor
rule falls back to the input text role.

## Themes and widget styles

`canopy::style::themes` holds the built-in themes: `default_dark`, `dracula`,
`gruvbox_dark`, `solarized_dark`, and `solarized_light`. Each returns a
`Palette` of role colours, such as `fg`, `accent`, `panel_bg`, and the named
`green` and `red`. `Palette::style_map` builds the rule set every built-in
theme shares.

Style application and widget-crate paths from the palette rather than from
colour constants:

```rust
setup.widget_styles(|palette, rules| {
    rules.fg("app/badge", palette.green).apply();
});
```

The rules apply over the theme at once, and again after every
`Setup::set_theme` or `Context::set_theme`, so a switch keeps them. Edits made
through `Canopy::style_mut` or a map installed with `Context::set_style` do not
survive a theme switch.

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

A `KeyHint` names an action, not a key. `KeyHint::for_command(call, label)` and
`KeyHint::for_intent(name, label)` resolve the key through binding discovery
when the hint mounts and whenever a binding or the mode stack changes, so a
rebind updates the hint. `Root::register` installs that resync; an app without
`Root` calls `KeyHint::register`. A hint whose action no key reaches draws
nothing.

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
with selection. The built-in themes define every frame path.

## Buttons

`Button` paints its border with `border`, its label with `text`, and the one
label character an accelerator names with `key`, all beneath the `button` layer
and its state layers. So `button/key` styles every accelerator, and
`button/focused/border` the border of a focused button. The built-in themes
give the accelerator the help overlay's key colour and leave the button on
whatever ground it sits on.

## Dialogs

`Dialog` centres a titled frame that fits its body over what it covers,
keeps a margin clear around it, and swallows clicks there. It pushes
`dialog`, so the frame resolves `dialog/frame` and a body's `background`
resolves `dialog/background`, whatever host holds the dialog. Buttons in a
dialog resolve `dialog/button/border`, `dialog/button/text`, and
`dialog/button/key` before the plain `button` paths. The built-in themes give
all of them the panel background, so every dialog reads as one surface.

`Confirm`, `Picker`, and the help overlay are dialogs. `Confirm` also pushes
`confirm` around its dialog, and its question paints `message`, which resolves
`confirm/message`. `Picker` pushes `picker` on its body, inside the dialog, so
its list and filter keep the `picker/*` paths while its frame takes the shared
dialog rules.

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
`dialog/button/border` still beats `button/border`, and a host rule such as
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
