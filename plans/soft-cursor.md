# Soft Cursor and Motion

## Description

Canopy draws its own cursor. The terminal cursor stays hidden for the whole session, and after
Canopy renders a frame it overlays the cursor of the focused widget onto one cell of the frame. This
soft cursor is worth keeping:

- Snapshots, screen dumps, replays, and headless evaluation see the cursor, because it is part of
  the frame. A terminal cursor is invisible to all of them.
- Style effects, such as the dimming behind a modal, apply to the cursor cell like any other cell.
- A frame can hold more than one cursor. The terminal has exactly one, and it cannot show an
  embedded terminal pane beside an editor, or a field that is active without focus.
- Canopy owns no terminal cursor state, so it has nothing to restore after a crash, and it depends
  on no terminal's support for cursor shape escape codes.

The soft cursor has three problems today:

1. **The shapes do not work.** `CursorShape::Line` and `CursorShape::Block` both swap the foreground
   and background of the cell (`core/termbuf/mod.rs`, `TermBuf::overlay_cursor`), so a vi editor in
   insert mode shows the same block as in normal mode. `Underscore` sets the underline attribute. A
   bar cannot be drawn in a character grid: a cell holds one grapheme, and no glyph shows both the
   character and a bar beside it. A bar is exact only on an empty cell.
2. **The cursor does not move in time.** Canopy has no blink, and no way to change the cursor without
   a new frame.
3. **The cursor has no meaning of its own.** A widget chooses a geometric shape, and the cursor takes
   its colors from the cell under it. A theme cannot color it, automation cannot read it, and the
   `Input` widget paints a `▏` caret glyph by hand when it is active without focus.

Blink is one case of a wider gap. Canopy interpolates color over space (`Paint::Gradient` in
`core/style/mod.rs`) but not over time, and every animation today pays for a whole frame:

- The font gym (`examples/gyms/src/fontgym.rs`) moves a banner gradient by polling and replacing the
  whole style map with `Context::set_style` on each tick. Each tick runs layout and renders every
  node, because every mutable callback invalidates layout, and a theme change discards the edit.
- `Spinner` users, the diff view and fh's find, poll and repaint the whole application every 80 ms
  while they are busy.
- A modal dims what it covers in one step (`effects::brightness(MODAL_DIM)`), with no fade.
- An application has no way to pulse an activity indicator, or to flash a row that changed, without
  a poll and a full frame per step.

This plan gives color a time axis, and builds the cursor on it. It has two parts:

- **Part A, motion.** A paint can change over time. Canopy renders and publishes frames at rest,
  so snapshots stay deterministic, and it evaluates motion only when it emits cells to the terminal.
  An emission for motion alone writes only the moving cells, without layout or rendering. Colors mix
  in the perceptual OKLab space.
- **Part B, cursor roles.** A widget names what its cursor means, such as `cursor/vi/insert`, and the
  theme decides how the role looks: a shape that Canopy can draw on every cell, a hue, and a motion.
  In the vi editor, insert mode is a blinking green block, normal mode a steady blue block, and visual
  mode a steady magenta block. These match the mode colors that applications already use for mode
  labels, such as the `INSERT` and `NORMAL` labels of the Verber prompt.

Motion covers color only. A change of glyphs or of geometry, such as a spinner frame or a size,
stays with polls and rendering. Glyph cycles are later work.

There are no compatibility constraints. The plan removes `Widget::cursor`, `CursorShape::Line`, the
`text/cursor` style role, the `Input` caret glyph, and `Color::blend`, and migrates every caller,
fh included, in the same change.

## Changes

### Part A: Motion

#### C1: Perceptual color mixing

`Color::blend` mixes RGB channels, so a mix of green and blue passes through a dull teal, and equal
steps do not look equal.

- `Color::mix(self, other, t, space)` replaces `Color::blend`, with `Mix::Rgb`, `Mix::Oklab`, and
  `Mix::Oklch`. `Oklch` takes the shorter way around the hue circle, for hue sweeps.
- `Color` gains conversions to and from OKLab, and `relative_luminance` and `contrast_ratio` (C7).
- `GradientSpec` gains a `mix: Mix` field, with `Mix::Oklab` as the default. Themes and the font
  banner change color slightly, which is acceptable.

#### C2: Animated paints

`Paint` gains a variant that changes over time, the way `Paint::Gradient` changes over space:

```rust
pub enum Paint {
    Solid(Color),
    Gradient(GradientSpec),
    Animated(Arc<Animation>),
}

pub struct Animation {
    /// Colors over one run, as gradient stops are colors over space.
    pub stops: Vec<GradientStop>,
    /// Length of one run.
    pub duration: Duration,
    /// What follows one run.
    pub repeat: Repeat,
    /// How time maps to the stops.
    pub easing: Easing,
    /// When the first run starts.
    pub start: AnimationStart,
    /// The space that mixes colors between stops.
    pub mix: Mix,
}

pub enum Repeat { Once, Loop, Alternate }
pub enum Easing { Linear, InOut, Hold }
pub enum AnimationStart {
    /// The shared motion epoch, which keeps loops in step with each other.
    Epoch,
    /// A fixed time, for one-shot changes such as a fade.
    At(Instant),
    /// The last input event, so the motion restarts whenever the operator acts.
    LastInput,
    /// The first published frame that shows the animation. Clones share the
    /// bound time, so a paint that renders again keeps its start.
    Shown,
}
```

- `Easing::Hold` keeps each stop until the next one, so a blink is a two-stop `Hold` loop.
- Widgets and effects have no clock. `AnimationStart::Shown` binds the start to the time of the
  first frame that renders the animation, which gives a one-shot change a start without one.
- Constructors cover the common cases: `Animation::fade(from, to, duration)`,
  `Animation::pulse(from, to, period)`, and `Animation::blink(on, off, on_time, off_time)`.
- `GradientSpec` gains `drift: Option<Duration>`: the gradient slides across its rectangle once in
  each period, and wraps. The font gym uses it in place of per-tick style maps.
- An animation has a **rest** color: the last stop of a `Once` animation, and the first stop of a
  repeating one. A drifting gradient rests at offset zero.
- `Paint::map_colors` maps the stops of an animation, so style effects apply to animated paints.

#### C3: Motion at emission

Motion belongs to emission, the last step of the frame pipeline (see "Rendering" in
`docs/architecture.md`). Preparing and publishing never move.

- Rendering writes the rest color of every animated or drifting paint into the `TermBuf`. The
  `TermBuf` also keeps a motion record for each such grapheme: its base cell, its span, its
  foreground and background paints, and the rectangle and point that resolve a gradient.
- Every write to a cell replaces or clears the motion record of the grapheme it overwrites. A later
  layer, such as a modal, an overlapping sibling, or a cursor, thus owns its cells, and the motion
  below it stops.
- Motion evaluates each record at its base point and writes the result to every cell of its span,
  so a wide grapheme keeps the canonical form that `TermBuf` requires: a continuation cell has the
  style of its base cell.
- Snapshots, screen text, cell styles, and replays show every cell at rest, so tests and automation
  do not depend on time.
- A `Motion` scheduler on the driver evaluates the recorded cells on the driver clock, so
  `testing::ManualClock` controls it. `Canopy::next_deadline` includes its next change: the next stop
  edge of a `Hold` animation, and the next sample of a continuous one. Samples come at most
  `max_fps` times a second, 30 by default, and a sample that changes no 8-bit channel emits nothing.
- A turn advances the scheduler. When a moving cell changes, `TurnOutcome` reports it in a new
  `motion: bool` field, and the terminal adapter emits although no frame was published.
- `emit_frame` writes the current colors of the moving cells into a copy of the published buffer,
  then diffs that against the last emitted buffer. The composed buffer becomes the last emitted
  buffer, and the published buffer stays at rest. A diff and a full repaint still give the same
  screen.
- `MotionSettings { enabled, max_fps, idle_pause }` sets motion globally, with `Setup::set_motion`,
  `Canopy::set_motion`, and `canopy.set_motion({ enabled = false, max_fps = 30, idle_ms = 10000 })`
  in Luau. With
  `enabled = false`, every cell stays at rest, for operators who want no motion. After `idle_pause`
  without input, 10 s by default, loops pause at rest and the scheduler sets no deadline, so an idle
  application wakes for nothing. One-shot animations always finish.
- Headless evaluation and the test harness keep motion off unless a test turns it on. The terminal
  adapter turns it on.

#### C4: Motion sources

- **Style rules.** A style path takes an animated paint like any other paint:
  `rules.fg("status/busy", Paint::Animated(Animation::pulse(...)))`. Every widget that paints the
  path moves, with no widget code.
- **Effect transitions.** `effects::transition(effect, duration)` wraps an effect so that each color
  fades from its value to the value of `effect` over `duration`. It starts at `Shown`, the first
  frame that renders it. Modal dimming uses a 120 ms transition of `effects::brightness(MODAL_DIM)`, so a modal
  fades the view behind it in. At rest, the transition is the dimmed color.
- **Cursor looks.** A cursor look maps its motion to an animation (C9).

### Part B: Cursor roles

#### C5: Cursor roles and looks

A cursor role is a style path below `cursor`, such as `cursor/text` or `cursor/vi/insert`. A role
resolves to a `CursorLook`, with the same fallback that style paths use: `cursor/vi/insert` falls
back to `cursor/vi`, then to `cursor`.

```rust
pub enum CursorShape {
    /// The whole cell takes the cursor color, and the grapheme takes a
    /// contrasting color.
    Block,
    /// The grapheme and its underline take the cursor color.
    Underline,
}

pub enum CursorMotion {
    /// Always shown.
    Steady,
    /// Shown for `on`, then the cell below for `off`.
    Blink { on: Duration, off: Duration },
    /// Fades between the cursor color and the cell below over one period.
    Pulse { period: Duration },
}

pub struct CursorLook {
    pub shape: CursorShape,
    pub color: Color,
    pub motion: CursorMotion,
}
```

- `Setup::cursor_looks(|palette, rules| ...)` installs looks for roles, the way
  `Setup::widget_styles` installs styles. Canopy reapplies the rules on every theme change.
- The built-in themes give each role a look:

  | Role | Shape | Color | Motion |
  | --- | --- | --- | --- |
  | `cursor` | Block | `fg` | Blink |
  | `cursor/text` | Block | `fg` | Blink |
  | `cursor/vi/insert` | Block | `green` | Blink |
  | `cursor/vi/normal` | Block | `blue` | Steady |
  | `cursor/vi/visual` | Block | `magenta` | Steady |
  | `cursor/inactive` | Block | `fg` mixed 35% into `bg` | Steady |
  | `cursor/terminal` | Block | from the child program | from the child program |

- Luau configuration overrides a look with
  `canopy.set_cursor_look(role, { shape = "block", color = "#98c379", motion = "blink" })`.

#### C6: Cursor declarations during render

`Render::cursor(location, request)` replaces `Widget::cursor`. A widget declares a cursor while it
renders, in the same local coordinates as its other drawing, and only when it wants a cursor shown:

```rust
pub struct CursorRequest {
    /// Role of the cursor.
    pub role: Cow<'static, str>,
    /// A color that replaces the color of the role, such as the cursor
    /// color that a child terminal program sets.
    pub color: Option<Color>,
    /// A motion that replaces the motion of the role.
    pub motion: Option<CursorMotion>,
    /// A shape that replaces the shape of the role.
    pub shape: Option<CursorShape>,
}
```

- The runtime clips each declaration to the visible view of its node and translates it to the
  screen, as it does for text. A clipped cursor is not drawn.
- One node holds at most one cursor. A later declaration in the same render replaces an earlier one.
- The **primary** cursor is the declaration of the deepest node on the focus path that declared one.
  It takes the motion of its look. Every other declaration is a **secondary** cursor, which is always
  steady.
- A widget decides when it shows a cursor. The `Editor` declares one only while it has focus.
  `Input` declares `cursor/text` while it has focus, and `cursor/inactive` while it is active
  without focus, which replaces its `▏` caret glyph.

#### C7: Painting a look

`TermBuf` paints each cursor over the whole grapheme span at its location, with the same span rules
as today's overlay.

- **Block**: the background of each cell takes the cursor color. The foreground takes the color that
  contrasts most with the cursor color: the background of the cell below, its foreground, black, or
  white, ranked by the WCAG contrast ratio. An empty cell becomes a solid cell of the cursor color.
- **Underline**: the grapheme takes the cursor color and the underline attribute. An empty cell
  shows an underline in the cursor color.

The style effects of a node apply to its cursor, so a dimmed region dims its cursor too.

#### C8: Frames record their cursors

- `FrameSnapshot` gains `cursors: Vec<CursorSnapshot>`. Each entry holds the node, the screen
  location, the role, the resolved look, and whether it is primary.
- The published buffer shows every cursor at rest, which is the cursor color.
- The Luau `FrameSnapshot` type gains `cursors`, so automation asserts the role and the location of
  the cursor directly, and never infers it from colors.

#### C9: Cursor motion

A cursor look becomes an animated paint on the cells of its cursor, so the motion scheduler (C3)
runs the cursor like any other moving cell:

- `Blink` is a `Hold` loop of the cursor color and the color of the cell below. `Pulse` is an
  `InOut` alternating animation between the same two colors. Both start at `LastInput`, so a key,
  a mouse event, or a paste shows the cursor at full strength, and the cursor stays solid while the
  operator types.
- A change of the primary cursor's node, location, or role restarts its motion the same way.
- Secondary cursors are always at rest.
- The idle pause and `MotionSettings::enabled` apply to the cursor as to every moving cell.

#### C10: Terminal focus

- `TerminalSession` acquires focus reporting (crossterm `EnableFocusChange`) after the keyboard
  enhancements, and releases it in reverse order.
- The runtime records terminal focus when it dispatches `FocusGained` and `FocusLost`, before it
  routes them to widgets as it does now.
- While the terminal lacks focus, every loop pauses at rest, and the primary cursor's color mixes
  halfway into the cell below. `FocusGained` restores both, and restarts the cursor's motion. A
  terminal that never reports focus, such as tmux without `focus-events on`, stays focused.

#### C11: The hidden terminal cursor follows the primary cursor

Input methods place their candidate windows at the terminal cursor, and screen magnifiers follow it.
The terminal cursor is hidden, but it still has a position.

- `RenderBackend` gains `park_cursor(Option<Point>)`. After each emission, the crossterm backend moves
  the hidden cursor to the primary cursor location. With no primary cursor, the hidden cursor stays
  where it is. `TestRender` records the position for tests.

#### C12: Embedded terminals

The terminal widget (`canopy-widgets/src/terminal.rs`) declares `cursor/terminal` with the state
that its child program sets, which itty reports:

- The color is the child's cursor color, from the itty cursor state (`CursorState::color`).
- The motion is `Blink` when the child enables cursor blinking (`Tty::cursor_blink_enabled`), and
  `Steady` otherwise.
- A child block is a block. A child underline or a child bar is an underline. A bar cannot be drawn,
  and an underline keeps its meaning: a thin cursor marks insertion, as in vim's insert mode.

#### C13: Migrations

- `Editor` declares `cursor/text` in text mode, and `cursor/vi/insert`, `cursor/vi/normal`, or
  `cursor/vi/visual` by vi mode.
- `Input` declares `cursor/text` or `cursor/inactive`, as C6 describes, and stops restyling the
  cursor grapheme with `text/cursor`.
- fh moves every cursor to `Render::cursor`, its terminals take C12, and its color mixes move to
  `Color::mix`.
- The font gym drops its per-tick style maps and poll, and uses a drifting gradient.
- Modal dimming uses an effect transition (C4).
- The gyms example gains a cursor gym that shows every role, motion, and shape, and a motion gym that
  shows fades, pulses, blinks, and a drifting gradient, each with a smoke script.

#### C14: Removals

- `Widget::cursor`, and the overlay sweep of the focus chain in `Canopy::overlay_cursor`
  (`core/canopy/rendering.rs`).
- `CursorShape::Line`, and the name `Underscore`, which becomes `Underline`.
- The `text/cursor` style role and the `Input` caret glyph (`canopy-widgets/src/input.rs`, `CARET`).
- `Color::blend`, replaced by `Color::mix`.

## Decisions

- **A general facility, not cursor motion alone.** The cursor needs a clock, deadlines, composition at
  emission, and snapshots at rest. Those are the whole facility; building them for the cursor alone
  and generalizing later would build them twice.
- **Motion at emission, not in polls.** A poll invalidates layout and renders every node. A moving
  cell at emission costs one cell write, which makes a 30 fps pulse affordable.
- **Snapshots at rest.** Tests and automation never see an intermediate color, so they need no clock.
- **OKLab by default.** Mixes and gradients look even, and hue sweeps take the shorter way round.
- **Loops share one epoch.** Indicators that pulse together stay in step. The cursor starts at the
  last input instead, so that typing keeps it solid.
- **Keep `Underline` as a second shape.** It is exact on every cell, and it carries the thin cursor
  of child terminal programs. The alternative is a block for everything, which loses that meaning.
- **Blink is the default cursor motion.** `Pulse` fades rather than flashes, and it costs a sample at
  each step. It stays available for themes and users who prefer it.
- **Normal and visual modes are steady.** An insert cursor that blinks and a normal cursor that does
  not differ in motion as well as in hue, which helps operators who cannot tell green from blue.
- **Secondary cursors never move.** One moving cursor on screen shows where typing lands.

## Later Work

- **Glyph cycles at emission.** A cell whose grapheme cycles through frames, recorded like a moving
  paint, would let a `Spinner` turn without a poll or a whole frame.
- **Fading an effect out.** A removed effect would stay until its reverse transition ends, so a
  closing modal fades the view back in.
- **Transitions keyed by node.** A node whose style changes, such as a frame that gains focus, would
  fade from its old colors to its new ones. A cell-level transition cannot tell a style change from
  scrolled content, so this needs node identity.
- **Flashes.** A one-shot highlight of a node or a row, built from an effect transition that removes
  itself.

## Execution Plan

### Stage 1: Cursor roles and painting

- [x] C5: add `CursorShape`, `CursorMotion`, `CursorLook`, and the role registry with fallback, in
      `core/cursor.rs`, with `Setup::cursor_looks` and the look of each built-in theme.
- [x] C6: add `Render::cursor` and `CursorRequest`, collect declarations while rendering, and
      resolve the primary and secondary cursors.
- [x] C7: paint `Block` and `Underline` in `TermBuf` with the contrast rule, and add
      `Color::relative_luminance` and `Color::contrast_ratio`.
- [x] C8: add `FrameSnapshot::cursors` and the Luau `cursors` field.
- [x] C13, C14: migrate `Editor` and `Input`, remove `Widget::cursor`, `CursorShape::Line`,
      `text/cursor`, and `CARET`, and migrate fh.
- [x] C5: add `canopy.set_cursor_look` to the Luau API.
- [x] Test the contrast rule over light, dark, and named colors, and each shape over empty, narrow,
      and wide graphemes, clipped cursors, and cursors under a dimming effect.
- [x] Test the choice of the primary cursor over focus paths, and a secondary cursor beside it.
- [x] Test that the vi editor declares the role of each mode, and that `Input` declares
      `cursor/inactive` while it is active without focus.

### Stage 2: Color mixing and animated paints

- [x] C1: add `Color::mix` with `Mix`, the OKLab and OKLCH conversions, and `GradientSpec::mix`, and
      replace `Color::blend` in Canopy and fh.
- [x] C2: add `Paint::Animated`, `Animation` with its constructors, `GradientSpec::drift`, the rest
      color, and `Paint::map_colors` over animations.
- [x] Test OKLab round trips, mixes against reference values, the shorter hue path, and the color of
      each easing and repeat at chosen times, and at rest.

### Stage 3: Motion at emission

- [x] C3: keep motion records in `TermBuf` by grapheme, clear them on overwrite, and render at
      rest.
- [x] C3: add the `Motion` scheduler on the driver clock, its deadline in `Canopy::next_deadline`,
      `TurnOutcome::motion`, composition in `emit_frame`, and emission on motion in the terminal run
      loop.
- [x] C3: add `MotionSettings` with `Setup::set_motion`, and `canopy.set_motion` in Luau.
- [x] C4: animated paints in style rules, and `effects::transition`. Fade modal dimming in.
- [x] C13: move the font gym to a drifting gradient, and add the motion gym and its smoke script.
      `FontBanner` paints through `Render::put_covered`, so antialiased cells keep their motion.
      The gym scripts live in `examples/gyms/scripts/`, and the gyms' tests run each against its
      own demo.
- [x] Test with `ManualClock` and `TestRender`: stop edges and samples, the `max_fps` limit, the
      idle pause, `enabled = false`, an emission for motion alone that writes only the moving cells,
      the parity of a diff and a full repaint, snapshots that do not change with time, a modal over
      moving cells, and a moving wide grapheme.

### Stage 4: Cursor motion and terminal integration

- [x] C9: map cursor looks to animations that start at the last input, restart the primary cursor's
      motion when it changes, and keep secondary cursors at rest.
- [x] C10: acquire and release focus reporting in `TerminalSession`, pause loops, and dim the primary
      cursor while the terminal lacks focus.
- [x] C11: add `RenderBackend::park_cursor`, and park the hidden terminal cursor after each emission.
- [x] C12: declare `cursor/terminal` from the itty cursor state in the terminal widget.
- [x] Test blink edges and the restart on a key with `ManualClock`, the acquire and release order
      with a fake `TerminalOperations`, focus loss and gain, and the parked position in `TestRender`.

### Stage 5: Consumers and validation

- [x] C13: add the cursor gym and its smoke script.
- [ ] Check fh by hand in a terminal: editors, fields, finds, and a terminal pane that runs vim.
      Not done: this needs a live terminal. fh builds, and its tests and lints pass.
- [x] Move the Verber prompt (`../../private/verber-cli`) to the vi roles. Its mode labels and border
      colors already use the same palette colors.
- [x] Update `docs/styles.md`, `docs/scripting.md`, and the "Rendering" and "Runtime Turns"
      sections of `docs/architecture.md`.
- [x] Run `ncode tidy --check`, `ncode test`, `cargo xtask smoke`, and `ncode api` in Canopy, and
      `ncode test` in fh.
