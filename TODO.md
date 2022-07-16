
# TODO


## 0.1

- Key binding management system
  - Input to and from string conversion
- Commands improvements
  - Optional core argument
    - This will make "dual-purpose" functions that are intended both to be
      called from scripts and in code more ergonomic
  - Arguments
  - Return types, add as needed
    - Int
    - Core data types?
      - Node reference
      - Focus path
- Root object
  - Manage modal windows
  - Command help system
  - Manage inspector
  - Key rebinding
  - Standard commands
    - Reloading/changing key bindings and color schemes
    - Command mode pop
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
    - Maybe enable inspector with an env variable?
  - Add node names for relevant errors
  - Add warning logs where needed to aid debugging
- Core
  - Improve ergonomics - adding a function that returns a handle which re-enters rendering?
- Renderer
  - Explicit colors - at the moment, we can only get colors from our color scheme
- Support virtual cursors
  - At the moment, we use the terminal cursor. This means we have to disable the
    cursor display before a render sweep then re-enable it afterwards, causing
    flickering under some rare circumstances. We could draw the cursor ourselves
    in widgets that need one - is there a reason not to do this?
- Ergonomics:
  - Better error returns
    - consider https://github.com/zkat/miette
  - script execution errors need to be improved
- ctrl-c/ctrl-z
- Widgets
  - Editor
    - Wrap Helix?
  - Pad
  - Center
  - text line widget
  - markdown
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

## 0.2

- serialization/deserialization for color scheme
- Termion backend
  - Extract a common set of backend conversion traits
- use half-blocks to improve smoothness of scrollbars
- Remote commander
  - Standard way to execute scripts within an application remotely

# User traps and inelegances

  - Errors don't carry location information, so are often not useful for debugging
  - Not implementing render if a node has children
  - Not remembering both layout and taint after making a node modification

# Bugs

  - Apps crash if terminal is too small. We should just not display in this case.
  - pager example seems to have problems with some special characters (like tabs?)
