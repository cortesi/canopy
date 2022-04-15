
# TODO


## 0.1

- Better debugging and monitoring story
  - Inspector GUI
    - Logs
    - Active nodes tree
      - Define name() for all built-in node types
    - Shrink/specify app area
    - Screenshots of app area
  - Add node names for relevant errors
  - Benchmarking and integration tests
- Further simplify Node trait by shifting wrap and friends into module namespace
- ControlBackend
  - Improve ergonomics - adding a function that returns a handle which re-enters rendering?
- Renderer
  - Explicit colors - at the moment, we can only get colors from our color scheme
- Ergonomics:
  - Better error returns
- Key binding management system
- ctrl-c/ctrl-z
- Widgets
  - Pad
  - Center
  - text line widget
  - markdown
- serialization/deserialization for color scheme
- Testing
  - Improve the test render backend
    - At the moment, its only client is the internal code, so it only implements
      logging of text. We should make it more complete for general use.
  - Integration tests
  - Benchmarks
  - Fuzzing

## 0.2

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
