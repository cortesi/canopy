
# TODO


## 0.1

- Key binding management system
  - Key binding structure
  - Commands improvements
    - Arguments
    - Multiple return types
      - None, Result<Outcome>, Result<()>
  - Help system
    - Will require a `Root` object
  - Builder patttern extension to widgets so we can rename them
- Make module structure better
  - The import situation is a bit confusing
  - It's not clear where to find everything
- Better debugging and monitoring story
  - Inspector GUI
    - Logs
      - Level selection
      - Coloration
      - Filtering
      - Follow
      - Search
    - Active nodes tree
      - Define name() for all built-in node types
    - Command execution
    - Shrink/specify app area
    - Screenshots of app area
    - Graphs/stats
    - Maybe enable inspector with an env variable?
  - Add node names for relevant errors
  - Add warning logs where needed to aid debugging
- ControlBackend
  - Improve ergonomics - adding a function that returns a handle which re-enters rendering?
- Renderer
  - Explicit colors - at the moment, we can only get colors from our color scheme
- Support virtual cursors
  - At the moment, we use the terminal cursor. This means we have to disable the
    cursor display before a render sweep then re-enable it afterwards, causing
    flickering under some rare circumstances. We could draw the cursor ourselves
    in widgets that need one - is there a reason not to do this?
- Ergonomics:
  - Action -> Command?
  - Better error returns
- ctrl-c/ctrl-z
- Widgets
  - Editor
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
  - Numeric types and constant conversions in the geom module
  - The Outcome type

## 0.2

- serialization/deserialization for color scheme
- Termion backend
  - Extract a common set of backend conversion traits
- use half-blocks to improve smoothness of scrollbars


# User traps and inelegances

  - Errors don't carry location information, so are often not useful for debugging
  - Not implementing render if a node has children
  - Not remembering both layout and taint after making a node modification

# Bugs

  - Apps crash if terminal is too small. We should just not display in this case.
  - pager example seems to have problems with some special characters (like tabs?)
