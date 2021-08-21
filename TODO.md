
# TODO

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
- use half-blocks to improve smootnness of scrollbars
- serialization/deserialization for color scheme
- Better debugging and monitoring story
  - Inspector GUI
    - Logs
    - Active nodes tree
  - Functions to dump the tree of nodes
    - Define name() for all built-in node types
  - Log-to-file
  - Add node names for relevant errors
  - Benchmarking and integration tests
- Testing
  - Improve the test render backend
    - At the moment, its only client is the internal code, so it only implements
      logging of text. We should make it more complete for general use.
  - Integration tests
  - Benchmarks
  - Termion backend


# User traps and inelegances

  - Errors don't carry location information, so are often not useful for debugging
  - Not implementing render if a node has children
  - Not remembering both layout and taint after making a node modification
  - Not remembering to clear unused space
  - Not remembering to implement children and children_mut

# Bugs

  - Apps crash if terminal is too small. We should just not display in this case.