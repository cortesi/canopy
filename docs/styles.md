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
| `roles::TEXT` | `text` | `button/focused/face/text` |
| `roles::BACKGROUND` | `background` | `input/focused/background` |
| `roles::BORDER` | `border` | `button/focused/border` |
| `roles::KEY` | `key` | `status_bar/key` |
| `roles::PROMPT` | `prompt` | `input/focused/prompt` |
| `roles::TITLE` | `title` | |
| `roles::THUMB` | `thumb` | |

Leaves that push their layer: `button`, `input`, `picker`, `confirm`, `help`, `dialog`,
`status_bar`, `search_bar`, `search_progress`, `selector`, `dropdown`, `editor`, `diff_view`,
`sparkline`, `meter`, `big_text`, and `column_chart`. Containers
that paint prefixed paths: `frame`, `tabs`, `columns`, and `root`.

The built-in themes are captured in `crates/canopy/src/core/style/themes.golden`.
Set `UPDATE_GOLDEN` when running the style tests to rewrite that capture from
the themes rather than editing it.

## Cursor roles

A cursor names what it means with a role, and the theme decides how the role
looks. A role resolves like a style path: `cursor/vi/insert` falls back to
`cursor/vi`, then to `cursor`. Each `CursorLook` has a shape, a color, and a
motion:

| Role | Shape | Color | Motion |
| --- | --- | --- | --- |
| `cursor` | block | `fg` | blink |
| `cursor/text` | block | `fg` | blink |
| `cursor/vi/insert` | block | `green` | blink |
| `cursor/vi/normal` | block | `blue` | steady |
| `cursor/vi/visual` | block | `magenta` | steady |
| `cursor/inactive` | block | `fg` mixed 35% into `bg` | steady |
| `cursor/terminal` | block | set by the child program | set by the child program |

Canopy draws only shapes that it can draw exactly on every cell. A block fills
the cell with the cursor color, and the grapheme takes the candidate color with
the highest WCAG contrast: the ground, the text color, black, or white. An
underline colors the grapheme and underlines it. A child terminal program's bar
becomes an underline.

Add looks for a theme with `Setup::cursor_looks`. The rules apply over the
built-in looks now and after every theme switch. `Canopy::set_cursor_look`
and Luau `canopy.set_cursor_look(role, look)` replace parts of a look in every
theme.

## Motion

`Paint::Animated` holds an `Animation`: colors at stops over one run, like the
stops of a gradient over space. `Repeat` sets what follows a run, `Easing` how
time maps to the stops, and `AnimationStart` when the first run starts.
`Easing::Hold` keeps each stop until the next one. `Animation::fade`,
`Animation::pulse`, and `Animation::blink` build the common cases.

`GradientSpec::with_drift` moves a gradient across its rectangle. A
`Drift::Slide` gradient slides across once in each period and wraps around. A
`Drift::Sweep` gradient swings to and fro, and slows at each edge. A sweep of
a gradient whose ends match shows a crest that runs back and forth, which suits
a busy indicator.

Repeating motion holds at rest while the operator is idle or the terminal lacks
focus. A busy indicator must move while the operator waits, so its animation or
gradient takes `with_pause(Pause::Never)`.

```rust
setup.widget_styles(|palette, rules| {
    rules
        .fg("status/busy", Animation::pulse(palette.accent, palette.muted_fg, Duration::from_secs(1)))
        .apply();
});
```

Frames show every animation at rest: the last stop of a single run, and the
first stop of a repeating one. Colors mix in OKLab by default, so equal steps
look equal. `Mix::Oklch` takes the shorter way around the hue circle, and
`Mix::Rgb` mixes the sRGB channels. `Color::mix` mixes two colors in any of
these spaces.

`effects::transition(effect, duration)` fades each color from its value to the
value of another effect. The fade starts with the first frame that shows it,
so keep the effect for as long as it applies. Modal dimming uses
`effects::modal_dim`, a transition to `effects::MODAL_DIM`.

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

A rule that puts one color on another can derive the pair from the palette and
a contrast target, so it reads in every theme. `Color::contrast_ratio` gives
the WCAG ratio of two colors. `Color::with_contrast(others, ratio)` moves a
color's OKLCH lightness away from `others`, just far enough that it reaches
`ratio` on each of them. It keeps the hue, and gives up chroma only where sRGB
cannot show it:

```rust
setup.widget_styles(|palette, rules| {
    // The badge keeps the accent's hue, dark enough that the text reads on it.
    let ground = palette.accent.with_contrast(&[palette.fg], 4.5);
    rules.fg("app/badge", palette.fg).bg("app/badge", ground).apply();
});
```

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
`Input::set_active` lights a field that a composite writes into while its
focus stays elsewhere; without focus the field draws its own caret.

A `SearchProgress` row under the results keeps the count in sight while the
results scroll. It pushes `search_progress` and paints `text`: a spinner while
the search runs, then what the search found. The built-in themes give the text
a quiet colour and no ground, so the row takes the ground of the pane that
holds it.

Custom lists can paint their selected row with `roles::selection(active)`.
It returns `selection` or `selection/dimmed`, both supplied by the built-in
themes. Usually `active` is `context.is_focused()`. A composite that routes
keys through an owner must instead use that owner's actual active part. Being
an ancestor of the focused field does not make the result list active.
Unselected rows keep their normal item styles.

`Picker` follows this pattern for its filter and list. Its list uses the
shared selection role, and its filter is an `Input` the list lights while it
takes filter text, so it resolves `picker/input/*` before the plain `input/*`
paths. Use the standard roles instead of copying a
second focus state into every row or resetting selection when focus moves.

A `Picker` item can show its label in styled runs through `ItemLabel::runs`,
such as a name in one colour and a badge in another. The list paints each run
with its role beneath the style of the row: `text/<role>`, `muted/<role>`, or
`selection/<role>`. A run takes its foreground from that path and the ground
of the row. A role without a rule takes the style of the row, so a muted or
selected row reads as one unless a rule such as `muted/badge` keeps a colour.
A label too wide for the list shows cut, as one run.

## Tabs

`Tabs` paints a one-row bar above its pages. The bar fill uses `tabs/bar`, each
label uses `tabs/tab`, and the active label uses `tabs/tab/active`. While focus
is within the tabs, the active label uses `tabs/tab/active/focused`, which falls
back to `tabs/tab/active`. The built-in themes define all four paths.

## Charts

The `chart` module paints stacked bars and columns to an eighth of a cell, and
the chart widgets build on it. A segment of a bar takes the foreground of its
style path. Where two segments meet inside a cell, the cell shows the first as
its foreground and the next as its background. A column that grows down swaps
the two, because it draws the top of a cell with the lower block of the rest of
the cell. Cells that no segment fills are blank on the ground: the background of
the empty path beneath the layers. A gradient on a segment's style spans the
whole bar, so its colour at a cell tells how far along the cell is.

- `Sparkline` pushes `sparkline`. Its bars paint `bar`, and a missing value
  paints `·` in `gap` among them. Its braille line paints `line`, and a missing
  value breaks the line.
- `Meter` pushes `meter`. A single value paints `fill`, the rest of the bar
  paints `track`, and the label paints `label`. Stacked values paint the style
  paths they name.
- `BigText` pushes `big_text`. A plain run paints `text`, and a run with its
  own path paints that path, so a value can dim its unit with a rule such as
  `big_text/unit`.
- `ColumnChart` pushes `column_chart`. The axis rule paints `axis`, the axis
  and gutter labels paint `label`, a reference line paints `reference`, and the
  cursor and hover marks paint `cursor`. Segments and markers paint the style
  paths they name. The cursor column mixes its colours toward the foreground of
  the empty path, and a muted column mixes them toward the ground, so both keep
  the two tones of each cell.

The built-in themes paint bars, lines, and fills in the accent, tracks in a
faint tone near the ground, gaps and labels quietly, and large text in the
foreground. A column chart draws its axis in the frame colour, its reference
lines faint, and its cursor in the accent.

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

`Button` draws one of four looks, chosen with `with_look`:

| Look | Rows | Shape |
| --- | --- | --- |
| `ButtonLook::Solid` | 1 | A filled row. The default. |
| `ButtonLook::Inset` | 3 | A filled row inside half-block edges, so the fill has rounded ends. |
| `ButtonLook::Bevel` | 3 | A filled block with a light hairline above and a dark one below. |
| `ButtonLook::Bordered` | 3 | A frame of the given box glyphs around the label. |

`ButtonLook::rows` gives the height of a look, and `Button::size` gives the
size of the whole button. The label keeps two columns of padding on each side.

The filled looks paint their parts under `face`: the fill with `face`, the
label with `face/text`, and the one label character an accelerator names with
`face/key`. Inset paints its edges with `face/edge`, whose foreground is the
face and whose background is the ground around the button. Bevel paints its
hairlines with `face/highlight` and `face/shadow`. The bordered look paints its
border with `border`, the inside with `fill`, its label with `text`, and the
accelerator with `key`.

All parts resolve beneath the `button` layer and one state layer: `disabled`,
then `active` while the button is pressed, then `focused`. A state layer hides
the plain paths beneath it, so a theme sets each part again under each state:
`button/focused/face/key` styles the accelerator of a focused filled button.
The built-in themes give the face a tint of the panel. The label and the
accelerator keep one color in every state, and the accelerator is bold. The
focused and pressed faces keep the accent's hue at the lightness that lets the
label and the accelerator read as well as the theme's text reads on its panel,
up to WCAG AA, and the pressed face moves further. Inset edges in a dialog take
the panel as their ground through `dialog/button/face/edge`.

## Dialogs

`Dialog` centres a titled frame that fits its body over what it covers,
keeps a margin clear around it, and swallows clicks there. It pushes
`dialog`, so the frame resolves `dialog/frame` and a body's `background`
resolves `dialog/background`, whatever host holds the dialog. Buttons in a
dialog resolve `dialog/button/...` before the plain `button` paths. The
built-in themes give a bordered answer the panel background, so the dialog
reads as one surface: an unfocused answer takes the border of an unfocused
frame, and the focused answer takes the accent. A filled answer keeps its
face, and its key keeps the key colour.

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

Button pushes `button`, then at most one state layer: `disabled` when its
configured command is disabled, else `active` while it is pressed, else
`focused` when focus is within it. For example, `button/disabled/face/key`
targets the accelerator of a disabled filled button. Active state does not
imply command eligibility or selection. Input pushes `input`, then `focused` when it holds focus. Custom
widgets can use `selected` independently of focus.

The resolver searches paint-path prefixes from longest to shortest. For each
prefix, it tries the whole layer stack, then the stack with its outer layers
dropped one at a time, then the stack with its inner layers dropped, and
finally no layers. Each style component uses its first matching rule. So a
widget's own rules apply wherever it is mounted: an editor under a host's
layer still resolves `editor/text`. A context rule such as
`dialog/button/border` still beats `button/border`, and a host rule such as
`file_select/selection` still fills what an inner layer leaves unset.

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
