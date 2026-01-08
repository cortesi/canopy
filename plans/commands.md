# Context-aware Help for Canopy

This plan implements the context-aware help system as described in the proposal, with adaptations
based on the actual codebase structure.

## Phase 1: Command Documentation Metadata

1. Stage One: Add CommandDocSpec and extend CommandSpec

1. [x] Add `CommandDocSpec` struct to `crates/canopy/src/core/commands.rs`:
   - `short: Option<&'static str>` - single-line description
   - `long: Option<&'static str>` - full description
   - `hidden: bool` - hide from interactive help

2. [x] Add `doc: CommandDocSpec` field to `CommandSpec` struct

3. [x] Update `CommandSpec::signature()` to optionally include description

2. Stage Two: Extend derive macro for doc extraction

4. [x] In `canopy-derive/src/lib.rs`, update `MacroArgs` to include:
   - `desc: Option<syn::LitStr>` - override short description
   - `hidden: bool` - mark command as hidden

5. [x] Add `extract_doc_comments(attrs: &[syn::Attribute]) -> (Option<String>, Option<String>)`
   function that extracts short/long docs from `#[doc = "..."]` attributes

6. [x] Update `parse_command_method` to handle new attributes and extract doc comments

7. [x] Update code generation to populate `CommandDocSpec` in generated `CommandSpec`

3. Stage Three: Update print_command_table

8. [x] Update `Canopy::print_command_table` to include description column
9. [x] Add option to hide `doc.hidden` commands

## Phase 2: Binding Introspection and Storage

4. Stage Four: Store path_filter string

10. [x] Add `path_filter: Box<str>` field to `BoundAction` in `inputmap.rs`
11. [x] Update `InputMode::insert` to accept `&str` and store it alongside the `PathMatcher`
12. [x] Update all callers of `insert` to pass the path filter string

5. Stage Five: Normalize InputSpec on insert

13. [x] Modify `InputMap::bind_action` to normalize `InputSpec` before inserting
14. [x] Verify key resolution still works correctly with existing tests

6. Stage Six: Add introspection API

15. [x] Add `BindingInfo` struct with: `input: InputSpec`, `path_filter: &str`, `target: &BindingTarget`
16. [x] Add `MatchedBindingInfo` struct extending `BindingInfo` with `PathMatch`
17. [x] Add `InputMap::current_mode(&self) -> &str` method
18. [x] Add `InputMode::bindings(&self) -> Vec<BindingInfo>` method
19. [x] Add `InputMap::bindings_for_mode(&self, mode: &str) -> Vec<BindingInfo>` method
20. [x] Add `InputMode::bindings_for_path(&self, path: &Path) -> Vec<MatchedBindingInfo>` method
21. [x] Add `InputMap::bindings_matching_path(&self, mode: &str, path: &Path) -> Vec<MatchedBindingInfo>`

## Phase 3: Binding Execution Scope Fix

7. Stage Seven: Push event scope for binding-triggered commands

22. [x] In `Canopy::key`, wrap binding execution in command scope frame with `Event::Key(k)`
23. [x] In `Canopy::mouse`, wrap binding execution in command scope frame with local `MouseEvent`
24. [x] Update `Core` to expose `command_scope_for_event` helper if needed
25. [x] Add tests verifying injected params work in binding-triggered commands

## Phase 4: Command Availability

8. Stage Eight: Implement dispatch-accurate command availability

26. [x] Add `CommandResolution` enum: `Free`, `Subtree { target: NodeId }`, `Ancestor { target: NodeId }`
27. [x] Add `CommandAvailability` struct: `spec: &CommandSpec`, `resolution: Option<CommandResolution>`
28. [x] Add internal `build_owner_target_index(core: &Core, start: NodeId)` function
29. [x] Add `Canopy::command_availability_from_focus(&self) -> Vec<CommandAvailability>` method
30. [x] Add tests verifying correct target selection matches actual dispatch

## Phase 5: Help Snapshot API

9. Stage Nine: Create unified help snapshot

31. [x] Create `crates/canopy/src/core/help.rs` module with help types
32. [x] Add `BindingKind` enum: `PreEventOverride`, `PostEventFallback`
33. [x] Add `HelpBinding` struct with binding info + kind + label
34. [x] Add `HelpCommand` struct with command spec + resolution
35. [x] Add `HelpSnapshot` struct combining bindings + commands + focus context
36. [x] Implement `Canopy::help_snapshot(&self) -> HelpSnapshot` method
37. [x] Add tests for help snapshot accuracy

10. Stage Ten: Final cleanup and documentation

38. [x] Run all tests and fix any failures
39. [x] Run clippy and fix warnings
40. [x] Format code with rustfmt
