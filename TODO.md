
# TODO


## 0.1

- Editor
  - Consolidate modal key bindings and consider how to support editor
- Focus
  - Add ability to "pop" focus and inspect previous focus states within a subtree. This will unlock a lot of
    functionality for the inspector.
- Commands improvements
  - Return and arg types, add as needed
- Root object
  - Manage modal windows
  - Command help system
  - Key rebinding
  - Standard commands
    - Reloading/changing key bindings and color schemes
    - Command mode set/pop
- Better debugging and monitoring story
  - Inspector GUI
    - Logs
      - Follow
      - Level selection
      - Coloration
      - Filtering
      - Search
    - Command execution
    - Shrink/specify app area
    - Screenshots of app area
    - Graphs/stats
  - Add node names for relevant errors
  - Add warning logs where needed to aid debugging
- Renderer
  - Explicit colors - at the moment, we can only get colors from our color scheme
- Support virtual cursors
  - At the moment, we use the terminal cursor to display a cursor. This means we have to disable the cursor display
    before a render sweep then re-enable it afterwards, causing flickering under some rare circumstances. We could draw
    the cursor ourselves in widgets that need one - is there a reason not to do this?
  - https://en.wikipedia.org/wiki/Combining_character
- Ergonomics:
  - Warn when no matching node::command is found
  - Better error returns
    - consider https://github.com/zkat/miette
  - script execution errors need to be improved
- ctrl-c/ctrl-z
- Widgets
  - Tree
  - Pad
  - Center
  - text line widget
  - markdown
- Key binding management system
  - Input to and from string conversion
- Testing
  - Improve the test render backend
    - At the moment, its only client is the internal code, so it only implements
      logging of text. We should make it more complete for general use.
  - Integration tests
  - Benchmarks
  - Fuzzing
- Things that don't smell too good
  - focus_next wraps, but focus_prev doesn't
  - Numeric types and constant conversions in the geom module
  - The Outcome type
  - Improve module structure
    - The import situation is a bit confusing
    - It's not clear where to find everything
    - Punting on passing state around may be a mistake

## 0.2

- serialization/deserialization for color scheme
- Termion backend
  - Extract a common set of backend conversion traits
- Native rendered backend without a terminal emulator
- use half-blocks to improve smoothness of scrollbars
- Remote commander
  - Standard way to execute scripts within an application remotely

# User traps and inelegances

  - Errors don't carry location information, so are often not useful for debugging
  - Not implementing render if a node has children
  - Not remembering to call fit() on all child nodes on render
    - It's not clear that it's necessary to call fit every render sweep, so I'm
      not sure if we can just add a check for this.

# Bugs

  - Apps crash if terminal is too small. We should just not display in this case.
  - pager example seems to have problems with some special characters (like tabs?)
