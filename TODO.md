
- Explicit colors - at the moment, we can only get colors from our color scheme
- Actions
- Ergonomics:
  - Better EventOutcome
  - Better error returns
- Key binding management system
- ctrl-c/ctrl-z
- Fixed-width text widget
- Better debugging and monitoring story
  - Functions to dump the tree of nodes
    - Define name() for all built-in node types
  - Log-to-file
  - Add node names for relevant errors
- Improve the test render backend
  - At the moment, it's only client is the internal code, so it only implements
    logging of text. We should make it more complete for general use.



User traps
  - Not implementing layout if a node has children