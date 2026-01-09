# Contextual Help Modal for Canopy

This document describes the design for a contextual help system that displays available
bindings and commands from the current focus context.

## Overview

The help modal displays a snapshot of:
- **Bindings**: Key/mouse bindings that would fire from the current focus path
- **Commands**: Commands that are available (have a dispatch target) vs unavailable

The help modal is a centered overlay managed by the `Root` widget. When activated, it captures
a snapshot of the current help context and dims the background. Users can browse bindings and
commands, then dismiss the modal to return to normal operation.

## Design Decisions

- **Layout**: Centered modal overlay with dim effect on background
- **Snapshot behavior**: Capture once when opened; close and reopen to refresh
- **Hidden commands**: Excluded by default
- **Default keybinding**: `Ctrl+?`

## Architecture

### Module Structure

```
crates/canopy-widgets/src/
  help/
    mod.rs          # Help widget, install logic, owned snapshot types
    bindings.rs     # Bindings list view
    commands.rs     # Commands list view
```

### Root Integration

Root gains:
- `help_active: bool` state
- `HelpSlot` typed key for the help widget
- Commands: `show_help`, `hide_help`, `toggle_help`
- Default binding: `Ctrl+?` to toggle help

When help is shown:
1. Capture `HelpSnapshot` and store owned copy in Help widget
2. Apply dim effect to the app subtree
3. Show the help modal (Stack layout overlay)
4. Focus moves into the help modal

When help is hidden:
1. Remove dim effect from app subtree
2. Hide the help modal
3. Restore focus to previous location in app

### Help Widget

The Help widget:
1. Is placed as a sibling to the app in Root using Stack layout for overlay
2. Contains the help content wrapped in Frame for visual boundary
3. Uses Modal widget for centering
4. Stores an owned version of the help snapshot for display

### Owned Snapshot Types

The `HelpSnapshot` from canopy core uses lifetimes tied to `Canopy`. For storage in the Help
widget, we need owned versions:

```rust
pub struct OwnedHelpBinding {
    pub input: InputSpec,
    pub mode: String,
    pub path_filter: String,
    pub kind: BindingKind,
    pub label: String,
}

pub struct OwnedHelpCommand {
    pub id: String,
    pub owner: Option<String>,
    pub short: Option<String>,
    pub resolution: Option<CommandResolution>,
}

pub struct OwnedHelpSnapshot {
    pub focus_path: Path,
    pub input_mode: String,
    pub bindings: Vec<OwnedHelpBinding>,
    pub commands: Vec<OwnedHelpCommand>,
}
```

### Display Format

**Bindings Tab:**
```
┌─ Bindings ─────────────────────────────┐
│ Key        Filter       Description    │
│ ─────────────────────────────────────  │
│ q          root         Exit program   │
│ j          editor/      Move down      │
│ k          editor/      Move up        │
│ Ctrl+s     *            Save file      │
│ ...                                    │
└────────────────────────────────────────┘
```

**Commands Tab:**
```
┌─ Commands ─────────────────────────────┐
│ Command              Target     Status │
│ ─────────────────────────────────────  │
│ root::quit           root       ●      │
│ editor::save         editor     ●      │
│ list::select_next    (none)     ○      │
│ ...                                    │
└────────────────────────────────────────┘
```

Legend: ● = available (has target), ○ = unavailable

## Implementation Stages

### Stage 1: Help Widget Skeleton and Root Integration

1. [x] Create `help/mod.rs` with `Help` widget struct
2. [x] Add `Help::install(core)` to build the help subtree (Frame > Modal > content)
3. [x] Implement `Widget`, `DefaultBindings`, `Loader` for Help
4. [x] Add `HelpSlot` key and `help_active` state to Root
5. [x] Change Root layout to Stack (app and help as siblings)
6. [x] Add `show_help`, `hide_help`, `toggle_help` commands to Root
7. [x] Implement dim effect toggle on app when help shown/hidden
8. [x] Add default binding `Ctrl+?` for toggle_help
9. [x] Update `Root::install` to create help widget (hidden by default)

### Stage 2: Owned Snapshot Types and Capture

10. [x] Define `OwnedHelpBinding`, `OwnedHelpCommand`, `OwnedHelpSnapshot` in help module
11. [x] Add conversion from `HelpSnapshot` to `OwnedHelpSnapshot`
12. [x] Add `snapshot: Option<OwnedHelpSnapshot>` field to Help widget
13. [x] Capture and store snapshot when `show_help` is called
14. [x] Clear snapshot when `hide_help` is called (handled by overwrite on next show)

### Stage 3: Bindings Display

15. [ ] Create `help/bindings.rs` with `BindingsList` widget
16. [ ] Display bindings using Text widget with formatted columns
17. [ ] Sort bindings by kind (pre-event first), then input
18. [ ] Add selection and scroll navigation

### Stage 4: Commands Display

19. [ ] Create `help/commands.rs` with `CommandsList` widget
20. [ ] Display commands with ID, target owner, availability indicator
21. [ ] Sort alphabetically by command ID
22. [ ] Filter out hidden commands
23. [ ] Add selection and scroll navigation

### Stage 5: Tabbed Interface

24. [ ] Add Tabs widget to Help to switch between Bindings and Commands
25. [ ] Tab navigation with Tab key
26. [ ] Style tabs to indicate active view

### Stage 6: Polish

27. [ ] Style layer "help" for help panel visual distinction
28. [x] Escape and `q` keys to close help
29. [ ] Tests for help activation, snapshot capture, and dismiss
