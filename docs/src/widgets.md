# Widgets

Canopy ships with a small widget library. This section highlights the editor and input widgets;
the rest follow the core `Widget` trait and the event/lifecycle guidelines in this book.

## Editor

The `Editor` widget is a multi-line text editor that can also be configured for single-line input.
It supports wrapping, vi-style modal editing, mouse selection, search and replace, and optional
syntax highlighting.

### Configuration

`EditorConfig` controls behavior:
- `multiline`: Allow newlines. When false, Enter bubbles and newlines are normalized to spaces.
- `wrap`: `WrapMode::None` for horizontal scrolling, `WrapMode::Soft` for soft wrap.
- `auto_grow`, `min_height`, `max_height`: Size the editor to its content.
- `mode`: `EditMode::Text` or `EditMode::Vi`.
- `line_numbers`: `None`, `Absolute`, or `Relative`.
- `tab_stop`, `read_only`.

### Vi mode

Normal/insert/visual with a focused subset:
- Motions: `h/j/k/l`, `w/b/e`, `0/$`, `^`, `gg/G`, `gj/gk`.
- Edits: `x`, `dd`, `cc`, `D`, `C`, visual `d/y/c`, `>`/`<`.
- Yank/put: `yy`, `p`, `P`.
- Search: `/`, `?`, `n`, `N`, replace with `R`.
- Undo/redo: `u`, `Ctrl+r`, repeat with `.`.

### Syntax highlighting

You can plug in a highlighter by implementing `Highlighter` or using `SyntectHighlighter`:

```rust
use canopy::widgets::editor::{Editor, EditorConfig};
use canopy::widgets::editor::highlight::SyntectHighlighter;

let mut editor = Editor::with_config("", EditorConfig::new());
editor.set_highlighter(Some(Box::new(SyntectHighlighter::plain())));
```

Highlight spans inherit the editor background, so themes only influence foreground colors and
attributes unless you supply a custom highlighter. The default syntect palette is
`Solarized (dark)` to match the editor's dark background.

`SyntectHighlighter::with_theme_name` selects a named theme from syntect's default theme set, and
`SyntectHighlighter::with_theme` lets you pass a `Theme` directly for full palette control.

## Input

The `Input` widget is a single-line text field that shares the editor's buffer and column mapping
logic. It renders with horizontal scrolling to keep the cursor visible.

## Terminal

`Terminal` embeds a PTY-backed terminal using `alacritty_terminal`.

`TerminalConfig::kitty_keyboard` controls whether kitty keyboard protocol negotiation is enabled
(default on). When the child enables `DISAMBIGUATE_ESC_CODES`, the widget emits CSI-u sequences for
ambiguous modified keys; otherwise it falls back to legacy sequences.

For the crossterm backend, `RunloopOptions::enable_keyboard_enhancements` controls whether
`KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES` is pushed on startup.

## FontBanner

`FontBanner` renders large terminal text from a TTF font into the available region. It scales to the
target height and clips overflow by default.

```rust
use canopy_widgets::{Font, FontBanner, FontRenderer, LayoutOptions};

let font = Font::from_bytes(include_bytes!("MyFont-Regular.ttf"))?;
let renderer = FontRenderer::new(font);
let banner = FontBanner::new("Canopy", renderer).with_layout_options(LayoutOptions::default());
```

Use the style system to apply solid or gradient paint to the banner via its style path.
