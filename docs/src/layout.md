# Layout

Layout is probably the most complex part of Canopy, and understanding the
principles behind it will make writing powerful widgets much easier.

## View

Layout computes a `View` for every node. A view captures:

- **Outer rect**: the node's allocated rectangle in screen coordinates.
- **Content rect**: the inner rectangle after subtracting padding (also in screen
  coordinates). Children are laid out and clipped to this area.
- **Scroll offset**: the top-left of the visible window in content coordinates.
- **Canvas size**: the total scrollable extent in content coordinates.

Children are positioned in the parent's content coordinate space. During render,
the engine translates child positions by the parent's scroll offset and clips
children to the parent's content rect. Widgets can query the current view to draw
scrollbars or react to available content space.


## Fit




## Rendering
