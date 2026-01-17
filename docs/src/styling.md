# Themes and styling

Canopy uses a path-based styling system. Widgets render with style names (strings like
"frame/focused"), and a `StyleMap` resolves those names into concrete colors and attributes.

## Style paths

Style paths are slash-separated components. The empty path `""` or `"/"` refers to the root style.
Child paths extend the base style and override only the fields you set.

Example paths:

- `"frame"`
- `"frame/focused"`
- `"list/selected"`

## Defining a style map

A `StyleMap` stores partial style rules. You can build it with the fluent `rules()` API:

```rust
use canopy::style::{Attr, StyleMap, StyleRules, solarized};

let mut style = StyleMap::new();
style
    .rules()
    .fg("frame", solarized::BASE0)
    .fg("frame/focused", solarized::BASE3)
    .bg("list/selected", solarized::BLUE)
    .attr("list/selected", Attr::Bold)
    .apply();
```

You can also add attributes directly with `StyleMap::add_attr` for small tweaks.

## Applying styles

To switch the active style map at runtime, call `Context::set_style`. The new style map is applied
before the next render.

```rust
ctx.set_style(style_map);
```

## Rendering with styles

Rendering APIs accept a style path string. The resolved style is computed by merging rules along
that path.

```rust
fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
    let line = ctx.view().outer_rect_local().line(0);
    r.text("frame/focused", line, "Hello")
}
```

## Paint and gradients

Style foreground and background channels now accept `Paint`, which can be a solid color or a
gradient. Gradients are defined with an angle (in degrees) and color stops.

```rust
use canopy::style::{GradientSpec, Paint, StyleMap, solarized};

let mut style = StyleMap::new();
style
    .rules()
    .fg(
        "banner",
        Paint::gradient(GradientSpec::new(
            90.0,
            solarized::CYAN,
            solarized::BLUE,
        )),
    )
    .apply();
```

Renderers resolve gradients per-cell within the supplied bounds, so the same style can be reused
for different sized widgets.

## Style effects

Style effects are dynamic, composable transforms that apply to a subtree. They are pushed via the
context and combined during render.

```rust
use canopy::style::effects;

ctx.push_effect(ctx.node_id(), effects::dim(0.5))?;
```

Effects are useful for overlays, disabled states, or hover/focus treatments that should apply to
all descendants without redefining their rules.

## Theme modules

Canopy ships with a few theme palettes (`solarized`, `dracula`, `gruvbox`) under
`canopy::style`. Use them to build a `StyleMap` or as a starting point for your own theme.
