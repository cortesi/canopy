# Keyboard disambiguation for terminal input

Address the canopy terminal widget's legacy-only key emission and the crossterm backend's lack of
keyboard enhancement flags, with optional alignment across sibling projects (eguitty/eguito).

1. Stage One: Scope and API decisions

Confirm the relevant APIs and decide how the new behavior is configured.

1. [x] Inspect alacritty_terminal Config/TermMode with ruskel to confirm kitty_keyboard and
       DISAMBIGUATE_ESC_CODES behavior and defaults.
       Result: `term::Config` has `kitty_keyboard: bool` (Default derives; defaults false). TermMode
       exposes `DISAMBIGUATE_ESC_CODES`.
2. [x] Inspect crossterm 0.29 keyboard enhancement API (ruskel/docs) to confirm enable/disable and
       flag restoration patterns.
       Result: use `PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)`
       and restore with `PopKeyboardEnhancementFlags` (stack-based).
3. [x] Decide whether kitty keyboard support and crossterm enhancements are default-on or optional,
       and document the choice in code/docs.
       Decision: enable kitty keyboard by default in the terminal widget because CSI-u emission is
       gated by `TermMode::DISAMBIGUATE_ESC_CODES`; add a config knob to opt out if needed. Enable
       crossterm keyboard enhancements by default, with a RunloopOptions opt-out for compatibility.

2. Stage Two: Terminal widget kitty protocol support

Add kitty keyboard protocol support and gate CSI-u emission on DISAMBIGUATE_ESC_CODES.

1. [x] Extend `TerminalConfig` (and defaults) with a kitty keyboard enable flag if needed; plumb it
       into `alacritty_terminal::Config` when constructing `Term`.
       Result: added `TerminalConfig::kitty_keyboard` (default true) and wired into term config.
2. [x] Update `Terminal::encode_key` to emit CSI-u sequences only when
       `TermMode::DISAMBIGUATE_ESC_CODES` is active, falling back to legacy sequences otherwise.
       Result: CSI-u now used for Esc + Ctrl/Alt-modified raw keys when DISAMBIGUATE is set.
3. [x] Add unit tests for key encoding (legacy vs CSI-u) in `crates/canopy-widgets/src/terminal.rs`.
       Result: tests cover legacy vs disambiguated Ctrl+Tab/Ctrl+C/Esc.

3. Stage Three: Crossterm keyboard enhancement

Enable disambiguated escape codes for crossterm key input and restore prior flags on exit.

1. [x] Enable `KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES` on backend start, capture the
       previous flags, and restore them on backend stop/drop.
       Result: `CrosstermControl` now pushes DISAMBIGUATE on enter and pops on exit with tracking.
2. [x] If required, extend `RunloopOptions` to allow opting out, and update examples using
       `runloop_with_options`.
       Result: `RunloopOptions::enable_keyboard_enhancements` added (default true); examples rely on
       defaults.
3. [x] Add or document validation coverage for the keyboard enhancement enable/restore behavior.
       Result: no direct unit tests; behavior is exercised through the runloop path.

4. Stage Four: Cross-repo alignment (eguitty/eguito)

Apply matching changes in sibling repos, if they exist locally.

1. [x] Search eguitty/eguito for alacritty_terminal or crossterm key handling and align changes or
       document intentional differences.
       Result: eguitty commit a67c975 already adds kitty-aware modified-Tab encoding; eguito has no
       matches for alacritty/crossterm keyboard handling.
2. [x] Update relevant docs/README entries to describe the new keyboard behavior and config knobs.
       Result: added Terminal widget notes in docs.

5. Stage Five: Validation

Run the repo's required lint/format/test steps before delivery.

1. [x] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests --examples`.
2. [x] Run `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml` (or `cargo +nightly fmt
       --all` if needed).
3. [x] Run `cargo nextest run --all --all-features` (or `cargo test --all --all-features`).
       Result: nextest passed (335 tests, 9 skipped).
