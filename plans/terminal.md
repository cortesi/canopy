# Terminal widget plan

This plan covers adding a Terminal widget backed by `alacritty_terminal` and `portable-pty` so
Canopy can embed a full terminal (mouse reporting, truecolor, hyperlinks, bracketed paste) with
scrollback managed by the terminal itself.

## Design decisions (locked in)

Based on codebase analysis and user input, these architectural choices are pre-decided:

- **Concurrency model**: PTY reader thread writes to `Arc<Mutex<Vec<u8>>>` buffer. Widget's
  `poll()` method returns `Some(Duration::from_millis(16))` (~60fps) to drain buffer and feed
  bytes to alacritty's `Term`. This matches the pattern used by `inspector/logs.rs`.

- **Event wake strategy**: Use polling, not event injection. The `event_tx` channel is not
  exposed to widgets, and adding that coupling isn't worth the complexity. 16ms polling is
  responsive enough for terminal output.

- **Scrollback**: Return `view` from `canvas()` so Canopy never scrolls; alacritty manages all
  scrollback internally. The widget just renders what alacritty's `renderable_content()` provides.
  Scrollback navigation keys (Shift+PageUp/Down) are handled internally by the widget.

- **PTY lifecycle**: Spawn PTY in `on_mount()`. Reader thread runs until child exits or widget is
  dropped. On child exit, show "exited" overlay and fire callback for parent handling.

- **Default command**: `TerminalConfig::command` defaults to user's shell (`$SHELL` or `/bin/sh`)
  but permits explicit specification.

- **Colors**: Terminal color scheme is configurable via `TerminalConfig`, not derived from
  Canopy's StyleMap.

1. Stage One: API design

Define the public API surface before implementation.

1. [ ] Inspect `alacritty_terminal` APIs via docs.rs: `Term`, `Grid`, `Cell`, `TermMode`,
       `selection`, `vi_mode`, color types, and `EventListener` trait requirements.
2. [ ] Define `TerminalConfig` struct with fields:
       - `command: Option<Vec<String>>` (argv; None = user's shell)
       - `cwd: Option<PathBuf>`
       - `env: Vec<(String, String)>`
       - `scrollback_lines: usize`
       - `mouse_reporting: bool`
       - `bracketed_paste: bool`
       - `colors: TerminalColors` (16 ANSI + fg/bg/cursor colors)
3. [ ] Define `Terminal` widget struct with internal state: config, `Term<EventProxy>`, PTY
       handles, reader thread handle, shared byte buffer, child exit status, exited flag.
4. [ ] Define `EventProxy` (implements alacritty's `EventListener`) to capture title changes,
       clipboard requests, bell, and color queries. Wire clipboard for selection support.
5. [ ] Define `TerminalColors` struct with sensible defaults (e.g., alacritty defaults or a
       common scheme like Solarized/Dracula).
6. [ ] Document mapping from alacritty `Cell` attributes to Canopy `Style`: fg/bg colors (RGB),
       bold→Bold attr, italic→Italic, underline→Underline, strikethrough→Crossedout, dim→Dim,
       reverse→swap fg/bg, hidden→render space.

2. Stage Two: Core infrastructure

Add dependencies and minimal widget skeleton.

1. [ ] Add dependencies: `cargo add alacritty_terminal portable-pty`.
2. [ ] Create `crates/canopy/src/widgets/terminal.rs` with `Terminal` struct, `TerminalConfig`,
       `TerminalColors`, and placeholder `Widget` impl.
3. [ ] Export from `crates/canopy/src/widgets/mod.rs`.
4. [ ] Implement `Terminal::new(config: TerminalConfig)` that initializes alacritty `Term` with
       dummy size (will resize on first render). If `config.command` is None, resolve user's shell.
5. [ ] Implement `on_mount()`: spawn PTY via `portable_pty::native_pty_system().openpty()`, spawn
       command on slave, start reader thread that reads master and appends to shared buffer.
6. [ ] Implement `poll()`: drain shared buffer, feed bytes to `Term` via `term.advance()` or
       direct grid manipulation, return `Some(Duration::from_millis(16))` while child alive.
7. [ ] Handle PTY resize: track last rendered size, call `master.resize()` when view changes.

3. Stage Three: Rendering

Connect alacritty grid to Canopy's TermBuf.

1. [ ] Implement `render()`: iterate `term.renderable_content()` cells, map each `Cell` to Canopy
       `Style` using `TerminalColors`, write to `TermBuf` via `Render::put()` or `Render::text()`.
2. [ ] Handle wide characters: alacritty marks wide char spacers with `Flags::WIDE_CHAR_SPACER`;
       skip these in rendering.
3. [ ] Handle cursor: implement `cursor()` to return `Some(Cursor)` when focused, using
       alacritty's `term.cursor_style()` for shape (Block, Underline, Beam) and position from
       `renderable_content().cursor`.
4. [ ] Implement `accept_focus()` returning `true`.
5. [ ] Implement `canvas()` returning `view` (same as input) to disable Canopy scrolling.
6. [ ] Implement `measure()` returning `c.wrap()` (flex sizing, no intrinsic size).
7. [ ] When `exited` flag is set, render "Process exited (status N)" overlay centered in the
       terminal area.

4. Stage Four: Input handling

Translate Canopy events to PTY input.

1. [ ] Implement `on_event()` for `Event::Key`: translate key + modifiers to terminal escape
       sequence using alacritty's input handling or manual CSI sequences. Write to PTY master.
2. [ ] Handle special keys: Enter→`\r`, Tab→`\t`, Backspace→`\x7f` or `\x08`, arrows→CSI
       sequences, function keys, Home/End/PageUp/PageDown.
3. [ ] Handle Shift+PageUp/PageDown internally for scrollback navigation (call
       `term.scroll_display()` rather than sending to PTY).
4. [ ] Handle `Event::Paste`: wrap content in bracketed paste sequences (`\x1b[200~`...
       `\x1b[201~`) if `TermMode::BRACKETED_PASTE` is set.
5. [ ] Handle `Event::Mouse`: if `TermMode::MOUSE_MODE` active, encode mouse events as SGR or
       legacy sequences and write to PTY.
6. [ ] Handle `Event::FocusGained`/`Event::FocusLost`: update `term.is_focused` flag, optionally
       send focus reporting sequences if enabled.

5. Stage Five: Selection and clipboard

Enable text selection and copy functionality.

1. [ ] Wire alacritty's selection state: track selection start on mouse down, extend on drag,
       finalize on mouse up.
2. [ ] Render selection highlighting in `render()` by checking `term.selection` and applying
       reverse video or a distinct background color to selected cells.
3. [ ] Implement copy: on Ctrl+Shift+C (or configurable binding), extract selected text from
       alacritty's grid and invoke a clipboard callback or write to system clipboard.
4. [ ] Handle double-click for word selection, triple-click for line selection (alacritty has
       semantic selection support).
5. [ ] Clear selection on any PTY input or when user starts typing.

6. Stage Six: Child lifecycle

Handle process termination gracefully.

1. [ ] In reader thread: detect EOF (read returns 0), set exit flag in shared state, stop loop.
2. [ ] In `poll()`: check exit flag, when set call `child.try_wait()` to get `ExitStatus`, store
       it, return `None` to stop polling.
3. [ ] Add `Terminal::exit_status() -> Option<ExitStatus>` method.
4. [ ] Add `Terminal::is_running() -> bool` method.
5. [ ] Add `Terminal::on_exit` callback field (or define an `on_exit()` method for the parent to
       override) that fires when the child process terminates.
6. [ ] Ensure PTY handles are dropped cleanly (writer closed, reader thread joined) on widget
       drop.

7. Stage Seven: Example and validation

Create a demo app with multi-terminal support.

1. [ ] Create `crates/examples/termgym/` with a multi-terminal app:
       - Left panel: list of terminal instances with selector to switch between them
       - Right panel: active terminal widget
       - Button/keybinding to create new terminal instances
       - Each terminal runs the user's shell independently
2. [ ] Test: basic shell interaction (typing, output, prompt).
3. [ ] Test: colors and attributes (run `ls --color`, check output styling).
4. [ ] Test: resize behavior (resize window, verify terminal adjusts).
5. [ ] Test: mouse reporting (run `vim` or similar, verify mouse clicks work).
6. [ ] Test: scrollback (Shift+PageUp/Down scrolls history).
7. [ ] Test: selection (click-drag selects text, Ctrl+Shift+C copies).
8. [ ] Test: exit overlay appears when shell exits.
9. [ ] Test: switching between terminals preserves each terminal's state.
10. [ ] Test: creating new terminals while others are running.

8. Stage Eight: Hygiene

Final cleanup and lint pass.

1. [ ] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests
       --examples 2>&1` and fix all warnings.
2. [ ] Run tests: `cargo nextest run --all --all-features` (or `cargo test --all --all-features`).
3. [ ] Run formatting: `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml`.
4. [ ] Review public API surface with `ruskel crates/canopy --private --search Terminal` to
       ensure minimal, clean API.

## Future enhancements (not in scope)

- OSC 52 clipboard integration (read/write system clipboard via terminal escapes)
- Hyperlink detection and click handling (OSC 8)
- Bell notification callback
- Title change callback
- Alternate screen buffer state exposure
- Sixel/image protocol support
